use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use slot_retro::{ButtonMask, RetroCore, Rumble, GBA_H, GBA_W};

use crate::audio::Ring;
use crate::drc::{drc_ratio, drc_target};
use crate::frames::{FrameRef, Frames};
use crate::persist::Snapshot;
use crate::resample::Resampler;
use crate::rewind::{Rewind, REWIND_BYTES};

/// Present is locked to the 60 Hz panel and the core is stepped once per present, so the
/// 0.456% the GBA runs slow lands entirely on audio rate control.
const PRESENT: Duration = Duration::from_nanos(16_666_667);

/// Core frames per present while fast forwarding. There is no ramp and no adaptive cap: the
/// core is stepped this many times and the deadline below absorbs whatever that costs.
///
/// Four, because an H700 cannot serve more. Eight was measured on hardware and pegged the
/// worker at the same 100.5% of one core that four does — a thread already at 100% does the
/// same work per second either way, so the extra steps bought no speed at all. What they cost
/// was presents: the deadline sleep was never reached, so the picture fell from 60 Hz to
/// around 30 at the same ~4x. A step count the hardware cannot serve does not run faster, it
/// runs choppier.
pub const FAST_STEPS: u32 = 4;

/// Snapshot every other frame, so rewinding at one pop per present runs back at 2x. Every
/// frame would double the serialize cost for playback nobody watches at real time anyway.
const SNAPSHOT_EVERY: u32 = 2;

/// Frames between traced pacing lines, about five seconds.
const TRACE_EVERY: u64 = 300;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Speed {
    Paused,
    Normal,
    Fast,
}

impl Speed {
    /// The one place the wire encoding is decided, so the worker's read of what it was told
    /// and the handle's read of what the worker saw cannot drift apart by having two matches
    /// that quietly stop agreeing.
    fn from_u8(v: u8) -> Speed {
        match v {
            0 => Speed::Paused,
            1 => Speed::Normal,
            _ => Speed::Fast,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CoreState {
    Loading,
    Ready,
    Failed,
}

pub struct EmuHandle {
    frames: Arc<Frames>,
    shared: Arc<Shared>,
    cmds: Sender<Cmd>,
    join: Option<JoinHandle<()>>,
    rumble: Rumble,
}

enum Cmd {
    Load(Vec<u8>),
    Save(Sender<Vec<u8>>),
    Sav(Sender<Option<Vec<u8>>>),
    Thumb(Sender<Option<Vec<u8>>>),
}

struct Shared {
    input: AtomicU16,
    speed: AtomicU8,
    /// What the worker last read `speed` as, stored right after that read with `Release` so
    /// `EmuHandle::observed_speed` can tell a stopped core from a merely descheduled one — see
    /// its doc comment.
    observed: AtomicU8,
    state: AtomicU8,
    rewind: AtomicBool,
    /// How much rewind history is left, 0 to 100, for the HUD bar to draw.
    rewind_fill: AtomicU8,
    stop: AtomicBool,
    /// 0 to 100. Read by the worker every batch, so a change lands within one frame.
    volume: AtomicU8,
    /// Frames this core has published. Counted rather than peeked because `Frames::latest`
    /// consumes: anything that asks the buffer a question steals a frame from the renderer.
    published: AtomicU64,
}

impl EmuHandle {
    /// The ring rather than the device: it was opened before this cart and it outlives it,
    /// so the slot can still make a noise with no core running.
    pub fn spawn(
        core: Box<dyn RetroCore>,
        rom: PathBuf,
        ring: Arc<Ring>,
        sav: Option<Vec<u8>>,
        resume: Option<Vec<u8>>,
    ) -> Self {
        // Taken before the core goes to its thread, which is the last moment this side can
        // reach it.
        let rumble = core.rumble();
        let frames = Frames::new((GBA_W * GBA_H * 4) as usize);
        let shared = Arc::new(Shared {
            input: AtomicU16::new(0),
            // Paused until told otherwise. A core spawned during the insert would
            // otherwise run a frame or two before the session's first `sync_speed` lands,
            // and those frames are the start of the bios boot animation.
            speed: AtomicU8::new(Speed::Paused as u8),
            // Matches `speed`'s own initial value: no iteration has run yet, so nothing has
            // been observed but the value it will start from.
            observed: AtomicU8::new(Speed::Paused as u8),
            state: AtomicU8::new(CoreState::Loading as u8),
            rewind: AtomicBool::new(false),
            rewind_fill: AtomicU8::new(0),
            stop: AtomicBool::new(false),
            volume: AtomicU8::new(100),
            published: AtomicU64::new(0),
        });
        let (tx, rx) = channel();
        let worker = Worker {
            frames: frames.clone(),
            shared: shared.clone(),
            cmds: rx,
        };
        let join = std::thread::Builder::new()
            .name("slot-emu".into())
            .spawn(move || worker.run(core, rom, ring, sav, resume))
            .ok();
        if join.is_none() {
            shared
                .state
                .store(CoreState::Failed as u8, Ordering::Release);
        }
        EmuHandle {
            frames,
            shared,
            cmds: tx,
            join,
            rumble,
        }
    }

    /// The core's end of the motor, written from the emulator thread and read from the
    /// render one. Keeping the device write on this side is the whole reason it is a cell.
    pub fn rumble(&self) -> &Rumble {
        &self.rumble
    }

    pub fn set_input(&self, mask: ButtonMask) {
        self.shared.input.store(mask.0, Ordering::Relaxed);
    }

    pub fn latest_frame(&self) -> Option<FrameRef> {
        self.frames.latest()
    }

    pub fn set_speed(&self, speed: Speed) {
        self.shared.speed.store(speed as u8, Ordering::Relaxed);
    }

    /// L2 is momentary and takes precedence over fast forward, so this is a separate axis
    /// from `Speed` rather than another value of it: releasing it returns to whatever the
    /// speed already was.
    /// Whether this core has produced anything yet. Never gate the game layer on the
    /// handle existing: it is built before its worker has run a single frame.
    pub fn has_published(&self) -> bool {
        self.shared.published.load(Ordering::Relaxed) > 0
    }

    pub fn frame_ready(&self) -> bool {
        self.frames.is_ready()
    }

    pub fn frames_taken(&self) -> u64 {
        self.frames.taken()
    }

    pub fn published_count(&self) -> u64 {
        self.shared.published.load(Ordering::Relaxed)
    }

    /// What the worker last read `speed` as, not what this side last told it to be — the gap
    /// between those two is exactly the race an eject has to close. `Acquire`, paired with the
    /// worker's `Release` store, means a caller who sees `Paused` here is also guaranteed to
    /// see every frame `publish` counted before that store: a fact about the last iteration
    /// the worker actually ran, not an inference from a count that merely has not moved yet.
    pub fn observed_speed(&self) -> Speed {
        Speed::from_u8(self.shared.observed.load(Ordering::Acquire))
    }

    pub fn set_volume(&self, level: u8) {
        self.shared.volume.store(level.min(100), Ordering::Relaxed);
    }

    pub fn set_rewinding(&self, on: bool) {
        self.shared.rewind.store(on, Ordering::Relaxed);
    }

    pub fn rewind_fill(&self) -> u8 {
        self.shared.rewind_fill.load(Ordering::Relaxed)
    }

    pub fn state(&self) -> CoreState {
        match self.shared.state.load(Ordering::Acquire) {
            0 => CoreState::Loading,
            1 => CoreState::Ready,
            _ => CoreState::Failed,
        }
    }

    /// The state arrives on the receiver once the worker reaches a frame boundary. A dead
    /// worker closes the channel rather than leaving the caller waiting forever.
    pub fn request_state(&self) -> Receiver<Vec<u8>> {
        let (tx, rx) = channel();
        let _ = self.cmds.send(Cmd::Save(tx));
        rx
    }

    pub fn request_load(&self, state: Vec<u8>) {
        let _ = self.cmds.send(Cmd::Load(state));
    }

    pub fn snapshot(&self) -> EmuSnapshot {
        EmuSnapshot {
            cmds: self.cmds.clone(),
        }
    }
}

/// The flush paths need the core's bytes, not its thread or its frames. Cloning the
/// command sender is the whole of that.
#[derive(Clone)]
pub struct EmuSnapshot {
    cmds: Sender<Cmd>,
}

impl Snapshot for EmuSnapshot {
    fn state(&self) -> Option<Vec<u8>> {
        let (tx, rx) = channel();
        self.cmds.send(Cmd::Save(tx)).ok()?;
        rx.recv().ok()
    }

    fn save_ram(&self) -> Option<Vec<u8>> {
        let (tx, rx) = channel();
        self.cmds.send(Cmd::Sav(tx)).ok()?;
        rx.recv().ok().flatten()
    }

    fn thumb(&self) -> Option<Vec<u8>> {
        let (tx, rx) = channel();
        self.cmds.send(Cmd::Thumb(tx)).ok()?;
        rx.recv().ok().flatten()
    }

    fn load(&self, state: Vec<u8>) {
        let _ = self.cmds.send(Cmd::Load(state));
    }
}

impl Drop for EmuHandle {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

struct Worker {
    frames: Arc<Frames>,
    shared: Arc<Shared>,
    cmds: Receiver<Cmd>,
}

impl Worker {
    fn run(
        self,
        mut core: Box<dyn RetroCore>,
        rom: PathBuf,
        ring: Arc<Ring>,
        sav: Option<Vec<u8>>,
        resume: Option<Vec<u8>>,
    ) {
        if let Err(e) = core.load(&rom) {
            eprintln!("slot: {e}");
            self.shared
                .state
                .store(CoreState::Failed as u8, Ordering::Release);
            return;
        }
        // After the load: there is no save ram to copy into until the rom says how much of
        // it there is. A game with none at all is not a failure to boot.
        if let Some(sav) = sav {
            if let Err(e) = core.load_save_ram(&sav) {
                eprintln!("slot: save ram: {e}");
            }
        }
        // Before Ready, so the reveal shows where the cart left off rather than a frame of
        // the intro. A state the core will not take leaves the save ram loaded above, which
        // costs the player their position but not their progress.
        if let Some(resume) = resume {
            if let Err(e) = core.unserialize(&resume) {
                eprintln!("slot: resume: {e}");
            }
        }
        let av = core.av_info();
        // A device that refused the GBA's rate reports its own, and a device that failed to
        // open reports zero, which the resampler reads as "no conversion to do".
        let device_hz = match ring.sample_rate() {
            0 => av.sample_rate,
            hz => hz as f64,
        };
        // The core is stepped once per present, so a 60 Hz frame carries a 59.7275 Hz frame's
        // worth of audio. That surplus is the resampler's to absorb in its base rate. Left to
        // DRC's trim, which is proportional and only reaches full authority at twice target,
        // it parks occupancy at 91% of the ring: measured, and one late frame from the top.
        let core_hz = match av.fps {
            fps if fps > 0.0 => av.sample_rate / (fps * PRESENT.as_secs_f64()),
            _ => av.sample_rate,
        };
        let mut resampler = Resampler::new(core_hz, device_hz);
        ring.clear_faults();
        self.shared
            .state
            .store(CoreState::Ready as u8, Ordering::Release);

        let mut out = Vec::new();
        let mut muted_at = Speed::Normal;
        let mut rewind = Rewind::new(REWIND_BYTES);
        let mut since_snapshot = 0;
        let mut deadline = Instant::now();
        let mut paced = 0u64;
        while !self.shared.stop.load(Ordering::Relaxed) {
            for cmd in self.cmds.try_iter() {
                match cmd {
                    Cmd::Save(reply) => match core.serialize() {
                        Ok(state) => {
                            let _ = reply.send(state);
                        }
                        Err(e) => eprintln!("slot: {e}"),
                    },
                    Cmd::Load(state) => {
                        if let Err(e) = core.unserialize(&state) {
                            eprintln!("slot: {e}");
                        }
                    }
                    Cmd::Sav(reply) => {
                        let _ = reply.send(core.save_ram());
                    }
                    Cmd::Thumb(reply) => {
                        let _ = reply.send(crate::thumb::png(core.video_xrgb8888()));
                    }
                }
            }

            let speed = self.speed();
            // Published before anything below acts on it, and with `Release`: a reader who
            // observes `Paused` from this store is thereby also guaranteed to see every frame
            // `publish` counted on an earlier pass, because that publish happened-before this
            // store in program order and `Release`/`Acquire` makes that ordering visible across
            // threads. `publish`'s own counter is `Relaxed` and leans on this pairing for it —
            // `Relaxed` here would leave that unordered, trading the scheduling race this exists
            // to close for a subtler visibility one.
            self.shared.observed.store(speed as u8, Ordering::Release);
            if speed != muted_at {
                // Fast forward produces audio nobody asked to hear, so it is gated. A pause
                // produces none at all and `fill` pads a dry ring with silence, so there is
                // nothing to gate: what is left simply runs out. Muting on a pause silenced
                // the insert as well, which is mixed into this ring while the core is held
                // still and does not come from the core at all.
                ring.set_muted(speed == Speed::Fast);
                // A held core feeds it nothing, so the device reading silence out of it is
                // the arrangement working rather than a starve worth reporting.
                ring.set_idle(speed == Speed::Paused);
                muted_at = speed;
            }
            let input = ButtonMask(self.shared.input.load(Ordering::Relaxed));
            let rewinding = speed != Speed::Paused && self.shared.rewind.load(Ordering::Relaxed);
            let steps = match speed {
                Speed::Paused => 0,
                Speed::Normal => 1,
                Speed::Fast => FAST_STEPS,
            };
            if rewinding {
                if let Some(state) = rewind.pop() {
                    if let Err(e) = core.unserialize(&state) {
                        eprintln!("slot: rewind: {e}");
                    }
                    // A core is not obliged to repaint from a load, so the frame the user
                    // sees comes from running one. The pop before it is two frames back, so
                    // this still nets one frame of travel backwards per present.
                    core.run_frame(input);
                    self.publish(core.video_xrgb8888());
                }
                self.shared
                    .rewind_fill
                    .store(rewind.fill(), Ordering::Relaxed);
                // Reverse audio is noise, and the sink runs itself dry into silence.
                let _ = core.take_audio();
            } else if steps > 0 {
                for _ in 0..steps {
                    core.run_frame(input);
                }
                self.publish(core.video_xrgb8888());

                let audio = core.take_audio();
                // Fast forward drops the core's audio outright. Resampling accelerated
                // playback down to real time would be pitch shifted noise nobody wants to
                // hear.
                if speed == Speed::Normal {
                    since_snapshot += 1;
                    if since_snapshot >= SNAPSHOT_EVERY {
                        since_snapshot = 0;
                        // A core that will not serialize has already said so through the
                        // save path. Rewind is not the place to say it again at 30 Hz.
                        if let Ok(state) = core.serialize() {
                            rewind.push(&state);
                            self.shared
                                .rewind_fill
                                .store(rewind.fill(), Ordering::Relaxed);
                        }
                    }
                    let target = drc_target(ring.capacity_frames());
                    let queued = ring.queued_frames();
                    resampler.set_ratio(drc_ratio(queued, target));
                    resampler.process(&audio, &mut out);
                    crate::audio::volume::apply(
                        &mut out,
                        self.shared.volume.load(Ordering::Relaxed),
                    );
                    ring.push_blocking(&out);
                    // Where occupancy actually sits against target is the one thing a
                    // crackle complaint needs and no test can watch on real hardware.
                    paced += 1;
                    if crate::session::trace() && paced.is_multiple_of(TRACE_EVERY) {
                        let (dropped, starved) = (ring.overruns(), ring.underruns());
                        eprintln!(
                            "slot: audio: {queued}/{target} queued, {dropped} dropped, {starved} starved"
                        );
                    }
                }
            }

            // The write above holds this thread whenever the device has no room, which is
            // the backstop. This is the pacing the rest of the time, and the only pacing at
            // all with no audio to pace against: paused, fast forwarding, rewinding.
            deadline += PRESENT;
            let now = Instant::now();
            match deadline.checked_duration_since(now) {
                Some(wait) => std::thread::sleep(wait),
                // Falling behind by more than a frame means a stall, not a slow frame.
                // Catching up would sprint through frames nobody sees.
                None => deadline = now,
            }
        }
        // The ring belongs to the session, so a cart that left while fast forwarding would
        // otherwise take every sound after it with it.
        ring.set_muted(false);
        ring.set_idle(false);
        let (dropped, starved) = (ring.overruns(), ring.underruns());
        if dropped > 0 || starved > 0 || crate::session::trace() {
            eprintln!("slot: audio: {dropped} samples dropped, {starved} starved");
        }
    }

    fn publish(&self, video: &[u8]) {
        let mut buf = self.frames.take_write();
        buf.clear();
        buf.extend_from_slice(video);
        self.frames.publish(buf);
        self.shared.published.fetch_add(1, Ordering::Relaxed);
    }

    fn speed(&self) -> Speed {
        Speed::from_u8(self.shared.speed.load(Ordering::Relaxed))
    }
}

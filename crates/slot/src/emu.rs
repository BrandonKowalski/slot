use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use slot_retro::{
    ButtonMask, Link, LinkChannel, RetroCore, Rumble, GBA_H, GBA_W, NETPACKET_RELIABLE,
};

use crate::audio::Ring;
use crate::drc::{drc_ratio, drc_target};
use crate::frames::{FrameRef, Frames};
use crate::persist::Snapshot;
use crate::resample::Resampler;
use crate::rewind::{RewindThread, REWIND_BYTES};

/// Present is locked to the 60 Hz panel and the core is stepped once per present, so the
/// 0.456% the GBA runs slow lands entirely on audio rate control.
const PRESENT: Duration = Duration::from_nanos(16_666_667);

/// The speed a card that never chose one gets, and what the quick menu's 4× asks for. The menu
/// picks 2, 3, this, 6 or 8, through `EmuHandle::set_fast_steps`.
pub const FAST_STEPS: u32 = 4;

/// The top of the Fast Forward row, and so the most core frames one present will ever run.
///
/// A ceiling is not a multiplier. It is the most a present may run, never what it must: the
/// budget below stops a present that cannot afford the whole of it, so a game too heavy for the
/// speed asked gives that speed back a frame at a time instead of overrunning the present and
/// dropping off 60 Hz. That is the whole reason a number this high can sit on the row at all.
///
/// Eight, from measurement on the device (`.superpowers/flags/results.md`). At eight frames a
/// present the light gpSP content finishes well inside the 13.5 ms budget and runs a genuinely
/// constant 8× — Apotris 0.76 ms a frame, 6.1 ms of work; Recharged Yellow 0.84 ms, 6.7 ms —
/// while heavier mGBA content backs off on its own: Metroid Fusion at 2.05 ms a frame settles
/// near six, Drill Dozer at 5.65 ms near two. Across those four the speed swings from 2 to 6,
/// which is the point of the number. Light content can sustain 24 to 28 frames a present and
/// that is deliberately left on the table: a speed that holds still is worth more than the
/// highest one the hardware can reach.
///
/// It also stays well under the 30 consecutive skips both cores force a render after
/// (`RETRO_FRAMESKIP_MAX` in mGBA, `FRAMESKIP_MAX` in gpSP), which would draw a picture
/// mid-present that nothing goes on to show: a present of eight frames skips seven in a row,
/// because its last frame always draws and resets their counters.
pub const FAST_STEPS_MAX: u32 = 8;

/// How much of a present a fast forward may spend inside the core.
///
/// The spike held a steady 60 Hz with about 15 ms of each present used in total, still reaching
/// the deadline sleep every frame. What is left of the present after the core is the measured
/// fixed cost of one: 0.44 ms converting the one frame that is shown, a snapshot every second
/// present (0.57 ms on gpSP, 2.05 on mGBA), and under 0.1 ms of publish, audio and link pump.
/// Reserving the worst of that off 15 ms leaves this for core frames.
const FAST_BUDGET: Duration = Duration::from_micros(13_500);

/// How much of the running per-frame estimate one present's measurement replaces: a quarter.
/// Slow enough that one descheduled present does not collapse the next one to a single frame,
/// quick enough to follow a game walking from a menu into a busy scene within a few presents.
const COST_BLEND: u32 = 4;

/// Snapshot every other frame, so rewinding at one pop per present runs back at 2x.
///
/// Not raiseable to 1 without a fight: measured on the H700 a snapshot is 9.2 ms of the
/// 16.67 ms frame — serialize 6.6, compress 2.6 — so every frame would spend most of the
/// budget before the core has run at all. The Mac does the same work in 0.33 ms, which is
/// why this has to be measured on the device and not the desk.
const SNAPSHOT_EVERY: u32 = 2;

/// Frames between traced pacing lines, about five seconds.
const TRACE_EVERY: u64 = 300;

/// A per-present cap on how many packets the worker will move from the transport into the
/// core's inbound queue. Real GBA serial hardware never comes close to this in a present's
/// worth of traffic; it exists for a peer that floods, so one present's worth of a flood
/// costs one present's worth of work — the transport's `try_recv` is a queue poll, not a
/// syscall, so this is cheap insurance rather than a real constraint on anything legitimate.
const MAX_LINK_PACKETS_PER_PRESENT: u32 = 256;

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
    /// The core's end of its own serial traffic, cloned off the core exactly once — see
    /// `spawn` for why calling `RetroCore::net` a second time would not do, for a core (the
    /// mock) whose default hands back a fresh, unrelated queue every time it is asked.
    link: Link,
}

enum Cmd {
    Load(Vec<u8>),
    Save(Sender<Vec<u8>>),
    Sav(Sender<Option<Vec<u8>>>),
    Thumb(Sender<Option<Vec<u8>>>),
    /// Wires a transport to the core's serial traffic. `client_id` is libretro's own: 0 the
    /// host, 1 the joiner.
    BeginLink(u16, Box<dyn LinkChannel>),
    /// Drops the transport — which is what actually closes the wire, see `TcpLink`'s `Drop`
    /// — and marks the session no longer active.
    EndLink,
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
    /// The most core frames a fast forward present may run, 1 to `FAST_STEPS_MAX`. A ceiling
    /// rather than a count: the worker runs as many frames as the present's budget affords, up
    /// to this. Read by the worker every present, so a change lands on the next one.
    fast_steps: AtomicU32,
    /// Whether fast forward is heard, sped up, rather than dropped.
    ff_sound: AtomicBool,
    /// Frames this core has published. Counted rather than peeked because `Frames::latest`
    /// consumes: anything that asks the buffer a question steals a frame from the renderer.
    published: AtomicU64,
    /// Set once, at open, if the core refused the resume state it was handed. A core running
    /// with an `unserialize` it rejected is not resuming the player's session — it is
    /// wherever `load` left it, most often frame zero — so its own `serialize()` is not the
    /// player's progress and must not be allowed to overwrite the resume file that state was
    /// refused instead of replacing. See `EmuSnapshot::resume_trusted`.
    resume_refused: AtomicBool,
    /// The save-ram twin of `resume_refused`, and deliberately a separate flag rather than
    /// one shared bit: a core can accept one and refuse the other (save ram is a fixed-size
    /// cartridge byte count that can coincidentally match across two unrelated cores; a
    /// serialized machine state almost never does), so only the region actually refused may
    /// be withheld. See `EmuSnapshot::save_ram_trusted`.
    sav_refused: AtomicBool,
    /// The transport's far end went away during a session. Cleared when a session begins or
    /// ends. See `EmuHandle::link_lost`.
    link_lost: AtomicBool,
    /// The transport's far end said it was ending the session, rather than merely going away.
    /// Cleared when a session begins or ends, exactly like `link_lost` — and deliberately a
    /// flag of its own, because a peer that says goodbye and then drops its wire sets both,
    /// and only this one can tell the screen which of the two actually happened. See
    /// `EmuHandle::peer_ended`.
    peer_ended: AtomicBool,
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
        // reach it. `net()` exactly once, for the same reason `rumble()` is: `RetroCore`'s
        // default hands back a fresh, disconnected queue on every call (there is nothing to
        // persist for a core with no serial traffic of its own), so calling it again inside
        // the worker to get "the same" link would not be the same link at all for the mock.
        let rumble = core.rumble();
        let link = core.net();
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
            fast_steps: AtomicU32::new(FAST_STEPS),
            ff_sound: AtomicBool::new(false),
            published: AtomicU64::new(0),
            resume_refused: AtomicBool::new(false),
            sav_refused: AtomicBool::new(false),
            link_lost: AtomicBool::new(false),
            peer_ended: AtomicBool::new(false),
        });
        let (tx, rx) = channel();
        let worker = Worker {
            frames: frames.clone(),
            shared: shared.clone(),
            cmds: rx,
        };
        // A clone rather than the value itself: the worker needs its own handle to pump every
        // frame, and this side keeps one so `EmuHandle::net` can hand it out too.
        let worker_link = link.clone();
        let join = std::thread::Builder::new()
            .name("slot-emu".into())
            .spawn(move || worker.run(core, rom, ring, sav, resume, worker_link))
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
            link,
        }
    }

    /// The core's end of the motor, written from the emulator thread and read from the
    /// render one. Keeping the device write on this side is the whole reason it is a cell.
    pub fn rumble(&self) -> &Rumble {
        &self.rumble
    }

    /// The core's end of its own serial traffic, the same shape `rumble` above is. Exists for
    /// whoever ends up showing a link indicator, and is what a test pushes a packet onto or
    /// reads one off to prove the worker's own pump moved it — see `crates/slot/tests/emu.rs`.
    pub fn net(&self) -> &Link {
        &self.link
    }

    /// The transport's far end went away during a session. Cleared when a session begins or
    /// ends.
    pub fn link_lost(&self) -> bool {
        self.shared.link_lost.load(Ordering::Relaxed)
    }

    /// The transport's far end said it was ending the session, rather than merely vanishing.
    /// Cleared when a session begins or ends.
    ///
    /// Read ahead of `link_lost` by whoever acts on either (`Session::update`), because the
    /// peer that sends this drops its wire immediately behind it: both flags are up within a
    /// frame of each other, and only the order they are asked in decides whether the player is
    /// told the link was ended or that it broke.
    pub fn peer_ended(&self) -> bool {
        self.shared.peer_ended.load(Ordering::Relaxed)
    }

    /// Wires a transport into the core's serial traffic, on the emulator thread — the only
    /// place a call into a libretro core is ever allowed to happen. `client_id` is libretro's
    /// own: 0 the host, 1 the joiner, the only two this product has.
    pub fn begin_link(&self, client_id: u16, transport: Box<dyn LinkChannel>) {
        let _ = self.cmds.send(Cmd::BeginLink(client_id, transport));
    }

    /// Drops the transport and marks the session no longer active. Safe to call whether or
    /// not one was ever begun — the peer vanishing and this end asking to stop are the same
    /// request as far as the worker is concerned.
    pub fn end_link(&self) {
        let _ = self.cmds.send(Cmd::EndLink);
    }

    pub fn set_input(&self, mask: ButtonMask) {
        self.shared.input.store(mask.0, Ordering::Relaxed);
    }

    /// What the worker will read on its next pass. The far side of the one boundary a
    /// button crosses to become the game's, and the only place a test can ask whether a
    /// press a menu was using reached the core anyway.
    pub fn input(&self) -> ButtonMask {
        ButtonMask(self.shared.input.load(Ordering::Relaxed))
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

    /// The most core frames a fast forward present may run: one of the quick menu's five
    /// ceilings. Never none, which is a pause, and never more than `FAST_STEPS_MAX`, which is
    /// the top of that row — the clamp is there so a number from anywhere else cannot ask the
    /// worker for a present it was never measured to finish.
    pub fn set_fast_steps(&self, steps: u32) {
        self.shared
            .fast_steps
            .store(steps.clamp(1, FAST_STEPS_MAX), Ordering::Relaxed);
    }

    /// The ceiling the worker will hold its next fast forward present to.
    pub fn fast_steps(&self) -> u32 {
        self.shared.fast_steps.load(Ordering::Relaxed)
    }

    /// Whether fast forward is heard, squeezed into real time by the resampler, rather than
    /// dropped. Rewind is silent either way.
    pub fn set_ff_sound(&self, on: bool) {
        self.shared.ff_sound.store(on, Ordering::Relaxed);
    }

    pub fn ff_sound(&self) -> bool {
        self.shared.ff_sound.load(Ordering::Relaxed)
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
            shared: self.shared.clone(),
        }
    }
}

/// The flush paths need the core's bytes, not its thread or its frames. Cloning the
/// command sender is most of that; `shared` rides along too, because the two flags on it are
/// how a flush path learns a region it is about to ask for was never the player's to begin
/// with — see `resume_trusted`/`save_ram_trusted` below.
#[derive(Clone)]
pub struct EmuSnapshot {
    cmds: Sender<Cmd>,
    shared: Arc<Shared>,
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

    /// `false` exactly when `Worker::run` handed this core a resume it went on to refuse.
    /// `state()` above still answers with whatever the core serializes regardless — a running
    /// core always has *some* state — so a flush path must check this before it is allowed to
    /// treat those bytes as the player's session and write them over the resume file.
    fn resume_trusted(&self) -> bool {
        !self.shared.resume_refused.load(Ordering::Acquire)
    }

    /// The save-ram twin of `resume_trusted`.
    fn save_ram_trusted(&self) -> bool {
        !self.shared.sav_refused.load(Ordering::Acquire)
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
        link: Link,
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
                // The core is about to run with its own idea of save ram rather than the
                // player's — most often a mock's or a mismatched core's own default — so
                // `save_ram()` from here on must never be allowed to overwrite the real file
                // that refusal left untouched. `EmuSnapshot::save_ram_trusted` is what a
                // flush path checks before it will.
                self.shared.sav_refused.store(true, Ordering::Release);
            }
        }
        // Before Ready, so the reveal shows where the cart left off rather than a frame of
        // the intro. A state the core will not take leaves the save ram loaded above, which
        // costs the player their position but not their progress.
        if let Some(resume) = resume {
            if let Err(e) = core.unserialize(&resume) {
                eprintln!("slot: resume: {e}");
                // Same reasoning as `sav_refused` above, for the resume half: a core running
                // from wherever `load` left it is not resuming anything, and its `serialize()`
                // must not be allowed to overwrite the resume file that was refused instead of
                // replacing.
                self.shared.resume_refused.store(true, Ordering::Release);
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
        // What the ring was last told: muted, and idle. Neither, to begin with.
        let mut gated = (false, false);
        let rewind = RewindThread::spawn(REWIND_BYTES);
        let mut since_snapshot = 0;
        // What one core frame has been costing, kept across presents so a fast forward present
        // can tell before it runs a frame whether there is room for another one after it.
        //
        // Seeded at a whole present, which is pessimistic on purpose: an estimate that starts
        // too low would let the very first fast present run to the ceiling before anything had
        // been measured. Normal play updates it too — one frame a present is still a
        // measurement — so by the time anyone reaches for the trigger this already holds the
        // real cost of a drawn frame on this machine, for this game.
        let mut frame_cost = PRESENT;
        let mut deadline = Instant::now();
        let mut paced = 0u64;
        // `None` until a session begins. Held here rather than on `Shared`: the transport is
        // not `Sync`-shaped state a render-thread read would make sense of, only something
        // this loop drains and feeds once a frame.
        let mut transport: Option<Box<dyn LinkChannel>> = None;
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
                    Cmd::BeginLink(client_id, t) => {
                        self.shared.link_lost.store(false, Ordering::Relaxed);
                        self.shared.peer_ended.store(false, Ordering::Relaxed);
                        core.start_link(client_id);
                        // Set here as well as by `LibretroCore::start_link` itself: this is
                        // the thing that actually knows a transport is wired and about to be
                        // pumped, whatever the concrete core does or does not do with
                        // `client_id` — the mock, in particular, has no session of its own to
                        // start and would otherwise leave `is_active` false with real traffic
                        // already flowing through it.
                        link.set_active(true);
                        transport = Some(t);
                    }
                    Cmd::EndLink => {
                        self.shared.link_lost.store(false, Ordering::Relaxed);
                        self.shared.peer_ended.store(false, Ordering::Relaxed);
                        // `Cmd::BeginLink`'s counterpart: tells the core the session is over,
                        // if it registered a `stop` to hear it through (`RetroCore::stop_link`
                        // — libretro documents `stop` as OPTIONAL, unlike `start`, so this is
                        // a no-op for a core that never offered one). Without this the core
                        // keeps believing a session is live and keeps producing packets
                        // nobody is left to carry.
                        core.stop_link();
                        // Word to the far end before the wire goes, so a deliberate ending
                        // arrives as one rather than as a peer that fell silent. This is the
                        // single seam every ending already passes through — the menu's own A,
                        // a power press, a shut lid, an eject, a critical battery — so none of
                        // them has to remember to say goodbye for itself.
                        //
                        // Ahead of the drop, and bounded inside `send_end`: the drop is what
                        // unblocks the transport's own threads, and a goodbye still queued when
                        // that happens would never reach the wire. Harmless on a wire the peer
                        // has already dropped, which is the lost-peer timeout arriving here —
                        // the write simply fails or goes nowhere.
                        if let Some(t) = transport.as_mut() {
                            t.send_end();
                        }
                        // The drop is what actually closes the wire (see `TcpLink`'s `Drop`);
                        // this is just letting go of it.
                        transport = None;
                        // Cleared *before* the flag flips, not after: `Link::clear` empties
                        // both queues (a packet that arrived a moment before this command
                        // would otherwise sit here until the *next* session begins and gets
                        // fed to a core that never sent or asked for it), and `set_active`'s
                        // `Release` store only carries a happens-before guarantee for what
                        // ran on this thread *before* it. Clearing first is what lets a
                        // reader who observes `is_active() == false` (`Acquire`) also see the
                        // queues already empty, with no sleep needed to bridge the gap.
                        link.clear();
                        link.set_active(false);
                    }
                }
            }

            // Pumped every present regardless of speed or phase, not only while the core is
            // stepping frames: a trade partner reading the local device's power menu, or
            // sitting in the switcher, must not see the link go quiet just because this
            // device paused its own picture. Neither direction may block the frame —
            // `try_recv` already never does — so this is always safe to run.
            if let Some(t) = transport.as_mut() {
                drain_transport(t.as_mut(), &link, MAX_LINK_PACKETS_PER_PRESENT);
                // Both checked after the drain, so the last packets a peer sent before leaving
                // still reach the core. Which of the two the screen acts on is decided by
                // whoever reads them (`Session::update`), not here: a peer that ends a session
                // deliberately sets this one and then, a moment later, the other.
                if t.peer_ended() {
                    self.shared.peer_ended.store(true, Ordering::Relaxed);
                }
                if t.is_closed() {
                    self.shared.link_lost.store(true, Ordering::Relaxed);
                }
            }
            core.pump_link();
            // A `poll` can make the core send, so this catches anything it just queued. The
            // send that matters is the one after the frame runs, below.
            flush_outbound(&mut transport, &link);

            let speed = self.speed();
            // Published before anything below acts on it, and with `Release`: a reader who
            // observes `Paused` from this store is thereby also guaranteed to see every frame
            // `publish` counted on an earlier pass, because that publish happened-before this
            // store in program order and `Release`/`Acquire` makes that ordering visible across
            // threads. `publish`'s own counter is `Relaxed` and leans on this pairing for it —
            // `Relaxed` here would leave that unordered, trading the scheduling race this exists
            // to close for a subtler visibility one.
            self.shared.observed.store(speed as u8, Ordering::Release);
            let ff_sound = self.shared.ff_sound.load(Ordering::Relaxed);
            // Fast forward with its sound off produces audio nobody asked to hear, so it is
            // gated; with it on, that audio is the point and the ring stays open. A pause
            // produces none at all and `fill` pads a dry ring with silence, so there is
            // nothing to gate: what is left simply runs out. Muting on a pause silenced the
            // insert as well, which is mixed into this ring while the core is held still and
            // does not come from the core at all.
            //
            // A held core feeds it nothing, so the device reading silence out of it is the
            // arrangement working rather than a starve worth reporting.
            let gate = (speed == Speed::Fast && !ff_sound, speed == Speed::Paused);
            if gate != gated {
                ring.set_muted(gate.0);
                ring.set_idle(gate.1);
                gated = gate;
            }
            let input = ButtonMask(self.shared.input.load(Ordering::Relaxed));
            let rewinding = speed != Speed::Paused && self.shared.rewind.load(Ordering::Relaxed);
            let ceiling = match speed {
                Speed::Paused => 0,
                Speed::Normal => 1,
                Speed::Fast => self.shared.fast_steps.load(Ordering::Relaxed),
            };
            if rewinding {
                if let Some(state) = rewind.pop() {
                    if let Err(e) = core.unserialize(&state) {
                        eprintln!("slot: rewind: {e}");
                    }
                    // A core is not obliged to repaint from a load, so the frame the user
                    // sees comes from running one. `pop` walks back two frames and this
                    // runs one forward, so the picture travels back two frames per present:
                    // reverse at 2x, showing every other frame.
                    //
                    // Empty input, never the live mask. The live one necessarily holds L2 —
                    // it is what is being held to rewind — so replaying with it re-simulated
                    // the frame under buttons that were not pressed at the time, and the
                    // state landed on when the trigger was released inherited the
                    // difference. Nothing was being replayed faithfully; it was being
                    // re-played.
                    //
                    // Drawn, not skipped: this frame is the picture the rewind shows. Every
                    // present already leaves the core with skipping off — the last frame of one
                    // is always a drawn frame — but saying so here keeps that a property of
                    // this branch rather than an inheritance from whatever ran before it.
                    core.set_frame_skip(false);
                    core.run_frame(ButtonMask(0));
                    self.publish(core.video_xrgb8888());
                }
                self.shared
                    .rewind_fill
                    .store(rewind.fill(), Ordering::Relaxed);
                // Reverse audio is noise, and the sink runs itself dry into silence.
                let _ = core.take_audio();
            } else if ceiling > 0 {
                // As many core frames as this present can afford, up to the ceiling, and only
                // the last of them draws a picture.
                //
                // A count rather than a multiplier is what makes a heavy game slow down
                // smoothly instead of falling off 60 Hz: the chosen speed is the most this may
                // run, not what it must, so content that cannot afford the whole ceiling gives
                // back speed a frame at a time while still presenting every 16.67 ms.
                //
                // Whether a frame is the last has to be decided *before* it runs, because that
                // is the only moment either core can still be told not to draw it — so the test
                // is predictive: there is room for another frame after this one only if the
                // present has time for both. Being wrong costs one frame of speed, never a
                // dropped present.
                let began = Instant::now();
                let mut ran = 0u32;
                loop {
                    ran += 1;
                    let last = ran >= ceiling || began.elapsed() + frame_cost * 2 > FAST_BUDGET;
                    core.set_frame_skip(!last);
                    core.run_frame(input);
                    if last {
                        break;
                    }
                }
                frame_cost = blend(frame_cost, began.elapsed() / ran);
                // Immediately, and this is the one that decides whether a link is playable.
                // The emulated serial hardware only executes inside `run_frame`, so every
                // packet a session actually produces is born here. Sending them from the top
                // of the loop instead means each one waits for the next present: a whole
                // frame, 16.7 ms, added to a wire measured at about 2 ms, in both directions
                // and on both devices. A GBA that asked a question and heard nothing for four
                // frames reports a communication error, which is what it should do.
                flush_outbound(&mut transport, &link);
                self.publish(core.video_xrgb8888());

                // Counted per present rather than per frame, so a fast forward pays the
                // same snapshot cost per present as normal play and simply records a
                // coarser trail that follows the speed actually reached: twice however many
                // frames a present ran, rather than two.
                //
                // This used to sit inside the `Normal` arm below, which exists to gate the
                // audio, and was swept in with it. The effect was a hole: nothing recorded
                // while fast forwarding, so the newest state was whatever predated the
                // trigger and the first pop of a rewind swallowed the entire stretch in one
                // step instead of walking back through it.
                since_snapshot += 1;
                if since_snapshot >= SNAPSHOT_EVERY {
                    since_snapshot = 0;
                    // A core that will not serialize has already said so through the save
                    // path. Rewind is not the place to say it again at 30 Hz.
                    if let Ok(state) = core.serialize() {
                        rewind.push(state);
                        self.shared
                            .rewind_fill
                            .store(rewind.fill(), Ordering::Relaxed);
                    }
                }

                let audio = core.take_audio();
                // Fast forward drops the core's audio unless its sound is on. On, the several
                // frames of audio a fast present produced are squeezed into one present's
                // worth by stepping through them that many times as fast: it comes out faster
                // and higher, at the device's own pace rather than backing the ring up.
                if speed == Speed::Normal || ff_sound {
                    let target = drc_target(ring.capacity_frames());
                    let queued = ring.queued_frames();
                    // `ran`, not the ceiling: the audio squeezed into this present is however
                    // many frames of it the present actually produced.
                    resampler.set_ratio(drc_ratio(queued, target) / f64::from(ran));
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

/// Moves up to `cap` packets from `transport` into `link`'s inbound queue, in order, and
/// leaves the rest — however many there are — queued in the transport for the next call.
/// A free function, rather than inline in `Worker::run`'s loop, so the cap can be driven
/// directly against a fake transport in a test with no worker thread and no real timing
/// involved (`MAX_LINK_PACKETS_PER_PRESENT`'s own point is to bound work in one present,
/// which a test racing a real 60 Hz loop could never pin down deterministically).
/// Everything the core has queued for its peer, onto the wire.
///
/// The flag `netpacket_send` was called with never reaches this queue — only the bytes do —
/// so this asks every transport for reliable delivery. Safe for `TcpLink`, which is reliable
/// regardless of what is asked: TCP cannot honour "unreliable" any other way, and
/// `LinkChannel::send`'s own contract is to fall back to reliable when a flag cannot be
/// honoured.
fn flush_outbound(transport: &mut Option<Box<dyn LinkChannel>>, link: &Link) {
    let Some(t) = transport.as_deref_mut() else {
        return;
    };
    while let Some(packet) = link.take_outbound() {
        t.send(NETPACKET_RELIABLE, &packet);
    }
}

/// Folds one present's measured per-frame cost into the running estimate, `COST_BLEND` being
/// how much of the old value the new measurement replaces.
fn blend(estimate: Duration, measured: Duration) -> Duration {
    (estimate * (COST_BLEND - 1) + measured) / COST_BLEND
}

fn drain_transport(transport: &mut dyn LinkChannel, link: &Link, cap: u32) {
    for _ in 0..cap {
        let Some(packet) = transport.try_recv() else {
            break;
        };
        link.push_inbound(packet);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slot_retro::LoopbackLink;

    /// I6: an unbounded drain here gives a flooding peer unbounded work in a single present.
    /// `LoopbackLink` holds everything sent to it in a plain queue, so filling it past the
    /// cap and draining once is enough to prove the cap actually holds — no thread, no
    /// timing, no worker loop needed.
    #[test]
    fn drain_transport_stops_at_the_cap_and_leaves_the_rest_queued() {
        let mut transport = LoopbackLink::default();
        for i in 0..10u8 {
            transport.send(0, &[i]);
        }
        let link = Link::default();

        drain_transport(&mut transport, &link, 4);

        let mut got = Vec::new();
        while let Some(p) = link.take_inbound() {
            got.push(p[0]);
        }
        assert_eq!(
            got,
            vec![0, 1, 2, 3],
            "the cap must stop the drain, not just slow it"
        );
        assert_eq!(
            transport.try_recv(),
            Some(vec![4]),
            "packets past the cap must stay queued in the transport, not be dropped"
        );
    }

    /// The ordinary case: a present's worth of traffic never comes close to the cap, so
    /// everything waiting moves in one call, same as an unbounded drain would.
    #[test]
    fn drain_transport_moves_everything_under_the_cap() {
        let mut transport = LoopbackLink::default();
        transport.send(0, b"one");
        transport.send(0, b"two");
        let link = Link::default();

        drain_transport(&mut transport, &link, MAX_LINK_PACKETS_PER_PRESENT);

        assert_eq!(link.take_inbound().as_deref(), Some(&b"one"[..]));
        assert_eq!(link.take_inbound().as_deref(), Some(&b"two"[..]));
        assert_eq!(link.take_inbound(), None);
    }
}

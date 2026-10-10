mod common;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use slot::audio::{AudioSink, StubSink};
use slot::emu::{CoreState, EmuHandle, Speed};
use slot_retro::{AvInfo, ButtonMask, CoreError, LibretroCore, MockCore, RetroCore};

const PANEL: Duration = Duration::from_micros(16_760);
const PRESENTS: u64 = 30;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Call {
    Run { drawn: bool, input: u16 },
    Save(u64),
    Load(u64),
}

type Log = Arc<Mutex<Vec<Call>>>;

struct Recorder {
    inner: MockCore,
    drawn: bool,
    log: Log,
}

impl Recorder {
    fn new() -> (Box<Recorder>, Log) {
        let log = Log::default();
        let core = Box::new(Recorder {
            inner: MockCore::new(),
            drawn: true,
            log: log.clone(),
        });
        (core, log)
    }

    fn note(&self, call: Call) {
        self.log.lock().expect("the call log").push(call);
    }
}

impl RetroCore for Recorder {
    fn load(&mut self, rom: &Path) -> Result<(), CoreError> {
        self.inner.load(rom)
    }
    fn run_frame(&mut self, input: ButtonMask) {
        self.note(Call::Run {
            drawn: self.drawn,
            input: input.0,
        });
        self.inner.run_frame(input);
    }
    fn set_frame_skip(&mut self, skip: bool) {
        self.drawn = !skip;
        self.inner.set_frame_skip(skip);
    }
    fn video_xrgb8888(&self) -> &[u8] {
        self.inner.video_xrgb8888()
    }
    fn take_audio(&mut self) -> Vec<i16> {
        self.inner.take_audio()
    }
    fn serialize(&mut self) -> Result<Vec<u8>, CoreError> {
        let state = self.inner.serialize()?;
        self.note(Call::Save(counter(&state)));
        Ok(state)
    }
    fn unserialize(&mut self, data: &[u8]) -> Result<(), CoreError> {
        self.note(Call::Load(counter(data)));
        self.inner.unserialize(data)
    }
    fn save_ram(&self) -> Option<Vec<u8>> {
        self.inner.save_ram()
    }
    fn load_save_ram(&mut self, data: &[u8]) -> Result<(), CoreError> {
        self.inner.load_save_ram(data)
    }
    fn av_info(&self) -> AvInfo {
        self.inner.av_info()
    }
}

fn counter(state: &[u8]) -> u64 {
    u64::from_le_bytes(state.try_into().expect("mock state is a u64 counter"))
}

fn wait_for(cond: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !cond() {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    true
}

fn spawn(core: Box<dyn RetroCore>, rom: PathBuf) -> EmuHandle {
    let mut sink = StubSink::new();
    sink.open(32_768).expect("the stub refused to open");
    let drain = sink.clone();
    std::thread::spawn(move || loop {
        drain.device_drain();
        std::thread::sleep(Duration::from_millis(2));
    });
    let emu = EmuHandle::spawn(core, rom, sink.ring(), None, None);
    assert!(
        wait_for(|| emu.state() != CoreState::Loading),
        "the core never finished loading"
    );
    assert_eq!(emu.state(), CoreState::Ready);
    emu
}

fn driven(core: Box<dyn RetroCore>, rom: PathBuf, ahead: u8) -> EmuHandle {
    let emu = spawn(core, rom);
    emu.set_runahead(ahead);
    emu.set_driven(true);
    emu.set_speed(Speed::Normal);
    assert!(
        wait_for(|| emu.locked()),
        "the worker never locked to the display"
    );
    emu
}

fn present(emu: &EmuHandle) -> Vec<u8> {
    emu.tick(PANEL);
    assert!(
        emu.wait_frame(Duration::from_secs(2)),
        "a tick's frame never arrived"
    );
    emu.latest_frame()
        .expect("the tick published nothing")
        .to_vec()
}

fn mock_frame_number(picture: &[u8]) -> u64 {
    u64::from(picture[2])
}

fn rom_frame_number(picture: &[u8]) -> u64 {
    let channel = |at: usize| u64::from(picture[at] >> 3);
    channel(2) | channel(1) << 5 | channel(0) << 10
}

fn check_the_picture_runs_ahead_of_the_game(
    emu: &EmuHandle,
    ahead: u8,
    frame_number: fn(&[u8]) -> u64,
) {
    let mut last = None;
    for n in 0..3 * PRESENTS {
        let now = match (PRESENTS..2 * PRESENTS).contains(&n) {
            true => ahead,
            false => 0,
        };
        emu.set_runahead(now);
        let shown = frame_number(&present(emu));
        let published = emu.published_count();
        if let Some((was, at, before)) = last.filter(|_| n > 1) {
            assert_eq!(
                shown + u64::from(before),
                was + (published - at) + u64::from(now),
                "going from run-ahead {before} to {now}, present {n} showed frame {shown} after \
                 {was}: the game did not move one frame a present, or the picture is not \
                 {now} ahead of it"
            );
        }
        last = Some((shown, published, now));
    }
}

#[test]
fn run_ahead_shows_frames_early_and_never_moves_the_game_on_faster() {
    for ahead in 0..=2u8 {
        let emu = driven(Box::new(MockCore::new()), PathBuf::from("mock"), 0);
        check_the_picture_runs_ahead_of_the_game(&emu, ahead, mock_frame_number);
    }
}

#[test]
fn on_mgba_run_ahead_shows_frames_early_and_never_moves_the_game_on_faster() {
    let _g = common::core_lock();
    let Some(dylib) = common::vendored_core() else {
        eprintln!("no vendored mGBA core on this host, skipping");
        return;
    };
    let rom = Path::new(env!("CARGO_TARGET_TMPDIR")).join("runahead.gba");
    std::fs::write(&rom, common::gba_rom()).expect("write rom");
    for ahead in 0..=2u8 {
        let core = LibretroCore::open(&dylib).expect("vendored core is present but would not open");
        let emu = driven(Box::new(core), rom.clone(), 0);
        check_the_picture_runs_ahead_of_the_game(&emu, ahead, rom_frame_number);
    }
}

#[test]
fn every_frame_of_a_present_gets_the_same_buttons_and_turbo_keeps_its_beat() {
    let (core, log) = Recorder::new();
    let emu = driven(core, PathBuf::from("mock"), 2);
    let held = ButtonMask(ButtonMask::X);
    emu.set_input(held);
    log.lock().expect("the call log").clear();
    for _ in 0..12 {
        present(&emu);
    }
    let runs: Vec<(bool, u16)> = log
        .lock()
        .expect("the call log")
        .iter()
        .filter_map(|c| match *c {
            Call::Run { drawn, input } => Some((drawn, input)),
            _ => None,
        })
        .collect();
    assert!(
        runs.len() >= 36 && runs.len().is_multiple_of(3),
        "{} frames over 12 presents at run-ahead 2, not 3 a present",
        runs.len()
    );
    let mut beat = Vec::new();
    for frames in runs.chunks(3) {
        assert_eq!(
            frames.iter().map(|r| r.0).collect::<Vec<_>>(),
            [false, false, true],
            "a present drew a frame other than its last"
        );
        assert!(
            frames.iter().all(|r| r.1 == frames[0].1),
            "one present's frames saw different buttons: {frames:?}"
        );
        beat.push(frames[0].1);
    }
    assert!(
        (0..6).any(|t| beat
            .iter()
            .zip(t..)
            .all(|(&mask, f)| ButtonMask(mask) == held.turbo(f))),
        "turbo no longer flips once every 3 presents: {beat:?}"
    );
}

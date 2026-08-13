#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use slot::app::App;
use slot::persist::Snapshot;
use slot::session::Session;
use slot_power::{
    Battery, Charge, LedState, Motor, Platform, Power, SimPlatform, SleepDepth, WakeReason,
};
use slot_retro::{ButtonMask, MockCore, RetroCore};
use slot_store::{write_slot_state, SlotState};
use tempfile::TempDir;

/// What the emulator was last told to load. `None` until something loads.
pub type Loaded = Arc<Mutex<Option<Vec<u8>>>>;

pub fn tmp_root_with_carts(stems: &[&str]) -> TempDir {
    let d = tmp_root();
    for stem in stems {
        let mut rom = vec![0u8; 0x100];
        let title = stem.to_uppercase();
        rom[0xa0..0xa0 + title.len()].copy_from_slice(title.as_bytes());
        std::fs::write(rom_path(&d, stem), rom).expect("write rom");
    }
    d
}

/// The headers `tmp_root_with_carts` writes are not roms, and a real core refuses them.
/// Anything that puts a cart in the slot for real needs these instead.
pub fn tmp_root_with_real_carts(stems: &[&str]) -> TempDir {
    let d = tmp_root();
    for stem in stems {
        std::fs::write(rom_path(&d, stem), gba_rom()).expect("write rom");
    }
    d
}

fn tmp_root() -> TempDir {
    let d = tempfile::tempdir().expect("tempdir");
    for sub in slot::root::DIRS {
        std::fs::create_dir(d.path().join(sub)).expect("create content dir");
    }
    d
}

fn rom_path(d: &TempDir, stem: &str) -> PathBuf {
    d.path().join("Games").join(format!("{stem}.gba"))
}

/// Tests do not run from the workspace root, so anything reaching a file that is checked in
/// has to get there from the crate.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The core `scripts/fetch-core.sh` pulls down. `None` when it has not been run, which is
/// the case every test that names it has to skip or fall back around.
pub fn vendored_core() -> Option<PathBuf> {
    let p = repo_root().join(format!(
        "vendor/mgba_libretro.{}",
        std::env::consts::DLL_EXTENSION
    ));
    p.exists().then_some(p)
}

/// Sets mode 3 and writes a frame counter into the first pixel once per vblank, so
/// consecutive frames differ and a savestate has both registers and VRAM worth restoring.
pub fn gba_rom() -> Vec<u8> {
    const CODE: [u32; 15] = [
        0xe3a00404, // mov  r0, #0x04000000
        0xe3a01c04, // mov  r1, #0x400
        0xe3811003, // orr  r1, r1, #3
        0xe5801000, // str  r1, [r0]          DISPCNT: mode 3, BG2 on
        0xe3a02406, // mov  r2, #0x06000000
        0xe3a03000, // mov  r3, #0
        0xe1d040b6, // vb:  ldrh r4, [r0, #6] VCOUNT
        0xe35400a0, //      cmp  r4, #160
        0x1afffffc, //      bne  vb
        0xe2833001, //      add  r3, r3, #1
        0xe1c230b0, //      strh r3, [r2]
        0xe1d040b6, // dr:  ldrh r4, [r0, #6]
        0xe35400a0, //      cmp  r4, #160
        0x0afffffc, //      beq  dr
        0xeafffff6, //      b    vb
    ];
    let mut rom = vec![0u8; 0x8000];
    rom[0..4].copy_from_slice(&0xea00002eu32.to_le_bytes()); // b 0xc0
    rom[0xa0..0xac].copy_from_slice(b"SLOT TEST\0\0\0");
    rom[0xac..0xb0].copy_from_slice(b"SLTE");
    rom[0xb0..0xb2].copy_from_slice(b"00");
    rom[0xb2] = 0x96; // fixed header byte, cores sniff it to identify a GBA rom
    let sum = rom[0xa0..0xbd].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    rom[0xbd] = 0u8.wrapping_sub(sum).wrapping_sub(0x19);
    for (i, w) in CODE.iter().enumerate() {
        let o = 0xc0 + i * 4;
        rom[o..o + 4].copy_from_slice(&w.to_le_bytes());
    }
    rom
}

/// Stands in for the emulator worker at a flush point.
pub struct StubSnapshot {
    pub state: Vec<u8>,
    pub sav: Option<Vec<u8>>,
    pub thumb: Option<Vec<u8>>,
    pub loaded: Loaded,
}

impl StubSnapshot {
    pub fn boxed() -> Box<dyn Snapshot> {
        StubSnapshot::pair().0
    }

    pub fn pair() -> (Box<dyn Snapshot>, Loaded) {
        let loaded = Loaded::default();
        let stub = StubSnapshot {
            state: vec![9u8; 1024],
            sav: None,
            thumb: Some(b"png".to_vec()),
            loaded: loaded.clone(),
        };
        (Box::new(stub), loaded)
    }
}

impl Snapshot for StubSnapshot {
    fn state(&self) -> Option<Vec<u8>> {
        Some(self.state.clone())
    }

    fn save_ram(&self) -> Option<Vec<u8>> {
        self.sav.clone()
    }

    fn thumb(&self) -> Option<Vec<u8>> {
        self.thumb.clone()
    }

    fn load(&self, state: Vec<u8>) {
        *self.loaded.lock().expect("loaded") = Some(state);
    }
}

/// A snapshot backed by a real core rather than by fixed bytes. Undoing a load is only
/// meaningful against something that can be moved and read back, which the `App` itself
/// cannot be asked for: it holds a `Snapshot`, never a core.
#[derive(Clone, Default)]
pub struct CoreSnapshot(Arc<Mutex<MockCore>>);

impl CoreSnapshot {
    pub fn new() -> Self {
        let core = CoreSnapshot::default();
        core.with(|c| c.load(Path::new("unused")).expect("load"));
        core
    }

    pub fn boxed(&self) -> Box<dyn Snapshot> {
        Box::new(self.clone())
    }

    pub fn run_frame(&self) {
        self.with(|c| c.run_frame(ButtonMask::default()));
    }

    /// Where the core actually is, as opposed to what the app last asked it for.
    pub fn bytes(&self) -> Vec<u8> {
        self.with(|c| c.serialize().expect("serialize"))
    }

    fn with<T>(&self, f: impl FnOnce(&mut MockCore) -> T) -> T {
        f(&mut self.0.lock().expect("core"))
    }
}

impl Snapshot for CoreSnapshot {
    fn state(&self) -> Option<Vec<u8>> {
        Some(self.bytes())
    }

    fn save_ram(&self) -> Option<Vec<u8>> {
        None
    }

    fn thumb(&self) -> Option<Vec<u8>> {
        Some(b"png".to_vec())
    }

    fn load(&self, state: Vec<u8>) {
        self.with(|c| c.unserialize(&state).expect("unserialize"));
    }
}

/// The stub device's hardware clock. It does not run: nothing in a test waits on a second
/// passing, and a clock that moved would make every reading of it a race.
#[derive(Clone, Default)]
pub struct Clock(Arc<AtomicI64>);

impl Clock {
    pub fn at(secs: i64) -> Self {
        Clock(Arc::new(AtomicI64::new(secs)))
    }

    pub fn get(&self) -> i64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// Stands in for the device the power path acts on.
pub struct StubPlatform {
    backlight: Arc<AtomicU8>,
    root: PathBuf,
    clock: Clock,
    /// 0 = Unknown, 1 = Discharging, 2 = Charging, 3 = Full. Shared so a test can move the
    /// cable mid-run instead of having to rebuild the platform to change it.
    charge: Arc<AtomicU8>,
    /// The gauge reading `battery()` hands back. Shared for the same reason `charge` is: a
    /// test that wants to tell "the fast tick touched only the charge half" apart from "the
    /// fast tick re-read the whole snapshot" has to be able to move this without the percent
    /// moving with it.
    percent: Arc<AtomicU8>,
    /// What `set_led` last wrote, coded by `led_code`. Shared so a test can watch what the
    /// fast tick actually sent the platform rather than only what `App::led_state` computes
    /// in isolation — that gap is what let the write itself be deleted with every test still
    /// green.
    led: Arc<AtomicU8>,
    /// How many times `set_led` was called. `led` alone cannot catch a write that repeats the
    /// same state every tick forever: the value does not move, only the count would.
    led_writes: Arc<AtomicUsize>,
}

/// `LedState` has no `Copy`-friendly integer form of its own — it is deliberately opaque to
/// everything on the far side of `Platform` — so the stub needs its own coding to carry a
/// reading through an `AtomicU8` the same way `charge` already does. Public so a test can
/// encode the state it expects `led` to hold, the same way it already compares against raw
/// `charge`/`percent` values.
pub fn led_code(state: LedState) -> u8 {
    match state {
        LedState::Off => 0,
        LedState::Running => 1,
        LedState::Low => 2,
        LedState::Charging => 3,
        LedState::Charged => 4,
    }
}

/// The power object and the panel behind it, so a test can see what the app lit.
/// A clock that reads like a real date. Not zero: a platform whose clock is at the epoch is
/// one whose RTC never came up, and `set_power` sends that to the clock screen — where the
/// levels are deliberately unreachable, which is not what a backlight test is asking about.
pub const CLOCK_IS_SET: i64 = 1_786_568_000;

pub fn panel(root: &Path, timeout: Duration) -> (Power, Arc<AtomicU8>) {
    let (power, backlight, _, _, _) = rig_with_charge(root, timeout, CLOCK_IS_SET, 0, 50);
    (power, backlight)
}

fn rig(root: &Path, timeout: Duration, secs: i64) -> (Power, Arc<AtomicU8>, Clock) {
    let (power, backlight, clock, _, _) = rig_with_charge(root, timeout, secs, 0, 50);
    (power, backlight, clock)
}

/// The same rig, with the charge state and the percent seeded rather than left at their
/// defaults, and both handed back so a test can move them independently. Kept apart from
/// `rig` so every caller that does not care about either keeps its three-value return
/// unchanged.
fn rig_with_charge(
    root: &Path,
    timeout: Duration,
    secs: i64,
    charge: u8,
    percent: u8,
) -> (Power, Arc<AtomicU8>, Clock, Arc<AtomicU8>, Arc<AtomicU8>) {
    let (power, backlight, clock, charge, percent, _led, _led_writes) =
        rig_with_led(root, timeout, secs, charge, percent);
    (power, backlight, clock, charge, percent)
}

/// The same rig again, with the platform's own record of the LED added to what a test can
/// move or watch independently. Kept apart from `rig_with_charge` for the same reason that
/// one is kept apart from `rig`: every caller that does not care leaves its return shape
/// alone.
#[allow(clippy::type_complexity)]
fn rig_with_led(
    root: &Path,
    timeout: Duration,
    secs: i64,
    charge: u8,
    percent: u8,
) -> (
    Power,
    Arc<AtomicU8>,
    Clock,
    Arc<AtomicU8>,
    Arc<AtomicU8>,
    Arc<AtomicU8>,
    Arc<AtomicUsize>,
) {
    let backlight = Arc::new(AtomicU8::new(0));
    let clock = Clock::at(secs);
    let charge = Arc::new(AtomicU8::new(charge));
    let percent = Arc::new(AtomicU8::new(percent));
    // u8::MAX codes as nothing `led_code` ever produces, so a test can tell "never written"
    // apart from a real `LedState::Off` (which codes as 0).
    let led = Arc::new(AtomicU8::new(u8::MAX));
    let led_writes = Arc::new(AtomicUsize::new(0));
    let platform = StubPlatform {
        backlight: backlight.clone(),
        root: root.to_path_buf(),
        clock: clock.clone(),
        charge: charge.clone(),
        percent: percent.clone(),
        led: led.clone(),
        led_writes: led_writes.clone(),
    };
    (
        Power::new(Box::new(platform), SleepDepth::Doze, timeout),
        backlight,
        clock,
        charge,
        percent,
        led,
        led_writes,
    )
}

/// A whole session over the host platform, which is the one implementation that records
/// what the motor was last set to. `StubPlatform` has no motor: nothing else asks it.
pub fn session_with_platform(root: &Path) -> (Session, Motor) {
    clocked(root);
    let platform = SimPlatform::at(root.to_path_buf());
    let motor = platform.motor();
    let mut session = Session::boot(root.to_path_buf());
    session.app_mut().set_power(Power::new(
        Box::new(platform),
        SleepDepth::Doze,
        Duration::from_secs(300),
    ));
    (session, motor)
}

/// Booted onto the clock screen with a platform whose clock can be read back.
pub fn app_booting_with_clock(root: &Path) -> (App, Clock) {
    app_booting_at(root, 0)
}

pub fn app_booting_at(root: &Path, secs: i64) -> (App, Clock) {
    let mut a = App::boot(root);
    let (power, _, clock) = rig(root, Duration::from_secs(60), secs);
    a.set_power(power);
    (a, clock)
}

impl Platform for StubPlatform {
    fn set_backlight(&mut self, step: u8) {
        self.backlight.store(step, Ordering::Relaxed);
    }

    fn charge(&self) -> Charge {
        match self.charge.load(Ordering::Relaxed) {
            1 => Charge::Discharging,
            2 => Charge::Charging,
            3 => Charge::Full,
            _ => Charge::Unknown,
        }
    }

    fn battery(&self) -> Option<Battery> {
        Some(Battery {
            percent: self.percent.load(Ordering::Relaxed),
            charge: self.charge(),
        })
    }

    /// Recorded rather than dropped: `App::led_state` alone cannot prove the fast tick ever
    /// reaches the platform, or that it stops reaching it once the state stops moving. See
    /// `led` and `led_writes`.
    fn set_led(&mut self, state: LedState) {
        self.led.store(led_code(state), Ordering::Relaxed);
        self.led_writes.fetch_add(1, Ordering::Relaxed);
    }

    fn sleep(&mut self, _depth: SleepDepth, _timeout: Duration) -> WakeReason {
        WakeReason::Timeout
    }

    fn poweroff(&mut self) -> ! {
        panic!("the stub platform never powers off")
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn now(&self) -> i64 {
        self.clock.get()
    }

    fn set_clock(&mut self, secs: i64) {
        self.clock.0.store(secs, Ordering::Relaxed);
    }

    /// No motor. `tests/rumble.rs` is the only test that reads one back and it runs over
    /// `SimPlatform`, which records it.
    fn set_rumble(&mut self, _strength: u16) {}
}

/// Says the clock has already been confirmed. A root with no `clock_set` stops on the clock
/// screen ahead of everything, which is only ever what `tests/clock.rs` is about.
pub fn clocked(root: &Path) {
    let mut s = slot_store::read_slot_state(root);
    s.clock_set = true;
    write_slot_state(root, &s).expect("write slot.state");
}

pub fn boot(root: &Path) -> App {
    clocked(root);
    App::boot(root)
}

/// Booted onto a seated cart and run past the insert floor, which is where every flush
/// path starts.
pub fn app_playing_in(root: &Path, stem: &str) -> App {
    app_playing_with(root, stem, StubSnapshot::boxed())
}

/// The same, wired to a platform whose charge state and percent a test can move
/// independently mid-run. Starts Discharging at 50%, ordinary readings, so a test that
/// never touches either cell still reads a state the device could have asserted rather
/// than an idle default. Two cells rather than one, so a test can move the percent behind
/// the charge tick's back and prove the fast tick never looked at it.
pub fn app_playing_with_charge(root: &Path, stem: &str) -> (App, Arc<AtomicU8>, Arc<AtomicU8>) {
    let mut a = app_playing_with(root, stem, StubSnapshot::boxed());
    let (power, _backlight, _clock, charge, percent) =
        rig_with_charge(root, Duration::from_secs(60), 0, 1, 50);
    a.set_power(power);
    (a, charge, percent)
}

/// The same, with the platform's own record of the LED added: `App::led_state` is a pure
/// function of the app's own fields and proves nothing about whether the fast tick ever
/// reaches `Platform::set_led`, or how often. `led` and `led_writes` are what let a test
/// watch the far side of that boundary instead of trusting it.
pub fn app_playing_with_led(
    root: &Path,
    stem: &str,
) -> (
    App,
    Arc<AtomicU8>,
    Arc<AtomicU8>,
    Arc<AtomicU8>,
    Arc<AtomicUsize>,
) {
    let mut a = app_playing_with(root, stem, StubSnapshot::boxed());
    let (power, _backlight, _clock, charge, percent, led, led_writes) =
        rig_with_led(root, Duration::from_secs(60), 0, 1, 50);
    a.set_power(power);
    (a, charge, percent, led, led_writes)
}

/// The same, with the state switcher open over it. The ring has to have something in it
/// already or the switcher refuses to open.
pub fn app_in_switcher(root: &Path, stem: &str) -> App {
    let mut a = app_playing_in(root, stem);
    a.apply(slot_input::Action::Polaroids);
    a
}

/// The same, with the mixer already at a stated level. Written to the card rather than
/// pressed in, since the keys only move by five and 0 and 100 are the interesting ones.
pub fn app_playing_with_volume(root: &Path, volume: u8) -> App {
    seated(
        root,
        StubSnapshot::boxed(),
        SlotState {
            cart: Some("Emerald".to_string()),
            volume,
            clock_set: true,
            utc_offset_min: 0,
            ..Default::default()
        },
    )
}

pub fn app_playing_with(root: &Path, stem: &str, snapshot: Box<dyn Snapshot>) -> App {
    seated(
        root,
        snapshot,
        SlotState {
            cart: Some(stem.to_string()),
            clock_set: true,
            utc_offset_min: 0,
            ..Default::default()
        },
    )
}

fn seated(root: &Path, snapshot: Box<dyn Snapshot>, state: SlotState) -> App {
    write_slot_state(root, &state).expect("write slot.state");
    let mut a = App::boot(root);
    a.set_snapshot(snapshot);
    a.on_core_ready();
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    a
}

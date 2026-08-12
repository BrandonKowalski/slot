mod common;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use slot::app::Phase;
use slot::audio::{AudioSink, StubSink};
use slot::core::open_core_for;
use slot::emu::{CoreState, EmuHandle};
use slot::session::Session;
use slot_input::{Btn, RawEvent};
use slot_retro::{ButtonMask, RetroCore};

/// A libretro core keeps its machine in dylib globals, so two live cores is not a
/// configuration the tests may reach.
static CORE_LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    CORE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn rom(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&p, common::gba_rom()).expect("write rom");
    p
}

/// The two cores are indistinguishable through the trait except by what they serialize:
/// mGBA writes out a whole machine, the mock writes a frame counter.
fn is_mgba(core: &mut dyn RetroCore, rom: &Path) -> bool {
    core.load(rom).expect("core refused the test rom");
    core.run_frame(ButtonMask::default());
    core.serialize().expect("core gave up no state").len() > 100_000
}

#[test]
fn the_vendored_core_is_preferred_over_the_mock() {
    let _g = lock();
    let Some(dylib) = common::vendored_core() else {
        return;
    };
    let d = common::tmp_root_with_real_carts(&[]);
    let mut core = open_core_for(d.path(), &[dylib]);
    assert!(
        is_mgba(core.as_mut(), &rom("preferred.gba")),
        "the mock ran with a vendored core sitting right there"
    );
}

#[test]
fn a_missing_core_falls_back_to_the_mock_rather_than_failing() {
    let d = common::tmp_root_with_real_carts(&[]);
    let mut core = open_core_for(d.path(), &[PathBuf::from("no/such/core.dylib")]);
    assert!(!is_mgba(core.as_mut(), &rom("missing.gba")));
}

/// A rom the core will not take has to reach the app as `Failed`. Anything else leaves the
/// insert animation waiting on a core that is never coming.
#[test]
fn a_rom_the_real_core_refuses_reports_failed() {
    let _g = lock();
    let Some(dylib) = common::vendored_core() else {
        return;
    };
    let d = common::tmp_root_with_carts(&["Broken"]);
    let emu = EmuHandle::spawn(
        open_core_for(d.path(), &[dylib]),
        d.path().join("Games/Broken.gba"),
        StubSink::new().ring(),
        None,
        None,
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while emu.state() == CoreState::Loading {
        assert!(Instant::now() < deadline, "the core never settled");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(emu.state(), CoreState::Failed);
}

/// And the refusal has to reach the slot. The mock takes anything, so this is the one path
/// that only exists once the real core is the default.
#[test]
fn a_cart_the_real_core_refuses_comes_back_out_of_the_slot() {
    let _g = lock();
    let Some(dylib) = common::vendored_core() else {
        return;
    };
    std::env::set_var("SLOT_CORE", dylib);
    let d = common::tmp_root_with_carts(&["Broken"]);
    common::clocked(d.path());
    let mut s = Session::boot(d.path().to_path_buf());
    s.feed([RawEvent::Down(Btn::A)], 16);
    s.update(1.0 / 60.0);
    assert!(matches!(s.app().phase(), Phase::Inserting { .. }));

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut now = 16;
    while !matches!(s.app().phase(), Phase::Shelf) {
        assert!(
            Instant::now() < deadline,
            "the refused cart never came back out"
        );
        now += 16;
        s.feed([], now);
        s.update(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(1));
    }
}

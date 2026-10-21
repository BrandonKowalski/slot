mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use common::core_lock;
use slot::app::Phase;
use slot::audio::{AudioSink, StubSink};
use slot::core::{apply_core_options, open_core_for};
use slot::emu::{CoreState, EmuHandle};
use slot::session::Session;
use slot_input::{Btn, RawEvent};
use slot_retro::{ButtonMask, LibretroCore, RetroCore};
use slot_store::{Core, Platform};

fn rom(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&p, common::gba_rom()).expect("write rom");
    p
}

fn is_mgba(core: &mut dyn RetroCore, rom: &Path) -> bool {
    core.load(rom).expect("core refused the test rom");
    core.run_frame(ButtonMask::default());
    core.serialize().expect("core gave up no state").len() > 100_000
}

#[test]
fn the_vendored_core_is_preferred_over_the_mock() {
    let _g = core_lock();
    let Some(dylib) = common::vendored_core() else {
        return;
    };
    let d = common::tmp_root_with_real_carts(&[]);
    let mut core = open_core_for(d.path(), Core::Mgba, "auto", false, &[dylib]);
    assert!(
        is_mgba(core.as_mut(), &rom("preferred.gba")),
        "the mock ran with a vendored core sitting right there"
    );
}

#[test]
fn a_missing_core_falls_back_to_the_mock_rather_than_failing() {
    let d = common::tmp_root_with_real_carts(&[]);
    let mut core = open_core_for(
        d.path(),
        Core::Mgba,
        "auto",
        false,
        &[PathBuf::from("no/such/core.dylib")],
    );
    assert!(!is_mgba(core.as_mut(), &rom("missing.gba")));
}

#[test]
fn a_rom_the_real_core_refuses_reports_failed() {
    let _g = core_lock();
    let Some(dylib) = common::vendored_core() else {
        return;
    };
    let d = common::tmp_root_with_carts(&["Broken"]);
    let emu = EmuHandle::spawn(
        open_core_for(d.path(), Core::Mgba, "auto", false, &[dylib]),
        d.path().join("Games/GBA/Broken.gba"),
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

#[test]
fn a_cart_the_real_core_refuses_comes_back_out_of_the_slot() {
    let _g = core_lock();
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

fn formatting_presses(frame: u32) -> ButtonMask {
    ButtonMask(match (frame / 15) % 4 {
        0 if frame > 120 => ButtonMask::START,
        2 if frame > 120 => ButtonMask::A,
        _ => 0,
    })
}

#[test]
fn on_mgba_loading_a_state_while_the_game_writes_its_second_flash_bank_keeps_the_save_whole() {
    let _g = core_lock();
    let rom = common::repo_root().join(
        "sdcard/Games/GBA/Super Mario Advance 4 - Super Mario Bros. 3 (USA, Australia) (Rev 1).gba",
    );
    let Some(dylib) = common::vendored_core().filter(|_| rom.exists()) else {
        eprintln!("no vendored mGBA core or no Super Mario Advance 4 under sdcard/Games, skipping");
        return;
    };
    let save_after_formatting = |reload: bool| {
        let mut core =
            LibretroCore::open(&dylib).expect("vendored core is present but would not open");
        apply_core_options(
            &mut core,
            Core::Mgba,
            Platform::Gba,
            "auto",
            false,
            false,
            None,
        );
        core.load(&rom).expect("mGBA refused the rom");
        for frame in 0..900 {
            core.run_frame(formatting_presses(frame));
            let _ = core.take_audio();
            if reload {
                let state = core.serialize().expect("mGBA would not save");
                core.unserialize(&state)
                    .expect("mGBA would not load its own state");
            }
        }
        core.save_ram().expect("mGBA gave no save")
    };
    let played = save_after_formatting(false);
    assert!(
        played[..8] == *b"SMA4MWE0",
        "the game never formatted its save, so this test proves nothing"
    );
    let reloaded = save_after_formatting(true);
    let wrong = reloaded.iter().zip(&played).filter(|(a, b)| a != b).count();
    assert_eq!(
        wrong, 0,
        "loading a state on every frame of the format left {wrong} bytes of the save different \
         from playing straight through"
    );
}

fn fusion() -> Option<(PathBuf, LibretroCore)> {
    let rom = common::repo_root().join("sdcard/Games/GBA/Metroid Fusion.gba");
    let dylib = common::vendored_core().filter(|_| rom.exists())?;
    let mut core = LibretroCore::open(&dylib).expect("vendored core is present but would not open");
    apply_core_options(
        &mut core,
        Core::Mgba,
        Platform::Gba,
        "auto",
        false,
        false,
        None,
    );
    core.load(&rom).expect("mGBA refused the rom");
    Some((rom, core))
}

fn run(core: &mut LibretroCore, frames: usize) -> Vec<u8> {
    for _ in 0..frames {
        core.run_frame(ButtonMask::default());
        let _ = core.take_audio();
    }
    core.video_xrgb8888().to_vec()
}

#[test]
fn on_mgba_states_shrink_to_the_save_chip_once_the_game_settles_on_one() {
    let _g = core_lock();
    let Some((_, mut early)) = fusion() else {
        eprintln!("no vendored mGBA core or no Metroid Fusion under sdcard/Games, skipping");
        return;
    };
    let undetected = early.serialize().expect("mGBA would not save").len();
    run(&mut early, 60);
    let after = early.serialize().expect("mGBA would not save").len();
    drop(early);

    let (_, mut late) = fusion().expect("Metroid Fusion went away");
    run(&mut late, 60);
    let exact = late.serialize().expect("mGBA would not save").len();
    assert!(
        exact < undetected,
        "the state never shrank once the chip was found, so this proves nothing"
    );
    assert_eq!(
        after,
        exact,
        "a state taken before the game found its chip left later ones {} bytes too big",
        after - exact
    );
}

#[test]
fn on_mgba_a_state_padded_by_an_older_slot_still_loads_and_plays_the_same() {
    let _g = core_lock();
    let Some((_, mut core)) = fusion() else {
        eprintln!("no vendored mGBA core or no Metroid Fusion under sdcard/Games, skipping");
        return;
    };
    let undetected = core.serialize().expect("mGBA would not save").len();
    run(&mut core, 60);
    let exact = core.serialize().expect("mGBA would not save");
    let mut padded = exact.clone();
    padded.resize(undetected, 0);

    core.unserialize(&exact)
        .expect("mGBA would not load its own state");
    let expected = run(&mut core, 120);
    core.unserialize(&padded)
        .expect("mGBA refused a state with an older slot's zeros after it");
    assert_eq!(
        run(&mut core, 120),
        expected,
        "the padded state plays on differently from the same state without the zeros"
    );
}

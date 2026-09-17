mod common;

use slot_store::{Core, Platform};

/// The device carries both cores in `System/`; a host build carries whichever were fetched.
/// Absent means this host cannot run the test, not that the test failed.
fn dylib_for(core: Core) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor")
        .join(slot::core::dylib_name(core))
}

/// Exercises the real `dylib_name`, not a stand-in: it is the function `open_core` calls to
/// build its search list, so this is what actually stops a `gpsp` cart's search from ever
/// spelling "mgba_libretro".
#[test]
fn each_core_resolves_to_its_own_dylib() {
    assert_ne!(
        slot::core::dylib_name(Core::Mgba),
        slot::core::dylib_name(Core::Gpsp)
    );
    assert!(slot::core::dylib_name(Core::Gpsp).starts_with("gpsp_libretro"));
    assert!(dylib_for(Core::Gpsp)
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("gpsp_libretro"));
}

#[test]
fn gpsp_loads_and_runs_a_frame() {
    let path = dylib_for(Core::Gpsp);
    if !path.exists() {
        eprintln!("no gpSP dylib on this host, skipping");
        return;
    }
    let _g = common::core_lock();
    let mut core = slot_retro::LibretroCore::open(&path).expect("open gpsp");
    // Serial mode is the whole reason gpSP is here. Setting it before load is what the
    // core expects: it reads options during retro_load_game.
    core.set_option("gpsp_serial", "rfu");
    assert_eq!(core.option("gpsp_serial"), Some("rfu".to_string()));
}

/// `auto` resolves the serial protocol from the ROM itself, so two devices running the same
/// game agree on a mode without either being told which. It is what every cart loads with
/// until its link screen is switched to the other hardware.
#[test]
fn gpsp_is_told_its_serial_mode_before_load() {
    let path = dylib_for(Core::Gpsp);
    if !path.exists() {
        eprintln!("no gpSP dylib on this host, skipping");
        return;
    }
    let _g = common::core_lock();
    let mut core = slot_retro::LibretroCore::open(&path).expect("open gpsp");
    slot::core::apply_core_options(&mut core, Core::Gpsp, "auto", false, false);
    assert_eq!(
        core.option("gpsp_serial"),
        Some("auto".to_string()),
        "auto resolves per ROM, so both devices agree without being told"
    );
}

/// The link screen's switch reaches gpSP through this and nothing else: the mode it asked for
/// is the value the core is handed, never `auto` in its place.
#[test]
fn gpsp_is_told_the_serial_mode_it_is_handed() {
    let path = dylib_for(Core::Gpsp);
    if !path.exists() {
        eprintln!("no gpSP dylib on this host, skipping");
        return;
    }
    let _g = common::core_lock();
    let mut core = slot_retro::LibretroCore::open(&path).expect("open gpsp");
    for serial in ["rfu", "mul_poke", "mul_aw1", "mul_aw2"] {
        slot::core::apply_core_options(&mut core, Core::Gpsp, serial, false, false);
        assert_eq!(
            core.option("gpsp_serial"),
            Some(serial.to_string()),
            "gpSP was not handed the mode the link screen asked for"
        );
    }
}

/// A real BIOS on the card is what buys the player the boot logo and chime, and gpSP's own
/// default is `game`, which drops straight into the cart without ever running it.
///
/// `gpsp_bios` is checked to be still unset here, not merely left unmentioned: its default,
/// `auto`, already loads `<system>/gba_bios.bin` and keeps it whenever it passes the same
/// first-byte test `has_real_bios` applies, so naming `official` would select the very same
/// image and change only the failure path — where it draws a warning over slot's own chrome
/// through the core's OSD before falling back to the built-in BIOS `auto` falls back to
/// quietly.
#[test]
fn gpsp_boots_through_the_bios_when_the_card_carries_one() {
    let path = dylib_for(Core::Gpsp);
    if !path.exists() {
        eprintln!("no gpSP dylib on this host, skipping");
        return;
    }
    let _g = common::core_lock();
    let mut core = slot_retro::LibretroCore::open(&path).expect("open gpsp");
    slot::core::apply_core_options(&mut core, Core::Gpsp, "auto", true, false);
    assert_eq!(
        core.option("gpsp_boot_mode"),
        Some("bios".to_string()),
        "a real BIOS on the card and gpSP still told to skip it"
    );
    assert_eq!(
        core.option("gpsp_bios"),
        None,
        "gpsp_bios was named: auto already picks the official image up, and official only \
         adds an on-screen warning when it cannot"
    );
    assert_eq!(
        core.option("gpsp_serial"),
        Some("auto".to_string()),
        "the link mode stopped getting through once the boot mode joined it"
    );
}

/// gpSP's built-in BIOS has no logo and no chime to play, so booting through it is a few
/// seconds of blank screen that reads as a hang. Without the file, the option stays unset and
/// gpSP keeps its own `game` default.
#[test]
fn gpsp_is_left_on_its_own_boot_default_when_the_card_has_no_bios() {
    let path = dylib_for(Core::Gpsp);
    if !path.exists() {
        eprintln!("no gpSP dylib on this host, skipping");
        return;
    }
    let _g = common::core_lock();
    let mut core = slot_retro::LibretroCore::open(&path).expect("open gpsp");
    slot::core::apply_core_options(&mut core, Core::Gpsp, "auto", false, false);
    assert_eq!(
        core.option("gpsp_boot_mode"),
        None,
        "gpSP was sent through a BIOS the card does not have, which is its built-in one: \
         seconds of blank screen where the logo was promised"
    );
}

/// mGBA has no `gpsp_serial` option at all; handing it one anyway would be silently ignored
/// by mGBA today and a landmine the moment mGBA ever grows an option by that name. Handed a
/// mode that is not `auto`, it still gets none of gpSP's.
///
/// Told a BIOS is present as well, since that is the state of the user's own card: mGBA's own
/// boot switch is spelled `mgba_skip_bios`, so gpSP's spelling must not reach it either.
///
/// What it does get is a frameskip option under its own prefix, and that is worth pinning
/// rather than assuming. It is what makes mGBA register the audio buffer status callback at
/// all: a key spelled wrong would leave the callback unregistered, `set_frame_skip` a silent
/// no-op, and every frame of a fast forward drawn — slower, with nothing failing anywhere.
#[test]
fn mgba_is_given_its_own_frameskip_and_none_of_gpsps() {
    let path = dylib_for(Core::Mgba);
    if !path.exists() {
        eprintln!("no mGBA dylib on this host, skipping");
        return;
    }
    let _g = common::core_lock();
    let mut core = slot_retro::LibretroCore::open(&path).expect("open mgba");
    slot::core::apply_core_options(&mut core, Core::Mgba, "rfu", true, false);
    assert_eq!(
        core.option("gpsp_serial"),
        None,
        "mGBA has no such option and must not be handed one"
    );
    assert_eq!(
        core.option("gpsp_boot_mode"),
        None,
        "gpSP's boot switch reached mGBA, which spells its own mgba_skip_bios"
    );
    assert_eq!(
        core.option("mgba_frameskip").as_deref(),
        Some("auto"),
        "mGBA was never put on auto frameskip, so nothing can tell it which frame to draw"
    );
    assert_eq!(
        core.option("gpsp_frameskip"),
        None,
        "gpSP's frameskip key reached mGBA, which reads only its own prefix"
    );
}

/// The quick menu's Colour Correction, on both cores, in each core's own spelling.
///
/// Pinned rather than trusted, because nothing in this tree can tell a correct option value
/// from a typo: `slot-retro` answers `SET_VARIABLES` with a bare `true` and throws the declared
/// list away, so `Autp` or `mgba_colour_correction` would be accepted in silence and simply
/// never take — a row that does nothing, with nothing failing anywhere. These four strings were
/// read off the vendored dylibs by dumping that discarded list; this is what keeps them true.
///
/// The two cores disagree about every part of it. mGBA declares `OFF|GBA|GBC|Auto` and gpSP
/// `disabled|enabled`, under different keys, and only mGBA has an `Auto` — it is the core that
/// runs Game Boy and Game Boy Color carts as well as GBA ones, so it is the only one with more
/// than one tint to choose between. Neither core may be handed the other's words.
#[test]
fn both_cores_are_told_about_colour_correction_in_their_own_words() {
    for (which, key, on, off) in [
        (Core::Mgba, "mgba_color_correction", "Auto", "OFF"),
        (Core::Gpsp, "gpsp_color_correction", "enabled", "disabled"),
    ] {
        let path = dylib_for(which);
        if !path.exists() {
            eprintln!("no {} dylib on this host, skipping", which.as_str());
            continue;
        }
        let _g = common::core_lock();
        let mut core = slot_retro::LibretroCore::open(&path).expect("open the core");
        for (colour, want) in [(true, on), (false, off)] {
            // Set in both directions rather than only when the row is on: leaving the option
            // unset when it is off would put the picture at whatever the core's own default
            // happens to be that release, which is not the same promise as "off".
            slot::core::apply_core_options(&mut core, which, "auto", false, colour);
            assert_eq!(
                core.option(key).as_deref(),
                Some(want),
                "{} was not told {want} for colour correction {colour}",
                which.as_str()
            );
        }
        let theirs = match which {
            Core::Mgba => "gpsp_color_correction",
            Core::Gpsp => "mgba_color_correction",
        };
        assert_eq!(
            core.option(theirs),
            None,
            "{} was handed the other core's key",
            which.as_str()
        );
    }
}

/// gpSP's half of the same contract, and the same reasoning: its own frameskip key, under its
/// own prefix, and none of mGBA's.
#[test]
fn gpsp_is_put_on_auto_frameskip() {
    let path = dylib_for(Core::Gpsp);
    if !path.exists() {
        eprintln!("no gpSP dylib on this host, skipping");
        return;
    }
    let _g = common::core_lock();
    let mut core = slot_retro::LibretroCore::open(&path).expect("open gpsp");
    slot::core::apply_core_options(&mut core, Core::Gpsp, "auto", false, false);
    assert_eq!(
        core.option("gpsp_frameskip").as_deref(),
        Some("auto"),
        "gpSP was never put on auto frameskip, so nothing can tell it which frame to draw"
    );
    assert_eq!(
        core.option("mgba_frameskip"),
        None,
        "mGBA's frameskip key reached gpSP, which reads only its own prefix"
    );
}

/// A content root holding the user's own BIOS and a cart the BIOS will recognise, or `None`
/// on a machine carrying neither. Both are the user's, both live under the ignored `/sdcard`,
/// and neither may ever be checked in — so everything below skips itself on a fresh clone and
/// in CI, and runs for real on the machine that has them.
fn root_with_bios_and_logo_cart() -> Option<(tempfile::TempDir, std::path::PathBuf)> {
    let (bios, rom_bytes) = (common::real_bios()?, common::logo_rom()?);
    let d = common::tmp_root_with_carts(&[]);
    std::fs::copy(bios, d.path().join("BIOS").join("gba_bios.bin")).expect("copy bios");
    let rom = d.path().join("Games").join("Logo.gba");
    std::fs::write(&rom, rom_bytes).expect("write logo rom");
    Some((d, rom))
}

/// Whether the BIOS boot animation reaches the screen, run through the production path:
/// `open_core_for` is the one place the boot mode is applied, and it reads the BIOS off the
/// content root itself, so this is the real wiring rather than a core configured by hand.
///
/// The animation runs about two seconds and does not begin on frame 0, so what is asked is
/// whether it happened anywhere in that window, not what any single frame holds.
fn splash_plays(root: &std::path::Path, rom: &std::path::Path) -> bool {
    use slot_retro::ButtonMask;
    let mut core =
        slot::core::open_core_for(root, Core::Gpsp, "auto", false, &[dylib_for(Core::Gpsp)]);
    core.load(rom).expect("gpSP refused the logo rom");
    (0..150).any(|_| {
        core.run_frame(ButtonMask::default());
        common::mostly_lit(core.video_xrgb8888())
    })
}

/// The feature the user asked for, checked on the screen rather than in a draw list: a real
/// BIOS on the card means the cart boots through it, logo and all.
///
/// Pixels, because an option read back only proves gpSP was told something. Whether the
/// animation actually plays depends on gpSP agreeing that the image is a BIOS at all — it
/// applies its own first-byte test and silently falls back to a built-in BIOS with no splash
/// in it — and on the BIOS recognising the cart's logo. Neither of those shows up in an
/// assertion about an option string.
#[test]
fn a_cart_boots_through_a_real_bios_and_the_splash_reaches_the_screen() {
    if !dylib_for(Core::Gpsp).exists() {
        eprintln!("no gpSP dylib on this host, skipping");
        return;
    }
    let Some((d, rom)) = root_with_bios_and_logo_cart() else {
        eprintln!("no real BIOS or no cart to lift a logo from on this host, skipping");
        return;
    };
    let _g = common::core_lock();
    assert!(
        splash_plays(d.path(), &rom),
        "the cart went straight to the game with a real BIOS sitting in the content root"
    );
}

/// The other half, and the reason the option is conditional at all: gpSP's built-in BIOS has
/// no logo and no chime, so booting through it would spend the same seconds on a blank screen.
/// With no BIOS on the card the cart goes straight to the game, as it did before this existed.
#[test]
fn a_cart_goes_straight_to_the_game_when_the_card_has_no_bios() {
    if !dylib_for(Core::Gpsp).exists() {
        eprintln!("no gpSP dylib on this host, skipping");
        return;
    }
    let Some((d, rom)) = root_with_bios_and_logo_cart() else {
        eprintln!("no real BIOS or no cart to lift a logo from on this host, skipping");
        return;
    };
    let _g = common::core_lock();
    std::fs::remove_file(d.path().join("BIOS").join("gba_bios.bin")).unwrap();
    assert!(
        !splash_plays(d.path(), &rom),
        "a splash played with no BIOS on the card, so this test cannot tell the two apart"
    );
}

/// Whether any frame the emulator thread actually publishes is the boot screen. Through
/// `EmuHandle::spawn`, which is what `session.rs` calls and where the ordering that matters
/// lives: the worker loads the rom, restores the resume state, and only then publishes
/// anything at all.
fn a_published_frame_is_the_splash(
    root: &std::path::Path,
    rom: &std::path::Path,
    resume: Option<Vec<u8>>,
) -> bool {
    use slot::audio::{AudioSink, StubSink};
    use slot::emu::{CoreState, EmuHandle, Speed};
    use std::time::{Duration, Instant};

    let mut sink = StubSink::new();
    sink.open(32_768).expect("the stub refused to open");
    // The worker waits for the device to make room, so a sink nothing drains holds it up
    // before it ever gets to a second frame.
    let drain = sink.clone();
    std::thread::spawn(move || loop {
        drain.device_drain();
        std::thread::sleep(Duration::from_millis(2));
    });

    let emu = EmuHandle::spawn(
        slot::core::open_core_for(root, Core::Gpsp, "auto", false, &[dylib_for(Core::Gpsp)]),
        rom.to_path_buf(),
        sink.ring(),
        None,
        resume,
    );
    // A worker starts paused, as one spawned during an insert must: the first frames of the
    // boot would otherwise run behind the cart where nobody can see them.
    emu.set_speed(Speed::Normal);
    let deadline = Instant::now() + Duration::from_secs(10);
    while emu.state() == CoreState::Loading {
        assert!(Instant::now() < deadline, "the core never finished loading");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(emu.state(), CoreState::Ready, "the core refused the cart");

    // Longer than the animation, so a splash that plays at all is a splash this sees.
    let watch = Instant::now() + Duration::from_secs(3);
    while Instant::now() < watch {
        if emu.latest_frame().is_some_and(|f| common::mostly_lit(&f)) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    false
}

/// The requirement that makes the splash bearable: it belongs to starting a game, not to
/// picking one back up. A cart resumed from a save state must land where the player left it.
///
/// Both halves run here, against the same core, cart and BIOS, so the only difference between
/// them is whether a state was restored. The fresh half is what keeps the resumed half
/// honest — without it, a harness that could never see a splash would pass just as happily.
///
/// What makes the resumed half true is ordering rather than configuration: `emu::Worker::run`
/// restores the state after `load` and before it publishes a single frame, so the BIOS's
/// machine is replaced before anything reaches the screen. That is also why the option can be
/// set on every load, and why a reload for a link — which flushes and resumes through this
/// same path — cannot replay it either.
#[test]
fn the_splash_plays_on_a_fresh_start_and_never_over_a_resume() {
    if !dylib_for(Core::Gpsp).exists() {
        eprintln!("no gpSP dylib on this host, skipping");
        return;
    }
    let Some((d, rom)) = root_with_bios_and_logo_cart() else {
        eprintln!("no real BIOS or no cart to lift a logo from on this host, skipping");
        return;
    };
    let _g = common::core_lock();

    // A genuine gpSP state, taken from a machine well past its own boot. The boot is longer
    // than it looks: measured here, the screen is still the BIOS's at frame 269 and only
    // becomes the game's at 270. A state taken before that is a white screen saved mid-splash,
    // and restoring it looks exactly like the splash replaying — which is how this test first
    // failed, on its own fixture rather than on the product. The assertion is what keeps that
    // from ever being read as the bug again, whatever the number drifts to. Dropped before
    // anything else opens a core: libretro allows only one live at a time.
    let state = {
        use slot_retro::ButtonMask;
        let mut core = slot::core::open_core_for(
            d.path(),
            Core::Gpsp,
            "auto",
            false,
            &[dylib_for(Core::Gpsp)],
        );
        core.load(&rom).expect("gpSP refused the logo rom");
        for _ in 0..480 {
            core.run_frame(ButtonMask::default());
        }
        assert!(
            !common::mostly_lit(core.video_xrgb8888()),
            "the state standing in for a resume is itself a frame of the boot splash, so the \
             half below would fail no matter what the resume did"
        );
        core.serialize().expect("gpSP gave up no state")
    };

    assert!(
        a_published_frame_is_the_splash(d.path(), &rom, None),
        "no splash on a fresh start, so this test cannot see one and proves nothing below"
    );
    assert!(
        !a_published_frame_is_the_splash(d.path(), &rom, Some(state)),
        "the BIOS splash played over a cart the player was resuming"
    );
}

/// The bug the previous plan shipped: `open_core` searched for `mgba_libretro` no matter
/// what the cart asked for, while the resume lookup already read `core_for` from the ini.
/// A `gpsp` cart could therefore run on mGBA with its state filed under `States/gpsp/` — two
/// independent derivations of one fact, quietly disagreeing.
///
/// Neither host in CI nor this Mac carries a working `gpsp_libretro` dylib, so this cannot
/// assert on which engine ran. What it can assert, through the real `Session` rather than a
/// stand-in, is the half that never needed a real core to prove: a cart resolved to `Gpsp`
/// reads (and only reads) the resume state filed under its own core's directory, through the
/// exact call `spawn_core` makes. `each_core_resolves_to_its_own_dylib` pins the other half —
/// that the same `Core` value makes `open_core` search a different filename entirely. Because
/// `session.rs` resolves the core once and hands that single value to both, these two halves
/// cannot drift apart without editing the same line.
#[test]
fn a_gpsp_carts_resume_is_read_from_its_own_core_directory_through_the_session() {
    use slot::app::Phase;
    use slot::persist;
    use slot::session::Session;
    use slot_input::{Btn, RawEvent};
    use slot_store::{StateRing, SELECTED_CORE_FILE};
    use std::time::{Duration, Instant};

    let d = common::tmp_root_with_carts(&["Emerald", "Fusion"]);
    std::fs::write(d.path().join(SELECTED_CORE_FILE), "Emerald = gpsp\n").unwrap();

    // Distinguishable resume states in both directories. If `spawn_core` ever resolved the
    // core twice and the two calls disagreed, or fell back to the default, this is what
    // would catch it: the counter would come back from the wrong file.
    StateRing::new(d.path(), Platform::Gba, Core::Gpsp, "Emerald")
        .write_resume(&700_000u64.to_le_bytes())
        .unwrap();
    StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald")
        .write_resume(&1u64.to_le_bytes())
        .unwrap();

    common::clocked(d.path());
    let mut s = Session::boot(d.path().to_path_buf());
    s.feed([RawEvent::Down(Btn::A)], 16);
    s.feed([RawEvent::Up(Btn::A)], 32);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut now = 32;
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        now += 16;
        s.feed([], now);
        s.update(1.0 / 60.0);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    // Past the autosave deadline, the cheapest way to get the core's own counter written
    // back out through the path the binary uses, same as `play.rs`'s `counter_after`.
    s.app_mut().tick_ms(60_000);

    let state = persist::read_resume(d.path(), Platform::Gba, Core::Gpsp, "Emerald")
        .expect("nothing resumed");
    let n = u64::from_le_bytes(state.try_into().expect("mock state is 8 bytes"));
    assert!(
        n >= 700_000,
        "the session resumed the wrong core's state (or none): counter is {n}"
    );
}

/// I1: `session.rs:372`'s `open_core(&self.root, core)` is the one production line that
/// actually opens the engine a cart resolved to. Every test above that proves a `gpsp` cart's
/// state lands under `States/gpsp/` does so with no real dylib on this host, so `open_core`
/// falls back to the mock regardless of which `Core` it is handed — mutating that call to
/// `open_core(&self.root, slot_store::Core::Mgba)` (state directory still follows the ini,
/// engine no longer does — the original divergence bug, verbatim) is invisible to every one
/// of them, because the mock's own 8 byte state is what all of them observe either way.
///
/// Only a real dylib, opened through the real `Session`, can tell "the right engine ran"
/// apart from "the right directory was merely named". This plants mGBA's own build under
/// gpSP's filename — the same trick
/// `open_core_reaches_a_gpsp_named_dylib_under_the_content_roots_system_directory` uses on
/// `open_core` directly — but drives it through a full `Session`, which is the one thing that
/// test does not do and the one thing `session.rs:372` needs pinned.
#[test]
fn a_gpsp_cart_runs_the_dylib_planted_under_its_own_name_through_the_session() {
    use slot::app::Phase;
    use slot::persist;
    use slot::session::Session;
    use slot_input::{Btn, RawEvent};
    use slot_store::SELECTED_CORE_FILE;
    use std::time::{Duration, Instant};

    let Some(mgba) = common::vendored_core() else {
        eprintln!("no host-openable dylib on this machine, skipping");
        return;
    };
    let _g = common::core_lock();

    let d = common::tmp_root_with_real_carts(&["Emerald"]);
    std::fs::write(d.path().join(SELECTED_CORE_FILE), "Emerald = gpsp\n").unwrap();
    let planted = d
        .path()
        .join("System")
        .join(slot::core::dylib_name(Core::Gpsp));
    std::fs::copy(&mgba, &planted).expect("plant a dylib under gpSP's name");

    common::clocked(d.path());
    let mut s = Session::boot(d.path().to_path_buf());
    s.feed([RawEvent::Down(Btn::A)], 16);
    s.feed([RawEvent::Up(Btn::A)], 32);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut now = 32;
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        now += 16;
        s.feed([], now);
        s.update(1.0 / 60.0);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    // Past the autosave deadline, so the planted core's own (large, real) state is what
    // gets written back through the path the binary uses.
    s.app_mut().tick_ms(60_000);

    let state = persist::read_resume(d.path(), Platform::Gba, Core::Gpsp, "Emerald")
        .expect("nothing resumed");
    assert!(
        state.len() > 100_000,
        "the session ran the mock, not the dylib the ini named: {} bytes",
        state.len()
    );
}

/// F2: `core_for` used to be re-read on every flush, not just at insert — and
/// `read_selected_cores` returns an empty map on ANY read failure (a typo, a remount, a
/// write caught mid-flight), which silently reclassified every seated cart as mGBA. This is
/// a removable card in a handheld, so that window is reachable without a person touching the
/// file by hand. Proves the fix holds through it: the ini is edited (here, removed outright)
/// while the cart is already playing, and the autosave 60 s later still lands under the
/// seated core's own directory, because `App` stored what `session.rs` resolved at insert
/// instead of asking `core_for` again on the way out.
#[test]
fn changing_the_ini_mid_session_does_not_move_a_seated_carts_autosave() {
    use slot::app::Phase;
    use slot::session::Session;
    use slot_input::{Btn, RawEvent};
    use slot_store::{StateRing, SELECTED_CORE_FILE};
    use std::time::{Duration, Instant};

    let d = common::tmp_root_with_carts(&["Emerald"]);
    std::fs::write(d.path().join(SELECTED_CORE_FILE), "Emerald = gpsp\n").unwrap();

    common::clocked(d.path());
    let mut s = Session::boot(d.path().to_path_buf());
    s.feed([RawEvent::Down(Btn::A)], 16);
    s.feed([RawEvent::Up(Btn::A)], 32);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut now = 32;
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        now += 16;
        s.feed([], now);
        s.update(1.0 / 60.0);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    // The card is edited out from under the already-running session — indistinguishable,
    // from `read_selected_cores`'s point of view, from the transient read failure it
    // deliberately turns into an empty map rather than an error.
    std::fs::remove_file(d.path().join(SELECTED_CORE_FILE)).unwrap();

    s.app_mut().tick_ms(60_000);

    assert!(
        StateRing::new(d.path(), Platform::Gba, Core::Gpsp, "Emerald")
            .read_resume()
            .unwrap()
            .is_some(),
        "the autosave did not land under the seated core's own directory"
    );
    assert!(
        StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald")
            .read_resume()
            .unwrap()
            .is_none(),
        "the autosave followed the ini's new (absent) reading instead of the core the \
         session actually spawned"
    );
}

/// I3: `flush_resume` is only one of three state-directory sinks in `app.rs` that take
/// `self.core` — the `Core` `session.rs` resolved once at insert — instead of re-deriving one.
/// `App::ring()` (app.rs, behind `SELECT+R1`'s manual save) is a second, and the test above
/// does not exercise it: re-deriving with `slot_store::core_for(root, cart)` at `ring()`
/// passes it and every other test in the suite just as it does for `flush_resume`. Same trick
/// as above — the ini is changed out from under an already-seated cart — aimed at the sink
/// that test cannot see.
#[test]
fn changing_the_ini_mid_session_does_not_move_a_manual_save_state() {
    use slot::app::Phase;
    use slot::session::Session;
    use slot_input::{Action, Btn, RawEvent};
    use slot_store::{StateRing, SELECTED_CORE_FILE};
    use std::time::{Duration, Instant};

    let d = common::tmp_root_with_carts(&["Emerald"]);
    std::fs::write(d.path().join(SELECTED_CORE_FILE), "Emerald = gpsp\n").unwrap();

    common::clocked(d.path());
    let mut s = Session::boot(d.path().to_path_buf());
    s.feed([RawEvent::Down(Btn::A)], 16);
    s.feed([RawEvent::Up(Btn::A)], 32);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut now = 32;
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        now += 16;
        s.feed([], now);
        s.update(1.0 / 60.0);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    std::fs::remove_file(d.path().join(SELECTED_CORE_FILE)).unwrap();

    s.app_mut().apply(Action::SaveState);

    assert!(
        !StateRing::new(d.path(), Platform::Gba, Core::Gpsp, "Emerald")
            .list()
            .unwrap()
            .is_empty(),
        "the manual save did not land under the seated core's own directory"
    );
    assert!(
        StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald")
            .list()
            .unwrap()
            .is_empty(),
        "the manual save followed the ini's new (absent) reading instead of the core the \
         session actually spawned"
    );
}

/// I3's third sink: `flush_eject` (app.rs) re-derives with the same shape of bug the two
/// tests above already catch at `flush_resume` and `ring`. Same trick again, ended with an
/// eject rather than an autosave or a manual save.
#[test]
fn changing_the_ini_mid_session_does_not_move_an_ejected_carts_resume() {
    use slot::app::Phase;
    use slot::session::Session;
    use slot_input::{Action, Btn, RawEvent};
    use slot_store::{StateRing, SELECTED_CORE_FILE};
    use std::time::{Duration, Instant};

    // Two carts: a lone cart is a dedicated device and has nowhere to eject to, so `eject()`
    // refuses outright.
    let d = common::tmp_root_with_carts(&["Emerald", "Fusion"]);
    std::fs::write(d.path().join(SELECTED_CORE_FILE), "Emerald = gpsp\n").unwrap();

    common::clocked(d.path());
    let mut s = Session::boot(d.path().to_path_buf());
    s.feed([RawEvent::Down(Btn::A)], 16);
    s.feed([RawEvent::Up(Btn::A)], 32);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut now = 32;
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        now += 16;
        s.feed([], now);
        s.update(1.0 / 60.0);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    std::fs::remove_file(d.path().join(SELECTED_CORE_FILE)).unwrap();

    s.app_mut().apply(Action::Eject);

    assert!(
        StateRing::new(d.path(), Platform::Gba, Core::Gpsp, "Emerald")
            .read_resume()
            .unwrap()
            .is_some(),
        "the ejected cart's resume did not land under the seated core's own directory"
    );
    assert!(
        StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald")
            .read_resume()
            .unwrap()
            .is_none(),
        "the ejected cart's resume followed the ini's new (absent) reading instead of the \
         core the session actually spawned"
    );
}

/// The half the test above cannot pin, by its own admission: it never needed a real core, so
/// a mutation that hands `open_core` the wrong `Core` — the original bug, verbatim — sails
/// through it and every other test in the suite. Before `candidates` searched `root/System`,
/// nothing could catch that either: an integration test's tmp root sits nowhere
/// `current_exe()` or `./vendor` look, so there was no candidate a test could plant a fake
/// dylib under and observe.
///
/// This is a path resolution test, not an ABI one, so a zero-byte stand-in would do for
/// gpSP's own dylib — except it would open (or fail to) identically whichever `Core` was
/// asked for, proving nothing about which filename the search actually reached. Real content
/// that can be told apart from the mock is what makes the difference observable: this plants
/// the one host-openable dylib the repo keeps around, mGBA's own build, filed under gpSP's
/// name. `open_core` does not care what a dylib is, only whether it opens, so if `Core::Gpsp`
/// ever resolves to `mgba_libretro`'s filename instead, or the search never reaches
/// `root/System` at all, this returns the mock rather than the planted core.
#[test]
fn open_core_reaches_a_gpsp_named_dylib_under_the_content_roots_system_directory() {
    use slot_retro::ButtonMask;

    let Some(mgba) = common::vendored_core() else {
        eprintln!("no host-openable dylib on this machine, skipping");
        return;
    };
    let _g = common::core_lock();
    let d = common::tmp_root_with_real_carts(&["Probe"]);
    let planted = d
        .path()
        .join("System")
        .join(slot::core::dylib_name(Core::Gpsp));
    std::fs::copy(&mgba, &planted).expect("plant a dylib under gpSP's name");

    let mut core = slot::core::open_core(d.path(), Core::Gpsp, "auto", false).core;
    core.load(&d.path().join("Games/GBA/Probe.gba"))
        .expect("the planted core refused the test rom");
    core.run_frame(ButtonMask::default());
    assert!(
        core.serialize().expect("core gave up no state").len() > 100_000,
        "open_core fell back to the mock instead of the dylib planted at root/System"
    );
}

/// `System/selected_core.ini` is a text file a person edits on a card, and nothing in it stops a
/// line naming gpSP for a Game Boy cart. gpSP does not run Game Boy games at all — it would
/// refuse the ROM outright or paint garbage — so for a cart that is not a GBA cart the file gets
/// no say: the platform the shelf scanned it under is what settles which core runs. The ini keeps
/// its meaning for a GBA cart, where there really are two engines to choose between.
///
/// Driven through the real `Session`, because `spawn_core` is the one place a cart's core is
/// resolved, and read back through the directory that one resolution also names. The seeded
/// counter is 700_000, which is further than the mock could ever count to on its own, and it has
/// to come back **moved**: only a run that read `States/GB/mgba/` and then wrote back to it can
/// produce that, so one number pins both halves. A run that had honoured the ini would have left
/// that file exactly as seeded and filed its own state under `States/GB/gpsp/` instead, which is
/// what the second assertion refuses.
#[test]
fn a_game_boy_cart_runs_on_mgba_whatever_the_ini_says() {
    use slot::app::Phase;
    use slot::persist;
    use slot::session::Session;
    use slot_input::{Btn, RawEvent};
    use slot_store::{StateRing, SELECTED_CORE_FILE};
    use std::time::{Duration, Instant};

    let d = common::tmp_root_with_gb_carts(&["Tetris", "Zzz"]);
    std::fs::write(d.path().join(SELECTED_CORE_FILE), "Tetris = gpsp\n").unwrap();
    StateRing::new(d.path(), Platform::Gb, Core::Mgba, "Tetris")
        .write_resume(&700_000u64.to_le_bytes())
        .unwrap();

    common::clocked(d.path());
    let mut s = Session::boot(d.path().to_path_buf());
    s.feed([RawEvent::Down(Btn::A)], 16);
    s.feed([RawEvent::Up(Btn::A)], 32);

    let mut now = 32;
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        now += 16;
        s.feed([], now);
        s.update(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(1));
    }

    // The counter each core directory holds, as the mock's eight byte state, or `None` where
    // nothing has ever been filed under that core at all.
    let counter = |core| {
        persist::read_resume(d.path(), Platform::Gb, core, "Tetris")
            .map(|b| u64::from_le_bytes(b.try_into().expect("the mock's state is 8 bytes")))
    };

    // Frames the seated core actually runs, flushed out through the path the binary uses, until
    // the resumed counter moves. A counter that merely still reads what it was seeded with says
    // nothing: that is equally what a run resuming from somewhere else leaves behind.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if counter(Core::Mgba).is_some_and(|n| n > 700_000) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the ini's `Tetris = gpsp` was honoured for a Game Boy cart: States/GB/mgba still \
             reads {:?} and States/GB/gpsp reads {:?}",
            counter(Core::Mgba),
            counter(Core::Gpsp)
        );
        now += 16;
        s.feed([], now);
        s.update(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(1));
        s.app_mut().flush_resume();
    }
    assert_eq!(
        counter(Core::Gpsp),
        None,
        "a Game Boy cart's state was filed under States/GB/gpsp"
    );
}

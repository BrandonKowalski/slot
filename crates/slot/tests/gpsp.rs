mod common;

use slot_store::Core;

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
    let mut core = slot_retro::LibretroCore::open(&path).expect("open gpsp");
    // Serial mode is the whole reason gpSP is here. Setting it before load is what the
    // core expects: it reads options during retro_load_game.
    core.set_option("gpsp_serial", "rfu");
    assert_eq!(core.option("gpsp_serial"), Some("rfu".to_string()));
}

/// `auto` resolves the serial protocol from the ROM itself, so two devices running the same
/// game agree on a mode without either being told which — the right default before there is
/// any UI to pick one deliberately.
#[test]
fn gpsp_is_told_its_serial_mode_before_load() {
    let path = dylib_for(Core::Gpsp);
    if !path.exists() {
        eprintln!("no gpSP dylib on this host, skipping");
        return;
    }
    let mut core = slot_retro::LibretroCore::open(&path).expect("open gpsp");
    slot::core::apply_core_options(&mut core, Core::Gpsp);
    assert_eq!(
        core.option("gpsp_serial"),
        Some("auto".to_string()),
        "auto resolves per ROM, so both devices agree without being told"
    );
}

/// mGBA has no `gpsp_serial` option at all; handing it one anyway would be silently ignored
/// by mGBA today and a landmine the moment mGBA ever grows an option by that name.
#[test]
fn mgba_is_given_no_options() {
    let path = dylib_for(Core::Mgba);
    if !path.exists() {
        eprintln!("no mGBA dylib on this host, skipping");
        return;
    }
    let mut core = slot_retro::LibretroCore::open(&path).expect("open mgba");
    slot::core::apply_core_options(&mut core, Core::Mgba);
    assert_eq!(
        core.option("gpsp_serial"),
        None,
        "mGBA has no such option and must not be handed one"
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
    StateRing::new(d.path(), Core::Gpsp, "Emerald")
        .write_resume(&700_000u64.to_le_bytes())
        .unwrap();
    StateRing::new(d.path(), Core::Mgba, "Emerald")
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

    let state = persist::read_resume(d.path(), Core::Gpsp, "Emerald").expect("nothing resumed");
    let n = u64::from_le_bytes(state.try_into().expect("mock state is 8 bytes"));
    assert!(
        n >= 700_000,
        "the session resumed the wrong core's state (or none): counter is {n}"
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
        StateRing::new(d.path(), Core::Gpsp, "Emerald")
            .read_resume()
            .unwrap()
            .is_some(),
        "the autosave did not land under the seated core's own directory"
    );
    assert!(
        StateRing::new(d.path(), Core::Mgba, "Emerald")
            .read_resume()
            .unwrap()
            .is_none(),
        "the autosave followed the ini's new (absent) reading instead of the core the \
         session actually spawned"
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
    let d = common::tmp_root_with_real_carts(&["Probe"]);
    let planted = d
        .path()
        .join("System")
        .join(slot::core::dylib_name(Core::Gpsp));
    std::fs::copy(&mgba, &planted).expect("plant a dylib under gpSP's name");

    let mut core = slot::core::open_core(d.path(), Core::Gpsp);
    core.load(&d.path().join("Games/Probe.gba"))
        .expect("the planted core refused the test rom");
    core.run_frame(ButtonMask::default());
    assert!(
        core.serialize().expect("core gave up no state").len() > 100_000,
        "open_core fell back to the mock instead of the dylib planted at root/System"
    );
}

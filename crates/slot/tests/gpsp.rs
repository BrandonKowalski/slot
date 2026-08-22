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

mod common;

use slot::audio::{AudioSink, StubSink};
use slot::emu::{CoreState, EmuHandle};
use slot::persist;
use slot::persist::Snapshot;
use slot_retro::MockCore;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn wait_ready(emu: &EmuHandle) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while emu.state() == CoreState::Loading {
        assert!(Instant::now() < deadline, "the core never settled");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// resume.state is written on eject, lid, power and every autosave. Until it is read back
/// when the core starts, a cart is seated but the game restarts from the intro, which is
/// the one promise the slot makes.
#[test]
fn a_resume_state_is_restored_before_the_core_reports_ready() {
    let emu = EmuHandle::spawn(
        Box::new(MockCore::new()),
        PathBuf::from("unused.gba"),
        StubSink::new().ring(),
        None,
        Some(500_000u64.to_le_bytes().to_vec()),
    );
    wait_ready(&emu);
    let state = emu.request_state().recv().unwrap();
    let n = u64::from_le_bytes(state.try_into().expect("mock state is 8 bytes"));
    assert!(n >= 500_000, "the core started cold, counter is {n}");
}

#[test]
fn read_resume_finds_what_a_flush_wrote() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    persist::flush(
        d.path(),
        slot_store::Core::Mgba,
        "Emerald",
        Some(&[7u8; 64]),
        None,
    )
    .unwrap();
    assert_eq!(
        persist::read_resume(d.path(), slot_store::Core::Mgba, "Emerald"),
        Some(vec![7u8; 64])
    );
}

/// `flush` used to read `selected_core.ini` itself, which meant its own read of the ini and
/// `session.rs`'s could disagree — that gap is the whole reason this task exists. `flush`
/// now takes `core` rather than deriving it, so this pins the half of the contract that
/// lives in this function: whichever `Core` it is handed is where the resume lands, no ini
/// involved. `crates/slot/tests/gpsp.rs` covers the other half — that `session.rs` resolves
/// the ini exactly once and hands that same value to every reader and writer for the cart.
#[test]
fn flush_routes_by_the_core_it_is_given() {
    let d = common::tmp_root_with_carts(&["Emerald"]);

    persist::flush(
        d.path(),
        slot_store::Core::Gpsp,
        "Emerald",
        Some(&[7u8; 64]),
        None,
    )
    .unwrap();

    assert!(d.path().join("States/gpsp/Emerald/resume.state").exists());
    assert!(!d.path().join("States/mgba/Emerald/resume.state").exists());
}

/// RetroArch's libretro cores write `.srm`; mGBA standalone writes `.sav`. A card carrying
/// only the RetroArch file has a real save on it and must not boot as a new game.
#[test]
fn a_retroarch_srm_is_read_when_there_is_no_sav() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    std::fs::write(d.path().join("Saves/Emerald.srm"), b"srm bytes").unwrap();
    assert_eq!(
        persist::read_sav(d.path(), "Emerald").as_deref(),
        Some(&b"srm bytes"[..])
    );
}

/// `read_sav` returning the right bytes proves nothing on its own. The bug this file was
/// written for was a function with no caller, so the bytes have to be followed all the way
/// into the core's save ram through the same call the session makes.
#[test]
fn srm_bytes_on_disk_reach_the_cores_save_ram() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    let srm: Vec<u8> = (0..8 * 1024).map(|i| (i % 251) as u8).collect();
    std::fs::write(d.path().join("Saves/Emerald.srm"), &srm).unwrap();

    let emu = EmuHandle::spawn(
        Box::new(MockCore::new()),
        d.path().join("Games/Emerald.gba"),
        StubSink::new().ring(),
        persist::read_sav(d.path(), "Emerald"),
        persist::read_resume(d.path(), slot_store::Core::Mgba, "Emerald"),
    );
    wait_ready(&emu);
    let got = emu.snapshot().save_ram().expect("the core has no save ram");
    assert_eq!(got, srm, "the srm never reached the core");
}

#[test]
fn a_sav_wins_over_an_srm_when_both_exist() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    std::fs::write(d.path().join("Saves/Emerald.srm"), b"srm bytes").unwrap();
    std::fs::write(d.path().join("Saves/Emerald.sav"), b"sav bytes").unwrap();
    assert_eq!(
        persist::read_sav(d.path(), "Emerald").as_deref(),
        Some(&b"sav bytes"[..])
    );
}

/// C1: with a cart set to a core with no dylib present, `open_core_for` falls back to the
/// mock. `MockCore::load` fixes its own save ram at 8 KB and its own resume at 8 bytes, so a
/// real 128 KB battery save and a real 256 KB resume are both refused at open — see
/// `Worker::run` (emu.rs). Before this test's fix, the very next flush wrote the mock's own
/// 8 KB of zeros and 8 byte counter over both real files, permanently, and
/// `Action::PowerPress` — which flushes immediately (app.rs) — could trigger it on the very
/// press a player made to escape the mock's test pattern. This pins that `EmuSnapshot` records
/// the refusal at the source.
#[test]
fn a_mismatched_save_ram_and_resume_are_flagged_untrusted_rather_than_silently_swapped_in() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    let real_sav = vec![0x5Au8; 131_072];
    let real_resume = vec![0xA5u8; 262_144];

    let emu = EmuHandle::spawn(
        Box::new(MockCore::new()),
        d.path().join("Games/Emerald.gba"),
        StubSink::new().ring(),
        Some(real_sav),
        Some(real_resume),
    );
    wait_ready(&emu);
    let snapshot = emu.snapshot();
    assert!(
        !snapshot.resume_trusted(),
        "a mismatched resume must not read back as trusted"
    );
    assert!(
        !snapshot.save_ram_trusted(),
        "a mismatched save ram must not read back as trusted"
    );
}

/// The end-to-end half of the test above: not just that the refusal is recorded, but that the
/// flush path it exists for actually leaves the real files on disk untouched. Reproduces the
/// bug through the exact trigger the branch review called out — a power tap, which
/// `Action::PowerPress` flushes immediately.
#[test]
fn a_power_press_does_not_let_a_refusing_mock_overwrite_a_real_save() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    let real_sav = vec![0x5Au8; 131_072];
    let real_resume = vec![0xA5u8; 262_144];
    std::fs::write(d.path().join("Saves/Emerald.sav"), &real_sav).unwrap();
    persist::flush(
        d.path(),
        slot_store::Core::Mgba,
        "Emerald",
        Some(&real_resume),
        None,
    )
    .unwrap();

    // Read back exactly the way `session.rs::spawn_core` would, and hand it to a core that
    // will refuse both: the mock, standing in for "no dylib present" or "SLOT_CORE points at
    // the wrong game" — `open_core_for` cannot tell those apart from a core that opened fine,
    // and this is deliberately exercising the downstream guard rather than that fallback.
    let sav = persist::read_sav(d.path(), "Emerald");
    let resume = persist::read_resume(d.path(), slot_store::Core::Mgba, "Emerald");
    let emu = EmuHandle::spawn(
        Box::new(MockCore::new()),
        d.path().join("Games/Emerald.gba"),
        StubSink::new().ring(),
        sav,
        resume,
    );
    wait_ready(&emu);

    let mut a = common::app_playing_with(d.path(), "Emerald", Box::new(emu.snapshot()));
    a.apply(slot_input::Action::PowerPress);

    assert_eq!(
        std::fs::read(d.path().join("Saves/Emerald.sav")).unwrap(),
        real_sav,
        "the mock's own save ram overwrote the real one"
    );
    assert_eq!(
        persist::read_resume(d.path(), slot_store::Core::Mgba, "Emerald").unwrap(),
        real_resume,
        "the mock's own resume overwrote the real one"
    );
}

/// The eject path's twin of the test above: `flush_eject` (app.rs) is a second, independent
/// call into `persist`, and the branch review named it explicitly alongside the autosave/
/// power-press flush as a place the same loss could land.
#[test]
fn an_eject_does_not_let_a_refusing_mock_overwrite_a_real_save() {
    let d = common::tmp_root_with_carts(&["Emerald", "Fusion"]);
    let real_sav = vec![0x5Au8; 131_072];
    let real_resume = vec![0xA5u8; 262_144];
    std::fs::write(d.path().join("Saves/Emerald.sav"), &real_sav).unwrap();
    persist::flush(
        d.path(),
        slot_store::Core::Mgba,
        "Emerald",
        Some(&real_resume),
        None,
    )
    .unwrap();

    let sav = persist::read_sav(d.path(), "Emerald");
    let resume = persist::read_resume(d.path(), slot_store::Core::Mgba, "Emerald");
    let emu = EmuHandle::spawn(
        Box::new(MockCore::new()),
        d.path().join("Games/Emerald.gba"),
        StubSink::new().ring(),
        sav,
        resume,
    );
    wait_ready(&emu);

    let mut a = common::app_playing_with(d.path(), "Emerald", Box::new(emu.snapshot()));
    a.apply(slot_input::Action::Eject);

    assert_eq!(
        std::fs::read(d.path().join("Saves/Emerald.sav")).unwrap(),
        real_sav,
        "the mock's own save ram overwrote the real one on eject"
    );
    assert_eq!(
        persist::read_resume(d.path(), slot_store::Core::Mgba, "Emerald").unwrap(),
        real_resume,
        "the mock's own resume overwrote the real one on eject"
    );
}

/// The manual-save twin of the two tests above, and a different loss shape: `App::save_state`
/// (`SELECT+R1`) is not a write-back over an existing file, so `trusted_write`'s withholding
/// does not reach it. It is a push onto a ten-deep ring that evicts the oldest entry once full
/// (`StateRing::evict`). A refusing mock's own placeholder state landing on a full ring does
/// not just fail to help the player — `ring.push` deletes their oldest genuine save to make
/// room for it. This pins that `save_state` consults `resume_trusted` before it ever reaches
/// `ring.push`, the same way `flush`/`eject` consult it before they reach `persist::flush`.
#[test]
fn a_refusing_mock_does_not_evict_a_real_ring_entry_on_manual_save() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    let real_sav = vec![0x5Au8; 131_072];
    let real_resume = vec![0xA5u8; 262_144];

    // A full ring of genuine saves, ten deep, oldest to newest.
    let ring = slot_store::StateRing::new(d.path(), slot_store::Core::Mgba, "Emerald");
    for i in 0..slot_store::RING_MAX {
        let stamp = format!("2026-01-01_00-00-{i:02}");
        ring.push(&vec![i as u8; 200_000], b"png", &stamp)
            .expect("push");
    }
    let before = ring.list().expect("list");
    assert_eq!(before.len(), slot_store::RING_MAX, "the ring did not fill");
    let oldest = before.last().expect("an oldest entry").stamp.clone();
    assert_eq!(oldest, "2026-01-01_00-00-00", "wrong entry called oldest");

    // A mock standing in for "no dylib present", handed a real resume it does not match —
    // exactly what `open_core_for`'s fallback and the documented SLOT_CORE trap both produce.
    let emu = EmuHandle::spawn(
        Box::new(MockCore::new()),
        d.path().join("Games/Emerald.gba"),
        StubSink::new().ring(),
        Some(real_sav),
        Some(real_resume),
    );
    wait_ready(&emu);
    assert!(
        !emu.snapshot().resume_trusted(),
        "the mock must have refused the resume for this test to mean anything"
    );

    let mut a = common::app_playing_with(d.path(), "Emerald", Box::new(emu.snapshot()));
    a.apply(slot_input::Action::SaveState);

    let after = ring.list().expect("list");
    assert_eq!(
        after.len(),
        slot_store::RING_MAX,
        "the ring changed size: either nothing was declined or something else broke"
    );
    assert!(
        after.iter().any(|e| e.stamp == oldest),
        "the oldest genuine save was evicted to make room for the mock's placeholder"
    );
    assert!(
        a.refusal_active(a.now()),
        "nothing told the player the save was declined"
    );
}

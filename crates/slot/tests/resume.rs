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
    persist::flush(d.path(), "Emerald", &[7u8; 64], None).unwrap();
    assert_eq!(
        persist::read_resume(d.path(), "Emerald"),
        Some(vec![7u8; 64])
    );
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
        persist::read_resume(d.path(), "Emerald"),
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

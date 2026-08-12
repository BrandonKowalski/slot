mod common;

use common::tmp_root;
use slot_store::{atomic_write, read_slot_state, write_slot_state, SlotState};
use tempfile::tempdir;

#[test]
fn atomic_write_leaves_no_partial_file_and_no_temp_behind() {
    let d = tempdir().unwrap();
    let p = d.path().join("x.bin");
    atomic_write(&p, b"first").unwrap();
    atomic_write(&p, &vec![7u8; 4_000_000]).unwrap();
    assert_eq!(std::fs::read(&p).unwrap().len(), 4_000_000);
    let strays: Vec<_> = std::fs::read_dir(d.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name() != "x.bin")
        .collect();
    assert!(strays.is_empty(), "temp files left behind: {strays:?}");
}

#[test]
fn corrupt_slot_state_reads_as_default_rather_than_panicking() {
    let d = tmp_root();
    std::fs::write(d.path().join("System/slot.state"), b"\x00\xff not json").unwrap();
    assert_eq!(read_slot_state(d.path()), SlotState::default());
}

#[test]
fn slot_state_round_trips_including_a_stem_with_an_equals_sign() {
    let d = tmp_root();
    let s = SlotState {
        cart: Some("Cheats = On".into()),
        brightness: 3,
        blue_light: 9,
        volume: 71,
        muted: true,
        clock_set: true,
        utc_offset_min: 0,
    };
    write_slot_state(d.path(), &s).unwrap();
    assert_eq!(read_slot_state(d.path()), s);
}

#[test]
fn a_slot_state_missing_a_key_reads_as_default_not_half_populated() {
    let d = tmp_root();
    std::fs::write(
        d.path().join("System/slot.state"),
        "cart=Emerald\nbrightness=3\nblue_light=1\n",
    )
    .unwrap();
    assert_eq!(read_slot_state(d.path()), SlotState::default());
}

#[test]
fn an_out_of_range_level_reads_as_default() {
    let d = tmp_root();
    for body in [
        "cart=\nbrightness=10\nblue_light=1\nvolume=50\n",
        "cart=\nbrightness=3\nblue_light=10\nvolume=50\n",
        "cart=\nbrightness=3\nblue_light=1\nvolume=101\n",
    ] {
        std::fs::write(d.path().join("System/slot.state"), body).unwrap();
        assert_eq!(
            read_slot_state(d.path()),
            SlotState::default(),
            "accepted {body:?}"
        );
    }
}

#[test]
fn a_first_boot_is_neither_dark_nor_silent() {
    let d = tmp_root();
    let s = read_slot_state(d.path());
    assert!(s.cart.is_none());
    assert!(s.brightness > 0, "boots with the backlight off");
    assert!(s.volume > 0, "boots muted");
}

/// The offset is what turns the card's UTC into the time on the shelf, so it has to outlive
/// the session that chose it.
#[test]
fn slot_state_round_trips_a_negative_utc_offset() {
    let d = tmp_root();
    let s = SlotState {
        cart: None,
        brightness: 5,
        blue_light: 0,
        volume: 60,
        muted: false,
        clock_set: true,
        utc_offset_min: -450,
    };
    write_slot_state(d.path(), &s).unwrap();
    assert_eq!(read_slot_state(d.path()).utc_offset_min, -450);
}

/// Half hour zones are real and whole hour steps would put several countries permanently
/// thirty minutes out.
#[test]
fn an_offset_outside_the_range_of_real_zones_reads_as_default() {
    let d = tmp_root();
    for body in [
        "cart=\nbrightness=5\nblue_light=0\nvolume=60\nmuted=0\nclock_set=1\nutc_offset_min=900\n",
        "cart=\nbrightness=5\nblue_light=0\nvolume=60\nmuted=0\nclock_set=1\nutc_offset_min=-780\n",
    ] {
        std::fs::write(d.path().join("System/slot.state"), body).unwrap();
        assert_eq!(read_slot_state(d.path()), SlotState::default(), "{body}");
    }
}

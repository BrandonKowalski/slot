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
        ..SlotState::default()
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
        ..SlotState::default()
    };
    write_slot_state(d.path(), &s).unwrap();
    assert_eq!(read_slot_state(d.path()).utc_offset_min, -450);
}

/// A later build writes lines this one has never heard of. Throwing the whole file away over
/// one of them would reset the levels and ask for the clock again on every trip back.
#[test]
fn a_line_the_reader_does_not_know_is_skipped() {
    let d = tmp_root();
    std::fs::write(
        d.path().join("System/slot.state"),
        "cart=Emerald\nbrightness=3\nfrom_a_later_build=7\nblue_light=1\nvolume=40\nmuted=1\n\
         no equals sign at all\nclock_set=1\nutc_offset_min=-300\n",
    )
    .unwrap();
    let s = read_slot_state(d.path());
    assert_eq!(
        s.cart.as_deref(),
        Some("Emerald"),
        "the file was thrown away"
    );
    assert_eq!(
        (
            s.brightness,
            s.blue_light,
            s.volume,
            s.muted,
            s.clock_set,
            s.utc_offset_min
        ),
        (3, 1, 40, true, true, -300)
    );
}

/// What slot did before any of these were settings: the motor on, fast forward at four times
/// and silent. A card that has never been asked goes on doing exactly that.
#[test]
fn a_first_boot_rumbles_and_fast_forwards_silently_at_four() {
    let s = SlotState::default();
    assert!(s.rumble, "boots with the motor off");
    assert_eq!(s.ff_speed, 4);
    assert!(!s.ff_sound, "boots with fast forward audible");
}

/// Every card written before the quick menu has none of its lines. The values that are there
/// have to survive the upgrade, and the missing ones read as what slot already did.
#[test]
fn a_card_from_before_the_settings_keeps_all_its_values() {
    let d = tmp_root();
    std::fs::write(
        d.path().join("System/slot.state"),
        "cart=Emerald\nbrightness=3\nblue_light=1\nvolume=40\nmuted=1\nclock_set=1\nutc_offset_min=-300\n",
    )
    .unwrap();
    assert_eq!(
        read_slot_state(d.path()),
        SlotState {
            cart: Some("Emerald".into()),
            brightness: 3,
            blue_light: 1,
            volume: 40,
            muted: true,
            clock_set: true,
            utc_offset_min: -300,
            rumble: true,
            ff_speed: 4,
            ff_sound: false,
        }
    );
}

#[test]
fn the_quick_menu_settings_round_trip_as_their_own_lines() {
    let d = tmp_root();
    let s = SlotState {
        clock_set: true,
        rumble: false,
        ff_speed: 2,
        ff_sound: true,
        ..SlotState::default()
    };
    write_slot_state(d.path(), &s).unwrap();
    assert_eq!(read_slot_state(d.path()), s);
    let text = std::fs::read_to_string(d.path().join("System/slot.state")).unwrap();
    for line in ["rumble=0", "ff_speed=2", "ff_sound=1"] {
        assert!(text.lines().any(|l| l == line), "no {line} in {text:?}");
    }
}

/// A setting nobody could have chosen goes back to its default on its own. It is not a reason
/// to disbelieve the brightness, the volume or the clock beside it.
#[test]
fn an_out_of_range_setting_falls_back_to_its_default() {
    let d = tmp_root();
    let known = "cart=Emerald\nbrightness=3\nblue_light=1\nvolume=40\nmuted=0\nclock_set=1\nutc_offset_min=0\n";
    for bad in [
        "rumble=2\nff_speed=5\nff_sound=9\n",
        "rumble=\nff_speed=1\nff_sound=on\n",
        "rumble=-1\nff_speed=0\nff_sound=-1\n",
        "ff_speed=x\n",
    ] {
        std::fs::write(d.path().join("System/slot.state"), format!("{known}{bad}")).unwrap();
        let s = read_slot_state(d.path());
        assert_eq!(
            (s.rumble, s.ff_speed, s.ff_sound),
            (true, 4, false),
            "accepted {bad:?}"
        );
        assert_eq!(
            (s.cart.as_deref(), s.brightness, s.volume, s.clock_set),
            (Some("Emerald"), 3, 40, true),
            "{bad:?} took the rest of the card with it"
        );
    }
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

mod common;

use common::tmp_root;
use slot_store::{
    atomic_write, read_slot_state, write_slot_state, SlotState, FF_SPEEDS, FF_SPEED_DEFAULT,
};
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

/// A name the card accepts must be a name this can write. The temp file is longer than the
/// name it stands in for — a dot, the process id, a sequence and `.tmp` — so a cart named up to
/// the filesystem's limit used to be listed, inserted and played while every write of its
/// battery save failed with `File name too long`, silently, every session. The state ring was
/// untouched, because only the directory there carries the stem, so such a cart resumed
/// perfectly and never once saved.
///
/// Walked up to the limit rather than tested at it, because the exact length that first failed
/// is the filesystem's business and the property is that none of them does.
#[test]
fn a_name_the_card_accepts_is_a_name_that_can_be_written() {
    let d = tempdir().unwrap();
    for len in [8usize, 100, 200, 239, 240, 245, 250, 251] {
        let p = d.path().join(format!("{}.sav", "a".repeat(len)));
        // The control: a plain write proves the filesystem really takes this name, so a
        // failure below is this function's and not the limit moving under the test.
        if std::fs::write(&p, b"control").is_err() {
            continue;
        }
        atomic_write(&p, b"save")
            .unwrap_or_else(|e| panic!("{len} character name: {e}, and a plain write took it"));
        assert_eq!(std::fs::read(&p).unwrap(), b"save");
    }
}

/// A multi-byte name is cut on a character boundary or not at all. Slicing one in half is a
/// panic, and the name reaching here is whatever the card spells — a Japanese rom title is three
/// bytes a character, so a legal name is a long one in bytes.
///
/// Three paddings, because where the cut falls depends on how many digits this process's own id
/// has. Shifting the run of three-byte characters by one and then two bytes puts the cut inside
/// one of them for at least one of the three, whatever that id turns out to be.
#[test]
fn a_long_multibyte_name_is_written_rather_than_panicking() {
    let d = tempdir().unwrap();
    for pad in 0..3 {
        let p = d
            .path()
            .join(format!("{}{}.sav", "x".repeat(pad), "ポ".repeat(82)));
        if std::fs::write(&p, b"control").is_err() {
            continue;
        }
        atomic_write(&p, b"save")
            .unwrap_or_else(|e| panic!("a Japanese cart name, pad {pad}: {e}"));
        assert_eq!(std::fs::read(&p).unwrap(), b"save");
    }
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

/// What a card that has never been asked gets: the motor on, fast forward at the default,
/// silent, and the picture the core's own colours. The speed is the one constant here that has
/// moved — it was four, inherited from when gpSP ran its interpreter and could not serve more,
/// and is now six, chosen on the device.
///
/// Colour correction off is not an aesthetic preference being enshrined: it is what every card
/// already renders as, because both cores default their own option off and slot never set it.
/// A default of on would change the look of every existing library on the strength of an
/// update nobody asked for.
#[test]
fn a_first_boot_rumbles_and_fast_forwards_silently_at_the_default() {
    let s = SlotState::default();
    assert!(s.rumble, "boots with the motor off");
    assert_eq!(s.ff_speed, FF_SPEED_DEFAULT);
    assert!(!s.ff_sound, "boots with fast forward audible");
    assert!(!s.colour_correction, "boots with the picture tinted");
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
            ff_speed: FF_SPEED_DEFAULT,
            ff_sound: false,
            colour_correction: false,
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
        colour_correction: true,
        ..SlotState::default()
    };
    write_slot_state(d.path(), &s).unwrap();
    assert_eq!(read_slot_state(d.path()), s);
    let text = std::fs::read_to_string(d.path().join("System/slot.state")).unwrap();
    for line in [
        "rumble=0",
        "ff_speed=2",
        "ff_sound=1",
        "colour_correction=1",
    ] {
        assert!(text.lines().any(|l| l == line), "no {line} in {text:?}");
    }
}

/// Every speed the row offers travels in `ff_speed` itself, so each of the four has to survive a
/// round trip through the card and be written as the plain number it is.
#[test]
fn every_speed_the_row_offers_round_trips_as_its_own_number() {
    for speed in FF_SPEEDS {
        let d = tmp_root();
        let s = SlotState {
            clock_set: true,
            ff_speed: speed,
            ..SlotState::default()
        };
        write_slot_state(d.path(), &s).unwrap();
        assert_eq!(read_slot_state(d.path()), s, "{speed}x did not come back");
        let text = std::fs::read_to_string(d.path().join("System/slot.state")).unwrap();
        let want = format!("ff_speed={speed}");
        assert!(text.lines().any(|l| l == want), "no {want} in {text:?}");
    }
}

/// What an older build does with the speed it never had, pinned rather than assumed. Every slot
/// that shipped before this one reads this line as "a number from 2 to 4, anything else is not
/// mine", so a card written here at 6 falls back to that build's own default on it — acceptable,
/// and the reason 6 must stay outside 2..=4 rather than, say, the row growing a 5 that an older
/// build would read as a speed the user never chose.
///
/// The default is deliberately *not* that any more. It was 4x, held inside 2..=4 so the common
/// card read identically everywhere; it is now 6x, chosen on the device, and a fresh card reads
/// as 4x on a build that predates this row — the same fallback the two new speeds already take.
/// The property kept here is the one that still earns its place: every speed on the row either
/// reads as itself on an older build or falls back to that build's own default, and none of
/// them reads as a *different* speed the player never chose.
#[test]
fn an_older_build_reads_the_new_speed_as_its_own_default() {
    for speed in FF_SPEEDS.iter().filter(|&&n| n > 4) {
        assert!(
            !(2..=4).contains(speed),
            "{speed}x is inside the range an older build accepts, so it would read as a speed"
        );
    }
    assert_eq!(SlotState::default().ff_speed, FF_SPEED_DEFAULT);
    assert!(
        FF_SPEEDS.contains(&FF_SPEED_DEFAULT),
        "the default is not one of the speeds the row offers"
    );
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
        // Colour correction's own line, unreadable the same three ways. `Auto` is what the row
        // hands mGBA, and is exactly the sort of thing a hand-edited card might end up holding
        // here — it is not one of this line's two values and reads as the default.
        "colour_correction=2\n",
        "colour_correction=\n",
        "colour_correction=Auto\n",
        // Inside the row's ends but not on it: the row steps 4 to 6.
        "ff_speed=5\n",
        "ff_speed=7\n",
        // 8, which a card written on the night the row briefly had five ceilings still holds,
        // and 255, which one written in the adaptive era does. Both fall back the same way.
        "ff_speed=8\n",
        "ff_speed=255\n",
    ] {
        std::fs::write(d.path().join("System/slot.state"), format!("{known}{bad}")).unwrap();
        let s = read_slot_state(d.path());
        assert_eq!(
            (s.rumble, s.ff_speed, s.ff_sound, s.colour_correction),
            (true, FF_SPEED_DEFAULT, false, false),
            "accepted {bad:?}"
        );
        assert_eq!(
            (s.cart.as_deref(), s.brightness, s.volume, s.clock_set),
            (Some("Emerald"), 3, 40, true),
            "{bad:?} took the rest of the card with it"
        );
    }
}

/// The builds that ran Game Boy carts too wrote which folder the seated cart came from. A card
/// left with a Game Boy cart in the slot must come up on the shelf, not resume a GBA cart that
/// shares its stem; one that says `gba`, in any case, or nothing, is the cart it names.
#[test]
fn a_card_left_holding_a_game_boy_cart_comes_up_empty() {
    let d = tmp_root();
    let known = "cart=Tetris\nbrightness=3\nblue_light=1\nvolume=40\nmuted=0\nclock_set=1\nutc_offset_min=0\n";
    for (line, want) in [
        ("", Some("Tetris")),
        ("cart_platform=\n", Some("Tetris")),
        ("cart_platform=gba\n", Some("Tetris")),
        ("cart_platform=GBA\n", Some("Tetris")),
        ("cart_platform=gb\n", None),
        ("cart_platform=gbc\n", None),
    ] {
        std::fs::write(d.path().join("System/slot.state"), format!("{known}{line}")).unwrap();
        let s = read_slot_state(d.path());
        assert_eq!(s.cart.as_deref(), want, "read {line:?} wrong");
        assert_eq!(
            (s.brightness, s.volume, s.clock_set),
            (3, 40, true),
            "{line:?} took the rest of the card with it"
        );
    }
}

/// Nothing this build writes names a platform: there is only the one.
#[test]
fn the_state_file_no_longer_names_a_platform() {
    let d = tmp_root();
    let s = SlotState {
        cart: Some("Emerald".into()),
        clock_set: true,
        ..SlotState::default()
    };
    write_slot_state(d.path(), &s).unwrap();
    let text = std::fs::read_to_string(d.path().join("System/slot.state")).unwrap();
    assert!(!text.contains("cart_platform"), "{text:?}");
    assert_eq!(read_slot_state(d.path()), s);
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

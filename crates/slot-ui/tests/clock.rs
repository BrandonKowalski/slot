use slot_ui::{clock_label, time_text, Polaroids};

#[test]
fn recent_states_read_relative_and_old_ones_read_absolute() {
    let now = "2026-08-09_14-36-05";
    assert_eq!(
        Polaroids::relative_time("2026-08-09_14-32-05", now, true),
        "4 min ago"
    );
    assert_eq!(
        Polaroids::relative_time("2026-08-09_03-00-00", now, true),
        "11 hr ago"
    );
    assert_eq!(
        Polaroids::relative_time("2026-08-08_20-00-00", now, true),
        "2026-08-08 20:00"
    );
    assert_eq!(
        Polaroids::relative_time("2025-01-02_09-05-00", now, true),
        "2025-01-02 09:05"
    );
}

#[test]
fn an_old_state_reads_on_the_12_hour_clock_when_that_is_picked() {
    assert_eq!(
        Polaroids::relative_time("2026-08-08_20-00-00", "2026-08-09_14-36-05", false),
        "2026-08-08 8:00 PM"
    );
}

#[test]
fn the_clock_is_24_hour_and_shows_no_seconds() {
    assert_eq!(
        clock_label("2026-08-09_21-07-00"),
        "21:07",
        "not 24 hour, or showing seconds"
    );
}

#[test]
fn the_carousel_clock_reads_on_either_hour_clock() {
    let at = |stamp: &str| slot_store::parse_stamp(stamp).expect("a stamp");
    assert_eq!(time_text(at("2026-08-09_21-07-00"), true), "21:07");
    assert_eq!(time_text(at("2026-08-09_21-07-00"), false), "9:07 PM");
    assert_eq!(time_text(at("2026-08-09_00-30-00"), false), "12:30 AM");
    assert_eq!(time_text(at("2026-08-09_12-00-00"), false), "12:00 PM");
}

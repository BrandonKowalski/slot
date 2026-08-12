mod common;

use common::{app_booting_at, app_booting_with_clock, tmp_root_with_carts};
use slot::app::{App, Phase};
use slot_input::{Action, Btn};
use slot_store::{read_slot_state, write_slot_state, SlotState};
use slot_ui::{ClockPicker, Field};

#[test]
fn the_clock_is_asked_for_once_and_only_once() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let a = App::boot(d.path());
    assert!(
        matches!(a.phase(), Phase::SetClock { .. }),
        "not asked on first launch"
    );

    let mut a = App::boot(d.path());
    a.confirm_clock();
    assert!(read_slot_state(d.path()).clock_set);
    let again = App::boot(d.path());
    assert!(
        !matches!(again.phase(), Phase::SetClock { .. }),
        "asked twice"
    );
}

/// It outranks a seated cart. Resuming into a game whose rtc is wrong is the failure this
/// screen exists to prevent.
#[test]
fn the_clock_comes_before_a_seated_cart() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    write_slot_state(
        d.path(),
        &SlotState {
            cart: Some("Emerald".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let a = App::boot(d.path());
    assert!(matches!(a.phase(), Phase::SetClock { .. }));
}

#[test]
fn confirming_writes_the_picked_time_to_the_platform() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let (mut a, clock) = app_booting_with_clock(d.path());
    a.apply(Action::GbaDown(Btn::Up)); // one step on the field under the cursor
    a.confirm_clock();
    assert_ne!(clock.get(), 0, "the platform clock was never set");
}

/// A device whose rtc is already right is confirmed, not typed in. Anything else would make
/// the screen data entry every time a battery is changed.
#[test]
fn the_picker_starts_from_the_clock_the_platform_already_has() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let (mut a, clock) = app_booting_at(d.path(), 1_700_000_000);
    a.confirm_clock();
    assert_eq!(clock.get(), 1_700_000_000 - 1_700_000_000 % 60);
}

#[test]
fn a_field_wraps_rather_than_running_off_the_end() {
    let mut c = ClockPicker::from_secs(0); // 1970-01-01 00:00
    c.field(Field::Month);
    for _ in 0..13 {
        c.up();
    }
    assert!(
        (1..=12).contains(&c.month()),
        "month {} left the calendar",
        c.month()
    );
}

#[test]
fn february_29_is_reachable_in_a_leap_year_and_not_otherwise() {
    let mut c = ClockPicker::from_ymd(2028, 2, 28);
    c.field(Field::Day);
    c.up();
    assert_eq!(c.day(), 29, "2028 is a leap year");
    let mut c = ClockPicker::from_ymd(2027, 2, 28);
    c.field(Field::Day);
    c.up();
    assert_eq!(c.day(), 1, "2027 has no 29th of February");
}

/// The day is picked before the month as often as after it, and a 31st carried into
/// February would confirm a date that does not exist.
#[test]
fn a_day_the_new_month_does_not_have_is_pulled_back() {
    let mut c = ClockPicker::from_ymd(2026, 1, 31);
    c.field(Field::Month);
    c.up();
    assert_eq!((c.month(), c.day()), (2, 28));
}

/// The card keeps UTC because the base system's clock and its ntp both assume it. What the
/// screen asks for is the time on the wall in front of you, so the offset is what the picker
/// takes back off before it hands over an epoch.
#[test]
fn the_picker_hands_back_utc_rather_than_what_was_typed() {
    let mut c = ClockPicker::from_ymd(2026, 8, 12);
    c.field(Field::Hour);
    for _ in 0..9 {
        c.up();
    }
    let typed = c.secs();
    c.field(Field::Offset);
    // Ten half hour steps down is UTC-5, which is what most of the eastern seaboard runs on.
    for _ in 0..10 {
        c.down();
    }
    assert_eq!(c.offset_min(), -300);
    assert_eq!(
        c.secs(),
        typed + 300 * 60,
        "09:00 local at UTC-5 is 14:00 UTC"
    );
}

#[test]
fn the_offset_steps_in_half_hours_because_real_zones_do() {
    let mut c = ClockPicker::from_ymd(2026, 8, 12);
    c.field(Field::Offset);
    c.up();
    assert_eq!(c.offset_min(), 30);
    c.up();
    assert_eq!(c.offset_min(), 60);
}

/// Walking off either end would offer zones no country keeps.
#[test]
fn the_offset_stops_at_the_ends_of_the_real_range() {
    let mut c = ClockPicker::from_ymd(2026, 8, 12);
    c.field(Field::Offset);
    for _ in 0..40 {
        c.up();
    }
    assert_eq!(c.offset_min(), 840, "walked past UTC+14");
    for _ in 0..80 {
        c.down();
    }
    assert_eq!(c.offset_min(), -720, "walked past UTC-12");
}

/// The binary re-rasterises the line when this changes, so an offset the text does not
/// mention is one the screen never shows moving.
#[test]
fn the_offset_is_part_of_the_line_of_type() {
    let mut c = ClockPicker::from_ymd(2026, 8, 12);
    let before = c.text();
    c.field(Field::Offset);
    c.down();
    assert_ne!(c.text(), before);
    assert!(c.text().contains("-00:30"), "{}", c.text());
}

/// The offset is chosen on the clock screen and read by everything that prints a time, so it
/// has to reach the card rather than living for the one session that picked it.
#[test]
fn confirming_persists_the_offset_and_sets_the_platform_to_utc() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let (mut a, clock) = app_booting_at(d.path(), 1_700_000_000);
    let typed = a.picker().expect("not on the clock screen").secs();
    for _ in 0..8 {
        a.apply(Action::GbaDown(Btn::Right));
    }
    for _ in 0..4 {
        a.apply(Action::GbaDown(Btn::Down));
    }
    a.confirm_clock();
    assert_eq!(read_slot_state(d.path()).utc_offset_min, -120);
    assert_eq!(
        clock.get(),
        typed + 120 * 60,
        "the platform was set to local rather than to utc"
    );
}

/// Everything user facing reads the wall clock: the shelf, the polaroid captions and the
/// stamps the states are named by. One of them showing utc and another local would be worse
/// than both showing utc.
#[test]
fn the_wall_clock_is_local_rather_than_the_utc_the_card_keeps() {
    let d = tmp_root_with_carts(&["Emerald"]);
    write_slot_state(
        d.path(),
        &SlotState {
            clock_set: true,
            utc_offset_min: -300,
            ..SlotState::default()
        },
    )
    .unwrap();
    let (a, _clock) = app_booting_at(d.path(), 1_700_000_000);
    assert_eq!(a.wall_secs(), 1_700_000_000 - 300 * 60);
}

/// An RTC that lost power sets its fault flag, the kernel refuses every read, and the system
/// clock comes up at the epoch. The one screen that can put it right is gated on a flag that
/// is already set by then, so without this the clock is wrong for good and there is no way
/// back to it.
#[test]
fn a_clock_that_never_got_set_is_asked_for_again() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    write_slot_state(
        d.path(),
        &SlotState {
            clock_set: true,
            ..SlotState::default()
        },
    )
    .unwrap();
    let (a, _clock) = app_booting_at(d.path(), 0);
    assert!(
        matches!(a.phase(), Phase::SetClock { .. }),
        "a 1970 clock was taken at face value: {:?}",
        a.phase()
    );
}

/// Only when it is obviously wrong. A clock that reads like a real date is the user's, and
/// asking again every boot would make a one-time screen into a settings prompt.
#[test]
fn a_clock_that_looks_like_a_real_date_is_not_asked_for_again() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    write_slot_state(
        d.path(),
        &SlotState {
            clock_set: true,
            ..SlotState::default()
        },
    )
    .unwrap();
    let (a, _clock) = app_booting_at(d.path(), 1_786_568_000);
    assert!(
        !matches!(a.phase(), Phase::SetClock { .. }),
        "asked again for a clock that was already right"
    );
}

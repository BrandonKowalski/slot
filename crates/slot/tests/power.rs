mod common;

use std::sync::atomic::Ordering;
use std::time::Duration;

use common::{app_playing_in, boot, panel, tmp_root_with_carts, StubSnapshot};
use slot::app::Phase;
use slot_input::Action;
use slot_store::{read_slot_state, write_slot_state, SlotState, StateRing};

#[test]
fn lid_close_flushes_resume_before_dozing() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::LidClose);
    let r = StateRing::new(d.path(), "Emerald");
    assert!(
        r.read_resume().unwrap().is_some(),
        "state must be durable before doze"
    );
    assert!(matches!(a.phase(), Phase::Doze { .. }));
    assert!(
        r.list().unwrap().is_empty(),
        "lid close must not create a polaroid"
    );
}

#[test]
fn lid_open_returns_to_the_game_without_a_button() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::LidClose);
    a.apply(Action::LidOpen);
    assert!(matches!(a.phase(), Phase::Playing { .. }));
}

#[test]
fn doze_timeout_powers_off_with_the_cart_still_seated() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::LidClose);
    a.on_doze_timeout();
    assert_eq!(
        read_slot_state(d.path()).cart,
        Some("Emerald".into()),
        "power off is not an eject"
    );
}

/// Closing the lid on an empty slot is still a doze. There is nothing to flush and nothing
/// to wake back into.
#[test]
fn lid_close_on_the_shelf_wakes_back_to_the_shelf() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = boot(d.path());
    a.set_snapshot(StubSnapshot::boxed());
    a.apply(Action::LidClose);
    assert!(matches!(a.phase(), Phase::Doze { cart: None }));
    assert!(StateRing::new(d.path(), "Emerald")
        .read_resume()
        .unwrap()
        .is_none());
    a.apply(Action::LidOpen);
    assert!(matches!(a.phase(), Phase::Shelf));
}

/// The switcher is a pause over the game, so the lid closes on the game underneath it and
/// opens back onto it.
#[test]
fn lid_close_over_the_switcher_wakes_into_the_game() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::SaveState);
    a.apply(Action::Polaroids);
    a.apply(Action::LidClose);
    a.apply(Action::LidOpen);
    assert!(matches!(a.phase(), Phase::Playing { .. }));
    assert_eq!(
        StateRing::new(d.path(), "Emerald").list().unwrap().len(),
        1,
        "only the deliberate save belongs in the ring"
    );
}

/// A dozing app is still ticking, which is the only clock the timeout has.
#[test]
fn a_doze_that_outlasts_the_timeout_powers_off_by_itself() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.set_power(panel(d.path(), Duration::from_secs(2)).0);
    a.apply(Action::LidClose);
    for _ in 0..100 {
        a.update(1.0 / 60.0);
    }
    assert!(!a.powering_off(), "1.6 s is short of the 2 s timeout");
    for _ in 0..40 {
        a.update(1.0 / 60.0);
    }
    assert!(a.powering_off());
}

#[test]
fn a_stray_doze_timeout_does_not_power_off_a_running_game() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.on_doze_timeout();
    assert!(!a.powering_off());
    assert!(matches!(a.phase(), Phase::Playing { .. }));
}

/// The panel comes up at the level the card remembers, not at whatever the kernel left it.
#[test]
fn the_backlight_follows_brightness_from_boot() {
    let d = tmp_root_with_carts(&["Emerald"]);
    write_slot_state(
        d.path(),
        &SlotState {
            brightness: 3,
            clock_set: true,
            utc_offset_min: 0,
            ..Default::default()
        },
    )
    .unwrap();
    let mut a = boot(d.path());
    let (power, step) = panel(d.path(), Duration::from_secs(60));
    a.set_power(power);
    assert_eq!(step.load(Ordering::Relaxed), 3);
    a.apply(Action::BrightnessUp);
    assert_eq!(step.load(Ordering::Relaxed), 4);
}

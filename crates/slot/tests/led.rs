mod common;

use std::sync::atomic::Ordering;

use common::{
    app_playing_in, app_playing_with_charge, app_playing_with_led, led_code, tmp_root_with_carts,
};
use slot_input::{Action, Btn};
use slot_power::LedState;

/// The final whole-branch review found that deleting `power.set_led(state)` from the fast
/// tick left every one of the (then) 504 tests green: both fakes recorded nothing, and the
/// only slot-side LED test called the pure `App::led_state` accessor, which needs no
/// platform at all. This is the seam that closes — it watches the platform's own idea of
/// what it was last told, not just what the app computed.
#[test]
fn the_fast_tick_actually_reaches_the_platforms_set_led() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let (mut a, charge, percent, led, _writes) = app_playing_with_led(d.path(), "Emerald");
    charge.store(2, Ordering::Relaxed); // Charging
    percent.store(50, Ordering::Relaxed);
    a.tick_ms(5_000);
    assert_eq!(
        led.load(Ordering::Relaxed),
        led_code(LedState::Charging),
        "led_state computed a value the platform never heard about"
    );
}

/// The governing invariant of the whole feature is that a device where `status` never
/// populates behaves exactly as it did before any of this was added — which for the LED
/// means `Low` and `Running` are the *only* two states ever reachable, since neither needs a
/// charge reading. Both were untested before this file existed.
#[test]
fn a_full_battery_reads_charged_not_running() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let (mut a, charge, percent) = app_playing_with_charge(d.path(), "Emerald");
    charge.store(3, Ordering::Relaxed); // Full
    percent.store(100, Ordering::Relaxed);
    a.tick_ms(10_000);
    assert_eq!(a.led_state(), LedState::Charged);
}

#[test]
fn a_flat_battery_that_is_not_charging_reads_low() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let (mut a, charge, percent) = app_playing_with_charge(d.path(), "Emerald");
    charge.store(1, Ordering::Relaxed); // Discharging
    percent.store(10, Ordering::Relaxed);
    a.tick_ms(10_000);
    assert_eq!(a.led_state(), LedState::Low);
}

/// The threshold is documented (`app.rs`'s `BATTERY_LOW`) as 20, and this pins the actual
/// number rather than reading the constant back at itself: a test that asked the source for
/// its own threshold and then probed exactly there would still pass if the threshold's value
/// changed, since both sides of the comparison would move together. Only a literal on the
/// test's side of the line can tell "the comparison broke" apart from "the threshold moved."
#[test]
fn the_low_threshold_is_twenty_percent_not_just_a_number_comfortably_below_it() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let (mut a, charge, percent) = app_playing_with_charge(d.path(), "Emerald");
    charge.store(1, Ordering::Relaxed); // Discharging

    percent.store(20, Ordering::Relaxed);
    a.tick_ms(10_000);
    assert_eq!(
        a.led_state(),
        LedState::Low,
        "20% is still at the threshold"
    );

    percent.store(21, Ordering::Relaxed);
    a.tick_ms(20_000);
    assert_eq!(
        a.led_state(),
        LedState::Running,
        "21% is one point above the threshold"
    );
}

/// A device slot has not been ported to yet, or has booted ahead of the first slow tick,
/// still has to show *something* on a case with no LED node to have found either — the same
/// case the governing invariant covers for the gauge and the power-off policy. Green, not
/// dark: `Off` is what the platform is told on the way to a real shutdown, and a device that
/// has not read a battery yet is not shutting down.
#[test]
fn no_reading_yet_reads_running_not_off() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let a = app_playing_in(d.path(), "Emerald"); // no platform attached at all
    assert_eq!(a.led_state(), LedState::Running);
}

/// `motor_change` exists because a rumble strength asked for every frame is not a write worth
/// making every frame; the LED's own fast tick recomputes a state every second whether or not
/// it moved, for a write nobody has confirmed is safe to hammer on the real node. Ten
/// unchanged seconds should be one write, not ten.
#[test]
fn the_led_is_written_once_per_change_not_once_per_tick() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let (mut a, charge, percent, _led, writes) = app_playing_with_led(d.path(), "Emerald");
    charge.store(1, Ordering::Relaxed); // Discharging
    percent.store(50, Ordering::Relaxed); // above BATTERY_LOW, so this reads Running
    a.tick_ms(2_000);
    let after_first = writes.load(Ordering::Relaxed);
    assert!(
        after_first > 0,
        "the very first tick never reached the platform at all"
    );
    // Ten more seconds of the same unchanged reading, one tick per simulated second — the
    // fast tick's own cadence.
    for extra_s in 1..=10 {
        a.tick_ms(2_000 + extra_s * 1_000);
    }
    assert_eq!(
        writes.load(Ordering::Relaxed),
        after_first,
        "an unchanged LED state kept writing to the platform anyway"
    );
}

/// A device shutting down is still holding a lit case in someone's hand until `poweroff`
/// actually cuts power. `Off` is the last thing the platform hears rather than whatever the
/// charge state happened to compute a moment before the button was held.
#[test]
fn power_off_leaves_the_led_off_rather_than_lit_through_shutdown() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let (mut a, charge, percent, led, _writes) = app_playing_with_led(d.path(), "Emerald");
    charge.store(1, Ordering::Relaxed); // Discharging, so the LED is lit green beforehand
    percent.store(50, Ordering::Relaxed);
    a.tick_ms(2_000);
    assert_ne!(
        led.load(Ordering::Relaxed),
        led_code(LedState::Off),
        "the rig should start lit, or this test proves nothing"
    );
    // The hold only raises the menu, and a menu the user may still cancel is not a shutdown:
    // darkening the case light there would report a state the device is not in.
    a.apply(Action::PowerHold);
    assert_ne!(
        led.load(Ordering::Relaxed),
        led_code(LedState::Off),
        "the menu is a question, not a shutdown"
    );

    a.apply(Action::GbaDown(Btn::Down));
    a.apply(Action::GbaDown(Btn::Down));
    a.apply(Action::GbaDown(Btn::A));
    assert_eq!(
        led.load(Ordering::Relaxed),
        led_code(LedState::Off),
        "the LED was still reporting a running state after power_off"
    );
}

/// `begin_power_off` is shared by two call sites — a held button and an idle doze timing
/// out — and the button-hold path above only proves one of them. An idle handheld that
/// nobody is watching is exactly the case where a case light left on actually matters, so it
/// gets its own assertion rather than trusting the shared helper by association.
#[test]
fn a_doze_timeout_also_leaves_the_led_off_rather_than_lit_through_shutdown() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let (mut a, charge, percent, led, _writes) = app_playing_with_led(d.path(), "Emerald");
    charge.store(1, Ordering::Relaxed); // Discharging, so the LED is lit green beforehand
    percent.store(50, Ordering::Relaxed);
    a.tick_ms(2_000);
    assert_ne!(
        led.load(Ordering::Relaxed),
        led_code(LedState::Off),
        "the rig should start lit, or this test proves nothing"
    );
    a.apply(Action::LidClose);
    a.on_doze_timeout();
    assert_eq!(
        led.load(Ordering::Relaxed),
        led_code(LedState::Off),
        "the LED was still reporting a running state after the doze timeout"
    );
}

/// Every policy test in `tests/flush.rs` injects a reading straight through `App::on_battery`,
/// which proves the policy but not that the slow tick is the thing that actually calls it.
/// Deleting that call left every one of them green regardless.
#[test]
fn the_slow_tick_actually_runs_the_power_off_policy_on_what_it_reads() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let (mut a, charge, percent) = app_playing_with_charge(d.path(), "Emerald");
    charge.store(1, Ordering::Relaxed); // Discharging: nothing here suppresses the cutoff
    percent.store(3, Ordering::Relaxed); // below BATTERY_CRITICAL
    a.tick_ms(10_000);
    assert!(
        a.powering_off(),
        "the slow tick read a critical battery but never ran the power-off policy on it"
    );
}

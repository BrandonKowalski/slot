mod common;

use std::time::{Duration, Instant};

use common::{session_with_platform, tmp_root_with_carts, tmp_root_with_real_carts};
use slot::app::Phase;
use slot::session::Session;
use slot_input::{Btn, Millis, RawEvent, MENU_HOLD_MS, POWER_HOLD_MS};

const FRAME_MS: Millis = 16;
const DT: f32 = 1.0 / 60.0;

/// `RETRO_RUMBLE_STRONG`, which is what the core passes.
const STRONG: u32 = 0;

/// The whole point of the interface: what the core asks for reaches the motor. Neither the
/// mock nor the test rom drives a rumble register, so this writes the cell the core's
/// callback writes.
#[test]
fn what_the_core_asks_for_reaches_the_motor() {
    // Two, because a single cart has nowhere to eject to and the slot refuses.
    let d = tmp_root_with_real_carts(&["Advance Wars", "Emerald"]);
    let (mut s, motor) = session_with_platform(d.path());
    let mut now = 0;
    play(&mut s, &mut now);
    s.core_rumble()
        .expect("a seated cart has a core")
        .set(0, STRONG, u16::MAX);
    step(&mut s, &mut now);
    assert_eq!(motor.last(), u16::MAX, "the core asked and nothing moved");

    // The core is never told to stop. The phase is what takes the motor down, and it has to
    // keep it down for every frame the cart is still on its way out.
    let pressed = now;
    event(&mut s, RawEvent::Down(Btn::Menu), &mut now);
    while now < pressed + MENU_HOLD_MS + FRAME_MS {
        step(&mut s, &mut now);
    }
    assert_eq!(motor.last(), 0, "the motor outlived the cart");
    step(&mut s, &mut now);
    assert_eq!(motor.last(), 0, "the next frame turned it back on");
}

/// An unplugged cart leaving the motor on would run until the battery died.
#[test]
fn ejecting_stops_the_motor() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let (mut s, motor) = session_with_platform(d.path());
    s.rumble(u16::MAX);
    assert_ne!(motor.last(), 0);
    s.feed([RawEvent::Down(Btn::Menu)], 0);
    s.feed([], MENU_HOLD_MS + 1);
    assert_eq!(
        motor.last(),
        0,
        "the motor kept running after the cart came out"
    );
}

/// Same reasoning as the lid: nothing should be buzzing while the device is asleep.
#[test]
fn dozing_stops_the_motor() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let (mut s, motor) = session_with_platform(d.path());
    s.rumble(u16::MAX);
    s.feed([RawEvent::Down(Btn::Lid)], 0);
    assert_eq!(motor.last(), 0);
}

/// The power button dozes as well, and on a device with a stuck hall sensor it is the only
/// one of the two that gets there.
#[test]
fn the_power_button_stops_the_motor() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let (mut s, motor) = session_with_platform(d.path());
    s.rumble(u16::MAX);
    s.feed([RawEvent::Down(Btn::Power)], 0);
    assert_eq!(motor.last(), 0);
}

fn step(s: &mut Session, now: &mut Millis) {
    *now += FRAME_MS;
    s.feed([], *now);
    s.update(DT);
}

fn event(s: &mut Session, ev: RawEvent, now: &mut Millis) {
    *now += FRAME_MS;
    s.feed([ev], *now);
    s.update(DT);
}

/// Puts the selected cart in and waits out the load, which happens on its own thread.
fn play(s: &mut Session, now: &mut Millis) {
    event(s, RawEvent::Down(Btn::A), now);
    event(s, RawEvent::Up(Btn::A), now);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        step(s, now);
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// The menu replaces the screen but not the phase, so a cart that was buzzing as the button
/// went down kept buzzing while the user read a question about turning the device off — and
/// then straight through the shutdown, since `poweroff` ends the process with `exit` and
/// `Motor`'s own destructor never runs to put it down.
#[test]
fn the_power_menu_takes_the_motor_down() {
    let d = tmp_root_with_real_carts(&["Advance Wars", "Emerald"]);
    let (mut s, motor) = session_with_platform(d.path());
    let mut now = 0;
    play(&mut s, &mut now);
    s.core_rumble()
        .expect("a seated cart has a core")
        .set(0, STRONG, u16::MAX);
    step(&mut s, &mut now);
    assert_eq!(
        motor.last(),
        u16::MAX,
        "the motor should be running, or this test proves nothing"
    );

    let pressed = now;
    event(&mut s, RawEvent::Down(Btn::Power), &mut now);
    while now < pressed + POWER_HOLD_MS + FRAME_MS {
        step(&mut s, &mut now);
    }
    assert_eq!(s.app().power_menu(), Some(0), "the menu never opened");
    assert_eq!(
        motor.last(),
        0,
        "the cart kept buzzing under the power menu"
    );
}

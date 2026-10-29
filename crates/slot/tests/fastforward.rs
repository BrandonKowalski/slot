mod common;

use std::time::{Duration, Instant};

use slot::app::Phase;
use slot::session::Session;
use slot_input::{Btn, Millis, RawEvent};
use slot_store::{
    write_shortcuts, write_slot_state, Binding, Device, Shortcut, ShortcutKeys, SlotState,
};
use slot_ui::Icon;

#[test]
fn the_badge_follows_the_latch_rather_than_the_button() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    write_slot_state(
        d.path(),
        &SlotState {
            cart: Some("Emerald".into()),
            clock_set: true,
            ..Default::default()
        },
    )
    .expect("write slot.state");
    let mut s = Session::boot(d.path().to_path_buf());
    let mut now: Millis = 0;

    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        step(&mut s, &mut now, None);
        std::thread::sleep(Duration::from_millis(1));
    }

    step(&mut s, &mut now, Some(RawEvent::Down(Btn::R2)));
    assert!(badge(&s).is_some(), "no badge while R2 is held");
    step(&mut s, &mut now, Some(RawEvent::Up(Btn::R2)));
    assert!(badge(&s).is_none(), "the badge outlived the hold");

    for _ in 0..20 {
        step(&mut s, &mut now, None);
    }
    step(&mut s, &mut now, Some(RawEvent::Down(Btn::R2)));
    step(&mut s, &mut now, Some(RawEvent::Up(Btn::R2)));
    step(&mut s, &mut now, Some(RawEvent::Down(Btn::R2)));
    step(&mut s, &mut now, Some(RawEvent::Up(Btn::R2)));
    assert!(badge(&s).is_some(), "the latched badge left with R2");

    step(&mut s, &mut now, Some(RawEvent::Down(Btn::R2)));
    step(&mut s, &mut now, Some(RawEvent::Up(Btn::R2)));
    assert!(badge(&s).is_none(), "the badge survived the latch it lost");
}

#[test]
fn fast_forward_toggle_on_r2_stays_on_after_the_release_until_the_next_press() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    write_slot_state(
        d.path(),
        &SlotState {
            cart: Some("Emerald".into()),
            clock_set: true,
            ..Default::default()
        },
    )
    .expect("write slot.state");
    let mut keys = ShortcutKeys::default();
    keys.set(Shortcut::FastForwardToggle, Binding::plain(Device::R2));
    write_shortcuts(d.path(), &keys).expect("write shortcuts");
    let mut s = Session::boot(d.path().to_path_buf());
    let mut now: Millis = 0;

    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        step(&mut s, &mut now, None);
        std::thread::sleep(Duration::from_millis(1));
    }

    step(&mut s, &mut now, Some(RawEvent::Down(Btn::R2)));
    step(&mut s, &mut now, Some(RawEvent::Up(Btn::R2)));
    for _ in 0..20 {
        step(&mut s, &mut now, None);
    }
    assert!(badge(&s).is_some(), "the toggle let go with R2");

    step(&mut s, &mut now, Some(RawEvent::Down(Btn::R2)));
    assert!(badge(&s).is_none(), "the second press did not stop it");
    step(&mut s, &mut now, Some(RawEvent::Up(Btn::R2)));
    assert!(badge(&s).is_none());
}

#[test]
fn a_frame_is_late_only_when_the_core_paces_it_and_misses() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    write_slot_state(
        d.path(),
        &SlotState {
            cart: Some("Emerald".into()),
            clock_set: true,
            ..Default::default()
        },
    )
    .expect("write slot.state");
    let mut s = Session::boot(d.path().to_path_buf());
    let mut now: Millis = 0;

    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        step(&mut s, &mut now, None);
        std::thread::sleep(Duration::from_millis(1));
    }

    s.set_driven(true);
    let deadline = Instant::now() + Duration::from_secs(2);
    while !s.emu().expect("a seated cart has a core").locked() {
        assert!(
            Instant::now() < deadline,
            "the core never locked to the screen"
        );
        step(&mut s, &mut now, None);
        std::thread::sleep(Duration::from_millis(1));
    }
    for _ in 0..30 {
        assert!(
            !s.step_emulator(Duration::from_millis(16), Duration::from_millis(100)),
            "a frame the core delivered in time counted as late"
        );
        step(&mut s, &mut now, None);
    }

    step(&mut s, &mut now, Some(RawEvent::Down(Btn::R2)));
    let deadline = Instant::now() + Duration::from_secs(2);
    while s.emu().expect("a seated cart has a core").locked() {
        assert!(
            Instant::now() < deadline,
            "fast forward never unlocked the core"
        );
        step(&mut s, &mut now, None);
        std::thread::sleep(Duration::from_millis(1));
    }
    for _ in 0..30 {
        assert!(
            !s.step_emulator(Duration::from_millis(16), Duration::from_millis(12)),
            "a frame the core was never asked to pace counted as late"
        );
        step(&mut s, &mut now, None);
    }
}

#[test]
fn the_fast_forward_settings_reach_the_emulator() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    write_slot_state(
        d.path(),
        &SlotState {
            cart: Some("Emerald".into()),
            clock_set: true,
            ff_speed: 6,
            ff_sound: true,
            ..Default::default()
        },
    )
    .expect("write slot.state");
    let mut s = Session::boot(d.path().to_path_buf());
    let mut now: Millis = 0;

    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        step(&mut s, &mut now, None);
        std::thread::sleep(Duration::from_millis(1));
    }

    let emu = s.emu().expect("a seated cart has a core");
    assert_eq!(
        emu.fast_quarters(),
        6,
        "the chosen speed never reached the core"
    );
    assert!(emu.ff_sound(), "fast forward sound never reached the core");
}

#[test]
fn with_fast_forward_off_holding_it_is_refused_and_the_game_keeps_its_pace() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    write_slot_state(
        d.path(),
        &SlotState {
            cart: Some("Emerald".into()),
            clock_set: true,
            ff_speed: 0,
            ..Default::default()
        },
    )
    .expect("write slot.state");
    let mut s = Session::boot(d.path().to_path_buf());
    let mut now: Millis = 0;

    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        step(&mut s, &mut now, None);
        std::thread::sleep(Duration::from_millis(1));
    }

    step(&mut s, &mut now, Some(RawEvent::Down(Btn::R2)));
    assert!(
        s.app().refusal_active(now),
        "holding fast forward with it off was not refused"
    );
    assert!(badge(&s).is_none(), "fast forward ran with it off");
    step(&mut s, &mut now, Some(RawEvent::Up(Btn::R2)));
}

fn step(s: &mut Session, now: &mut Millis, ev: Option<RawEvent>) {
    *now += 16;
    s.feed(ev, *now);
    s.update(1.0 / 60.0);
}

fn badge(s: &Session) -> Option<Icon> {
    s.app().ff_badge()
}

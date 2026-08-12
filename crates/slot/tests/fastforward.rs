mod common;

use std::time::{Duration, Instant};

use slot::app::Phase;
use slot::session::Session;
use slot_input::{Btn, Millis, RawEvent};
use slot_store::{write_slot_state, SlotState};
use slot_ui::Icon;

/// A latch is the one fast forward state that outlives the button, so it is the one the badge
/// has to be told about separately: a held R2 leaves with the finger, a latched one does not.
#[test]
fn the_badge_follows_the_latch_rather_than_the_button() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    write_slot_state(
        d.path(),
        &SlotState {
            cart: Some("Emerald".into()),
            clock_set: true,
            utc_offset_min: 0,
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

    // Well past the double tap window, so the next press is a first tap rather than a second.
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

fn step(s: &mut Session, now: &mut Millis, ev: Option<RawEvent>) {
    *now += 16;
    s.feed(ev, *now);
    s.update(1.0 / 60.0);
}

/// The badge is read as state rather than as quads: the glyph needs an uploaded face and a
/// headless session has none, so a drawn list would be empty however R2 was pressed.
fn badge(s: &Session) -> Option<Icon> {
    s.app().ff_badge()
}

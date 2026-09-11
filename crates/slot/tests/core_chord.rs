//! SELECT + START through the real gesture layer: the picker on the shelf, the game's keys in game.

mod common;

use std::time::{Duration, Instant};

use slot::app::Phase;
use slot::session::Session;
use slot_input::{Btn, Millis, RawEvent};

fn step(s: &mut Session, now: &mut Millis, events: &[RawEvent]) {
    *now += 16;
    s.feed(events.iter().copied(), *now);
    s.update(1.0 / 60.0);
}

#[test]
fn select_and_start_open_the_picker_on_the_shelf() {
    let d = common::tmp_root_with_carts(&["Emerald", "Zzz"]);
    common::clocked(d.path());
    let mut s = Session::boot(d.path().to_path_buf());
    let mut now: Millis = 0;
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(s.app().phase(), Phase::Shelf) {
        assert!(Instant::now() < deadline, "never reached the shelf");
        step(&mut s, &mut now, &[]);
    }
    step(
        &mut s,
        &mut now,
        &[RawEvent::Down(Btn::Select), RawEvent::Down(Btn::Start)],
    );
    assert!(
        s.app().core_picker().is_some(),
        "SELECT + START did not open the picker"
    );
}

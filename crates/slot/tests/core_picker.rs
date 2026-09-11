use slot::core_picker::{CorePicker, Outcome, Press, CLOSE_MS, HOP_MS, OPEN_MS};
use slot_store::Core;

#[test]
fn it_opens_over_the_open_time_and_then_rests() {
    let p = CorePicker::open(Core::Mgba, 1000);
    assert_eq!(p.openness(1000), 0.0);
    let half = p.openness(1000 + OPEN_MS / 2);
    assert!(half > 0.0 && half < 1.0, "halfway open is {half}");
    assert_eq!(p.openness(1000 + OPEN_MS), 1.0);
    assert_eq!(p.openness(1000 + 10 * OPEN_MS), 1.0);
}

/// The chip is already where the cart runs, so the board says what is true before it asks.
#[test]
fn the_chip_starts_seated_in_the_carts_own_core() {
    let chip = CorePicker::open(Core::Gpsp, 0).chip(0);
    assert_eq!(chip.seated, Some(Core::Gpsp));
    assert_eq!((chip.across, chip.lift, chip.tip), (1.0, 0.0, 0.0));
}

#[test]
fn right_from_mgba_hops_blank_and_lands_named_in_gpsp() {
    let mut p = CorePicker::open(Core::Mgba, 0);
    assert_eq!(p.press(Press::Right, 500), Outcome::Nothing);
    assert_eq!(p.seat(), Core::Gpsp);

    let mid = p.chip(500 + HOP_MS / 2);
    assert_eq!(mid.seated, None, "the chip wears a name in flight");
    assert!((mid.across - 0.5).abs() < 0.01, "across {}", mid.across);
    assert!((mid.lift - 1.0).abs() < 0.01, "lift {}", mid.lift);
    assert!(mid.tip > 0.0, "a chip moving right should lean right");

    let landed = p.chip(500 + HOP_MS);
    assert_eq!(landed.seated, Some(Core::Gpsp));
    assert_eq!((landed.across, landed.lift), (1.0, 0.0));
}

/// Toward the socket it is already in there is nowhere to go. The chip shakes, and nothing
/// else about the picker changes.
#[test]
fn toward_the_socket_it_is_in_is_refused_and_only_shakes() {
    let mut p = CorePicker::open(Core::Gpsp, 0);
    assert_eq!(p.press(Press::Right, 400), Outcome::Refused);
    assert_eq!(p.seat(), Core::Gpsp);
    assert_ne!(p.chip(400).shake, 0.0, "a refusal with no shake");
    assert_eq!(p.chip(400).seated, Some(Core::Gpsp));
    assert_eq!(p.chip(700).shake, 0.0, "the shake outlived its 300 ms");
}

/// Back toward the socket it is leaving, the chip retraces its arc from where it is rather
/// than jumping to the start of a new one.
#[test]
fn back_mid_hop_turns_the_chip_round_from_where_it_is() {
    let mut p = CorePicker::open(Core::Mgba, 0);
    p.press(Press::Right, 400);
    let at = 400 + HOP_MS / 4;
    let before = p.chip(at).across;
    assert_eq!(p.press(Press::Left, at), Outcome::Nothing);
    assert_eq!(p.seat(), Core::Mgba);
    assert!(
        (p.chip(at).across - before).abs() < 0.02,
        "the chip jumped from {before} to {} as it turned",
        p.chip(at).across
    );
    assert_eq!(p.chip(at + HOP_MS).seated, Some(Core::Mgba));
}

#[test]
fn onward_mid_hop_does_nothing_and_is_not_a_refusal() {
    let mut p = CorePicker::open(Core::Mgba, 0);
    p.press(Press::Right, 400);
    assert_eq!(p.press(Press::Right, 450), Outcome::Nothing);
    assert_eq!(p.chip(450).shake, 0.0);
    assert_eq!(p.seat(), Core::Gpsp);
}

#[test]
fn keep_writes_where_the_chip_is_heading_and_closes() {
    let mut p = CorePicker::open(Core::Mgba, 0);
    p.press(Press::Right, 400);
    assert_eq!(p.press(Press::Keep, 450), Outcome::Write(Core::Gpsp));
    assert!(p.closing());
    assert!(!p.finished(450));
    assert!(p.finished(450 + CLOSE_MS));
}

#[test]
fn back_closes_without_a_write() {
    let mut p = CorePicker::open(Core::Mgba, 0);
    assert_eq!(p.press(Press::Back, 400), Outcome::Nothing);
    assert!(p.closing());
    assert!(p.finished(400 + CLOSE_MS));
}

/// A close that begins while the lid is still lifting starts from where the lid is. Starting
/// from rest would snap it up before bringing it back down.
#[test]
fn a_close_during_the_lift_reverses_from_where_the_lid_had_got_to() {
    let mut p = CorePicker::open(Core::Mgba, 0);
    let at = OPEN_MS / 2;
    let open = p.openness(at);
    p.press(Press::Back, at);
    assert!(
        (p.openness(at) - open).abs() < 1e-6,
        "the lid snapped as the close began"
    );
    assert!(
        p.finished(at + CLOSE_MS),
        "a partial close outlasted a whole one"
    );
}

#[test]
fn presses_during_the_close_do_nothing() {
    let mut p = CorePicker::open(Core::Mgba, 0);
    p.press(Press::Back, 400);
    assert_eq!(p.press(Press::Right, 420), Outcome::Nothing);
    assert_eq!(p.press(Press::Keep, 430), Outcome::Nothing);
    assert_eq!(p.seat(), Core::Mgba);
}

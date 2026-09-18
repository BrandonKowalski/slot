//! The emulated cable's frame clock, without a socket or a core.

use slot::cable::{decode, encode, Cable, DELAY};
use slot_retro::ButtonMask;

const A: ButtonMask = ButtonMask(ButtonMask::A);
const B: ButtonMask = ButtonMask(ButtonMask::B);

/// Both ends start able to run `DELAY` frames without hearing anything, which is the whole point
/// of the delay: neither device waits on the other to draw its first frame.
#[test]
fn a_fresh_cable_can_run_the_delay_before_the_peer_says_anything() {
    let mut c = Cable::new(0);
    for _ in 0..DELAY {
        assert!(c.ready().is_some(), "a seeded frame needed the peer");
        c.advance();
    }
    assert!(
        c.ready().is_none(),
        "it ran past the seed without the peer's masks"
    );
}

/// A mask is stamped for a frame ahead, not this one, so the wire has that long to deliver it.
#[test]
fn a_sampled_mask_is_stamped_for_a_later_frame() {
    let mut c = Cable::new(0);
    let (frame, mask) = decode(&c.sample(A)).expect("its own packet did not parse");
    assert_eq!(frame, DELAY);
    assert_eq!(mask, A);
}

/// Port order, not local-first. Player 1 handing its own mask over first would drive port 0 with
/// the wrong console, and both ends would disagree about who is who while each looked correct.
#[test]
fn each_end_reports_the_pair_in_port_order() {
    let mut p0 = Cable::new(0);
    let mut p1 = Cable::new(1);
    for _ in 0..DELAY {
        p0.advance();
        p1.advance();
    }
    let from_p0 = p0.sample(A);
    let from_p1 = p1.sample(B);
    assert!(p0.accept(&from_p1));
    assert!(p1.accept(&from_p0));

    assert_eq!(p0.ready(), Some((A, B)), "player 0 read the pair swapped");
    assert_eq!(
        p1.ready(),
        Some((A, B)),
        "player 1 disagreed with player 0 about port order"
    );
}

/// A frame will not run on a guess. This is the property the whole delay exists to protect: a
/// stepped frame cannot be taken back, so a missing mask stalls rather than substituting idle.
#[test]
fn a_frame_waits_rather_than_guessing_a_missing_mask() {
    let mut c = Cable::new(0);
    for _ in 0..DELAY {
        c.advance();
    }
    c.sample(A);
    assert!(c.ready().is_none(), "it ran without the peer's mask");
    c.stall();
    c.stall();
    assert_eq!(c.stalled(), 2);

    assert!(c.accept(&encode(DELAY, B)));
    assert_eq!(c.ready(), Some((A, B)));
    c.advance();
    assert_eq!(c.stalled(), 0, "the stall count survived a frame running");
}

/// Two ends fed the same inputs agree on every frame's pair, which is what determinism across
/// devices rests on.
#[test]
fn both_ends_agree_on_every_frame() {
    let mut p0 = Cable::new(0);
    let mut p1 = Cable::new(1);
    let script = [A, B, A, ButtonMask::default(), B, B, A];

    let mut seen0 = Vec::new();
    let mut seen1 = Vec::new();
    for (i, m) in script.iter().enumerate() {
        let a = p0.sample(*m);
        let b = p1.sample(script[script.len() - 1 - i]);
        p0.accept(&b);
        p1.accept(&a);
        if let Some(pair) = p0.ready() {
            seen0.push(pair);
            p0.advance();
        }
        if let Some(pair) = p1.ready() {
            seen1.push(pair);
            p1.advance();
        }
    }
    assert_eq!(seen0, seen1, "the two ends ran different frames");
    assert!(seen0.len() >= script.len() - DELAY as usize);
}

/// A duplicate is not an error and a short read is not worth ending a session over.
#[test]
fn junk_and_duplicates_are_dropped_rather_than_raised() {
    let mut c = Cable::new(0);
    assert!(!c.accept(&[1, 2, 3]), "a short packet was taken");
    assert!(!c.accept(&[]), "an empty packet was taken");
    for _ in 0..DELAY {
        c.advance();
    }
    assert!(c.accept(&encode(DELAY, A)));
    c.sample(B);
    assert_eq!(c.ready(), Some((B, A)));
    c.advance();
    assert!(
        !c.accept(&encode(DELAY, A)),
        "a mask for a frame already run was taken back in"
    );
}

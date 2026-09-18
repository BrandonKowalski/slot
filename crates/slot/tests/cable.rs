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
    // The state swap is what does this in a real session; here there is no core to copy.
    p1.prime();
    // Sample, exchange, advance: the order the worker uses, until the seeded frames run out and
    // the next frame is one whose masks both ends actually sent each other.
    for _ in 0..DELAY {
        let a = p0.sample(A);
        let b = p1.sample(B);
        assert!(p0.accept(&b));
        assert!(p1.accept(&a));
        p0.advance();
        p1.advance();
    }

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
        c.sample(A);
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
    p1.prime();
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
    // Junk that is not one of the packet kinds. A leading 1 is a state chunk now.
    assert!(!c.accept(&[9, 2, 3]), "a short packet was taken");
    assert!(!c.accept(&[]), "an empty packet was taken");
    for _ in 0..DELAY {
        c.sample(B);
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

/// The input delay stays what it was however much the peer stalls, even when `sample` is called
/// on every present the way the worker does. It used to grow by a frame per stalled present,
/// without bound, and was felt worst on whichever device stalled more. The invariant lives in
/// `sample` now rather than in the caller remembering to ask only once per frame.
#[test]
fn a_stall_does_not_add_to_the_input_delay() {
    let mut c = Cable::new(0);
    for _ in 0..DELAY {
        c.sample(A);
        c.advance();
    }
    let (stamped, _) = decode(&c.sample(B)).expect("packet");
    let ahead = stamped - c.frame();

    // Twenty presents with nothing from the peer, each sampling exactly as the worker does.
    for _ in 0..20 {
        c.sample(ButtonMask::default());
        c.stall();
    }
    assert_eq!(c.stalled(), 20);

    let (stamped, mask) = decode(&c.sample(ButtonMask::default())).expect("packet");
    assert_eq!(
        stamped - c.frame(),
        ahead,
        "the input delay grew across a stall: masks are being stamped per present again"
    );
    assert_eq!(
        mask, B,
        "a decided mask was revised, which desyncs the pair"
    );
}

/// The joiner runs nothing until it holds the host's machine. Both devices simulate both
/// consoles, so identical inputs on different starting states produce two different games: one
/// device waiting for a cable while the other is already in a race. The seeded frames are not an
/// exception, which is the trap here, since they need no peer mask and would otherwise run.
#[test]
fn a_joiner_runs_nothing_until_it_has_the_hosts_machine() {
    let mut join = Cable::new(1);
    assert!(!join.primed(), "a fresh joiner claimed to be ready");
    assert!(
        join.ready().is_none(),
        "the joiner ran a seeded frame on its own machine"
    );

    let state: Vec<u8> = (0..150_000u32).map(|n| n as u8).collect();
    let packets = slot::cable::state_packets(&state);
    assert!(packets.len() > 1, "a state this size must be cut up");
    for (i, p) in packets.iter().enumerate() {
        assert!(p.len() <= 65_535, "chunk {i} cannot cross the wire");
        assert!(join.accept(p));
        if i + 1 < packets.len() {
            assert!(
                join.take_state().is_none(),
                "a part was taken for the whole"
            );
        }
    }
    assert_eq!(join.take_state().as_deref(), Some(state.as_slice()));
    assert!(
        join.take_state().is_none(),
        "the state was handed over twice"
    );

    join.prime();
    assert!(join.ready().is_some(), "priming did not let the frames run");
}

/// The host needs no priming: its own machine is the one being copied.
#[test]
fn a_host_is_ready_from_the_start() {
    let host = Cable::new(0);
    assert!(host.primed());
    assert!(host.ready().is_some());
}

/// A state and a mask are told apart by the packet itself, so neither is ever read as the other.
#[test]
fn a_state_chunk_is_never_read_as_a_button_mask() {
    let mut c = Cable::new(0);
    for p in slot::cable::state_packets(&[9u8; 32]) {
        assert!(decode(&p).is_none(), "a state chunk parsed as a mask");
        assert!(c.accept(&p));
    }
    // And the masks still work alongside it.
    assert!(c.accept(&encode(DELAY, A)));
}

/// A stall is only a lost peer once both ends are running the agreed machine. A joiner restoring
/// a megabyte of state stalls every present by design, and on hardware that ended the session a
/// second into the join, before the swap could finish.
#[test]
fn a_stall_during_the_swap_is_not_a_lost_peer() {
    let mut host = Cable::new(0);
    assert!(
        !host.armed(),
        "the host armed before the joiner said it was running"
    );
    for _ in 0..200 {
        host.stall();
    }
    assert!(
        !host.armed(),
        "a long swap was read as a peer that vanished"
    );

    assert!(host.accept(&slot::cable::ready_packet()));
    assert!(host.armed(), "the joiner said it was ready and was ignored");
}

/// The joiner's own side: it is not armed while it is still restoring, so its own stalls waiting
/// for a state cannot end the session either.
#[test]
fn a_joiner_is_not_armed_until_it_has_restored() {
    let mut join = Cable::new(1);
    assert!(!join.armed(), "an unprimed joiner armed");
    join.prime();
    assert!(join.armed(), "the joiner never armed after restoring");
}

mod common;

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use slot::app::Phase;
use slot::rewind::Rewind;
use slot::session::Session;
use slot_input::{Btn, Millis, RawEvent};
use slot_store::{write_slot_state, SlotState};
use slot_ui::Draw;

const STATE_LEN: usize = 400_000;
/// About 3% of the state, which is what a frame of a real game touches.
const CHURN: usize = 12_000;

fn noise(seed: u32, len: usize) -> Vec<u8> {
    let mut x = seed;
    (0..len)
        .map(|_| {
            x = x.wrapping_mul(1664525).wrapping_add(1013904223);
            (x >> 24) as u8
        })
        .collect()
}

/// The memory that did not move this frame. Pseudorandom rather than flat, so the ring is
/// measured on what its deltas cancel rather than on a fixture lz4 would have flattened
/// whatever it was handed.
fn stale() -> &'static [u8] {
    static BASE: OnceLock<Vec<u8>> = OnceLock::new();
    BASE.get_or_init(|| noise(0x5eed, STATE_LEN))
}

/// Churn is one moving window plus a few registers, which is how a frame's writes cluster.
fn synthetic_state(i: u32) -> Vec<u8> {
    let mut s = stale().to_vec();
    let at = (i as usize * 997) % (STATE_LEN - CHURN);
    s[at..at + CHURN].copy_from_slice(&noise(i.wrapping_add(1), CHURN));
    s[..8].copy_from_slice(&(i as u64).to_le_bytes());
    s
}

#[test]
fn rewind_reconstructs_states_exactly_in_reverse() {
    let mut r = Rewind::new(4 * 1024 * 1024);
    let states: Vec<Vec<u8>> = (0..200u32).map(synthetic_state).collect();
    for s in &states {
        r.push(s);
    }
    for i in (0..200).rev() {
        assert_eq!(r.pop().unwrap(), states[i], "mismatch rewinding to {i}");
    }
}

#[test]
fn rewind_respects_its_byte_budget() {
    let mut r = Rewind::new(256 * 1024);
    for i in 0..5000u32 {
        r.push(&synthetic_state(i));
    }
    assert!(r.bytes_used() <= 256 * 1024, "used {}", r.bytes_used());
    assert!(r.depth() > 0);
}

#[test]
fn delta_compression_beats_storing_raw_states() {
    let mut r = Rewind::new(64 * 1024 * 1024);
    for i in 0..120u32 {
        r.push(&synthetic_state(i));
    }
    assert!(
        r.bytes_used() < 120 * 400_000 / 4,
        "delta gained less than 4x: {} bytes",
        r.bytes_used()
    );
}

#[test]
fn eviction_leaves_what_it_kept_intact_and_then_bottoms_out() {
    let mut r = Rewind::new(64 * 1024);
    let states: Vec<Vec<u8>> = (0..40u32).map(synthetic_state).collect();
    for s in &states {
        r.push(s);
    }
    let kept = r.depth();
    assert!(kept > 1 && kept < 40, "the budget kept {kept} of 40");
    for i in (40 - kept..40).rev() {
        assert_eq!(r.pop().unwrap(), states[i], "mismatch rewinding to {i}");
    }
    assert!(r.pop().is_none(), "a state that was evicted came back");
    assert_eq!(r.depth(), 0);
}

/// Releasing L2 resumes play from wherever the rewind landed, so the next snapshot has to
/// chain onto that state rather than onto the one the ring last saw pushed.
#[test]
fn play_resuming_after_a_rewind_chains_onto_the_state_it_landed_on() {
    let mut r = Rewind::new(4 * 1024 * 1024);
    let states: Vec<Vec<u8>> = (0..20u32).map(synthetic_state).collect();
    for s in &states {
        r.push(s);
    }
    for _ in 0..5 {
        r.pop().expect("history was shorter than the rewind");
    }
    let resumed = synthetic_state(100);
    r.push(&resumed);
    assert_eq!(r.pop().unwrap(), resumed);
    assert_eq!(r.pop().unwrap(), states[14]);
    assert_eq!(r.pop().unwrap(), states[13]);
}

/// The bar reads the byte budget, since nothing else bounds the history: how many states
/// fit is whatever the deltas happened to compress to.
#[test]
fn fill_reads_full_on_a_full_ring_and_empty_once_it_is_spent() {
    let mut r = Rewind::new(1024 * 1024);
    assert_eq!(r.fill(), 0);
    for i in 0..200u32 {
        r.push(&synthetic_state(i));
    }
    assert!(r.fill() > 90, "a ring at its budget read {}", r.fill());
    while r.pop().is_some() {}
    assert_eq!(r.fill(), 0, "a spent ring still reads as holding history");
}

/// The bar is held open by L2 rather than by a timer, so it has to still be there long
/// after the 1500 ms a level bar gets.
#[test]
fn the_rewind_bar_is_up_while_l2_is_held_and_gone_once_it_is_let_go() {
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

    step(&mut s, &mut now, Some(RawEvent::Down(Btn::L2)));
    for _ in 0..200 {
        step(&mut s, &mut now, None);
    }
    assert!(
        !drawn(&s).is_empty(),
        "the rewind bar timed out while L2 was held"
    );
    step(&mut s, &mut now, Some(RawEvent::Up(Btn::L2)));
    assert!(drawn(&s).is_empty(), "the bar outlived the hold");
}

fn step(s: &mut Session, now: &mut Millis, ev: Option<RawEvent>) {
    *now += 16;
    s.feed(ev, *now);
    s.update(1.0 / 60.0);
}

/// Nothing but the HUD draws over a playing game, so the list is the bar.
fn drawn(s: &Session) -> Vec<Draw> {
    let mut out = Vec::new();
    s.app().draw(&mut out);
    out
}

#[test]
fn a_state_that_changed_size_drops_the_history_rather_than_corrupting_it() {
    let mut r = Rewind::new(4 * 1024 * 1024);
    for i in 0..5u32 {
        r.push(&synthetic_state(i));
    }
    let bigger = vec![7u8; STATE_LEN + 64];
    r.push(&bigger);
    assert_eq!(r.pop().unwrap(), bigger);
    assert!(
        r.pop().is_none(),
        "a state was rebuilt across a size change"
    );
}

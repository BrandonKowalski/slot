//! The cutting itself. What the committed assets have to be is asserted over in the
//! frontend, against the assets: `the_contacts_in_the_clip_are_where_the_lead_says` and
//! `neither_clip_starts_or_ends_on_a_step`. This file is about the tool that makes them.

use slot_sfxcut::{cut, takes, CutError, HZ, INSERT_LEAD, INSERT_LEN, INSERT_PEAK};

/// Room tone with a hard transient at each of `events`, which is the shape of a recording of
/// somebody doing the same thing ten times with pauses in between.
fn recording(seconds: f32, events: &[f32]) -> Vec<f32> {
    let n = (seconds * HZ) as usize;
    let mut s = 0x1234_5678u32;
    let mut buf: Vec<f32> = (0..n)
        .map(|_| {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((s >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.002
        })
        .collect();
    for &at in events {
        let i = (at * HZ) as usize;
        // A click that decays over 40 ms, loud enough to clear the detector's floor.
        for k in 0..(0.040 * HZ) as usize {
            if i + k >= n {
                break;
            }
            let env = (-(k as f32) / (0.008 * HZ)).exp();
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            buf[i + k] += ((s >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * env * 0.9;
        }
    }
    buf
}

#[test]
fn every_event_is_found_once_and_in_time_order() {
    let want = [0.500, 1.200, 2.000, 3.100];
    let found = takes(&recording(4.0, &want));
    assert_eq!(found.len(), want.len(), "found {found:?}");
    for (t, &w) in found.iter().zip(&want) {
        assert!(
            (t.at - w).abs() <= 0.003,
            "found {:.3}s, wanted {w:.3}s",
            t.at
        );
    }
}

/// The detector works on a 5 ms hop grid but the cut needs better than that, so the onset is
/// refined to the sample. A transient landing 5 ms from where it is reported is 5 ms of the
/// cartridge animation out of step with its own sound.
#[test]
fn the_onset_is_accurate_to_better_than_a_hop() {
    for at in [0.5001, 0.5033, 0.5067, 0.5099] {
        let found = takes(&recording(2.0, &[at]));
        assert_eq!(found.len(), 1);
        let off = (found[0].at - at).abs();
        assert!(
            off < 0.003,
            "reported {:.4}s for a transient at {at:.4}s",
            found[0].at
        );
    }
}

/// The whole reason the tool exists: whatever the recording did, the transient comes out
/// exactly `lead` into the clip.
#[test]
fn the_cut_puts_the_transient_exactly_on_the_lead() {
    let pcm = recording(4.0, &[1.500]);
    let clip = cut(&pcm, 1.500, INSERT_LEAD, INSERT_LEN, INSERT_PEAK, 0.0).expect("should fit");

    assert_eq!(clip.len(), INSERT_LEN);
    let loudest = clip
        .iter()
        .enumerate()
        .max_by_key(|(_, s)| s.unsigned_abs())
        .map(|(i, _)| i as f32 / HZ)
        .unwrap();
    assert!(
        (loudest - INSERT_LEAD).abs() <= 0.005,
        "loudest sample at {loudest:.3}s, lead is {INSERT_LEAD:.3}s"
    );
}

#[test]
fn a_cut_is_normalised_and_faded_so_it_neither_clips_nor_clicks() {
    let clip = cut(
        &recording(4.0, &[1.500]),
        1.500,
        INSERT_LEAD,
        INSERT_LEN,
        INSERT_PEAK,
        0.0,
    )
    .unwrap();

    let peak = clip.iter().map(|s| s.unsigned_abs()).max().unwrap();
    assert_eq!(peak, INSERT_PEAK as u16);
    assert!(clip.iter().all(|&s| s != i16::MIN && s != i16::MAX));
    // The same bound the frontend's own step test uses.
    assert!(clip[..8].iter().all(|s| s.abs() < 400), "starts on a step");
    assert!(
        clip[clip.len() - 200..].iter().all(|s| s.abs() < 400),
        "ends on a step"
    );
}

/// A take too near the start of the recording cannot be cut, and saying which side ran out
/// is the difference between re-recording and just picking a different take.
#[test]
fn a_take_without_room_around_it_is_refused_by_name() {
    let pcm = recording(4.0, &[0.020]);
    match cut(&pcm, 0.020, INSERT_LEAD, INSERT_LEN, INSERT_PEAK, 0.0) {
        Err(CutError::NotEnoughBefore { .. }) => {}
        other => panic!("expected NotEnoughBefore, got {other:?}"),
    }

    let pcm = recording(1.0, &[0.950]);
    match cut(&pcm, 0.950, INSERT_LEAD, INSERT_LEN, INSERT_PEAK, 0.0) {
        Err(CutError::NotEnoughAfter { .. }) => {}
        other => panic!("expected NotEnoughAfter, got {other:?}"),
    }
}

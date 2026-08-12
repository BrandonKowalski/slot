use slot::audio::{ring_capacity, AudioSink, Ring, StubSink};

fn ramp(frames: usize, from: i16) -> Vec<i16> {
    (0..frames * 2)
        .map(|i| from.wrapping_add(i as i16))
        .collect()
}

#[test]
fn queued_frames_tracks_the_device_backlog() {
    let mut s = StubSink::new();
    s.open(32768).unwrap();
    let r = s.ring();
    let primed = r.queued_frames();
    r.push_blocking(&ramp(1000, 0));
    assert_eq!(r.queued_frames(), primed + 1000);
    s.device_read(600);
    assert_eq!(r.queued_frames(), primed + 400);
}

/// The device pulls its first block before the core has run a frame, so an empty ring at
/// open is an underrun on every boot. Opening at the DRC target is silence instead.
#[test]
fn opening_primes_the_ring_to_the_drc_target() {
    let mut s = StubSink::new();
    s.open(48_000).unwrap();
    let r = s.ring();
    assert_eq!(r.queued_frames(), r.capacity_frames() / 2);
    s.device_read(64);
    assert_eq!(
        (r.overruns(), r.underruns()),
        (0, 0),
        "the device was starved at open"
    );
}

/// The producer is the emulator, the consumer is the device. Neither is allowed to lose a
/// sample when they are running at the same average rate, however uneven the arrivals.
#[test]
fn a_steady_producer_and_consumer_neither_drop_nor_starve() {
    let r = Ring::new(ring_capacity(48_000));
    let mut out = vec![0i16; 1024];
    for i in 0..2_000 {
        // A jittery producer: sometimes two device periods' worth, sometimes none.
        let n = match i % 5 {
            0 => 2048,
            4 => 0,
            _ => 1024,
        };
        let block = vec![1i16; n];
        r.push_blocking(&block);
        r.fill(&mut out);
    }
    assert_eq!(
        r.overruns(),
        0,
        "dropped audio: the producer was never made to wait"
    );
    assert_eq!(r.underruns(), 0, "starved the device");
}

#[test]
fn push_blocking_waits_instead_of_discarding() {
    let r = std::sync::Arc::new(Ring::new(64));
    let w = r.clone();
    let h = std::thread::spawn(move || w.push_blocking(&[7i16; 256]));
    let mut out = vec![0i16; 64];
    for _ in 0..8 {
        std::thread::sleep(std::time::Duration::from_millis(2));
        r.fill(&mut out);
    }
    h.join().unwrap();
    assert_eq!(r.overruns(), 0, "push_blocking dropped rather than waited");
}

#[test]
fn the_ring_holds_enough_to_ride_out_a_late_frame() {
    let ms = ring_capacity(48_000) as f64 / 48_000.0 * 1000.0;
    assert!(
        ms > 100.0,
        "only {ms:.0} ms of slack, a single late frame empties it"
    );
}

/// The fallback still has to exist, or a paused core deadlocks waiting for a drain that
/// never comes.
#[test]
fn a_muted_or_paused_core_does_not_block_on_audio() {
    let r = std::sync::Arc::new(Ring::new(64));
    r.set_muted(true);
    let w = r.clone();
    let h = std::thread::spawn(move || w.push_blocking(&[7i16; 4096]));
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(h.is_finished(), "a muted sink blocked the emulator thread");
}

/// A device that stops draining must cost the audio and not the game: the emulator waits
/// once, gives up, and keeps running.
#[test]
fn a_device_that_stopped_draining_does_not_freeze_the_emulator() {
    let r = Ring::new(8);
    r.push_blocking(&[1i16; 32]);
    let start = std::time::Instant::now();
    for _ in 0..20 {
        r.push_blocking(&[1i16; 32]);
    }
    assert!(
        start.elapsed() < std::time::Duration::from_millis(500),
        "twenty frames took {:?} against a dead device",
        start.elapsed()
    );
    assert!(r.overruns() > 0, "the drops went uncounted");
}

#[test]
fn samples_come_back_in_order_across_the_wrap() {
    let r = Ring::new(8);
    r.push(&ramp(6, 1));

    let mut out = vec![0i16; 8];
    r.fill(&mut out);
    assert_eq!(out, ramp(6, 1)[..8]);

    r.push(&ramp(5, 100));
    let mut rest = vec![0i16; 14];
    r.fill(&mut rest);
    let mut want = ramp(6, 1)[8..].to_vec();
    want.extend(ramp(5, 100));
    assert_eq!(rest, want);
}

#[test]
fn a_full_ring_drops_new_samples_instead_of_evicting_queued_audio() {
    let r = Ring::new(4);
    r.push(&ramp(3, 1));
    r.push(&ramp(3, 50));
    assert_eq!(r.queued_frames(), 4);

    let mut out = vec![0i16; 8];
    r.fill(&mut out);
    let mut want = ramp(3, 1);
    want.extend(&ramp(3, 50)[..2]);
    assert_eq!(out, want);
    assert_eq!(r.queued_frames(), 0);
}

#[test]
fn an_underrun_pads_with_silence_rather_than_repeating() {
    let r = Ring::new(64);
    r.push(&ramp(2, 9));
    let mut out = vec![-1i16; 8];
    r.fill(&mut out);
    assert_eq!(&out[..4], &ramp(2, 9)[..]);
    assert_eq!(&out[4..], &[0, 0, 0, 0]);
}

#[test]
fn muting_silences_the_device_but_still_drains_the_ring() {
    let r = Ring::new(64);
    r.push(&ramp(4, 1));
    r.set_muted(true);
    let mut out = vec![0i16; 4];
    r.fill(&mut out);
    assert!(out.iter().all(|s| *s == 0));
    assert_eq!(r.queued_frames(), 2);
}

/// The core is held still for the whole of the insert, so nothing is feeding the ring and the
/// device reads silence out of it. That is the arrangement working. Counted as starvation it
/// printed "41984 samples starved" after every single cart.
#[test]
fn a_held_core_does_not_report_the_device_as_starved() {
    let r = Ring::new(ring_capacity(48_000));
    r.reopen(48_000);
    let mut out = vec![0i16; 4_000];
    while r.queued_frames() > 0 {
        r.fill(&mut out);
    }
    r.set_idle(true);
    let before = r.underruns();
    for _ in 0..20 {
        r.fill(&mut out);
    }
    assert_eq!(
        r.underruns(),
        before,
        "a held core was reported as a starve"
    );

    // And a running one still is: this is the counter that catches a device the emulator
    // cannot keep up with.
    r.set_idle(false);
    r.fill(&mut out);
    assert!(r.underruns() > before, "a real starve is no longer counted");
}

/// A ring that ran dry rebuilds its cushion before playing again. Handing over whatever
/// fragments the producer has managed so far gives the device audio, silence, audio, silence
/// for as long as the ramp lasts, and that alternation is the scratch heard when a game
/// starts: the cart is loading while the opening cushion drains, and the core's first samples
/// arrive in pieces too small to fill a period.
#[test]
fn a_ring_that_ran_dry_rebuilds_its_cushion_before_playing_again() {
    let r = Ring::new(ring_capacity(48_000));
    let target = r.capacity_frames() / 2;
    let mut out = vec![0i16; 512 * 2];
    r.fill(&mut out);
    assert!(
        r.underruns() > 0,
        "a dry ring with a producer is an underrun"
    );

    r.push(&ramp(200, 1));
    r.fill(&mut out);
    assert!(
        out.iter().all(|s| *s == 0),
        "a fragment was played before the cushion was back"
    );
    assert_eq!(
        r.queued_frames(),
        200,
        "the fragment was drained rather than held"
    );

    r.push(&ramp(target, 1));
    r.fill(&mut out);
    assert!(
        out.iter().any(|s| *s != 0),
        "the cushion was full and still nothing played"
    );
}

/// A cart sound has no producer behind it to build a cushion with. Holding one back would
/// silence the shelf, where there is no core at all.
#[test]
fn a_cart_sound_is_not_held_back_by_the_cushion() {
    let r = Ring::new(ring_capacity(48_000));
    let mut out = vec![0i16; 512 * 2];
    r.fill(&mut out);
    r.mix(&ramp(100, 1));
    r.fill(&mut out);
    assert!(
        out.iter().any(|s| *s != 0),
        "the cart sound was swallowed by the cushion"
    );
}

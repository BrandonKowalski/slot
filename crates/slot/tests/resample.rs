use slot::resample::Resampler;

fn ramp(frames: usize) -> Vec<i16> {
    (0..frames * 2)
        .map(|i| (i as i16).wrapping_mul(37).wrapping_sub(400))
        .collect()
}

#[test]
fn chunked_resampling_matches_one_pass() {
    let src = ramp(1000);
    let mut out = Vec::new();

    let mut whole = Resampler::new(32768.0, 48000.0);
    whole.process(&src, &mut out);
    let want = out.clone();

    let mut chunked = Resampler::new(32768.0, 48000.0);
    let mut got = Vec::new();
    for chunk in src.chunks(2 * 137) {
        chunked.process(chunk, &mut out);
        got.extend_from_slice(&out);
    }
    assert_eq!(got, want, "the seam between core frames is not continuous");
}

#[test]
fn a_starved_ratio_stretches_and_a_full_one_compresses() {
    let src = ramp(4096);
    let mut out = Vec::new();

    let mut flat = Resampler::new(32768.0, 32768.0);
    flat.process(&src, &mut out);
    let neutral = out.len();
    assert_eq!(neutral, src.len());

    let mut starved = Resampler::new(32768.0, 32768.0);
    starved.set_ratio(1.005);
    starved.process(&src, &mut out);
    assert!(
        out.len() > neutral,
        "starved ratio produced {} frames",
        out.len() / 2
    );

    let mut full = Resampler::new(32768.0, 32768.0);
    full.set_ratio(0.995);
    full.process(&src, &mut out);
    assert!(
        out.len() < neutral,
        "full ratio produced {} frames",
        out.len() / 2
    );
}

/// `src_hz` is whatever a core put in `retro_system_av_info.timing.sample_rate`, and the only
/// guard used to be on `dst_hz`. A rate that is not a rate has to come out as a passthrough,
/// because `process` walks its input by `step` until it reaches the end and every degenerate
/// value stops it getting there: NaN and infinity fail the comparison and the resampler goes
/// silent for the rest of the session, while zero and anything negative never advance and the
/// emulator thread spins inside one call pushing output until the device is out of memory.
///
/// The NaN case is asserted first on purpose. It is the one that fails rather than hangs, so a
/// suite run against a resampler with the guard taken back out stops here instead of at the two
/// below it.
#[test]
fn a_core_rate_that_is_not_a_rate_passes_the_samples_through() {
    for (name, hz) in [
        ("nan", f64::NAN),
        ("infinity", f64::INFINITY),
        ("zero", 0.0),
        ("negative", -32768.0),
    ] {
        let mut r = Resampler::new(hz, 48_000.0);
        let mut out = Vec::new();
        let src = ramp(64);
        r.process(&src, &mut out);
        assert_eq!(
            out.len(),
            src.len(),
            "a core rate of {name} produced {} frames from {}",
            out.len() / 2,
            src.len() / 2
        );
    }
}

#[test]
fn the_output_rate_follows_the_device_not_the_core() {
    let mut r = Resampler::new(32768.0, 48000.0);
    let mut out = Vec::new();
    let mut frames = 0;
    for _ in 0..60 {
        r.process(&ramp(546), &mut out);
        frames += out.len() / 2;
    }
    let want = 546 * 60 * 48000 / 32768;
    assert!(
        (frames as i64 - want as i64).abs() <= 2,
        "produced {frames} frames, expected about {want}"
    );
}

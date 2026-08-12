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

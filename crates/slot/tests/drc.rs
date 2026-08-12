use slot::drc::drc_ratio;

#[test]
fn drc_ratio_is_bounded_and_corrects_toward_target() {
    assert!(drc_ratio(0, 2048) > 1.0); // starved, speed up consumption
    assert!(drc_ratio(4096, 2048) < 1.0); // overfull, slow down
    assert_eq!(drc_ratio(2048, 2048), 1.0);
    for q in [0, 1, 100, 100_000] {
        let r = drc_ratio(q, 2048);
        assert!(
            (0.995..=1.005).contains(&r),
            "ratio {r} out of bounds for q={q}"
        );
    }
}

#[test]
fn a_sink_with_no_buffer_yet_yields_a_neutral_ratio() {
    assert_eq!(drc_ratio(0, 0), 1.0);
}

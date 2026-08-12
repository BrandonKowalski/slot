use slot::audio::volume;

/// The level was stored in slot.state and drawn as a bar but never reached the audio, so
/// the number moved and nothing got quieter.
#[test]
fn full_volume_is_bit_identical() {
    let mut s = [i16::MIN, -1234, 0, 1234, i16::MAX];
    let before = s;
    volume::apply(&mut s, 100);
    assert_eq!(s, before);
}

#[test]
fn zero_volume_is_silent() {
    let mut s = [i16::MIN, -1234, 4567, i16::MAX];
    volume::apply(&mut s, 0);
    assert_eq!(s, [0, 0, 0, 0]);
}

#[test]
fn a_middle_level_attenuates_without_clipping_or_inverting() {
    let mut s = [i16::MIN, -8000, 8000, i16::MAX];
    volume::apply(&mut s, 50);
    assert!(
        s[0] > i16::MIN && s[0] < 0,
        "negative peak inverted or clipped"
    );
    assert!(
        s[3] < i16::MAX && s[3] > 0,
        "positive peak inverted or clipped"
    );
    assert!(s[2] < 8000 && s[2] > 0);
    assert_eq!(s[1], -s[2], "the curve is not symmetric");
}

/// Every step has to change something, or the bar moves without the sound following.
#[test]
fn each_step_of_five_is_audible_movement() {
    let mut last = gain_at(0);
    for v in (5..=100).step_by(5) {
        let g = gain_at(v);
        assert!(g > last, "volume {v} was not louder than the step below it");
        last = g;
    }
}

fn gain_at(v: u8) -> f32 {
    volume::gain(v)
}

#[test]
fn a_level_above_the_maximum_is_clamped_rather_than_amplifying() {
    assert_eq!(volume::gain(200), volume::gain(100));
}

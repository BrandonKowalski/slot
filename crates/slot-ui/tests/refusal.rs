use slot_ui::Refusal;

/// Peak displacement over a window, so the assertion does not depend on where in the cycle
/// a given millisecond lands.
fn peak(r: &Refusal, from: u64, to: u64) -> f32 {
    (from..to).map(|t| r.offset(t).abs()).fold(0.0, f32::max)
}

#[test]
fn the_shake_decays_and_ends() {
    let r = Refusal::started(1_000);
    let early = peak(&r, 1_000, 1_100);
    let late = peak(&r, 1_200, 1_300);
    assert!(early > 0.0, "nothing moved");
    assert!(
        late < early,
        "the shake is not decaying: {early} then {late}"
    );
    assert_eq!(r.offset(1_400), 0.0, "the shake never ends");
    assert!(!r.active(1_400), "the refusal outlives its own shake");
}

/// It shakes both ways. A plate that only ever slides right is a plate that has moved.
#[test]
fn the_shake_crosses_where_it_started() {
    let r = Refusal::started(0);
    let left = (0..300).map(|t| r.offset(t)).fold(f32::INFINITY, f32::min);
    let right = (0..300)
        .map(|t| r.offset(t))
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(left < 0.0 && right > 0.0, "{left} to {right}");
}

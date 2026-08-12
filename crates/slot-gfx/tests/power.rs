use slot_gfx::{screen_brightness, screen_scale, screen_width};

#[test]
fn power_on_expands_from_a_line_to_the_full_frame() {
    assert!(screen_scale(0.0) < 0.02, "it starts at full height");
    assert!(screen_scale(0.5) > 0.2 && screen_scale(0.5) < 1.0);
    assert!(
        (screen_scale(1.0) - 1.0).abs() < 0.001,
        "it never reaches full height"
    );
}

/// The overshoot is what makes it read as a tube or panel striking rather than a wipe.
#[test]
fn the_brightness_overshoots_before_it_settles() {
    let peak = (0..=100)
        .map(|i| screen_brightness(i as f32 / 100.0))
        .fold(0.0f32, f32::max);
    assert!(peak > 1.05, "no overshoot, peak {peak:.2}");
    assert!(
        (screen_brightness(1.0) - 1.0).abs() < 0.01,
        "it settles bright or dim"
    );
}

/// The picture ends as a dot, not as a line the full width of the screen. Only the last of
/// the collapse does it, or the whole power on reads as an iris rather than a panel.
#[test]
fn the_line_closes_to_a_dot_at_the_very_end() {
    assert!(screen_width(0.0) < 0.02, "the line never closes to a dot");
    assert!(
        screen_width(0.5) > 0.99,
        "the collapse eats the whole animation"
    );
}

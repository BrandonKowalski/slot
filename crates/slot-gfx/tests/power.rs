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

#[test]
fn the_line_closes_to_a_dot_at_the_very_end() {
    assert!(screen_width(0.0) < 0.02, "the line never closes to a dot");
    assert!(
        screen_width(0.5) > 0.99,
        "the collapse eats the whole animation"
    );
}

#[test]
fn the_screen_powers_up_inside_the_game_rect() {
    let dest = (80.0, 80.0, 480.0, 320.0);
    let (x, y, w, h) = slot_gfx::screen_rect_in(1.0, dest);
    assert_eq!((x, y, w, h), dest);
    let (x, y, w, h) = slot_gfx::screen_rect_in(0.0, dest);
    assert!(w < 480.0 && h < 320.0, "a dark screen is still full size");
    assert!((x + w / 2.0 - 320.0).abs() < 0.01 && (y + h / 2.0 - 240.0).abs() < 0.01);
}

use slot_gfx::{blit_rect, blit_rect_fit, fit_rect, fit_scale};

#[test]
fn integer_scale_never_fractional() {
    assert_eq!(fit_scale(1440, 960), 2);
    assert_eq!(fit_scale(1500, 1000), 2); // rounds down, never 2.08
    assert_eq!(fit_scale(700, 400), 1); // clamps to 1 below native
    assert_eq!(fit_scale(2160, 1440), 3);
}

#[test]
fn integer_scale_is_limited_by_the_tighter_axis() {
    assert_eq!(fit_scale(2160, 960), 2);
    assert_eq!(fit_scale(1440, 1440), 2);
}

#[test]
fn blit_rect_centres_the_scaled_output_in_the_window() {
    assert_eq!(fit_rect(1440, 960), (0, 0, 1440, 960));
    assert_eq!(fit_rect(1500, 1000), (30, 20, 1440, 960));
    assert_eq!(fit_rect(2160, 960), (360, 0, 1440, 960));
}

#[test]
fn blit_rect_overflows_symmetrically_when_the_window_is_too_small() {
    assert_eq!(fit_rect(700, 400), (-10, -40, 720, 480));
}

#[test]
fn a_panel_smaller_than_the_composite_fits_rather_than_crops() {
    let (x, y, w, h) = blit_rect_fit((640, 480), 0.0);
    assert!(w <= 640 && h <= 480, "{w}x{h} does not fit a 640x480 panel");
    assert!(
        (w as f32 / h as f32 - 1.5).abs() < 0.01,
        "aspect was not preserved"
    );
    assert!(x >= 0 && y >= 0);
}

#[test]
fn the_device_panel_takes_the_fit_and_a_desktop_window_does_not() {
    assert_eq!(blit_rect((640, 480), 0.0), blit_rect_fit((640, 480), 0.0));
    assert_eq!(blit_rect((1500, 1000), 0.0), fit_rect(1500, 1000));
}

#[test]
fn the_fit_is_centred_and_shakes_with_the_picture() {
    let (x, y, w, h) = blit_rect_fit((800, 480), 0.0);
    assert_eq!((x, w), ((800 - w) / 2, 720));
    assert_eq!((y, h), (0, 480));
    assert!(blit_rect_fit((800, 480), 6.0).0 > x);
}

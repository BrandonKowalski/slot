use slot_gfx::blit_rect;

#[test]
fn a_shake_moves_the_whole_image_sideways() {
    let still = blit_rect((1440, 960), 0.0);
    let shaken = blit_rect((1440, 960), 6.0);
    assert_eq!(shaken.1, still.1, "the shake is vertical, it should not be");
    assert_ne!(shaken.0, still.0, "the image did not move");
    assert_eq!(
        (shaken.2, shaken.3),
        (still.2, still.3),
        "the image was resized, not moved"
    );
}

#[test]
fn the_shake_scales_with_the_window_so_it_reads_the_same_at_any_size() {
    let small = blit_rect((720, 480), 6.0).0 - blit_rect((720, 480), 0.0).0;
    let big = blit_rect((2160, 1440), 6.0).0 - blit_rect((2160, 1440), 0.0).0;
    assert!(
        big.abs() > small.abs(),
        "a 3x window shook by the same pixel count"
    );
}

/// Both ways, and the same distance each way. A blit that only ever moved right would read as
/// the picture having slid rather than the device having flinched.
#[test]
fn the_shake_is_symmetric_about_the_centred_rect() {
    let still = blit_rect((1440, 960), 0.0).0;
    let left = blit_rect((1440, 960), -6.0).0;
    let right = blit_rect((1440, 960), 6.0).0;
    assert_eq!(still - left, right - still);
    assert!(left < still && still < right);
}

/// The shake is a displacement of the same rect, so with none of it the geometry is exactly
/// what a still frame gets. Anything else and every frame would be a shaken frame.
#[test]
fn no_shake_is_the_plain_centred_rect() {
    assert_eq!(blit_rect((1500, 1000), 0.0), slot_gfx::fit_rect(1500, 1000));
}

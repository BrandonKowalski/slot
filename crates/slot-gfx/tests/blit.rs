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

#[test]
fn the_shake_is_symmetric_about_the_centred_rect() {
    let still = blit_rect((1440, 960), 0.0).0;
    let left = blit_rect((1440, 960), -6.0).0;
    let right = blit_rect((1440, 960), 6.0).0;
    assert_eq!(still - left, right - still);
    assert!(left < still && still < right);
}

#[test]
fn no_shake_is_the_plain_centred_rect() {
    assert_eq!(blit_rect((1500, 1000), 0.0), slot_gfx::fit_rect(1500, 1000));
}

#[test]
fn a_4_by_3_panel_fills_its_width_from_the_top() {
    assert!(slot_gfx::framed((640, 480)));
    assert!(!slot_gfx::framed((720, 480)));
    assert_eq!(slot_gfx::screen_rect((640, 480)), (0, 0, 640, 427));
    assert_eq!(blit_rect((640, 480), 0.0), (0, 53, 640, 427));
    assert_eq!(blit_rect((1280, 960), 0.0), (0, 107, 1280, 853));
}

#[test]
fn the_shelf_sits_at_the_bottom_and_a_game_rises_to_the_top() {
    assert_eq!(slot_gfx::lifted_rect((640, 480), 0.0), (0, 53, 640, 427));
    assert_eq!(slot_gfx::lifted_rect((640, 480), 1.0), (0, 0, 640, 427));
    let mid = slot_gfx::lifted_rect((640, 480), 0.5).1;
    assert!(0 < mid && mid < 53);
}

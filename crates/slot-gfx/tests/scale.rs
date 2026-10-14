use slot_gfx::{blit_rect, blit_rect_fit, fit_rect, fit_scale};

#[test]
fn integer_scale_never_fractional() {
    assert_eq!(fit_scale(1440, 960), 2);
    assert_eq!(fit_scale(1500, 1000), 2);
    assert_eq!(fit_scale(700, 400), 1);
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
    assert_eq!(blit_rect((600, 480), 0.0), blit_rect_fit((600, 480), 0.0));
    assert_eq!(blit_rect((1500, 1000), 0.0), fit_rect(1500, 1000));
}

#[test]
fn the_fit_is_centred_and_shakes_with_the_picture() {
    let (x, y, w, h) = blit_rect_fit((800, 480), 0.0);
    assert_eq!((x, w), ((800 - w) / 2, 720));
    assert_eq!((y, h), (0, 480));
    assert!(blit_rect_fit((800, 480), 6.0).0 > x);
}

mod game_placement {
    use slot_gfx::{canvas_is_whole_texture, game_rect, OUT_H, OUT_W, SRC_H, SRC_W};

    #[test]
    fn the_game_texture_is_gba_sized_whatever_the_canvas() {
        assert_eq!((SRC_W, SRC_H), (240, 160));
    }

    #[test]
    fn actual_size_is_the_largest_whole_scale_that_fits_centred() {
        let (x, y, w, h) = game_rect((SRC_W, SRC_H), false);
        let scale = (OUT_W / SRC_W).min(OUT_H / SRC_H) as f32;
        assert_eq!((w, h), (SRC_W as f32 * scale, SRC_H as f32 * scale));
        assert_eq!(
            (x, y),
            (
                ((OUT_W as f32 - w) / 2.0).floor(),
                ((OUT_H as f32 - h) / 2.0).floor()
            )
        );

        let (_, _, w, h) = game_rect((160, 144), false);
        let scale = (OUT_W / 160).min(OUT_H / 144) as f32;
        assert_eq!((w, h), (160.0 * scale, 144.0 * scale));
    }

    #[test]
    fn stretch_fills_the_canvas_with_the_aspect_kept() {
        for shown in [(SRC_W, SRC_H), (160, 144)] {
            let (x, y, w, h) = game_rect(shown, true);
            assert!(w <= OUT_W as f32 && h <= OUT_H as f32, "{w}x{h} overflows");
            assert!(
                (w - OUT_W as f32).abs() < 1.0 || (h - OUT_H as f32).abs() < 1.0,
                "{w}x{h} touches neither edge"
            );
            let want = shown.0 as f32 / shown.1 as f32;
            assert!((w / h - want).abs() < 0.01, "{w}x{h} is not {want:.3}");
            assert!(x >= 0.0 && y >= 0.0);
        }
    }

    #[test]
    fn a_720_canvas_is_the_whole_texture_at_3x_and_a_640_one_is_not() {
        assert_eq!(canvas_is_whole_texture(), OUT_W == 720);
        if OUT_W == 720 {
            assert_eq!(game_rect((SRC_W, SRC_H), false), (0.0, 0.0, 720.0, 480.0));
        } else {
            assert_eq!(game_rect((SRC_W, SRC_H), false), (80.0, 80.0, 480.0, 320.0));
            assert_eq!(game_rect((160, 144), false), (80.0, 24.0, 480.0, 432.0));
            assert_eq!(game_rect((SRC_W, SRC_H), true), (0.0, 26.0, 640.0, 427.0));
            assert_eq!(game_rect((160, 144), true), (53.0, 0.0, 533.0, 480.0));
        }
    }

    #[test]
    fn a_4_by_3_panel_is_only_framed_when_the_canvas_is_not_4_by_3() {
        assert_eq!(slot_gfx::framed((640, 480)), OUT_W * 3 != OUT_H * 4);
        assert!(!slot_gfx::framed((720, 480)));
    }
}

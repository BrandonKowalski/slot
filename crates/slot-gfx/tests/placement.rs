use slot_gfx::{framed, game_layout, screen_rect, GameLayout, Sizing, OUT_H, OUT_W};

fn px(window: (u32, u32), l: GameLayout) -> (i32, i32, i32, i32) {
    let area = match l.panel || !framed(window) {
        true if framed(window) => window,
        true => (OUT_W, OUT_H),
        false => (window.0, screen_rect(window).3 as u32),
    };
    let (sx, sy) = (area.0 as f32 / OUT_W as f32, area.1 as f32 / OUT_H as f32);
    (
        (l.rect[0] * sx).round() as i32,
        (l.rect[1] * sy).round() as i32,
        (l.rect[2] * sx).round() as i32,
        (l.rect[3] * sy).round() as i32,
    )
}

#[test]
fn integer_picks_the_largest_whole_multiple_and_takes_the_strip_only_when_it_must() {
    let gb = game_layout((720, 480), (160, 144), Sizing::Integer);
    assert!(!gb.panel);
    assert_eq!(px((720, 480), gb), (120, 24, 480, 432));
    let gb = game_layout((640, 480), (160, 144), Sizing::Integer);
    assert!(gb.panel);
    assert_eq!(px((640, 480), gb), (80, 24, 480, 432));
    let gba = game_layout((640, 480), (240, 160), Sizing::Integer);
    assert!(!gba.panel);
    assert_eq!(px((640, 480), gba), (80, 53, 480, 320));
}

#[test]
fn band_and_panel_fill_their_area() {
    for window in [(720, 480), (640, 480)] {
        assert_eq!(
            game_layout(window, (160, 144), Sizing::Band),
            GameLayout::BAND
        );
    }
    assert_eq!(
        game_layout((720, 480), (160, 144), Sizing::Panel),
        GameLayout::BAND
    );
    assert_eq!(
        game_layout((640, 480), (160, 144), Sizing::Panel),
        GameLayout {
            panel: true,
            ..GameLayout::BAND
        }
    );
}

#[test]
fn fit_is_the_largest_uncropped_picture() {
    let wide = game_layout((720, 480), (160, 144), Sizing::Fit);
    assert!(!wide.panel);
    assert_eq!(px((720, 480), wide), (93, 0, 533, 480));
    let square = game_layout((640, 480), (160, 144), Sizing::Fit);
    assert!(square.panel);
    assert_eq!(px((640, 480), square), (53, 0, 533, 480));
    for (window, l) in [((720, 480), wide), ((640, 480), square)] {
        let (_, _, w, h) = px(window, l);
        assert!((w as f32 - h as f32 * 160.0 / 144.0).abs() <= 1.0);
    }
}

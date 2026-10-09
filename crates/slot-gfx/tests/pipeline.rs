use slot_gfx::{
    blue_light_gain, canvas_size, lcd3x_factors, lcd3x_mask, Compositor, Draw, HeadlessSurface,
    ScreenEffect, OUT_H, OUT_W, SRC_H, SRC_W,
};
use std::sync::{Mutex, MutexGuard, PoisonError};

static GL: Mutex<()> = Mutex::new(());

fn compositor() -> Option<(MutexGuard<'static, ()>, HeadlessSurface, Compositor)> {
    let guard = GL.lock().unwrap_or_else(PoisonError::into_inner);
    let surface = HeadlessSurface::new().ok()?;
    let compositor = Compositor::new(&surface).ok()?;
    Some((guard, surface, compositor))
}

fn px(frame: &[u8], x: usize, y: usize) -> [u8; 3] {
    let o = (y * OUT_W as usize + x) * 4;
    [frame[o], frame[o + 1], frame[o + 2]]
}

fn flat_shot(rgb: [u8; 3]) -> Vec<u8> {
    std::iter::repeat_n([rgb[0], rgb[1], rgb[2], 255], (SRC_W * SRC_H) as usize)
        .flatten()
        .collect()
}

const GB_X: usize = 40;
const GB_Y: usize = 8;
const GB_W: usize = 160;
const GB_H: usize = 144;

const GB_RECT: [f32; 4] = [
    GB_X as f32 / SRC_W as f32,
    GB_Y as f32 / SRC_H as f32,
    GB_W as f32 / SRC_W as f32,
    GB_H as f32 / SRC_H as f32,
];

const WHOLE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

fn gb_shaped(inside: impl Fn(usize, usize) -> [u8; 3], margin: [u8; 3]) -> Vec<u8> {
    let mut buf = Vec::with_capacity((SRC_W * SRC_H * 4) as usize);
    for y in 0..SRC_H as usize {
        for x in 0..SRC_W as usize {
            let in_window = (GB_X..GB_X + GB_W).contains(&x) && (GB_Y..GB_Y + GB_H).contains(&y);
            let rgb = match in_window {
                true => inside(x - GB_X, y - GB_Y),
                false => margin,
            };
            buf.extend_from_slice(&[rgb[2], rgb[1], rgb[0], 0]);
        }
    }
    buf
}

fn stretched(rgb: [u8; 3], x: usize, y: usize) -> [i32; 3] {
    let fx = ((x as f32 + 0.5) * GB_W as f32 / OUT_W as f32).fract();
    let fy = ((y as f32 + 0.5) * GB_H as f32 / OUT_H as f32).fract();
    let cell = lcd3x_factors(fx, fy);
    [0, 1, 2].map(|ch| (rgb[ch] as f32 * cell[ch]).round() as i32)
}

fn masked(rgb: [u8; 3], ox: usize, oy: usize) -> [i32; 3] {
    let cell = lcd3x_mask()[oy][ox];
    [0, 1, 2].map(|ch| (rgb[ch] as f32 * cell[ch]).round() as i32)
}

fn period_x(frame: &[u8], xs: std::ops::Range<usize>, ys: std::ops::Range<usize>) -> Option<usize> {
    (1..=8).find(|p| {
        (xs.start..xs.end - p).all(|x| ys.clone().all(|y| px(frame, x, y) == px(frame, x + p, y)))
    })
}

fn period_y(frame: &[u8], xs: std::ops::Range<usize>, ys: std::ops::Range<usize>) -> Option<usize> {
    (1..=8).find(|p| {
        (ys.start..ys.end - p).all(|y| xs.clone().all(|x| px(frame, x, y) == px(frame, x, y + p)))
    })
}

#[test]
fn game_pass_multiplies_every_output_cell_by_the_lcd3x_mask() {
    let Some((_g, _s, mut c)) = compositor() else {
        return;
    };
    let grey = vec![0x80u8; (SRC_W * SRC_H * 4) as usize];
    c.begin_frame();
    c.upload_game(&grey);
    c.draw_game();
    let frame = c.read_frame();

    let mask = lcd3x_mask();
    let mut worst = 0i32;
    for y in 0..OUT_H as usize {
        for x in 0..OUT_W as usize {
            let got = px(&frame, x, y);
            for (ch, gain) in mask[y % 3][x % 3].iter().enumerate() {
                let want = (128.0 * gain).round() as i32;
                worst = worst.max((want - got[ch] as i32).abs());
            }
        }
    }
    assert!(worst <= 1, "max channel deviation {worst} from the mask");
}

#[test]
fn the_game_frame_keeps_its_orientation_from_upload_to_readback() {
    let Some((_g, _s, mut c)) = compositor() else {
        return;
    };
    let mut src = vec![0u8; (SRC_W * SRC_H * 4) as usize];
    src[0..3].copy_from_slice(&[255, 255, 255]);
    c.begin_frame();
    c.upload_game(&src);
    c.draw_game();
    let frame = c.read_frame();

    for y in 0..3 {
        for x in 0..3 {
            assert!(
                px(&frame, x, y)[0] > 100,
                "top left source pixel missing at {x},{y}"
            );
        }
    }
    assert_eq!(px(&frame, 3, 0), [0, 0, 0], "bled one cell right");
    assert_eq!(px(&frame, 0, 3), [0, 0, 0], "bled one cell down");
    assert_eq!(
        px(&frame, 0, OUT_H as usize - 1),
        [0, 0, 0],
        "frame is upside down"
    );
    assert_eq!(
        px(&frame, OUT_W as usize - 1, 0),
        [0, 0, 0],
        "frame is mirrored"
    );
}

#[test]
fn the_picture_strikes_as_a_band_at_the_centre_before_it_fills_the_frame() {
    let Some((_g, _s, mut c)) = compositor() else {
        return;
    };
    let grey = vec![0x80u8; (SRC_W * SRC_H * 4) as usize];
    let peak = |c: &mut Compositor, t: f32| {
        c.set_screen_power(t);
        c.begin_frame();
        c.upload_game(&grey);
        c.draw_game();
        let frame = c.read_frame();
        let brightest = frame.chunks_exact(4).map(|p| p[0]).max().unwrap_or(0);
        (frame, brightest)
    };

    let (striking, bright) = peak(&mut c, 0.35);
    let lit = |frame: &[u8], y: usize| (0..OUT_W as usize).any(|x| px(frame, x, y) != [0, 0, 0]);
    assert!(lit(&striking, OUT_H as usize / 2), "nothing at the centre");
    assert!(!lit(&striking, 1), "the picture already reaches the top");
    assert!(
        !lit(&striking, OUT_H as usize - 2),
        "the picture already reaches the bottom"
    );

    let (settled, normal) = peak(&mut c, 1.0);
    assert!(
        lit(&settled, 1) && lit(&settled, OUT_H as usize - 2),
        "the settled picture is short"
    );
    assert!(
        bright > normal,
        "the strike at {bright} is no brighter than the settled picture at {normal}"
    );
}

#[test]
fn the_game_marker_draws_the_picture_where_it_sits_in_the_list() {
    let Some((_g, _s, mut c)) = compositor() else {
        return;
    };
    let white = vec![0xffu8; (SRC_W * SRC_H * 4) as usize];
    let full = |colour: [f32; 4]| Draw::Rect {
        x: 0.0,
        y: 0.0,
        w: OUT_W as f32,
        h: OUT_H as f32,
        colour,
    };
    c.set_screen_power(1.0);

    c.begin_frame();
    c.upload_game(&white);
    c.draw_list(&[full([1.0, 0.0, 0.0, 1.0]), Draw::Game]);
    let under = c.read_frame();
    let blue = under.chunks_exact(4).map(|p| p[2]).max().unwrap_or(0);
    assert!(blue > 100, "the picture never drew over the red rect");

    c.begin_frame();
    c.draw_list(&[Draw::Game, full([0.0, 0.0, 1.0, 1.0])]);
    let over = c.read_frame();
    assert_eq!(
        px(&over, OUT_W as usize / 2, OUT_H as usize / 2),
        [0, 0, 255],
        "the picture was drawn over the item listed after it"
    );
}

#[test]
fn a_saved_shot_is_drawn_through_the_lcd_pass() {
    let Some((_g, _s, mut c)) = compositor() else {
        return;
    };
    let shot = flat_shot([200, 200, 200]);
    let tex = c.create_texture_nearest(SRC_W, SRC_H, &shot);
    c.set_screen_power(1.0);

    c.begin_frame();
    c.draw_list(&[Draw::Tex {
        x: 0.0,
        y: 0.0,
        w: OUT_W as f32,
        h: OUT_H as f32,
        tex,
        alpha: 1.0,
    }]);
    let plain = c.read_frame();

    c.begin_frame();
    c.draw_list(&[Draw::Shot { tex }]);
    let lit = c.read_frame();

    assert_ne!(plain, lit, "the shot is not going through the lcd pass");
    for (y, row) in lcd3x_mask().iter().enumerate() {
        for (x, cell) in row.iter().enumerate() {
            let got = px(&lit, x, y)[0] as f32;
            let want = 200.0 * cell[0];
            assert!(
                (got - want).abs() < 3.0,
                "cell {x},{y} does not match the mask: {got} against {want}"
            );
        }
    }
}

#[test]
fn a_still_is_not_cropped_by_the_picture_mode() {
    let Some((_g, _s, mut c)) = compositor() else {
        return;
    };
    let mut shot = flat_shot([120, 120, 120]);
    for (x, y) in [(0, 0), (SRC_W as usize - 1, SRC_H as usize - 1)] {
        let o = (y * SRC_W as usize + x) * 4;
        shot[o..o + 3].copy_from_slice(&[255, 0, 0]);
    }
    let tex = c.create_texture_nearest(SRC_W, SRC_H, &shot);
    let src = gb_shaped(|x, y| [(x * 3) as u8, (y * 5) as u8, 200], [0, 0, 0]);
    c.set_screen_power(1.0);

    let framed = |c: &mut Compositor, rect: [f32; 4]| {
        c.set_game_source_rect(rect);
        c.begin_frame();
        c.upload_game(&src);
        c.draw_list(&[Draw::Shot { tex }]);
        let still = c.read_frame();
        c.begin_frame();
        c.draw_game();
        (still, c.read_frame())
    };

    let (still_actual, game_actual) = framed(&mut c, WHOLE);
    let (still_full, game_full) = framed(&mut c, GB_RECT);

    assert!(
        still_actual == still_full,
        "the polaroid changed shape when the panel did"
    );
    assert!(
        game_actual != game_full,
        "the game did not stretch, so this proves nothing about the still"
    );
}

#[test]
fn draw_list_rects_land_in_top_left_pixel_space() {
    let Some((_g, _s, mut c)) = compositor() else {
        return;
    };
    c.begin_frame();
    c.draw_list(&[Draw::Rect {
        x: 2.0,
        y: 1.0,
        w: 4.0,
        h: 2.0,
        colour: [1.0, 0.0, 0.0, 1.0],
    }]);
    let frame = c.read_frame();

    assert_eq!(px(&frame, 2, 1), [255, 0, 0]);
    assert_eq!(px(&frame, 5, 2), [255, 0, 0]);
    assert_ne!(px(&frame, 1, 1), [255, 0, 0], "rect starts one pixel early");
    assert_ne!(px(&frame, 6, 2), [255, 0, 0], "rect runs one pixel long");
    assert_ne!(px(&frame, 2, 3), [255, 0, 0], "rect runs one row long");
}

#[test]
fn a_replaced_texture_draws_its_new_contents() {
    let Some((_g, _s, mut c)) = compositor() else {
        return;
    };
    let tex = c.create_texture(1, 1, &[255, 0, 0, 255]);
    c.update_texture(tex, 1, 1, &[0, 0, 255, 255]);
    c.begin_frame();
    c.draw_list(&[Draw::Tex {
        x: 0.0,
        y: 0.0,
        w: 4.0,
        h: 4.0,
        tex,
        alpha: 1.0,
    }]);
    let frame = c.read_frame();
    assert_eq!(px(&frame, 1, 1), [0, 0, 255]);
}

#[test]
fn blue_light_warms_monotonically_and_clamps_at_the_last_step() {
    assert_eq!(blue_light_gain(0), [1.0, 1.0, 1.0]);
    let warmest = blue_light_gain(9);
    assert!(
        (warmest[1] - 0.82).abs() < 0.005 && (warmest[2] - 0.62).abs() < 0.005,
        "warmest step is {warmest:?}"
    );
    assert_eq!(blue_light_gain(200), warmest, "step must clamp, not wrap");
    for step in 0..9 {
        let a = blue_light_gain(step);
        let b = blue_light_gain(step + 1);
        assert_eq!(b[0], 1.0, "red must not move");
        assert!(b[1] < a[1] && b[2] < a[2], "step {step} did not warm");
        assert!(b[2] < b[1], "blue must fall faster than green");
    }
}

#[test]
fn an_unturned_image_draws_exactly_as_a_plain_one() {
    let Some((_g, _s, mut c)) = compositor() else {
        return;
    };
    let rgba: Vec<u8> = (0..16u32 * 8)
        .flat_map(|i| [(i * 7) as u8, (i * 13) as u8, (i * 29) as u8, 255])
        .collect();
    let tex = c.create_texture(16, 8, &rgba);

    c.begin_frame();
    c.draw_list(&[Draw::Tex {
        x: 101.3,
        y: 57.6,
        w: 37.0,
        h: 19.0,
        tex,
        alpha: 0.8,
    }]);
    let plain = c.read_frame();

    c.begin_frame();
    c.draw_list(&[Draw::Turned {
        x: 101.3,
        y: 57.6,
        w: 37.0,
        h: 19.0,
        tex,
        alpha: 0.8,
        turn: 0.0,
    }]);
    let turned = c.read_frame();

    assert!(plain == turned, "a turn of zero moved something");
}

#[test]
fn a_quarter_turn_takes_the_top_left_corner_to_the_top_right() {
    let Some((_g, _s, mut c)) = compositor() else {
        return;
    };
    let mut rgba = [0u8, 0, 255, 255].repeat(8);
    rgba[0..4].copy_from_slice(&[255, 0, 0, 255]);
    let tex = c.create_texture_nearest(4, 2, &rgba);

    c.begin_frame();
    c.draw_list(&[Draw::Turned {
        x: 100.0,
        y: 100.0,
        w: 40.0,
        h: 20.0,
        tex,
        alpha: 1.0,
        turn: std::f32::consts::FRAC_PI_2,
    }]);
    let frame = c.read_frame();

    assert_eq!(
        px(&frame, 127, 93),
        [255, 0, 0],
        "the red texel is not top right"
    );
    assert_eq!(
        px(&frame, 113, 93),
        [0, 0, 255],
        "the top left did not move"
    );
    assert_eq!(px(&frame, 113, 127), [0, 0, 255]);
}

#[test]
fn the_default_source_rect_draws_exactly_as_before() {
    let Some((_g, _s, mut c)) = compositor() else {
        return;
    };
    let src: Vec<u8> = (0..(SRC_W * SRC_H) as usize)
        .flat_map(|i| {
            let (x, y) = (i % SRC_W as usize, i / SRC_W as usize);
            let rgb = [
                (x * 7 + y * 3) as u8,
                (x * 13 + y * 29) as u8,
                (x * 31 + y * 11) as u8,
            ];
            [rgb[2], rgb[1], rgb[0], 0]
        })
        .collect();
    c.set_screen_power(1.0);

    c.begin_frame();
    c.upload_game(&src);
    c.draw_game();
    let default = c.read_frame();

    for (sx, sy) in [(0, 0), (1, 0), (0, 1), (113, 37), (120, 80), (239, 159)] {
        let o = (sy * SRC_W as usize + sx) * 4;
        let want_rgb = [src[o + 2], src[o + 1], src[o]];
        for oy in 0..3 {
            for ox in 0..3 {
                let got = px(&default, sx * 3 + ox, sy * 3 + oy);
                for (ch, want) in masked(want_rgb, ox, oy).iter().enumerate() {
                    assert!(
                        (want - got[ch] as i32).abs() <= 1,
                        "source {sx},{sy} cell {ox},{oy} channel {ch}: {} against {want}",
                        got[ch],
                    );
                }
            }
        }
    }

    c.set_game_source_rect(WHOLE);
    c.begin_frame();
    c.upload_game(&src);
    c.draw_game();
    assert!(
        c.read_frame() == default,
        "the whole texture asked for is not the whole texture by default"
    );
}

#[test]
fn fullscreen_puts_the_pictures_corners_in_the_panels_corners() {
    let Some((_g, _s, mut c)) = compositor() else {
        return;
    };
    let corner = |x: usize, y: usize| match (x, y) {
        (0, 0) => Some([255, 0, 0]),
        (x, 0) if x == GB_W - 1 => Some([0, 255, 0]),
        (0, y) if y == GB_H - 1 => Some([0, 0, 255]),
        (x, y) if x == GB_W - 1 && y == GB_H - 1 => Some([255, 255, 0]),
        _ => None,
    };
    const MARGIN: [u8; 3] = [0, 255, 255];
    let src = gb_shaped(|x, y| corner(x, y).unwrap_or([90, 90, 90]), MARGIN);
    c.set_screen_power(1.0);

    c.set_game_source_rect(WHOLE);
    c.begin_frame();
    c.upload_game(&src);
    c.draw_game();
    let actual = c.read_frame();
    let got = px(&actual, 0, 0);
    for (ch, want) in masked(MARGIN, 0, 0).iter().enumerate() {
        assert!(
            (want - got[ch] as i32).abs() <= 1,
            "at actual size the panel's corner is not the margin: {got:?}"
        );
    }

    c.set_game_source_rect(GB_RECT);
    c.begin_frame();
    c.upload_game(&src);
    c.draw_game();
    let full = c.read_frame();

    let last_x = OUT_W as usize - 1;
    let last_y = OUT_H as usize - 1;
    for (name, (x, y), rgb) in [
        ("top left", (0, 0), [255, 0, 0]),
        ("top right", (last_x, 0), [0, 255, 0]),
        ("bottom left", (0, last_y), [0, 0, 255]),
        ("bottom right", (last_x, last_y), [255, 255, 0]),
    ] {
        let cell = stretched(rgb, x, y);
        let got = px(&full, x, y);
        for (ch, want) in cell.iter().enumerate() {
            assert!(
                (want - got[ch] as i32).abs() <= 1,
                "{name}: {got:?} against {cell:?}"
            );
        }
    }

    let strayed = (0..OUT_H as usize).any(|y| {
        (0..OUT_W as usize).any(|x| {
            let p = px(&full, x, y);
            p[1] > 60 && p[2] > 60 && p[0] < 20
        })
    });
    assert!(!strayed, "the margin is still on screen in fullscreen");
}

#[test]
fn the_mask_follows_the_game_pixels_in_both_modes() {
    let Some((_g, _s, mut c)) = compositor() else {
        return;
    };
    let src = gb_shaped(|_, _| [0x80, 0x80, 0x80], [0, 0, 0]);
    c.set_screen_power(1.0);

    c.set_game_source_rect(WHOLE);
    c.begin_frame();
    c.upload_game(&src);
    c.draw_game();
    let actual = c.read_frame();

    c.set_game_source_rect(GB_RECT);
    c.begin_frame();
    c.upload_game(&src);
    c.draw_game();
    let full = c.read_frame();

    let lit_x = GB_X * 3..(GB_X + GB_W) * 3;
    let lit_y = GB_Y * 3..(GB_Y + GB_H) * 3;
    assert_eq!(
        period_x(&actual, lit_x.clone(), lit_y.clone()),
        Some(3),
        "actual size: the grille does not repeat every 3 px across"
    );
    assert_eq!(
        period_y(&actual, lit_x, lit_y),
        Some(3),
        "actual size: the grille does not repeat every 3 px down"
    );

    let mut worst = 0i32;
    for y in 0..OUT_H as usize {
        for x in 0..OUT_W as usize {
            let got = px(&full, x, y);
            for (ch, want) in stretched([0x80; 3], x, y).iter().enumerate() {
                worst = worst.max((want - got[ch] as i32).abs());
            }
        }
    }
    assert!(
        worst <= 1,
        "fullscreen: the grille strays {worst} from the game's pixels"
    );
}

fn grid_frame(c: &mut Compositor, window: (u32, u32)) -> (Vec<u8>, usize, usize) {
    c.fit(window);
    c.set_screen_effect(ScreenEffect::Grid);
    c.set_screen_power(1.0);
    c.begin_frame();
    c.upload_game(&flat_shot([0xc0, 0xc0, 0xc0]));
    c.draw_game();
    let (w, h) = canvas_size(window);
    (c.read_frame(), w as usize, h as usize)
}

fn grid_lines(frame: &[u8], w: usize, h: usize) -> (Vec<usize>, Vec<usize>) {
    let g = |x: usize, y: usize| frame[(y * w + x) * 4 + 1];
    let lit = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| g(x, y))
        .max()
        .unwrap();
    let dark = |v: u8| (v as u32) * 2 < lit as u32;
    let cols = (0..w)
        .filter(|&x| (0..h).filter(|&y| dark(g(x, y))).count() == h)
        .collect();
    let rows = (0..h)
        .filter(|&y| (0..w).filter(|&x| dark(g(x, y))).count() == w)
        .collect();
    (cols, rows)
}

#[test]
fn the_grid_draws_one_dark_line_per_game_pixel_at_3x() {
    let Some((_g, _s, mut c)) = compositor() else {
        return;
    };
    let (frame, w, h) = grid_frame(&mut c, (OUT_W, OUT_H));
    assert_eq!((w, h), (OUT_W as usize, OUT_H as usize));
    let (cols, rows) = grid_lines(&frame, w, h);
    assert_eq!(cols.len(), SRC_W as usize, "dark columns: {cols:?}");
    assert_eq!(rows.len(), SRC_H as usize, "dark rows: {rows:?}");
    assert!(
        cols.windows(2).all(|p| p[1] - p[0] == 3),
        "columns: {cols:?}"
    );
    assert!(rows.windows(2).all(|p| p[1] - p[0] == 3), "rows: {rows:?}");
}

#[test]
fn the_grid_lines_hold_still_under_a_hair_of_texture_drift() {
    let Some((_g, _s, mut c)) = compositor() else {
        return;
    };
    for drift in [-3e-4f32, 3e-4] {
        c.set_game_source_rect([drift / SRC_W as f32, drift / SRC_H as f32, 1.0, 1.0]);
        let (frame, w, h) = grid_frame(&mut c, (OUT_W, OUT_H));
        let (cols, rows) = grid_lines(&frame, w, h);
        assert!(
            cols.iter().all(|x| x % 3 == 0),
            "drift {drift}: columns {cols:?}"
        );
        assert!(
            rows.iter().all(|y| y % 3 == 0),
            "drift {drift}: rows {rows:?}"
        );
    }
}

fn grid_profile(frame: &[u8], w: usize, h: usize, across: bool) -> Vec<f32> {
    let g = |x: usize, y: usize| frame[(y * w + x) * 4 + 1] as f32;
    let (len, lines) = if across { (w, h) } else { (h, w) };
    let at = |i: usize, line: usize| if across { g(i, line) } else { g(line, i) };
    let line = (0..lines)
        .max_by(|a, b| {
            let sum = |l: usize| (0..len).map(|i| at(i, l)).sum::<f32>();
            sum(*a).total_cmp(&sum(*b))
        })
        .unwrap();
    let lit = (0..len).map(|i| at(i, line)).fold(0.0, f32::max);
    (0..len).map(|i| at(i, line) / lit).collect()
}

fn cell_means(profile: &[f32], cells: usize) -> Vec<f32> {
    let span = profile.len() as f32 / cells as f32;
    (0..cells)
        .map(|k| {
            let (a, b) = (k as f32 * span, (k + 1) as f32 * span);
            let lit: f32 = (a.floor() as usize..(b.ceil() as usize).min(profile.len()))
                .map(|x| (b.min(x as f32 + 1.0) - a.max(x as f32)).max(0.0) * profile[x])
                .sum();
            lit / span
        })
        .collect()
}

fn assert_even(profile: &[f32], cells: usize, spread: f32, what: &str) {
    let means = cell_means(profile, cells);
    let (lo, hi) = means
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), m| (lo.min(*m), hi.max(*m)));
    assert!(hi < 0.97, "{what}: a cell has no grid line, mean {hi}");
    assert!(
        hi - lo < spread,
        "{what}: cells run from {lo} to {hi}, unevenly lined"
    );
}

#[test]
fn the_grid_lines_every_game_pixel_evenly_on_a_4_by_3_panel() {
    let Some((_g, _s, mut c)) = compositor() else {
        return;
    };
    let (frame, w, h) = grid_frame(&mut c, (640, 480));
    assert_eq!((w, h), (640, 427));
    assert_even(
        &grid_profile(&frame, w, h, true),
        SRC_W as usize,
        0.18,
        "columns",
    );
    assert_even(
        &grid_profile(&frame, w, h, false),
        SRC_H as usize,
        0.18,
        "rows",
    );
}

fn stretched_gb_grid(c: &mut Compositor, window: (u32, u32)) -> (Vec<u8>, usize, usize) {
    c.set_picture([
        GB_RECT[0],
        GB_RECT[1],
        GB_RECT[0] + GB_RECT[2],
        GB_RECT[1] + GB_RECT[3],
    ]);
    c.set_game_source_rect(GB_RECT);
    c.fit(window);
    c.set_screen_effect(ScreenEffect::Grid);
    c.set_screen_power(1.0);
    c.begin_frame();
    c.upload_game(&gb_shaped(|_, _| [0xc0, 0xc0, 0xc0], [0, 0, 0]));
    c.draw_game();
    let (w, h) = canvas_size(window);
    (c.read_frame(), w as usize, h as usize)
}

#[test]
fn the_grid_lines_every_game_boy_pixel_evenly_when_stretched() {
    let Some((_g, _s, mut c)) = compositor() else {
        return;
    };
    let (frame, w, h) = stretched_gb_grid(&mut c, (OUT_W, OUT_H));
    assert_even(&grid_profile(&frame, w, h, true), GB_W, 0.12, "columns");
    assert_even(&grid_profile(&frame, w, h, false), GB_H, 0.15, "rows");
}

#[test]
fn the_grid_keeps_game_boy_rows_even_when_stretched_on_a_4_by_3_panel() {
    let Some((_g, _s, mut c)) = compositor() else {
        return;
    };
    let (frame, w, h) = stretched_gb_grid(&mut c, (640, 480));
    assert_eq!((w, h), (640, 427));
    let (cols, rows) = grid_lines(&frame, w, h);
    assert_eq!(cols.len(), GB_W, "dark columns: {cols:?}");
    assert!(
        cols.windows(2).all(|p| p[1] - p[0] == 4),
        "columns: {cols:?}"
    );
    assert!(rows.windows(2).all(|p| p[1] - p[0] == 3), "rows: {rows:?}");
    assert_eq!(rows.len(), 142, "dark rows: {rows:?}");
}

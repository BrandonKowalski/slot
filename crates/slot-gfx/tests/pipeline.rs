use slot_gfx::{
    blue_light_gain, lcd3x_mask, Compositor, Draw, HeadlessSurface, OUT_H, OUT_W, SRC_H, SRC_W,
};
use std::sync::{Mutex, MutexGuard, PoisonError};

/// `gl::load_with` writes global function pointers, so two GL tests must not overlap.
static GL: Mutex<()> = Mutex::new(());

/// A GL context is not available everywhere. Skip rather than fail, the same way the mGBA
/// test skips a missing dylib.
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

/// A screenshot as the switcher uploads one: RGBA at the source size, not the core's BGRA.
fn flat_shot(rgb: [u8; 3]) -> Vec<u8> {
    std::iter::repeat_n([rgb[0], rgb[1], rgb[2], 255], (SRC_W * SRC_H) as usize)
        .flatten()
        .collect()
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

/// The FBO is stored bottom up and the source is top down, so a missing flip anywhere in
/// upload, draw or readback shows up as a frame that is upside down or mirrored.
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

/// The power on is a uniform on the game pass, so only a readback says whether the picture
/// is genuinely being squeezed and flashed rather than the curve merely being computed.
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

/// The game layer is an item in the draw list rather than a pass before it, which is the
/// only way the picture can come up in front of a cart that is already seated. Both
/// directions matter: what is listed before the marker is covered by the picture, and what
/// is listed after it is drawn over the picture.
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

/// The switcher shows a still of the same panel, at the same 3x, so it has to carry the same
/// 3x3 cell structure. Blitted flat it reads as a different machine to the game behind it.
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

/// The switcher reuses one texture per ring slot instead of minting a fresh set on every
/// opening. A replace that quietly kept the old contents would show the previous ring.
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

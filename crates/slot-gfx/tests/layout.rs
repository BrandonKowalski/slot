#![cfg(target_os = "macos")]

use slot_gfx::{game_rect, Canvas, Compositor, Draw, HeadlessSurface, ScreenEffect, SRC_H, SRC_W};
use std::sync::{Mutex, MutexGuard, PoisonError};

static GL: Mutex<()> = Mutex::new(());

fn compositor() -> Option<(MutexGuard<'static, ()>, HeadlessSurface, Compositor)> {
    let guard = GL.lock().unwrap_or_else(PoisonError::into_inner);
    let surface = HeadlessSurface::new().ok()?;
    let compositor = Compositor::new(&surface).ok()?;
    Some((guard, surface, compositor))
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x0100_0000_01b3)
    })
}

fn pattern() -> Vec<u8> {
    (0..SRC_W * SRC_H)
        .flat_map(|i| {
            let (x, y) = (i % SRC_W, i / SRC_W);
            [(x * 7) as u8, (y * 11) as u8, ((x + y) * 3) as u8, 255]
        })
        .collect()
}

#[test]
fn the_canvas_on_a_4_by_3_panel_is_the_panel_at_480_lines() {
    assert_eq!(Canvas::of((640, 480)), Canvas { w: 640.0, h: 480.0 });
    assert_eq!(Canvas::of((1280, 960)), Canvas { w: 640.0, h: 480.0 });
}

#[test]
fn the_canvas_is_720_by_480_on_a_panel_that_is_not_4_by_3() {
    for panel in [(720, 480), (1440, 960), (0, 0)] {
        assert_eq!(Canvas::of(panel), Canvas::FRAMED);
    }
}

#[test]
fn the_game_area_on_a_4_by_3_panel_is_3_by_2_at_the_top_or_the_middle() {
    assert_eq!(game_rect((640, 480), true), (0, 0, 640, 427));
    assert_eq!(game_rect((640, 480), false), (0, 27, 640, 427));
}

#[test]
fn the_window_of_a_wider_texture_is_its_centre_and_a_fitting_one_is_whole() {
    let native = Canvas { w: 640.0, h: 480.0 };
    assert_eq!(
        native.window(720.0),
        [40.0 / 720.0, 0.0, 640.0 / 720.0, 1.0]
    );
    assert_eq!(Canvas::FRAMED.window(720.0), [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(native.centred(100.0), 270.0);
}

#[test]
fn a_part_draws_only_its_window_of_the_texture() {
    let Some((_g, _s, mut c)) = compositor() else {
        return;
    };
    let mut rgba = Vec::new();
    for _y in 0..2 {
        rgba.extend_from_slice(&[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255]);
    }
    let tex = c.create_texture_nearest(3, 2, &rgba);
    c.fit((720, 480));
    c.begin_frame();
    c.draw_list(&[Draw::Part {
        x: 0.0,
        y: 0.0,
        w: 720.0,
        h: 480.0,
        tex,
        alpha: 1.0,
        uv: [1.0 / 3.0, 0.0, 1.0 / 3.0, 1.0],
    }]);
    let frame = c.read_frame();
    let at = |x: usize, y: usize| {
        let o = (y * 720 + x) * 4;
        [frame[o], frame[o + 1], frame[o + 2]]
    };
    assert_eq!(
        at(10, 240),
        [0, 255, 0],
        "the left of the part is not the middle texel"
    );
    assert_eq!(
        at(710, 240),
        [0, 255, 0],
        "the right of the part is not the middle texel"
    );
}

#[test]
fn a_native_frame_on_a_4_by_3_panel_is_the_whole_panel() {
    let Some((_g, _s, mut c)) = compositor() else {
        return;
    };
    c.fit((640, 480));
    c.begin_frame();
    c.draw_list(&[Draw::Rect {
        x: 630.0,
        y: 0.0,
        w: 10.0,
        h: 10.0,
        colour: [1.0, 1.0, 1.0, 1.0],
    }]);
    assert_eq!(c.frame_size(), (640, 480));
    let frame = c.read_frame();
    let o = (5 * 640 + 635) * 4;
    assert_eq!(
        &frame[o..o + 3],
        &[255, 255, 255],
        "x 635 is not 1:1 in a native frame"
    );
}

fn game_only(c: &mut Compositor, sp_frame: bool) -> Vec<u8> {
    c.fit((640, 480));
    c.set_sp_frame(sp_frame);
    c.set_screen_effect(ScreenEffect::None);
    c.set_screen_power(1.0);
    c.begin_frame();
    c.upload_game(&pattern());
    c.draw_list(&[Draw::Game]);
    c.read_frame()
}

#[test]
fn a_game_on_a_4_by_3_panel_fills_its_game_area_and_nothing_else() {
    let Some((_g, _s, mut c)) = compositor() else {
        return;
    };
    let row = 640 * 4;
    let top = game_only(&mut c, true);
    let middle = game_only(&mut c, false);
    assert_eq!(top.len(), 640 * 480 * 4);
    assert_eq!(
        fnv(&top[..427 * row]),
        fnv(&middle[27 * row..454 * row]),
        "the game is not the same picture at the top and in the middle"
    );
    assert!(
        top[427 * row..].chunks(4).all(|p| p[..3] == [0, 0, 0]),
        "the strip under a game at the top is not clear"
    );
    assert!(
        middle[..27 * row]
            .chunks(4)
            .chain(middle[454 * row..].chunks(4))
            .all(|p| p[..3] == [0, 0, 0]),
        "the bands around a game in the middle are not clear"
    );
}

#[test]
fn a_frame_reaches_a_4_by_3_panel_one_to_one() {
    let Some((_g, _s, mut c)) = compositor() else {
        return;
    };
    c.fit((640, 480));
    c.begin_frame();
    c.draw_list(&[Draw::Rect {
        x: 0.0,
        y: 0.0,
        w: 640.0,
        h: 1.0,
        colour: [1.0, 1.0, 1.0, 1.0],
    }]);
    let canvas = c.read_frame();
    let panel = c.read_window((640, 480));
    assert_eq!(
        panel, canvas,
        "the frame was moved or scaled on its way out"
    );
}

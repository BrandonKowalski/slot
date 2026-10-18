#![cfg(target_os = "macos")]
//! Fork-only (FORK-NOTES.md): sharp-bilinear softening on colour pictures, the kind a Game Boy
//! Color game draws. Every channel must blend the same way, so stripes stay even in each channel
//! and a softened edge is a mix of the two colours it joins, never a third colour.

use slot_gfx::{Compositor, HeadlessSurface, ScreenEffect, OUT_H, OUT_W, SRC_H, SRC_W};
use std::sync::{Mutex, MutexGuard, PoisonError};

static GL: Mutex<()> = Mutex::new(());

fn compositor() -> Option<(MutexGuard<'static, ()>, HeadlessSurface, Compositor)> {
    let guard = GL.lock().unwrap_or_else(PoisonError::into_inner);
    let surface = HeadlessSurface::new().ok()?;
    let compositor = Compositor::new(&surface).ok()?;
    Some((guard, surface, compositor))
}

const GB_X: usize = 40;
const GB_Y: usize = 8;
const GB_W: usize = 160;
const GB_H: usize = 144;

/// A warm and a cool colour that differ a lot in every channel.
const A: [u8; 3] = [240, 200, 24];
const B: [u8; 3] = [24, 40, 232];

fn px(frame: &[u8], x: usize, y: usize) -> [u8; 3] {
    let o = (y * OUT_W as usize + x) * 4;
    [frame[o], frame[o + 1], frame[o + 2]]
}

fn fitted(c: &mut Compositor, soft: bool, a_at: impl Fn(usize, usize) -> bool) -> Vec<u8> {
    let mut texture = Vec::with_capacity((SRC_W * SRC_H * 4) as usize);
    for y in 0..SRC_H as usize {
        for x in 0..SRC_W as usize {
            let inside = (GB_X..GB_X + GB_W).contains(&x) && (GB_Y..GB_Y + GB_H).contains(&y);
            let rgb = match (inside, inside && a_at(x - GB_X, y - GB_Y)) {
                (false, _) => [0, 0, 0],
                (true, true) => A,
                (true, false) => B,
            };
            texture.extend_from_slice(&[rgb[2], rgb[1], rgb[0], 0]);
        }
    }
    let x0 = GB_X as f32 / SRC_W as f32;
    let y0 = GB_Y as f32 / SRC_H as f32;
    c.set_picture([
        x0,
        y0,
        x0 + GB_W as f32 / SRC_W as f32,
        y0 + GB_H as f32 / SRC_H as f32,
    ]);
    c.set_game_source_rect([
        12.0 / SRC_W as f32,
        y0,
        216.0 / SRC_W as f32,
        GB_H as f32 / SRC_H as f32,
    ]);
    c.set_soft_picture(soft);
    c.set_screen_effect(ScreenEffect::None);
    c.set_screen_power(1.0);
    c.begin_frame(slot_gfx::Layout::Framed);
    c.upload_game(&texture);
    c.draw_game();
    c.read_frame()
}

/// How far each channel is from B towards A: 0 is pure B, 1 is pure A.
fn towards_a(rgb: [u8; 3]) -> [f32; 3] {
    [0, 1, 2].map(|ch| (rgb[ch] as f32 - B[ch] as f32) / (A[ch] as f32 - B[ch] as f32))
}

/// How much of colour A each stripe holds in one channel, one entry per whole stripe.
fn stripe_weights(line: &[[u8; 3]], ch: usize) -> Vec<f32> {
    let mut weights = Vec::new();
    let mut run = 0.0;
    for rgb in line {
        let a = towards_a(*rgb)[ch];
        if a > 0.01 {
            run += a;
        } else if run > 0.0 {
            weights.push(run);
            run = 0.0;
        }
    }
    // The first stripe can start outside the scanned line; only whole ones are compared.
    weights
        .split_first()
        .map(|(_, rest)| rest.to_vec())
        .unwrap_or_default()
}

fn spread(weights: &[f32]) -> f32 {
    let max = weights.iter().cloned().fold(f32::MIN, f32::max);
    let min = weights.iter().cloned().fold(f32::MAX, f32::min);
    max - min
}

/// Fit scales 144 rows (and the 160 columns) by 3.33, so hard stripes alternate between 3 and 4
/// screen pixels. Softened, each channel must weigh every stripe the same, and every pixel must
/// sit on the line between A and B.
fn check_colour_stripes(
    direction: &str,
    line_of: impl Fn(&[u8]) -> Vec<[u8; 3]>,
    a_at: impl Fn(usize, usize) -> bool,
) {
    let Some((_g, _s, mut c)) = compositor() else {
        return;
    };
    let hard = line_of(&fitted(&mut c, false, &a_at));
    let soft = line_of(&fitted(&mut c, true, &a_at));
    for (ch, name) in ["red", "green", "blue"].into_iter().enumerate() {
        let hard_weights = stripe_weights(&hard, ch);
        let soft_weights = stripe_weights(&soft, ch);
        assert!(
            spread(&hard_weights) >= 0.9,
            "{direction}, {name}: hard stripes were already even: {hard_weights:?}"
        );
        assert_eq!(
            soft_weights.len(),
            hard_weights.len(),
            "{direction}, {name}: softening lost a stripe: {soft_weights:?}"
        );
        assert!(
            spread(&soft_weights) < 0.4,
            "{direction}, {name}: softened stripes are uneven: {soft_weights:?}"
        );
    }
    for (i, rgb) in soft.iter().enumerate() {
        let [r, g, b] = towards_a(*rgb);
        let (lo, hi) = (r.min(g).min(b), r.max(g).max(b));
        assert!(
            hi - lo < 0.06 && lo > -0.03 && hi < 1.03,
            "{direction}, pixel {i}: {rgb:?} is not a mix of {A:?} and {B:?}"
        );
    }
}

#[test]
fn softened_colour_rows_stay_even_in_every_channel_without_fringes() {
    let x = OUT_W as usize / 2 + 6;
    check_colour_stripes(
        "rows",
        |frame| (0..OUT_H as usize).map(|y| px(frame, x, y)).collect(),
        |_, y| y % 4 == 0,
    );
}

#[test]
fn softened_colour_columns_stay_even_in_every_channel_without_fringes() {
    // The fitted picture spans about x 93.3..626.7; stay clear of its edges.
    let y = OUT_H as usize / 2;
    check_colour_stripes(
        "columns",
        |frame| (95..624).map(|x| px(frame, x, y)).collect(),
        |x, _| x % 4 == 0,
    );
}

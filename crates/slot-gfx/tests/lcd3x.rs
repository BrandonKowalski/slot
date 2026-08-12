use slot_gfx::{lcd3x_mask, mask_texture_rgba8};
use std::f64::consts::PI;

const SRC_W: usize = 240;
const SRC_H: usize = 160;
const OUT_W: usize = 720;
const OUT_H: usize = 480;
const SCALE: usize = 3;

const BRIGHTEN_SCANLINES: f64 = 16.0;
const BRIGHTEN_LCD: f64 = 4.0;

fn pseudorandom_240x160() -> Vec<u8> {
    let mut s: u32 = 0x9e37_79b9;
    (0..SRC_W * SRC_H * 3)
        .map(|_| {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (s >> 24) as u8
        })
        .collect()
}

/// The shader as written: sin evaluated per output pixel, no table anywhere.
fn render_reference(src: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; OUT_W * OUT_H * 3];
    for oy in 0..OUT_H {
        let yfactor = (BRIGHTEN_SCANLINES + (PI * (oy as f64 + 0.5) * 2.0 / 3.0).sin())
            / (BRIGHTEN_SCANLINES + 1.0);
        for ox in 0..OUT_W {
            for c in 0..3 {
                let xfactor = (BRIGHTEN_LCD
                    + (PI * (ox as f64 + 0.5) * 2.0 / 3.0 + c as f64 * 2.0 * PI / 3.0).sin())
                    / (BRIGHTEN_LCD + 1.0);
                let texel = src[((oy / SCALE) * SRC_W + ox / SCALE) * 3 + c] as f64;
                out[(oy * OUT_W + ox) * 3 + c] = (texel * yfactor * xfactor).round() as u8;
            }
        }
    }
    out
}

fn render_with_mask(src: &[u8], mask: &[[[f32; 3]; 3]; 3]) -> Vec<u8> {
    let mut out = vec![0u8; OUT_W * OUT_H * 3];
    for oy in 0..OUT_H {
        for ox in 0..OUT_W {
            let cell = &mask[oy % 3][ox % 3];
            for c in 0..3 {
                let texel = src[((oy / SCALE) * SRC_W + ox / SCALE) * 3 + c] as f32;
                out[(oy * OUT_W + ox) * 3 + c] = (texel * cell[c]).round() as u8;
            }
        }
    }
    out
}

#[test]
fn mask_matches_reference_shader_exactly() {
    let src = pseudorandom_240x160();
    let reference = render_reference(&src);
    let optimized = render_with_mask(&src, &lcd3x_mask());
    let worst = reference
        .iter()
        .zip(&optimized)
        .map(|(a, b)| (*a as i32 - *b as i32).abs())
        .max()
        .unwrap();
    assert!(
        worst <= 1,
        "max channel deviation {worst}, expected <= 1 (rounding only)"
    );
}

#[test]
fn mask_phase_puts_one_rgb_triad_per_source_pixel() {
    for row in lcd3x_mask() {
        let reds: Vec<f32> = row.iter().map(|cell| cell[0]).collect();
        let brightest = reds.iter().cloned().fold(f32::MIN, f32::max);
        assert_eq!(
            reds.iter().filter(|v| **v == brightest).count(),
            1,
            "exactly one column should be the red-dominant one"
        );
    }
}

/// The 8 bit mask texture is what the GPU actually samples, so quantising it must not cost
/// more than the rounding the f32 table already spends.
#[test]
fn quantised_mask_texture_stays_within_one_lsb_of_the_reference() {
    let src = pseudorandom_240x160();
    let reference = render_reference(&src);
    let tex = mask_texture_rgba8();
    let mut mask = [[[0.0f32; 3]; 3]; 3];
    for (y, row) in mask.iter_mut().enumerate() {
        for (x, cell) in row.iter_mut().enumerate() {
            for (c, v) in cell.iter_mut().enumerate() {
                *v = tex[(y * 3 + x) * 4 + c] as f32 / 255.0;
            }
        }
    }
    let optimized = render_with_mask(&src, &mask);
    let worst = reference
        .iter()
        .zip(&optimized)
        .map(|(a, b)| (*a as i32 - *b as i32).abs())
        .max()
        .unwrap();
    assert!(
        worst <= 1,
        "max channel deviation {worst} from the 8 bit mask"
    );
}

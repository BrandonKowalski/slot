use std::path::Path;
use std::sync::OnceLock;

use slot_gfx::{Canvas, Draw, TexId, OUT_H, OUT_W};

use crate::art;

const DEFAULT_PNG: &[u8] = include_bytes!("../../../assets/background_default.png");
const BEZEL_GB_PNG: &[u8] = include_bytes!("../../../assets/bezel_gb.png");
const BEZEL_GBC_PNG: &[u8] = include_bytes!("../../../assets/bezel_gbc.png");
const BEZEL_GB_FIT_PNG: &[u8] = include_bytes!("../../../assets/bezel_gb_fit.png");
const BEZEL_GBC_FIT_PNG: &[u8] = include_bytes!("../../../assets/bezel_gbc_fit.png");
const BLUR_PNG: &[u8] = include_bytes!("../../../assets/background_default_blur.png");

const SCRIM: f32 = 0.62;

pub fn wallpaper_face(path: &Path) -> Option<Vec<u8>> {
    art::cover(path, OUT_W, OUT_H)
}

pub fn bezel_face(path: &Path, w: u32, h: u32) -> Option<Vec<u8>> {
    art::cover(path, w, h)
}

pub fn game_bezel_face(path: &Path) -> Option<Vec<u8>> {
    art::cover(path, OUT_W, OUT_H)
}

pub fn default_game_bezel_face(colour: bool) -> Option<Vec<u8>> {
    let png = if colour { BEZEL_GBC_PNG } else { BEZEL_GB_PNG };
    art::cover_bytes(png, OUT_W, OUT_H)
}

/// The built-in bezel for the full-height picture: art in the side bars only.
pub fn fit_game_bezel_face(colour: bool) -> Option<Vec<u8>> {
    let png = if colour {
        BEZEL_GBC_FIT_PNG
    } else {
        BEZEL_GB_FIT_PNG
    };
    art::cover_bytes(png, OUT_W, OUT_H)
}

/// The carousel ground, scaled to cover the whole screen, including behind the footer.
pub fn default_background() -> Option<&'static [u8]> {
    static P: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    P.get_or_init(|| art::cover_bytes(DEFAULT_PNG, OUT_W, OUT_H))
        .as_deref()
}

/// The blurred carousel ground, scaled to cover the whole screen the same way as the default.
pub fn blur_background() -> Option<&'static [u8]> {
    static P: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    P.get_or_init(|| art::cover_bytes(BLUR_PNG, OUT_W, OUT_H))
        .as_deref()
}

pub fn draw_ground(canvas: Canvas, face: Option<TexId>, out: &mut Vec<Draw>) {
    let Some(tex) = face else {
        return;
    };
    out.push(ground(canvas, tex));
}

pub fn draw_backdrop(canvas: Canvas, face: Option<TexId>, out: &mut Vec<Draw>) {
    let Some(tex) = face else {
        return;
    };
    out.push(ground(canvas, tex));
    out.push(Draw::Rect {
        x: 0.0,
        y: 0.0,
        w: canvas.w,
        h: OUT_H as f32,
        colour: [0.0, 0.0, 0.0, SCRIM],
    });
}

fn ground(canvas: Canvas, tex: TexId) -> Draw {
    Draw::Part {
        x: 0.0,
        y: 0.0,
        w: canvas.w,
        h: OUT_H as f32,
        tex,
        alpha: 1.0,
        uv: canvas.window(OUT_W as f32),
    }
}

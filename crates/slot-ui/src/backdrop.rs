use std::path::Path;
use std::sync::OnceLock;

use slot_gfx::{Draw, TexId, OUT_H, OUT_W};

use crate::art;

const DEFAULT_PNG: &[u8] = include_bytes!("../../../assets/background_default.png");

const SCRIM: f32 = 0.62;

pub fn wallpaper_face(path: &Path) -> Option<Vec<u8>> {
    art::cover(path, OUT_W, OUT_H)
}

pub fn bezel_face(path: &Path, w: u32, h: u32) -> Option<Vec<u8>> {
    art::cover(path, w, h)
}

/// The carousel ground, scaled to cover the whole screen, including behind the footer.
pub fn default_background() -> Option<&'static [u8]> {
    static P: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    P.get_or_init(|| art::cover_bytes(DEFAULT_PNG, OUT_W, OUT_H))
        .as_deref()
}

pub fn draw_ground(face: Option<TexId>, out: &mut Vec<Draw>) {
    let Some(tex) = face else {
        return;
    };
    out.push(Draw::Tex {
        x: 0.0,
        y: 0.0,
        w: OUT_W as f32,
        h: OUT_H as f32,
        tex,
        alpha: 1.0,
    });
}

pub fn draw_backdrop(face: Option<TexId>, out: &mut Vec<Draw>) {
    let Some(tex) = face else {
        return;
    };
    out.push(Draw::Tex {
        x: 0.0,
        y: 0.0,
        w: OUT_W as f32,
        h: OUT_H as f32,
        tex,
        alpha: 1.0,
    });
    out.push(Draw::Rect {
        x: 0.0,
        y: 0.0,
        w: OUT_W as f32,
        h: OUT_H as f32,
        colour: [0.0, 0.0, 0.0, SCRIM],
    });
}

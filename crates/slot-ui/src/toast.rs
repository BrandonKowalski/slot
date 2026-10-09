use slot_gfx::OUT_W;
use slot_store::GbPalette;

use crate::hud::{HUD_INK, PLATE_H};
use crate::icon::{haloed, HALO_PX};
use crate::text;
use crate::CartFace;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Toast {
    StateSaved,
    StateLoaded,
    NeedsGpsp,
    NoLink,
    LinkEnded,
    PeerEnded,
    BiosMismatch,
    ColourOn,
    ColourOff,
    ShaderOff,
    ShaderLcd3x,
    ShaderGrid,
    ShaderDot,
    ShaderSimpletex,
    Palette(GbPalette),
}

impl Toast {
    const FIXED: [Toast; 14] = [
        Toast::StateSaved,
        Toast::StateLoaded,
        Toast::NeedsGpsp,
        Toast::NoLink,
        Toast::LinkEnded,
        Toast::PeerEnded,
        Toast::BiosMismatch,
        Toast::ColourOn,
        Toast::ColourOff,
        Toast::ShaderOff,
        Toast::ShaderLcd3x,
        Toast::ShaderGrid,
        Toast::ShaderDot,
        Toast::ShaderSimpletex,
    ];

    pub fn all() -> Vec<Toast> {
        Self::FIXED
            .into_iter()
            .chain(GbPalette::all().map(Toast::Palette))
            .collect()
    }

    pub fn index(self) -> usize {
        match self {
            Toast::Palette(p) => Self::FIXED.len() + p.index(),
            fixed => Self::FIXED.iter().position(|t| *t == fixed).unwrap_or(0),
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            Toast::StateSaved => "State Saved",
            Toast::StateLoaded => "State Loaded",
            Toast::NeedsGpsp => "Please switch to gpSP",
            Toast::NoLink => "No link support",
            Toast::LinkEnded => "Link ended",
            Toast::PeerEnded => "Link was ended",
            Toast::BiosMismatch => "BIOS does not match",
            Toast::ColourOn => "Correction On",
            Toast::ColourOff => "Correction Off",
            Toast::ShaderOff => "Shader: Off",
            Toast::ShaderLcd3x => "Shader: LCD3x",
            Toast::ShaderGrid => "Shader: Grid",
            Toast::ShaderDot => "Shader: Dot",
            Toast::ShaderSimpletex => "Shader: Simpletex",
            Toast::Palette(p) => p.label(),
        }
    }
}

const TOAST_W: u32 = 240;
const TOAST_H: u32 = 22;
const TOAST_PX: f32 = 16.0;
const TOAST_MIN_PX: f32 = 12.0;

pub fn toast_rect() -> (f32, f32, f32, f32) {
    let (w, h) = toast_box();
    let (w, h) = (w as f32, h as f32);
    ((OUT_W as f32 - w) / 2.0, (PLATE_H - h) / 2.0, w, h)
}

pub fn toast_box() -> (u32, u32) {
    (TOAST_W + 2 * HALO_PX, TOAST_H + 2 * HALO_PX)
}

pub fn toast_face(toast: Toast) -> CartFace {
    let Some(font) = text::label_font() else {
        return CartFace {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        };
    };
    let layout = text::fit(
        font,
        toast.text(),
        TOAST_W as f32,
        1,
        TOAST_PX,
        TOAST_MIN_PX,
    );
    let cov = text::coverage(TOAST_W, TOAST_H, &layout);
    haloed(&cov, TOAST_W, TOAST_H, HUD_INK)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ink_width(toast: Toast) -> u32 {
        let font = text::label_font().expect("label font");
        let layout = text::fit(
            font,
            toast.text(),
            TOAST_W as f32,
            1,
            TOAST_PX,
            TOAST_MIN_PX,
        );
        let cov = text::coverage(TOAST_W, TOAST_H, &layout);
        let cols: Vec<u32> = (0..TOAST_W)
            .filter(|x| (0..TOAST_H).any(|y| cov[(y * TOAST_W + x) as usize] > 0))
            .collect();
        match (cols.first(), cols.last()) {
            (Some(a), Some(b)) => b - a + 1,
            _ => 0,
        }
    }

    fn ink_rows(face: &CartFace) -> (u32, u32) {
        let mut first = None;
        let mut last = 0;
        for y in 0..face.h {
            let inked = (0..face.w).any(|x| face.rgba[((y * face.w + x) * 4 + 3) as usize] > 0);
            if inked {
                first.get_or_insert(y);
                last = y;
            }
        }
        (first.unwrap_or(0), last)
    }

    fn ink_height_at(toast: Toast, px: f32) -> u32 {
        let font = text::label_font().expect("label font");
        let layout = text::fit(font, toast.text(), TOAST_W as f32, 1, px, px);
        let cov = text::coverage(TOAST_W, TOAST_H, &layout);
        let rows: Vec<u32> = (0..TOAST_H)
            .filter(|y| (0..TOAST_W).any(|x| cov[(y * TOAST_W + x) as usize] > 0))
            .collect();
        match (rows.first(), rows.last()) {
            (Some(a), Some(b)) => b - a + 1,
            _ => 0,
        }
    }

    #[test]
    fn the_ended_line_is_rastered_the_size_the_others_are() {
        let ended = toast_face(Toast::LinkEnded);
        let saved = toast_face(Toast::StateSaved);
        assert_eq!(
            (ended.w, ended.h),
            (saved.w, saved.h),
            "one box holds every banner"
        );
        let (top, bottom) = ink_rows(&ended);
        assert!(bottom > top, "the line rastered to nothing at all");

        let shipped = ink_height_at(Toast::LinkEnded, TOAST_PX);
        let shrunk = ink_height_at(Toast::LinkEnded, TOAST_MIN_PX);
        assert!(
            shipped > shrunk,
            "the shipped line is no taller than the {TOAST_MIN_PX} px fallback: {shipped} against {shrunk}"
        );
        let reference = ink_height_at(Toast::StateSaved, TOAST_PX);
        assert!(
            shipped.abs_diff(reference) <= 1,
            "LINK ENDED is {shipped} px of ink where STATE SAVED is {reference}"
        );
    }

    #[test]
    fn every_toast_is_set_at_full_size() {
        let font = text::label_font().expect("label font");
        for t in Toast::all() {
            let layout = text::fit(font, t.text(), TOAST_W as f32, 1, TOAST_PX, TOAST_MIN_PX);
            assert_eq!(
                layout.px,
                TOAST_PX,
                "{:?} ({:?}) was shrunk to {} px to fit {TOAST_W}",
                t,
                t.text(),
                layout.px
            );
            assert_eq!(layout.lines.len(), 1, "{t:?} wrapped onto a second line");
            let w = ink_width(t);
            assert!(w <= TOAST_W, "{:?} is {w} px wide in a {TOAST_W} px box", t);
        }
    }
}

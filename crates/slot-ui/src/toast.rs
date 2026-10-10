use slot_gfx::{OUT_H, OUT_W};
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
    CardUnwritable,
    Palette(GbPalette),
}

impl Toast {
    const FIXED: [Toast; 15] = [
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
        Toast::CardUnwritable,
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

    pub fn alarm(self) -> bool {
        self == Toast::CardUnwritable
    }

    pub fn detail(self) -> Option<&'static str> {
        match self {
            Toast::CardUnwritable => Some("Progress is not being saved"),
            _ => None,
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
            Toast::CardUnwritable => "Can't write to SD card",
            Toast::Palette(p) => p.label(),
        }
    }
}

const TOAST_W: u32 = 240;
const TOAST_H: u32 = 22;
const TOAST_PX: f32 = 16.0;
const TOAST_MIN_PX: f32 = 12.0;

const ALARM_W: u32 = 560;
const ALARM_H: u32 = 44;
const ALARM_PX: f32 = 32.0;
const DETAIL_H: u32 = 30;
const DETAIL_PX: f32 = 20.0;

struct Setting {
    w: u32,
    h: u32,
    px: f32,
    min_px: f32,
    middle: f32,
}

fn setting(toast: Toast) -> Setting {
    match toast.alarm() {
        true => Setting {
            w: ALARM_W,
            h: ALARM_H,
            px: ALARM_PX,
            min_px: ALARM_PX,
            middle: OUT_H as f32 / 2.0,
        },
        false => Setting {
            w: TOAST_W,
            h: TOAST_H,
            px: TOAST_PX,
            min_px: TOAST_MIN_PX,
            middle: PLATE_H / 2.0,
        },
    }
}

pub fn toast_rect(toast: Toast) -> (f32, f32, f32, f32) {
    let (w, h) = toast_box(toast);
    let (w, h) = (w as f32, h as f32);
    let middle = setting(toast).middle;
    ((OUT_W as f32 - w) / 2.0, middle - h / 2.0, w, h)
}

pub fn toast_box(toast: Toast) -> (u32, u32) {
    let s = setting(toast);
    (s.w + 2 * HALO_PX, s.h + detail_h(toast) + 2 * HALO_PX)
}

fn detail_h(toast: Toast) -> u32 {
    match toast.detail() {
        Some(_) => DETAIL_H,
        None => 0,
    }
}

pub fn toast_face(toast: Toast) -> CartFace {
    let Some(font) = text::label_font() else {
        return CartFace {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        };
    };
    let s = setting(toast);
    let layout = text::fit(font, toast.text(), s.w as f32, 1, s.px, s.min_px);
    let mut cov = text::coverage(s.w, s.h, &layout);
    if let Some(detail) = toast.detail() {
        let layout = text::fit(font, detail, s.w as f32, 1, DETAIL_PX, DETAIL_PX);
        cov.extend(text::coverage(s.w, DETAIL_H, &layout));
    }
    haloed(&cov, s.w, s.h + detail_h(toast), HUD_INK)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ink_width(toast: Toast) -> u32 {
        let font = text::label_font().expect("label font");
        let s = setting(toast);
        let layout = text::fit(font, toast.text(), s.w as f32, 1, s.px, s.min_px);
        let cov = text::coverage(s.w, s.h, &layout);
        let cols: Vec<u32> = (0..s.w)
            .filter(|x| (0..s.h).any(|y| cov[(y * s.w + x) as usize] > 0))
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
            let s = setting(t);
            let layout = text::fit(font, t.text(), s.w as f32, 1, s.px, s.min_px);
            assert_eq!(
                layout.px,
                s.px,
                "{:?} ({:?}) was shrunk to {} px to fit {}",
                t,
                t.text(),
                layout.px,
                s.w
            );
            assert_eq!(layout.lines.len(), 1, "{t:?} wrapped onto a second line");
            let w = ink_width(t);
            assert!(w <= s.w, "{:?} is {w} px wide in a {} px box", t, s.w);
        }
    }
}

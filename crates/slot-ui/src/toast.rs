use slot_gfx::OUT_W;

use crate::hud::{HUD_INK, PLATE_H};
use crate::icon::{haloed, HALO_PX};
use crate::text;
use crate::CartFace;

/// Everything the HUD ever says in words. Each answers something the user just did: two confirm
/// it, two answer the link shortcut where it cannot be carried out — on a core that cannot link
/// at all, and on a cart whose link gpSP cannot carry — either of which would otherwise do
/// nothing and say nothing, and the last two are a link session ending, from whichever end ended
/// it.
///
/// Three more used to name the shelf the shoulders had just moved to. They are gone: the shelf
/// is now said by the platform's mark on the case band, which is always there rather than fading
/// after a moment — see `slot_ui::mark`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Toast {
    StateSaved,
    StateLoaded,
    NeedsGpsp,
    NoLink,
    LinkEnded,
    /// The far end of a live session ended it and sent word before going. The other side of
    /// `LinkEnded`, and a separate sentence because which device ended it is the one thing the
    /// player on this one cannot see.
    PeerEnded,
}

impl Toast {
    pub const ALL: [Toast; 6] = [
        Toast::StateSaved,
        Toast::StateLoaded,
        Toast::NeedsGpsp,
        Toast::NoLink,
        Toast::LinkEnded,
        Toast::PeerEnded,
    ];

    /// Position in `ALL`, which is the order faces are uploaded in.
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn text(self) -> &'static str {
        match self {
            Toast::StateSaved => "State Saved",
            Toast::StateLoaded => "State Loaded",
            Toast::NeedsGpsp => "Please switch to gpSP",
            Toast::NoLink => "No link support",
            Toast::LinkEnded => "Link ended",
            // Passive, and deliberately so: on this device the link was ended by somebody else,
            // and "Link was ended" reads as something that happened to you rather than
            // something you did — which is the one distinction this end cannot see for itself.
            // Impersonal too, like every other line here ("No link support", "Nobody arrived");
            // the product says "friend" nowhere, so this is not the screen to start.
            Toast::PeerEnded => "Link was ended",
        }
    }
}

/// One box for every string, so which one it is never moves the line. Wide enough for the
/// longest at full size: `fit` would otherwise shrink that one line and no other.
const TOAST_W: u32 = 240;
const TOAST_H: u32 = 22;
const TOAST_PX: f32 = 16.0;
const TOAST_MIN_PX: f32 = 12.0;

/// Where the line lands, in offscreen pixels.
pub fn toast_rect() -> (f32, f32, f32, f32) {
    let (w, h) = toast_box();
    let (w, h) = (w as f32, h as f32);
    ((OUT_W as f32 - w) / 2.0, (PLATE_H - h) / 2.0, w, h)
}

/// The box every toast is rastered into, for callers laying the row out before they know
/// which string it will hold.
pub fn toast_box() -> (u32, u32) {
    (TOAST_W + 2 * HALO_PX, TOAST_H + 2 * HALO_PX)
}

/// Transparent apart from the type and the halo dilated out of it. Nothing backs a toast, so
/// the halo is the only thing keeping it legible over a bright game frame.
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

    /// The ink's own width in the box, which is what a line actually occupies once the font
    /// has uppercased it.
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

    /// The rows of a rastered face with any ink in them: where the line actually sits once it
    /// is pixels rather than a layout.
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

    /// The ink's height in a face rastered at a stated size, for holding the shipped one
    /// against the fallback it would have been given.
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

    /// The face, not the layout. What reaches the screen is a box of pixels, and a line that
    /// had to be shrunk arrives as a shorter run of ink in it — the same banner in a smaller
    /// hand, which is the failure this is here to catch.
    ///
    /// Against two references, because neither alone is enough: the 12 px fallback, which the
    /// shipped line has to stand clearly taller than, and a line already known to fit, which
    /// it has to match. Matching is to within a pixel rather than exactly, since round and
    /// pointed capitals (the S and A of STATE SAVED) overshoot the flat tops of LINK ENDED by
    /// one row at this size — a fact about the typeface, not about the size it is set at.
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

    /// Every banner shares one box, and `fit` answers a line that will not fit by shrinking it
    /// — to 12 px, against the 16 every other line is set at. Nothing warns: the banner simply
    /// comes up smaller, which reads as a different banner rather than as this one saying
    /// something else. So each line is held to full size here, where adding copy that does not
    /// fit fails rather than ships.
    #[test]
    fn every_toast_is_set_at_full_size() {
        let font = text::label_font().expect("label font");
        for t in Toast::ALL {
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

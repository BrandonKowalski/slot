use slot_gfx::OUT_W;

use crate::hud::{HUD_INK, PLATE_H};
use crate::icon::{haloed, HALO_PX};
use crate::text;
use crate::CartFace;

/// Everything the HUD ever says in words. Each answers something the user just did: two confirm
/// it, and the other two answer the link shortcut where it cannot be carried out — on a core
/// that cannot link at all, and on a cart whose link gpSP cannot carry — either of which would
/// otherwise do nothing and say nothing.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Toast {
    StateSaved,
    StateLoaded,
    NeedsGpsp,
    NoLink,
}

impl Toast {
    pub const ALL: [Toast; 4] = [
        Toast::StateSaved,
        Toast::StateLoaded,
        Toast::NeedsGpsp,
        Toast::NoLink,
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
            Toast::NoLink => "No link for this game",
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

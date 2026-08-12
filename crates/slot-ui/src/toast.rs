use slot_gfx::OUT_W;

use crate::hud::{HUD_INK, PLATE_H};
use crate::icon::{haloed, HALO_PX};
use crate::text;
use crate::CartFace;

/// Everything the HUD ever says in words. Two lines and no third: a toast is confirmation of
/// something the user just did, never a message.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Toast {
    StateSaved,
    StateLoaded,
}

impl Toast {
    pub const ALL: [Toast; 2] = [Toast::StateSaved, Toast::StateLoaded];

    /// Position in `ALL`, which is the order faces are uploaded in.
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn text(self) -> &'static str {
        match self {
            Toast::StateSaved => "State Saved",
            Toast::StateLoaded => "State Loaded",
        }
    }
}

/// One box for both strings, so which one it is never moves the line.
const TOAST_W: u32 = 220;
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

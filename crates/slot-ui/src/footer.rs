use slot_gfx::{Draw, TexId, OUT_H, OUT_W};
use slot_power::Battery;

use crate::battery::{draw_gauge, GAUGE_H};
use crate::plate::HINT_H;
use crate::slot_chrome::MOUTH_H;

/// Centred in the case, not measured off the bottom of the screen: the type is printed on
/// the plastic, so it belongs to the plastic's middle rather than to the panel's edge.
const FOOTER_Y: f32 = OUT_H as f32 - MOUTH_H + (MOUTH_H - HINT_H as f32) / 2.0;
/// Blank at each end. Matches the gap the row leaves beside the outer carts, so what is
/// printed on the case lines up with what is above it.
const FOOTER_MARGIN: f32 = 24.0;

/// A line of type and the width it rasterised to. The width cannot be recovered from a
/// `TexId`, and only the compositor can mint one, so the space is held from the width alone
/// while the face is still on its way.
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
pub struct Printed {
    pub face: Option<TexId>,
    pub w: u32,
}

impl Printed {
    pub fn new(face: TexId, w: u32) -> Self {
        Printed {
            face: Some(face),
            w,
        }
    }
}

/// The gauge on the left, the time on the right, both on the case. The wordmark used to have
/// the left shelf; a device that tells you its charge is worth more than one that tells you
/// its own name.
pub fn draw_footer(
    battery: Option<Battery>,
    percent: Printed,
    bolt: Option<TexId>,
    clock: Printed,
    out: &mut Vec<Draw>,
) {
    let y = FOOTER_Y + (HINT_H as f32 - GAUGE_H) / 2.0;
    draw_gauge(FOOTER_MARGIN, y, battery, percent, bolt, out);
    printed(OUT_W as f32 - FOOTER_MARGIN - clock.w as f32, clock, out);
}

/// A line of type at an arbitrary `y`. The placeholder is what holds the space while the
/// face is still on its way, so a row does not reflow the moment type arrives.
pub(crate) fn draw_printed(x: f32, y: f32, p: Printed, out: &mut Vec<Draw>) {
    if p.w == 0 {
        return;
    }
    let (w, h) = (p.w as f32, HINT_H as f32);
    out.push(match p.face {
        Some(tex) => Draw::Tex {
            x,
            y,
            w,
            h,
            tex,
            alpha: 1.0,
        },
        None => Draw::Rect {
            x,
            y,
            w,
            h,
            colour: [1.0, 1.0, 1.0, 0.08],
        },
    });
}

fn printed(x: f32, p: Printed, out: &mut Vec<Draw>) {
    draw_printed(x, FOOTER_Y, p, out);
}

use slot_gfx::{Draw, TexId, OUT_H, OUT_W};
use slot_power::Battery;

use crate::battery::{draw_gauge, gauge_width, GAUGE_H};
use crate::plate::HINT_H;
use crate::slot_chrome::MOUTH_H;

/// Centred in the case, not measured off the bottom of the screen: the type is printed on
/// the plastic, so it belongs to the plastic's middle rather than to the panel's edge.
const FOOTER_Y: f32 = OUT_H as f32 - MOUTH_H + (MOUTH_H - HINT_H as f32) / 2.0;
/// Blank at each end. Matches the gap the row leaves beside the outer carts, so what is
/// printed on the case lines up with what is above it.
const FOOTER_MARGIN: f32 = 24.0;

/// Between the time and the charge at the right end. Wider than the gap inside the gauge, so the
/// two read as two things rather than as one long run of glyphs.
const CLUSTER_GAP: f32 = 16.0;

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

/// What is printed on the case: which machine's shelf is showing, at the left margin, and the
/// time and the charge together at the right.
///
/// The shelf used to say its own name in a banner over the carts, then in a drawing in the top
/// plate's corner. It says it here now, in words, and the difference from the banner is that this
/// one does not fade: the answer to "which shelf am I on" is readable whenever it is asked rather
/// than for two seconds after the shoulder was pressed.
///
/// Only when there is more than one shelf to be on. A card with a single machine's carts has one
/// answer, and printing it is a label on a thing that could not be anything else.
///
/// The band's middle is not free: the cart slot's bay, opening and thumb scoop run from x 224 to
/// x 496, so the two ends are the whole of the space. The name has the left and the gauge and the
/// clock share the right.
///
/// Charge outermost, time inboard of it. Either order reads, and this is the one a phone uses, so
/// it is the one a thumb already knows where to look for. Swapping them is this function and
/// nothing else.
pub fn draw_footer(
    platform: Printed,
    battery: Option<Battery>,
    percent: Printed,
    bolt: Option<TexId>,
    clock: Printed,
    out: &mut Vec<Draw>,
) {
    let y = FOOTER_Y + (HINT_H as f32 - GAUGE_H) / 2.0;
    printed(FOOTER_MARGIN, platform, out);

    let right = OUT_W as f32 - FOOTER_MARGIN;
    let gauge_w = match battery {
        Some(_) => gauge_width(percent),
        // No battery node is no gauge at all, so the clock takes the corner rather than leaving
        // a hole where one would have been.
        None => 0.0,
    };
    draw_gauge(right - gauge_w, y, battery, percent, bolt, out);
    let clock_right = match gauge_w > 0.0 {
        true => right - gauge_w - CLUSTER_GAP,
        false => right,
    };
    printed(clock_right - clock.w as f32, clock, out);
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

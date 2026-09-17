use slot_gfx::{Draw, TexId, OUT_H, OUT_W};
use slot_power::Battery;

use crate::battery::{draw_gauge, GAUGE_H};
use crate::mark::mark_box;
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

/// Between the mark and the capsule beside it. Wider than the gap inside the gauge's own group,
/// so the shelf's mark reads as its own thing on the case rather than as part of the readout.
const MARK_GAP: f32 = 12.0;

/// Where the gauge starts, which is one mark and one gap in from the margin. Held whether or not
/// there is a mark to draw, for the reason the bolt's slot inside the gauge is held: a face that
/// arrives after boot must not shove the readout sideways on the frame it lands.
const GAUGE_X: f32 = FOOTER_MARGIN + MARK_SLOT_W + MARK_GAP;
/// `mark_box`'s width, spelled out because a `const` cannot call a function. The two are held
/// against each other by `the_band_reserves_exactly_the_mark_it_draws`, which reads the gap the
/// layout leaves and the box the mark actually rasters into from opposite ends: a slot narrower
/// than the mark would put the machine through the battery, and a wider one would leave a hole
/// that nothing on the band explains.
const MARK_SLOT_W: f32 = crate::mark::MARK_W as f32 + 2.0 * crate::icon::HALO_PX as f32;

/// The shelf's mark on the left, then the gauge, then the time on the right, all of it on the
/// case. The wordmark used to have the left shelf and lost it to the gauge, on the grounds that
/// a device which tells you its charge is worth more than one that tells you its own name; the
/// mark takes it back, because it is not the device's name — it changes with the shoulders, and
/// it is the one thing the row of carts above cannot say about itself.
///
/// It goes at this end rather than in the middle of the band because the middle of the band is
/// not empty: the cart slot's bay, opening and thumb scoop run from x 224 to x 496, and a mark
/// centred on the case would sit in the mouth of the slot looking like something stuck in it.
/// Nor does it go beside the clock — that is the other end's business — or after the percent,
/// which is the one place on the band whose width changes as the battery drains, and a mark
/// that crept sideways with the charge would be worse than no mark at all.
pub fn draw_footer(
    battery: Option<Battery>,
    percent: Printed,
    bolt: Option<TexId>,
    mark: Option<TexId>,
    clock: Printed,
    out: &mut Vec<Draw>,
) {
    let (mw, mh) = mark_box();
    if let Some(tex) = mark {
        out.push(Draw::Tex {
            x: FOOTER_MARGIN,
            y: FOOTER_Y + (HINT_H as f32 - mh as f32) / 2.0,
            w: mw as f32,
            h: mh as f32,
            tex,
            alpha: 1.0,
        });
    }
    let y = FOOTER_Y + (HINT_H as f32 - GAUGE_H) / 2.0;
    draw_gauge(GAUGE_X, y, battery, percent, bolt, out);
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

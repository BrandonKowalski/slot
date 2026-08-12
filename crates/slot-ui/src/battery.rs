use slot_gfx::{Draw, TexId};
use slot_power::{Battery, Charge};

use crate::footer::Printed;
use crate::hud::HUD_INK;
use crate::plate::HINT_H;

/// The capsule, in the proportions of the thing it is a picture of. Wider than tall, with a
/// nub on the positive end.
/// The bolt is sized to the gauge rather than to the HUD row. It sits beside the capsule on
/// the case band, not among the row's glyphs, so growing the row must not grow it: the slot
/// reserved for it here is the capsule's height, and a larger bolt would push the gauge
/// sideways for a control that is not being adjusted.
pub const BOLT_PX: f32 = 18.0;

pub const GAUGE_W: f32 = 22.0;
pub const GAUGE_H: f32 = 11.0;
// The doc comment above claims wider than tall; this is what makes that a fact the compiler
// enforces rather than a sentence someone could quietly falsify by editing one constant.
const _: () = assert!(GAUGE_H < GAUGE_W);
const NUB_W: f32 = 2.5;
const NUB_H: f32 = 4.0;
/// The wall of the capsule, drawn as four rects rather than an outline: the draw list has
/// only filled quads. Public so a test can bound the fill against the capsule's own inner
/// edge instead of trusting a hand-copied number that could drift from this one.
pub const WALL: f32 = 1.5;
/// Between the capsule and the number.
const GAP: f32 = 7.0;

/// The bolt's own slot, to the left of the capsule. Squeezed inside the capsule it had an 8
/// px box to live in and came out as a smudge, not a bolt, and it punched a hole in whatever
/// fill was under it. Out here it sits on the housing (and later a switcher plate) at a size
/// that actually reads, on the order of the capsule's own height rather than half of it.
const BOLT_W: f32 = 14.0;
const BOLT_H: f32 = 14.0;
/// Between the bolt's slot and the capsule.
const BOLT_GAP: f32 = 5.0;
// A zero or negative gap would let the bolt's own quad touch or cross into the capsule's,
// which is the defect this file exists to fix, just moved from vertical overlap to
// horizontal. `the_bolt_never_reaches_the_capsule` in tests/battery.rs holds the same
// property at runtime, against the actual draw list rather than these numbers.
const _: () = assert!(BOLT_GAP > 0.0);

const INK: [f32; 4] = [
    HUD_INK[0] as f32 / 255.0,
    HUD_INK[1] as f32 / 255.0,
    HUD_INK[2] as f32 / 255.0,
    1.0,
];

/// The capsule, its fill, and the number beside it. `x` and `y` are the top left of the whole
/// cluster, not the capsule: the bolt's slot is reserved first, unconditionally, so the
/// capsule and the number sit in the same place whether or not a cable is in. The bolt itself
/// only ever draws inside that reserved slot, never over the fill.
pub fn draw_gauge(
    x: f32,
    y: f32,
    battery: Option<Battery>,
    percent: Printed,
    bolt: Option<TexId>,
    out: &mut Vec<Draw>,
) {
    // No gauge is no capsule, rather than an empty one: a device with no battery node has
    // nothing to say, and an empty capsule says the battery is flat.
    let Some(b) = battery else {
        return;
    };

    let rect = |x: f32, y: f32, w: f32, h: f32, out: &mut Vec<Draw>| {
        out.push(Draw::Rect {
            x,
            y,
            w,
            h,
            colour: INK,
        });
    };

    // Held whether or not anything is charging. Making this depend on `b.charge` is exactly
    // the bug being fixed here in a different shape: the capsule would still jump sideways
    // the instant a cable went in, just horizontally instead of losing its fill.
    let cx = x + BOLT_W + BOLT_GAP;

    rect(cx, y, GAUGE_W, WALL, out);
    rect(cx, y + GAUGE_H - WALL, GAUGE_W, WALL, out);
    rect(cx, y, WALL, GAUGE_H, out);
    rect(cx + GAUGE_W - WALL, y, WALL, GAUGE_H, out);
    rect(cx + GAUGE_W, y + (GAUGE_H - NUB_H) / 2.0, NUB_W, NUB_H, out);

    let inner = GAUGE_W - 4.0 * WALL;
    let fill = inner * f32::from(b.percent.min(100)) / 100.0;
    if fill > 0.0 {
        rect(
            cx + 2.0 * WALL,
            y + 2.0 * WALL,
            fill,
            GAUGE_H - 4.0 * WALL,
            out,
        );
    }

    // In its own slot, left of the capsule and vertically centred on it. Never over the
    // fill: the fill has to read as the same length at a given percent whether or not the
    // device is charging.
    if let (Charge::Charging, Some(tex)) = (b.charge, bolt) {
        out.push(Draw::Tex {
            x,
            y: y + (GAUGE_H - BOLT_H) / 2.0,
            w: BOLT_W,
            h: BOLT_H,
            tex,
            alpha: 1.0,
        });
    }

    if percent.w > 0 {
        let px = cx + GAUGE_W + NUB_W + GAP;
        let py = y + (GAUGE_H - HINT_H as f32) / 2.0;
        out.push(match percent.face {
            Some(tex) => Draw::Tex {
                x: px,
                y: py,
                w: percent.w as f32,
                h: HINT_H as f32,
                tex,
                alpha: 1.0,
            },
            None => Draw::Rect {
                x: px,
                y: py,
                w: percent.w as f32,
                h: HINT_H as f32,
                colour: [1.0, 1.0, 1.0, 0.08],
            },
        });
    }
}

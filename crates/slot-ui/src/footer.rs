use slot_gfx::{Draw, TexId, OUT_H, OUT_W};
use slot_power::Battery;

use crate::battery::{draw_gauge, GAUGE_H};
use crate::plate::HINT_H;
use crate::slot_chrome::MOUTH_H;

pub const FOOTER_Y: f32 = OUT_H as f32 - MOUTH_H + (MOUTH_H - HINT_H as f32) / 2.0;
const FOOTER_MARGIN: f32 = 24.0;

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

pub fn draw_footer(
    battery: Option<Battery>,
    percent: Printed,
    bolt: Option<TexId>,
    clock: Printed,
    ink: [u8; 3],
    out: &mut Vec<Draw>,
) {
    let y = FOOTER_Y + (HINT_H as f32 - GAUGE_H) / 2.0;
    draw_gauge(FOOTER_MARGIN, y, battery, percent, bolt, ink, out);
    printed(OUT_W as f32 - FOOTER_MARGIN - clock.w as f32, clock, out);
}

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

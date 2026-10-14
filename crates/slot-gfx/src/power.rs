use crate::surface::{OUT_H, OUT_W};

const LINE_PX: f32 = 2.0;

const DOT_T: f32 = 0.12;

const OVERSHOOT: f32 = 0.6;

pub fn screen_scale(t: f32) -> f32 {
    let left = 1.0 - t.clamp(0.0, 1.0);
    1.0 - (1.0 - LINE_PX / OUT_H as f32) * left * left
}

pub fn screen_width(t: f32) -> f32 {
    let left = 1.0 - (t.clamp(0.0, 1.0) / DOT_T).min(1.0);
    1.0 - (1.0 - LINE_PX / OUT_W as f32) * left
}

pub fn screen_brightness(t: f32) -> f32 {
    let left = 1.0 - t.clamp(0.0, 1.0);
    1.0 + OVERSHOOT * left * left
}

/// The lit part of `dest` as the screen powers up, collapsing to a line and then a dot.
pub fn screen_rect(t: f32, dest: (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
    let (dx, dy, dw, dh) = dest;
    let w = dw * screen_width(t);
    let h = dh * screen_scale(t);
    (dx + (dw - w) / 2.0, dy + (dh - h) / 2.0, w, h)
}

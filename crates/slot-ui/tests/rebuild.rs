use slot_gfx::{Draw, OUT_H, OUT_W};
use slot_ui::{set_shelf_slack, RebuildScreen, TexId};

fn stack_middle(out: &[Draw]) -> (f32, f32) {
    let (mut top, mut bottom, mut left, mut right) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for d in &out[1..] {
        let (x, y, w, h) = match d {
            Draw::Tex { x, y, w, h, .. } | Draw::Rect { x, y, w, h, .. } => (*x, *y, *w, *h),
            _ => continue,
        };
        top = top.min(y);
        bottom = bottom.max(y + h);
        left = left.min(x);
        right = right.max(x + w);
    }
    ((left + right) / 2.0, (top + bottom) / 2.0)
}

#[test]
fn the_message_sits_in_the_middle_of_the_panel_whatever_its_shape() {
    let screen = RebuildScreen {
        title: Some((TexId::from_raw(1), 401, 29)),
        count: Some((TexId::from_raw(2), 117, 19)),
        fraction: 0.25,
    };
    for (panel, middle) in [((720u32, 480u32), 240.0), ((640, 480), 270.0)] {
        set_shelf_slack((OUT_W * panel.1 / panel.0).saturating_sub(OUT_H) as f32);
        let mut out = Vec::new();
        screen.draw(&mut out);
        let (x, y) = stack_middle(&out);
        assert_eq!(x, OUT_W as f32 / 2.0, "off centre across a {panel:?} panel");
        assert_eq!(y, middle, "off centre down a {panel:?} panel");
    }
}

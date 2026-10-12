use crate::art::render_svg;
use crate::bitmaps;
use crate::cart::CartFace;
use crate::text;

pub const PORT_W: u32 = 720;
pub const PORT_H: u32 = 92;
pub const PORT_Y: f32 = 388.0;
pub const PLUG_W: u32 = 92;
pub const PLUG_H: u32 = 244;
pub const PLUG_TIP_X: f32 = 46.0;
pub const ADAPTER_W: u32 = 268;
pub const ADAPTER_H: u32 = 182;
pub const ADAPTER_BASE_X: f32 = 134.0;
pub const ADAPTER_BASE_Y: f32 = 144.0;
pub const ARCS: [(f32, f32, u32, u32); 3] = [
    (508.0, 290.0, 16, 56),
    (530.0, 274.0, 20, 88),
    (552.0, 258.0, 25, 120),
];
pub const CLICKS_X: f32 = 270.0;
pub const CLICKS_Y: f32 = 360.0;
pub const CLICKS_W: u32 = 180;
pub const CLICKS_H: u32 = 30;
pub const ARROW_W: u32 = 16;
pub const ARROW_H: u32 = 22;
pub const ARROW_LEFT_X: f32 = 268.0;
pub const ARROW_RIGHT_X: f32 = 436.0;
pub const ARROW_Y: f32 = 52.0;

const ADAPTER_SCALE: f32 = 1.75;
const LABEL_INK: [u8; 3] = [0xec, 0xee, 0xef];
const LOGO_INK: [u8; 3] = [0x7d, 0x85, 0x8e];
const ARC_INK: &str = "#f6f4ef";

pub struct LinkArt {
    pub port: CartFace,
    pub plug_host: CartFace,
    pub plug_join: CartFace,
    pub adapter: CartFace,
    pub arcs_right: [CartFace; 3],
    pub arcs_left: [CartFace; 3],
    pub clicks: CartFace,
    pub arrow_left: CartFace,
    pub arrow_right: CartFace,
}

pub fn link_art() -> LinkArt {
    let arcs_right = [arc_face(0), arc_face(1), arc_face(2)];
    let arcs_left = [
        mirror(&arcs_right[0]),
        mirror(&arcs_right[1]),
        mirror(&arcs_right[2]),
    ];
    LinkArt {
        port: bitmap_face(bitmaps::link_port(), PORT_W, PORT_H),
        plug_host: bitmap_face(bitmaps::link_plug_host(), PLUG_W, PLUG_H),
        plug_join: bitmap_face(bitmaps::link_plug_join(), PLUG_W, PLUG_H),
        adapter: adapter_face(),
        arcs_right,
        arcs_left,
        clicks: svg_face(
            &format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {CLICKS_W} {CLICKS_H}"><g stroke="{ARC_INK}" stroke-width="4" stroke-linecap="round"><path d="M34 20 L20 4"/><path d="M146 20 L160 4"/><path d="M26 26 L4 26"/><path d="M154 26 L176 26"/></g></svg>"#
            ),
            CLICKS_W,
            CLICKS_H,
        ),
        arrow_left: svg_face(&arrow_svg("M14 2 L2 11 L14 20 Z"), ARROW_W, ARROW_H),
        arrow_right: svg_face(&arrow_svg("M2 2 L14 11 L2 20 Z"), ARROW_W, ARROW_H),
    }
}

fn bitmap_face(rgba: &[u8], w: u32, h: u32) -> CartFace {
    CartFace {
        rgba: rgba.to_vec(),
        w,
        h,
    }
}

fn svg_face(svg: &str, w: u32, h: u32) -> CartFace {
    let rgba = render_svg(svg, w, h).unwrap_or_else(|| vec![0; (w * h * 4) as usize]);
    CartFace { rgba, w, h }
}

fn arrow_svg(path: &str) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {ARROW_W} {ARROW_H}"><path d="{path}" fill="{ARC_INK}"/></svg>"#
    )
}

fn arc_face(ring: usize) -> CartFace {
    let (_, _, w, h) = ARCS[ring];
    let bulge = [16.0, 24.0, 34.0][ring];
    let path = format!("M4 4 Q{} {} 4 {}", 4.0 + bulge, h as f32 / 2.0, h - 4);
    svg_face(
        &format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}"><path d="{path}" fill="none" stroke="{ARC_INK}" stroke-width="5" stroke-linecap="round"/></svg>"#
        ),
        w,
        h,
    )
}

fn mirror(face: &CartFace) -> CartFace {
    let mut rgba = vec![0u8; face.rgba.len()];
    for y in 0..face.h {
        for x in 0..face.w {
            let s = ((y * face.w + x) * 4) as usize;
            let d = ((y * face.w + (face.w - 1 - x)) * 4) as usize;
            rgba[d..d + 4].copy_from_slice(&face.rgba[s..s + 4]);
        }
    }
    CartFace {
        rgba,
        w: face.w,
        h: face.h,
    }
}

#[derive(Copy, Clone)]
enum Align {
    Left,
    Centre,
    Right,
}

type LabelRow = (f32, f32, &'static str, f32, [u8; 3], Align);

fn adapter_face() -> CartFace {
    let mut face = bitmap_face(bitmaps::link_adapter(), ADAPTER_W, ADAPTER_H);
    let at = |x: f32, y: f32| {
        (
            ADAPTER_BASE_X + x * ADAPTER_SCALE,
            ADAPTER_BASE_Y + y * ADAPTER_SCALE,
        )
    };
    let s = ADAPTER_SCALE;
    let rows: [LabelRow; 6] = [
        (0.0, -58.9, "SLOT", 5.6 * s, LOGO_INK, Align::Centre),
        (
            0.0,
            -23.4,
            "WIRELESS ADAPTER",
            5.2 * s,
            LABEL_INK,
            Align::Centre,
        ),
        (
            -37.0,
            -15.8,
            "RADIO : WLAN1 5GHZ   PORT : 7211",
            3.4 * s,
            LABEL_INK,
            Align::Left,
        ),
        (
            -37.0,
            -11.8,
            "MODEL NO. / MODELE NO. AGS-015",
            3.4 * s,
            LABEL_INK,
            Align::Left,
        ),
        (
            -37.0,
            -7.8,
            "MADE IN ITHACA",
            2.7 * s,
            LABEL_INK,
            Align::Left,
        ),
        (
            37.0,
            -7.8,
            "S/LOT-A-WA-USA",
            2.7 * s,
            LABEL_INK,
            Align::Right,
        ),
    ];
    for (x, baseline, line, px, ink, align) in rows {
        let (fx, fy) = at(x, baseline);
        stamp(&mut face, fx, fy, line, px, ink, align);
    }
    face
}

fn stamp(
    face: &mut CartFace,
    x: f32,
    baseline: f32,
    line: &str,
    px: f32,
    ink: [u8; 3],
    align: Align,
) {
    let Some(font) = text::label_font() else {
        return;
    };
    let layout = text::fit(font, line, f32::MAX, 1, px, px);
    let width = layout
        .lines
        .first()
        .map(|l| text::line_width(font, l, px, layout.tracking))
        .unwrap_or(0.0);
    let (bw, bh) = ((width.ceil() as u32).max(1), (px * 1.6).ceil() as u32);
    let cov = text::coverage(bw, bh, &layout);
    let left = match align {
        Align::Left => x,
        Align::Centre => x - bw as f32 / 2.0,
        Align::Right => x - bw as f32,
    }
    .round() as i32;
    let top = (baseline - px * 1.2).round() as i32;
    for row in 0..bh as i32 {
        for col in 0..bw as i32 {
            let a = cov[(row as u32 * bw + col as u32) as usize] as u32;
            let (dx, dy) = (left + col, top + row);
            if a == 0 || dx < 0 || dy < 0 || dx >= face.w as i32 || dy >= face.h as i32 {
                continue;
            }
            let d = ((dy as u32 * face.w + dx as u32) * 4) as usize;
            for (k, c) in ink.iter().enumerate() {
                face.rgba[d + k] =
                    ((*c as u32 * a + face.rgba[d + k] as u32 * (255 - a)) / 255) as u8;
            }
            face.rgba[d + 3] = face.rgba[d + 3].max(a as u8);
        }
    }
}

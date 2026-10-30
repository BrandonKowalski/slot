use slot_gfx::Canvas;
use slot_store::{Cart, Core};

use crate::bitmaps;
use crate::cart::{clean_label, CartFace, CART_H, CART_W};
use crate::shelf::foot_y;
use crate::slot_chrome::ease;
use crate::text;

pub const BOARD_W: u32 = CART_W;
pub const BOARD_H: u32 = CART_H;

/// The black ROM chip on `assets/board.png`: x 61..132, y 48..138.
pub const ROM_X: u32 = 61;
pub const ROM_Y: u32 = 48;
pub const ROM_W: u32 = 71;
pub const ROM_H: u32 = 90;

const MARK_LINE_CHARS: usize = 10;
const MARK_LINES: usize = 3;
const MARK_PAD: u32 = 4;
const MARK_PX: f32 = 10.0;
const MARK_MIN_PX: f32 = 6.0;
const MARK_INK: [u8; 3] = [0xbd, 0xbd, 0xbd];

pub fn rom_marking(stem: &str) -> Vec<String> {
    let mut title: Vec<String> = Vec::new();
    for word in clean_label(stem).to_uppercase().split_whitespace() {
        match title.last_mut() {
            Some(line) if line.len() + 1 + word.len() <= MARK_LINE_CHARS => {
                line.push(' ');
                line.push_str(word);
            }
            _ => title.push(word.to_string()),
        }
    }
    if title.len() > MARK_LINES {
        let tail = title.split_off(MARK_LINES - 1).join(" ");
        title.push(tail);
    }
    title
}

pub fn rom_marking_face(stem: &str) -> CartFace {
    marking_face(&rom_marking(stem))
}

pub fn bare_board_face() -> CartFace {
    bitmap_face(bitmaps::board(), BOARD_W, BOARD_H)
}

pub fn update_cart_face() -> CartFace {
    bitmap_face(bitmaps::update_cart(), CART_W, CART_H)
}

pub fn baseos_cart_face() -> CartFace {
    bitmap_face(bitmaps::baseos_cart(), CART_W, CART_H)
}

fn marking_face(title: &[String]) -> CartFace {
    let mut face = CartFace {
        rgba: vec![0; (ROM_W * ROM_H * 4) as usize],
        w: ROM_W,
        h: ROM_H,
    };
    let Some(font) = text::label_font() else {
        return face;
    };
    let max_w = (ROM_W - 2 * MARK_PAD) as f32;
    let line_h = (MARK_PX * 1.25).ceil() as u32;
    let mut top = ROM_H.saturating_sub(line_h * title.len() as u32) / 2;
    for line in title {
        let layout = text::fit(font, line, max_w, 1, MARK_PX, MARK_MIN_PX);
        ink_band(&mut face, top, line_h, &layout, MARK_INK, 1.0);
        top += line_h;
    }
    face
}

pub fn board_face(cart: &Cart) -> CartFace {
    let mut face = CartFace {
        rgba: bitmaps::board().to_vec(),
        w: BOARD_W,
        h: BOARD_H,
    };
    over(&mut face, &rom_marking_face(&cart.stem), ROM_X, ROM_Y);
    face
}

fn ink_band(
    face: &mut CartFace,
    top: u32,
    band: u32,
    layout: &text::Layout,
    ink: [u8; 3],
    strength: f32,
) {
    let band = band.min(face.h.saturating_sub(top));
    for (i, a) in text::coverage(face.w, band, layout).into_iter().enumerate() {
        let a = (a as f32 * strength).round() as u8;
        if a == 0 {
            continue;
        }
        let at = ((top * face.w) as usize + i) * 4;
        let (a, d_a) = (a as f32 / 255.0, face.rgba[at + 3] as f32 / 255.0);
        let out_a = a + d_a * (1.0 - a);
        for (k, &c) in ink.iter().enumerate() {
            let d_rgb = face.rgba[at + k] as f32 / 255.0;
            let out_rgb = (c as f32 / 255.0 * a + d_rgb * d_a * (1.0 - a)) / out_a;
            face.rgba[at + k] = (out_rgb * 255.0).round() as u8;
        }
        face.rgba[at + 3] = (out_a * 255.0).round() as u8;
    }
}

fn over(dst: &mut CartFace, src: &CartFace, x: u32, y: u32) {
    for row in 0..src.h {
        for col in 0..src.w {
            let s = ((row * src.w + col) * 4) as usize;
            let a = src.rgba[s + 3] as u32;
            let (dx, dy) = (x + col, y + row);
            if a == 0 || dx >= dst.w || dy >= dst.h {
                continue;
            }
            let d = ((dy * dst.w + dx) * 4) as usize;
            for k in 0..3 {
                dst.rgba[d + k] =
                    ((src.rgba[s + k] as u32 * a + dst.rgba[d + k] as u32 * (255 - a)) / 255) as u8;
            }
            dst.rgba[d + 3] = dst.rgba[d + 3].max(a as u8);
        }
    }
}

fn bitmap_face(rgba: &[u8], w: u32, h: u32) -> CartFace {
    CartFace {
        rgba: rgba.to_vec(),
        w,
        h,
    }
}

pub fn board_x(canvas: Canvas) -> f32 {
    canvas.centred(BOARD_W as f32)
}

pub const BOARD_Y: f32 = 150.0;

pub const TURN_PAD: u32 = 2;

pub const LID_TURN: f32 = -5.0 * std::f32::consts::PI / 180.0;
pub const CHIP_TIP: f32 = 4.0 * std::f32::consts::PI / 180.0;
pub const HOP_LIFT: f32 = 10.0;

pub const SLIDE_SHARE: f32 = 160.0 / 420.0;
pub const SLIDE_UP: f32 = CART_H as f32 / 3.0;

pub fn slide_of(progress: f32) -> f32 {
    ease((progress / SLIDE_SHARE).clamp(0.0, 1.0))
}

pub fn lift_of(progress: f32) -> f32 {
    ease(((progress - SLIDE_SHARE) / (1.0 - SLIDE_SHARE)).clamp(0.0, 1.0))
}

pub const FINGERS: [(f32, f32); 32] = [
    (35.0, 43.0),
    (46.0, 52.0),
    (55.0, 61.0),
    (64.0, 70.0),
    (73.0, 79.0),
    (82.0, 88.0),
    (91.0, 97.0),
    (100.0, 106.0),
    (109.0, 115.0),
    (118.0, 124.0),
    (127.0, 133.0),
    (136.0, 142.0),
    (145.0, 151.0),
    (154.0, 160.0),
    (163.0, 169.0),
    (172.0, 178.0),
    (181.0, 187.0),
    (190.0, 196.0),
    (199.0, 205.0),
    (208.0, 214.0),
    (217.0, 223.0),
    (226.0, 232.0),
    (235.0, 241.0),
    (244.0, 250.0),
    (253.0, 259.0),
    (262.0, 268.0),
    (271.0, 277.0),
    (280.0, 286.0),
    (289.0, 295.0),
    (298.0, 304.0),
    (307.0, 313.0),
    (316.0, 324.0),
];
pub const FINGER_Y: f32 = 167.0;
pub const FINGER_H: f32 = 28.0;

pub const SOCKET_U: [f32; 2] = [99.5, 171.5];
pub const SOCKET_V: f32 = 59.1;
pub const SOCKET_W: u32 = 64;
pub const SOCKET_H: u32 = 46;

pub const CHIP_U: [f32; 2] = [101.0, 173.0];
pub const CHIP_V: f32 = 60.6;
pub const CHIP_W: u32 = 59;
pub const CHIP_H: u32 = 41;

pub const SHADOW_W: u32 = 66;
pub const SHADOW_H: u32 = 12;

const NAME_PX: f32 = 17.0;
const NAME_MIN_PX: f32 = 8.0;
const SOCKET_INK: [u8; 3] = [0xee, 0xf5, 0xe6];
const CHIP_INK: [u8; 3] = [0xf2, 0xf2, 0xf2];

fn lid_rest(canvas: Canvas) -> Placed {
    Placed {
        x: canvas.centred(144.0),
        y: 70.5 - LID_REST_H / 2.0,
        w: 144.0,
        h: LID_REST_H,
    }
}

const LID_REST_H: f32 = 144.0 * CART_H as f32 / CART_W as f32;

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Placed {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub fn shelf_cart(canvas: Canvas) -> Placed {
    shelf_cart_at(canvas.centred(CART_W as f32), 1.0)
}

pub fn shelf_cart_at(x: f32, scale: f32) -> Placed {
    let (w, h) = (CART_W as f32 * scale, CART_H as f32 * scale);
    Placed {
        x,
        y: foot_y(CART_H as f32) - h,
        w,
        h,
    }
}

pub fn board_at(canvas: Canvas, progress: f32) -> Placed {
    board_from(canvas, shelf_cart(canvas), progress)
}

pub fn board_from(canvas: Canvas, shelf: Placed, progress: f32) -> Placed {
    let rest = Placed {
        x: board_x(canvas),
        y: BOARD_Y,
        w: BOARD_W as f32,
        h: BOARD_H as f32,
    };
    lerp(board_in(shelf), rest, lift_of(progress))
}

/// The board and the back shell fill the cart they sit in, same size as the lid. Letterboxing them
/// to the board bitmap's own aspect made both look shorter the moment the cart opened.
pub fn board_in(cart: Placed) -> Placed {
    cart
}

pub fn lid_at(canvas: Canvas, progress: f32) -> (Placed, f32) {
    lid_from(canvas, shelf_cart(canvas), progress)
}

pub fn lid_from(canvas: Canvas, shelf: Placed, progress: f32) -> (Placed, f32) {
    let slid = Placed {
        y: shelf.y - SLIDE_UP * slide_of(progress),
        ..shelf
    };
    let lift = lift_of(progress);
    (lerp(slid, lid_rest(canvas), lift), LID_TURN * lift)
}

const UNITS_W: f32 = 240.0;
const UNITS_H: f32 = 135.0;

pub fn on_board(board: Placed, u: f32, v: f32) -> (f32, f32) {
    (
        board.x + u * board.w / UNITS_W,
        board.y + v * board.h / UNITS_H,
    )
}

pub fn board_zoom(board: Placed) -> f32 {
    board.w / BOARD_W as f32
}

pub fn grown(p: Placed, by: f32) -> Placed {
    Placed {
        x: p.x - by,
        y: p.y - by,
        w: p.w + 2.0 * by,
        h: p.h + 2.0 * by,
    }
}

fn lerp(a: Placed, b: Placed, t: f32) -> Placed {
    Placed {
        x: a.x + (b.x - a.x) * t,
        y: a.y + (b.y - a.y) * t,
        w: a.w + (b.w - a.w) * t,
        h: a.h + (b.h - a.h) * t,
    }
}

pub fn padded(face: &CartFace, pad: u32) -> CartFace {
    let (w, h) = (face.w + 2 * pad, face.h + 2 * pad);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let stride = (face.w * 4) as usize;
    for row in 0..face.h {
        let from = (row * face.w * 4) as usize;
        let to = (((row + pad) * w + pad) * 4) as usize;
        rgba[to..to + stride].copy_from_slice(&face.rgba[from..from + stride]);
    }
    CartFace { rgba, w, h }
}

pub fn socket_face(core: Core) -> CartFace {
    socket_face_text(core.text())
}

pub fn socket_face_text(name: &str) -> CartFace {
    let mut face = bitmap_face(bitmaps::socket(), SOCKET_W, SOCKET_H);
    if let Some(layout) = name_layout(name, (SOCKET_W - 12) as f32) {
        ink_band(&mut face, 4, 37, &layout, SOCKET_INK, 0.5);
    }
    face
}

pub fn chip_face(core: Option<Core>) -> CartFace {
    match core {
        Some(core) => chip_face_text(core.text()),
        None => padded(&bitmap_face(bitmaps::chip(), CHIP_W, CHIP_H), TURN_PAD),
    }
}

pub fn chip_face_text(name: &str) -> CartFace {
    let mut face = bitmap_face(bitmaps::chip(), CHIP_W, CHIP_H);
    if let Some(layout) = name_layout(name, (CHIP_W - 8) as f32) {
        ink_band(&mut face, 4, 34, &layout, CHIP_INK, 1.0);
    }
    padded(&face, TURN_PAD)
}

pub fn chip_shadow_face() -> CartFace {
    let (w, h) = (SHADOW_W, SHADOW_H);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let dx = (x as f32 + 0.5 - w as f32 / 2.0) / (w as f32 / 2.0);
            let dy = (y as f32 + 0.5 - h as f32 / 2.0) / (h as f32 / 2.0);
            let fall = (1.0 - (dx * dx + dy * dy)).max(0.0);
            rgba.extend_from_slice(&[0, 0, 0, (fall * fall * 255.0) as u8]);
        }
    }
    CartFace { rgba, w, h }
}

fn name_layout(name: &str, max_w: f32) -> Option<text::Layout> {
    let font = text::label_font()?;
    let tracking = |px: f32| (px * 0.10).round();
    let mut px = NAME_PX;
    while px > NAME_MIN_PX && text::line_width(font, name, px, tracking(px)) > max_w {
        px -= 1.0;
    }
    Some(text::Layout {
        lines: vec![name.to_string()],
        px,
        tracking: tracking(px),
    })
}

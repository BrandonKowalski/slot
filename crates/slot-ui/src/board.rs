//! The core picker's open cart: the back half of the shell with the board in it, rasterised
//! per cart because the shell is that cart's plastic and the ROM carries that cart's name.
//!
//! Drawn from `board.svg` and not from rects: the notch round the centre post, the patterned
//! legs and contacts and the 45° traces are drawings, not a layout.

use slot_store::Cart;

use crate::art;
use crate::cart::{clean_label, label_tags, CartFace};
use crate::shell::shell_for;
use crate::text;

const BOARD_SVG: &str = include_str!("../assets/board.svg");

/// The cart's 240×135 at 1.55×, the size the open cart is shown at. Sharp only at its own size.
pub const BOARD_W: u32 = 372;
pub const BOARD_H: u32 = 209;

/// The ROM's body inside the board face: board units (43, 31) to (89, 89).
pub const ROM_X: u32 = 67;
pub const ROM_Y: u32 = 48;
pub const ROM_W: u32 = 71;
pub const ROM_H: u32 = 90;

/// The placeholders `board.svg` paints its shell parts in.
const PLASTIC: &str = "#ff00ff";
const FLOOR: &str = "#800080";
const DEEP: &str = "#400040";

/// Words to a line on the ROM, and lines to the chip. Counted in characters rather than
/// measured, so the split is a fact about the name and the tests can state it; the fitter
/// then shrinks any line that is still too wide.
const MARK_LINE_CHARS: usize = 10;
const MARK_LINES: usize = 3;
const MARK_PAD: u32 = 4;
const MARK_PX: f32 = 10.0;
const MARK_MIN_PX: f32 = 6.0;
const TAG_PX: f32 = 7.0;
const TAG_MIN_PX: f32 = 5.0;
/// Grey on black, as a mask ROM is marked: legible, and nothing like the socket names, which
/// are the words on the board that mean something.
const MARK_INK: [u8; 3] = [0xbd, 0xbd, 0xbd];
const TAG_INK: [u8; 3] = [0x8a, 0x8a, 0x8a];
const CELL_INK: [u8; 3] = [0x6a, 0x6b, 0x70];

/// What the ROM says: the game a few words to a line, and the dump's tags beneath.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RomMarking {
    pub title: Vec<String>,
    pub tags: Option<String>,
}

pub fn rom_marking(stem: &str) -> RomMarking {
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
    let tags = label_tags(stem);
    RomMarking {
        title,
        tags: (!tags.is_empty()).then(|| tags.join(" · ").to_uppercase()),
    }
}

/// The marking alone, on nothing, the size of the ROM's body.
pub fn rom_marking_face(stem: &str) -> CartFace {
    let mark = rom_marking(stem);
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
    let tag_h = match mark.tags {
        Some(_) => (TAG_PX * 1.6).ceil() as u32,
        None => 0,
    };
    let mut top = ROM_H.saturating_sub(line_h * mark.title.len() as u32 + tag_h) / 2;
    for line in &mark.title {
        let layout = text::fit(font, line, max_w, 1, MARK_PX, MARK_MIN_PX);
        ink_band(&mut face, top, line_h, &layout, MARK_INK);
        top += line_h;
    }
    if let Some(tags) = &mark.tags {
        let layout = text::fit(font, tags, max_w, 1, TAG_PX, TAG_MIN_PX);
        ink_band(&mut face, top, tag_h, &layout, TAG_INK);
    }
    face
}

pub fn board_face(cart: &Cart) -> CartFace {
    let shell = shell_for(&cart.code);
    let svg = BOARD_SVG
        .replace(PLASTIC, &hex(shell.colour))
        .replace(FLOOR, &hex(shade(shell.colour, 0.62)))
        .replace(DEEP, &hex(shade(shell.colour, 0.35)));
    let rgba = art::render_svg(&svg, BOARD_W, BOARD_H)
        .unwrap_or_else(|| vec![0; (BOARD_W * BOARD_H * 4) as usize]);
    let mut face = CartFace {
        rgba,
        w: BOARD_W,
        h: BOARD_H,
    };
    over(&mut face, &rom_marking_face(&cart.stem), ROM_X, ROM_Y);
    print_cell(&mut face);
    face
}

/// One line of type, centred across the face, in `band` rows from `top`. Ink colour with the
/// coverage as alpha, so the face composites as straight alpha like every other face here.
fn ink_band(face: &mut CartFace, top: u32, band: u32, layout: &text::Layout, ink: [u8; 3]) {
    let band = band.min(face.h.saturating_sub(top));
    for (i, a) in text::coverage(face.w, band, layout).into_iter().enumerate() {
        if a == 0 {
            continue;
        }
        let at = ((top * face.w) as usize + i) * 4;
        face.rgba[at..at + 3].copy_from_slice(&ink);
        face.rgba[at + 3] = face.rgba[at + 3].max(a);
    }
}

/// `CR1616` on the cell, at board unit (204, 38).
fn print_cell(face: &mut CartFace) {
    let Some(font) = text::label_font() else {
        return;
    };
    let (w, h) = (30u32, 10u32);
    let mut cell = CartFace {
        rgba: vec![0; (w * h * 4) as usize],
        w,
        h,
    };
    let layout = text::fit(font, "CR1616", w as f32, 1, 6.0, 4.0);
    ink_band(&mut cell, 0, h, &layout, CELL_INK);
    over(face, &cell, 316 - w / 2, 59 - h / 2);
}

/// Straight alpha over an opaque face.
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

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn shade(c: [u8; 3], f: f32) -> [u8; 3] {
    c.map(|v| (v as f32 * f).round().clamp(0.0, 255.0) as u8)
}

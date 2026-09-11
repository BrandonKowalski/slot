//! The core picker's open cart: the back half of the shell with the board in it, rasterised
//! per cart because the shell is that cart's plastic and the ROM carries that cart's name.
//!
//! Drawn from `board.svg` and not from rects: the notch round the centre post, the patterned
//! legs and contacts and the 45° traces are drawings, not a layout.

use slot_gfx::OUT_W;
use slot_store::{Cart, Core};

use crate::art;
use crate::cart::{clean_label, label_tags, CartFace, CART_H, CART_W};
use crate::shelf::FOOT_Y;
use crate::shell::shell_for;
use crate::slot_chrome::ease;
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
        ink_band(&mut face, top, line_h, &layout, MARK_INK, 1.0);
        top += line_h;
    }
    if let Some(tags) = &mark.tags {
        let layout = text::fit(font, tags, max_w, 1, TAG_PX, TAG_MIN_PX);
        ink_band(&mut face, top, tag_h, &layout, TAG_INK, 1.0);
    }
    face
}

pub fn board_face(cart: &Cart) -> CartFace {
    let shell = shell_for(&cart.code);
    // Deepest placeholder first and the wall last, so a shell whose own hex or shade matches
    // a placeholder still further down the list finds nothing left to replace: once a
    // placeholder's `.replace` call has run, its literal text is gone from the SVG.
    let svg = BOARD_SVG
        .replace(DEEP, &hex(shade(shell.colour, 0.35)))
        .replace(FLOOR, &hex(shade(shell.colour, 0.62)))
        .replace(PLASTIC, &hex(shell.colour));
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

/// One line of type, centred across the face, in `band` rows from `top`, its coverage scaled by
/// `strength`. Composited straight-over whatever is already there: on a transparent pixel that
/// is ink colour at the coverage's own alpha, same as writing it outright, but on an opaque one
/// — the chip's body — the glyph's antialiased edge blends toward what was under it instead of
/// snapping to full ink.
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
    ink_band(&mut cell, 0, h, &layout, CELL_INK, 1.0);
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

const SOCKET_SVG: &str = include_str!("../assets/socket.svg");
const CHIP_SVG: &str = include_str!("../assets/chip.svg");

/// Where the open cart rests.
pub const BOARD_X: f32 = 174.0;
pub const BOARD_Y: f32 = 150.0;

/// Transparent border on every face that is drawn turned. A turned quad's own edge is not
/// antialiased; inside the texture, the linear filter softens it.
pub const TURN_PAD: u32 = 2;

/// The lid's tilt at rest, and the most the chip tips in flight.
pub const LID_TURN: f32 = -5.0 * std::f32::consts::PI / 180.0;
pub const CHIP_TIP: f32 = 4.0 * std::f32::consts::PI / 180.0;
/// How far the chip rises at mid-flight, in board units.
pub const HOP_LIFT: f32 = 10.0;

/// The open is one progress in two beats, and this is the slide's share of it: 160 ms of the
/// 420 ms in `slot::core_picker`, which a test there holds to its own constants.
pub const SLIDE_SHARE: f32 = 160.0 / 420.0;
/// How far the front half slides up off the back before it lifts: a third of the cart, the
/// travel that unhooks a real shell once its screw is out.
pub const SLIDE_UP: f32 = CART_H as f32 / 3.0;

/// The slide, eased on its own share of the progress: 0.0 closed, 1.0 unhooked.
pub fn slide_of(progress: f32) -> f32 {
    ease((progress / SLIDE_SHARE).clamp(0.0, 1.0))
}

/// The lift, eased on the rest of the progress: 0.0 still over the back, 1.0 at rest.
pub fn lift_of(progress: f32) -> f32 {
    ease(((progress - SLIDE_SHARE) / (1.0 - SLIDE_SHARE)).clamp(0.0, 1.0))
}

/// Each socket's face, in `Core::ALL` order: board units of its top left, and its size.
pub const SOCKET_U: [f32; 2] = [99.5, 171.5];
pub const SOCKET_V: f32 = 59.1;
pub const SOCKET_W: u32 = 64;
pub const SOCKET_H: u32 = 46;

/// The chip seated in each socket, unpadded: board units of its top left, and its size.
pub const CHIP_U: [f32; 2] = [101.0, 173.0];
pub const CHIP_V: f32 = 60.6;
pub const CHIP_W: u32 = 59;
pub const CHIP_H: u32 = 41;

pub const SHADOW_W: u32 = 66;
pub const SHADOW_H: u32 = 12;

/// 11 board units, the socket names' size in the mockup.
const NAME_PX: f32 = 17.0;
const NAME_MIN_PX: f32 = 8.0;
const SOCKET_INK: [u8; 3] = [0xee, 0xf5, 0xe6];
const CHIP_INK: [u8; 3] = [0xf2, 0xf2, 0xf2];

const LID_REST: Placed = Placed {
    x: 288.0,
    y: 30.0,
    w: 144.0,
    h: 81.0,
};

/// A rect on the panel.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Placed {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// The highlighted cart as the shelf stands it, once the row has settled.
pub fn shelf_cart() -> Placed {
    Placed {
        x: (OUT_W - CART_W) as f32 / 2.0,
        y: FOOT_Y - CART_H as f32,
        w: CART_W as f32,
        h: CART_H as f32,
    }
}

/// The back half: standing where the shelf stood it through the slide, then growing to its rest.
pub fn board_at(progress: f32) -> Placed {
    let rest = Placed {
        x: BOARD_X,
        y: BOARD_Y,
        w: BOARD_W as f32,
        h: BOARD_H as f32,
    };
    lerp(shelf_cart(), rest, lift_of(progress))
}

/// The front half and its turn: slid up off the back, then lifted from there to its rest.
pub fn lid_at(progress: f32) -> (Placed, f32) {
    let shelf = shelf_cart();
    let slid = Placed {
        y: shelf.y - SLIDE_UP * slide_of(progress),
        ..shelf
    };
    let lift = lift_of(progress);
    (lerp(slid, LID_REST, lift), LID_TURN * lift)
}

/// A board unit on the panel, wherever the open cart currently is.
pub fn on_board(board: Placed, u: f32, v: f32) -> (f32, f32) {
    (
        board.x + u * board.w / CART_W as f32,
        board.y + v * board.h / CART_H as f32,
    )
}

/// How much bigger or smaller than its own size a board face is drawn right now.
pub fn board_zoom(board: Placed) -> f32 {
    board.w / BOARD_W as f32
}

/// `p` with `by` more on every side.
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

/// A copy with `pad` transparent pixels on every side.
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

/// An empty socket: pads, outline, and the name of the core it is for at half strength.
pub fn socket_face(core: Core) -> CartFace {
    let rgba = art::render_svg(SOCKET_SVG, SOCKET_W, SOCKET_H)
        .unwrap_or_else(|| vec![0; (SOCKET_W * SOCKET_H * 4) as usize]);
    let mut face = CartFace {
        rgba,
        w: SOCKET_W,
        h: SOCKET_H,
    };
    if let Some(layout) = name_layout(core, (SOCKET_W - 12) as f32) {
        // Inside the outline, which runs from 2.9 to 26.9 units down the face.
        ink_band(&mut face, 4, 37, &layout, SOCKET_INK, 0.5);
    }
    face
}

/// The chip, named for the socket it is seated in, or blank in flight. Padded, since it tips.
pub fn chip_face(core: Option<Core>) -> CartFace {
    let rgba = art::render_svg(CHIP_SVG, CHIP_W, CHIP_H)
        .unwrap_or_else(|| vec![0; (CHIP_W * CHIP_H * 4) as usize]);
    let mut face = CartFace {
        rgba,
        w: CHIP_W,
        h: CHIP_H,
    };
    if let Some(layout) = core.and_then(|c| name_layout(c, (CHIP_W - 8) as f32)) {
        // The body, between the two rows of legs.
        ink_band(&mut face, 4, 34, &layout, CHIP_INK, 1.0);
    }
    padded(&face, TURN_PAD)
}

/// A soft dark oval, for under the chip while it is off the board.
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

/// A core's name exactly as the player reads it — `mGBA`, not `MGBA`. `text::fit` capitalises
/// everything it lays out, so this builds the layout itself, shrinking only if it must.
fn name_layout(core: Core, max_w: f32) -> Option<text::Layout> {
    let font = text::label_font()?;
    let tracking = |px: f32| (px * 0.10).round();
    let mut px = NAME_PX;
    while px > NAME_MIN_PX && text::line_width(font, core.text(), px, tracking(px)) > max_w {
        px -= 1.0;
    }
    Some(text::Layout {
        lines: vec![core.text().to_string()],
        px,
        tracking: tracking(px),
    })
}

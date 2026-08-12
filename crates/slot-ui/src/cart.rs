use slot_store::Cart;

use crate::art;
use crate::shell::{shell_for, Finish, Shell};
use crate::silhouette::{cart_depth, cart_mask, detail_mask};
use crate::text;

/// The traced outline's own aspect, so `cart.svg` rasterises unstretched. Three across a
/// 720 wide row exactly, so the shelf can show a neighbour either side of the selection.
pub const CART_W: u32 = 240;
pub const CART_H: u32 = 135;

/// The paper label, inset in the shell rather than covering it: 9% to 91% across and 22.8%
/// to 86.3% down. The vertical placement is the reference's, and the band it leaves above is
/// the moulded grip; that asymmetry is most of what makes the face read as a cartridge
/// rather than a bordered rectangle. The horizontal inset is deliberately tighter than the
/// reference's 14.1%, which was an icon's proportion rather than a cartridge's: a real
/// label runs nearly the full width with only a thin edge of plastic beside it.
pub const fn label_panel(w: u32, h: u32) -> (u32, u32, u32, u32) {
    (
        (w * 90 + 500) / 1000,
        (h * 228 + 500) / 1000,
        (w * 910 + 500) / 1000,
        (h * 863 + 500) / 1000,
    )
}

pub const LABEL_X: u32 = label_panel(CART_W, CART_H).0;
pub const LABEL_Y: u32 = label_panel(CART_W, CART_H).1;
pub const LABEL_W: u32 = label_panel(CART_W, CART_H).2 - LABEL_X;
pub const LABEL_H: u32 = label_panel(CART_W, CART_H).3 - LABEL_Y;

const PAD: u32 = 10;
const MAX_LINES: usize = 3;
/// Three lines have to clear the label's height, and Open Sans Bold sets at about 1.36x
/// the em. The label is landscape now, so it runs out of height long before width.
const MAX_PX: f32 = LABEL_H as f32 / (MAX_LINES as f32 * 1.36);
const MIN_PX: f32 = 10.0;

/// How far the translucent edge reaches in. Zero at this depth exactly, so a pixel any
/// further in is the plastic's own colour.
const RIM: u32 = 4;

pub struct CartFace {
    pub rgba: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

/// The cart's own shape in black. Drawn under a side cart so the dimming is a cart in shadow
/// rather than a cart you can see through: over a wallpaper a translucent face is a ghost,
/// and the shelf's carts are solid objects.
pub fn cart_shadow() -> CartFace {
    let mut rgba = Vec::with_capacity((CART_W * CART_H * 4) as usize);
    for cover in cart_mask() {
        rgba.extend_from_slice(&[0, 0, 0, *cover]);
    }
    CartFace {
        rgba,
        w: CART_W,
        h: CART_H,
    }
}

pub fn cart_face(cart: &Cart) -> CartFace {
    let shell = shell_for(&cart.code);
    let mut face = shell_face(&shell);
    let label = match cart
        .label
        .as_deref()
        .and_then(|p| art::cover(p, LABEL_W, LABEL_H))
    {
        Some(rgba) => rgba,
        None => generated_label(&label_text(cart)),
    };
    mould_detail(&mut face, &shell);
    recess_label(&mut face, &shell);
    paste_label(&mut face, &label);
    clip_to_silhouette(&mut face);
    face
}

/// Colour is left alone and only alpha is cut, because the sprite pass blends straight
/// alpha rather than premultiplied.
fn clip_to_silhouette(face: &mut CartFace) {
    for (px, cover) in face.rgba.chunks_exact_mut(4).zip(cart_mask()) {
        px[3] = ((px[3] as u32 * *cover as u32 + 127) / 255) as u8;
    }
}

/// Stable across runs, which the standard hasher is not: the same game must be the same
/// colour on every boot, or the shelf is unrecognisable from memory.
pub fn label_colour(title: &str) -> [u8; 3] {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in title.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hsv_to_rgb((h % 360) as f32, 0.52, 0.74)
}

/// The header title is capped at twelve characters, so it reads `POKEMON EMER`. The
/// filename holds the real name.
pub fn label_text(cart: &Cart) -> String {
    clean_label(&cart.stem)
}

/// A dumped filename carries region and revision tags and separates title from subtitle
/// with a spaced hyphen. A bare hyphen is part of a word, so `Spider-Man` keeps its own.
pub fn clean_label(stem: &str) -> String {
    let mut bare = String::with_capacity(stem.len());
    let mut depth = 0u32;
    for ch in stem.chars() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => bare.push(ch),
            _ => {}
        }
    }

    let mut out = String::with_capacity(bare.len());
    for word in bare.split_whitespace().filter(|w| *w != "-") {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    if out.is_empty() {
        stem.to_string()
    } else {
        out
    }
}

fn shell_face(shell: &Shell) -> CartFace {
    let mut rgba = Vec::with_capacity((CART_W * CART_H * 4) as usize);
    let edge = rim_colour(shell.colour);
    for depth in cart_depth() {
        let c = match shell.finish {
            Finish::Solid => shell.colour,
            Finish::Translucent => lerp(edge, shell.colour, (*depth as u32).min(RIM), RIM),
        };
        rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
    }
    CartFace {
        rgba,
        w: CART_W,
        h: CART_H,
    }
}

/// Light through the plastic reads as a lighter, less saturated edge. Desaturating as well
/// as lightening is what keeps it from looking like a white outline drawn on the shell.
fn rim_colour(base: [u8; 3]) -> [u8; 3] {
    let mean = (base[0] as u16 + base[1] as u16 + base[2] as u16) / 3;
    base.map(|c| {
        let grey = (3 * c as u16 + mean) / 4;
        (grey + (255 - grey) * 2 / 5) as u8
    })
}

fn lerp(a: [u8; 3], b: [u8; 3], num: u32, den: u32) -> [u8; 3] {
    let mut out = [0u8; 3];
    for c in 0..3 {
        out[c] = ((a[c] as u32 * (den - num) + b[c] as u32 * num) / den) as u8;
    }
    out
}

/// The wall of the moulded recess the label sits in. Light comes from the upper left, so
/// the top and left walls are turned away from it and fall into shadow while the bottom and
/// right walls catch it. Painted before the label, so the label sits on the floor of the
/// recess with the wall showing around it.
const BEVEL: u32 = 3;

/// The grip ridge and the thumb notch, cut into the shell. Darkened rather than coloured:
/// moulded plastic is the same plastic, just turned away from the light.
fn mould_detail(face: &mut CartFace, shell: &Shell) {
    let dark = [
        (shell.colour[0] as f32 * 0.62) as u8,
        (shell.colour[1] as f32 * 0.62) as u8,
        (shell.colour[2] as f32 * 0.62) as u8,
    ];
    for (px, cover) in face.rgba.chunks_exact_mut(4).zip(detail_mask()) {
        let a = *cover as u32;
        if a == 0 {
            continue;
        }
        for c in 0..3 {
            px[c] = ((dark[c] as u32 * a + px[c] as u32 * (255 - a) + 127) / 255) as u8;
        }
    }
}

fn recess_label(face: &mut CartFace, shell: &Shell) {
    let shade = |c: [u8; 3], f: f32| -> [u8; 3] {
        [
            (c[0] as f32 * f).clamp(0.0, 255.0) as u8,
            (c[1] as f32 * f).clamp(0.0, 255.0) as u8,
            (c[2] as f32 * f).clamp(0.0, 255.0) as u8,
        ]
    };
    let dark = shade(shell.colour, 0.55);
    let lit = shade(shell.colour, 1.45);

    let (x0, y0) = (LABEL_X - BEVEL, LABEL_Y - BEVEL);
    let (x1, y1) = (LABEL_X + LABEL_W + BEVEL, LABEL_Y + LABEL_H + BEVEL);
    let mut put = |x: u32, y: u32, c: [u8; 3]| {
        if x >= CART_W || y >= CART_H {
            return;
        }
        let d = ((y * CART_W + x) * 4) as usize;
        face.rgba[d] = c[0];
        face.rgba[d + 1] = c[1];
        face.rgba[d + 2] = c[2];
    };
    for y in y0..y1 {
        for x in x0..x1 {
            let inside = (LABEL_X..LABEL_X + LABEL_W).contains(&x)
                && (LABEL_Y..LABEL_Y + LABEL_H).contains(&y);
            if inside {
                continue;
            }
            // Which wall a pixel belongs to: the nearer of the two edges it sits between.
            let from_top = y.saturating_sub(y0);
            let from_left = x.saturating_sub(x0);
            let from_bottom = y1.saturating_sub(y + 1);
            let from_right = x1.saturating_sub(x + 1);
            let upper = from_top.min(from_left);
            let lower = from_bottom.min(from_right);
            put(x, y, if upper <= lower { dark } else { lit });
        }
    }
}

/// Source over, so a label with an alpha channel shows the shell through it rather than
/// punching a hole in the cart.
fn paste_label(face: &mut CartFace, label: &[u8]) {
    for y in 0..LABEL_H {
        for x in 0..LABEL_W {
            let s = ((y * LABEL_W + x) * 4) as usize;
            let a = label[s + 3] as u32;
            if a == 0 {
                continue;
            }
            let d = (((y + LABEL_Y) * CART_W + x + LABEL_X) * 4) as usize;
            for c in 0..3 {
                face.rgba[d + c] =
                    ((label[s + c] as u32 * a + face.rgba[d + c] as u32 * (255 - a) + 127) / 255)
                        as u8;
            }
        }
    }
}

fn generated_label(title: &str) -> Vec<u8> {
    let bg = label_colour(title);
    let mut rgba = Vec::with_capacity((LABEL_W * LABEL_H * 4) as usize);
    for _ in 0..LABEL_W * LABEL_H {
        rgba.extend_from_slice(&[bg[0], bg[1], bg[2], 255]);
    }

    if let Some(font) = text::label_font() {
        let layout = text::fit(
            font,
            title,
            (LABEL_W - 2 * PAD) as f32,
            MAX_LINES,
            MAX_PX,
            MIN_PX,
        );
        text::draw_centred(&mut rgba, LABEL_W, LABEL_H, &layout, ink(bg));
    }
    rgba
}

/// Hue rotation alone puts yellow and blue at very different luminance, so the ink flips
/// rather than sitting at one fixed value.
fn ink(bg: [u8; 3]) -> [u8; 3] {
    let luma = 0.2126 * bg[0] as f32 + 0.7152 * bg[1] as f32 + 0.0722 * bg[2] as f32;
    if luma > 140.0 {
        [0x1a, 0x18, 0x16]
    } else {
        [0xf4, 0xf1, 0xea]
    }
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    ]
}

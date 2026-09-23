use slot_store::Cart;

use crate::art;
use crate::shell::{shell_for, Finish, Shell};
use crate::silhouette::{cart_depth, cart_mask, detail_mask, Detail};
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

/// Everything about a face that is a property of the object rather than of the game: how big
/// the cartridge is, where its label sits, the masks its outline and moulding come from, and
/// the type size its generated label starts at.
struct Spec {
    w: u32,
    h: u32,
    /// Left, top, width, height of the label well.
    label: (u32, u32, u32, u32),
    mask: &'static [u8],
    depth: &'static [u8],
    detail: &'static Detail,
    max_px: f32,
    /// The height the type block has to fit inside, or infinite where the panel is wide enough
    /// for its height that only the width can ever bind.
    max_h: f32,
    /// How far in the translucent edge reaches.
    rim: u32,
}

/// Anything that is a property of the plastic goes here. Anything that is a property of the
/// game printed on it does not.
fn spec() -> Spec {
    Spec {
        w: CART_W,
        h: CART_H,
        label: (LABEL_X, LABEL_Y, LABEL_W, LABEL_H),
        mask: cart_mask(),
        depth: cart_depth(),
        detail: detail_mask(),
        max_px: MAX_PX,
        max_h: f32::INFINITY,
        rim: RIM,
    }
}

/// The cart's own shape in black. Drawn under a side cart so the dimming is a cart in shadow
/// rather than a cart you can see through: over a wallpaper a translucent face is a ghost,
/// and the shelf's carts are solid objects.
pub fn cart_shadow() -> CartFace {
    shadow(CART_W, CART_H, cart_mask())
}

fn shadow(w: u32, h: u32, mask: &[u8]) -> CartFace {
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for cover in mask {
        rgba.extend_from_slice(&[0, 0, 0, *cover]);
    }
    CartFace { rgba, w, h }
}

pub fn cart_face(cart: &Cart) -> CartFace {
    let s = spec();
    let shell = shell_for(cart);
    let mut face = shell_face(&s, &shell);
    let (_, _, lw, lh) = s.label;
    let label = match cart.label.as_deref().and_then(|p| art::cover(p, lw, lh)) {
        Some(rgba) => rgba,
        None => generated_label(&s, &label_text(cart)),
    };
    mould_detail(&s, &mut face, &shell);
    recess_label(&s, &mut face, &shell);
    paste_label(&s, &mut face, &label);
    clip_to_silhouette(&s, &mut face);
    face
}

/// Colour is left alone and only alpha is cut, because the sprite pass blends straight
/// alpha rather than premultiplied.
fn clip_to_silhouette(s: &Spec, face: &mut CartFace) {
    for (px, cover) in face.rgba.chunks_exact_mut(4).zip(s.mask) {
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

/// The bracketed groups `clean_label` throws away, in the order they appeared. A dump's
/// filename carries them as one run of parentheses — `(USA, Europe) (Rev 1)` — and each group
/// is one fact about this dump rather than about the game, which is why they are worth
/// keeping apart from the title instead of inside it.
///
/// One tag per group, not per comma: `(USA, Europe)` is a single release in two regions, and
/// splitting it would claim two.
pub fn label_tags(stem: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0u32;
    let mut cur = String::new();
    for ch in stem.chars() {
        match ch {
            '(' | '[' => {
                depth += 1;
                if depth == 1 {
                    cur.clear();
                    continue;
                }
            }
            ')' | ']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let t = cur.trim();
                    if !t.is_empty() {
                        out.push(t.to_string());
                    }
                    continue;
                }
            }
            _ => {}
        }
        if depth >= 1 {
            cur.push(ch);
        }
    }
    out
}

fn shell_face(s: &Spec, shell: &Shell) -> CartFace {
    let mut rgba = Vec::with_capacity((s.w * s.h * 4) as usize);
    let edge = rim_colour(shell.colour);
    for depth in s.depth {
        let c = match shell.finish {
            Finish::Solid => shell.colour,
            Finish::Translucent => lerp(edge, shell.colour, (*depth as u32).min(s.rim), s.rim),
        };
        rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
    }
    CartFace {
        rgba,
        w: s.w,
        h: s.h,
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

/// The moulding in the shell, the grip ridge and the thumb notch. Neither side is coloured: moulded
/// plastic is the same plastic, one face turned away from the light and one turned into it.
///
/// The shadow is multiplied and the light is mixed toward white, which is not an inconsistency
/// — it is how a surface behaves. Shade falls off in proportion to the colour underneath it,
/// so a darker shell shades darker; a highlight is light arriving on top of the surface, so it
/// lifts a dark shell about as far as a pale one. Multiplying the light as well would leave a
/// dark shell's moulding invisible.
fn mould_detail(s: &Spec, face: &mut CartFace, shell: &Shell) {
    let dark = shell.colour.map(|c| (c as f32 * 0.62) as u8);
    let lit = shell.colour.map(|c| c + ((255 - c) as f32 * 0.24) as u8);
    let mix = |px: &mut [u8], to: [u8; 3], a: u32| {
        for c in 0..3 {
            px[c] = ((to[c] as u32 * a + px[c] as u32 * (255 - a) + 127) / 255) as u8;
        }
    };
    let sides = s.detail.shadow.iter().zip(&s.detail.highlight);
    for (px, (shade, light)) in face.rgba.chunks_exact_mut(4).zip(sides) {
        if *shade > 0 {
            mix(px, dark, *shade as u32);
        }
        if *light > 0 {
            mix(px, lit, *light as u32);
        }
    }
}

fn recess_label(s: &Spec, face: &mut CartFace, shell: &Shell) {
    let shade = |c: [u8; 3], f: f32| -> [u8; 3] {
        [
            (c[0] as f32 * f).clamp(0.0, 255.0) as u8,
            (c[1] as f32 * f).clamp(0.0, 255.0) as u8,
            (c[2] as f32 * f).clamp(0.0, 255.0) as u8,
        ]
    };
    let dark = shade(shell.colour, 0.55);
    let lit = shade(shell.colour, 1.45);

    let (lx, ly, lw, lh) = s.label;
    let (w, h) = (s.w, s.h);
    let (x0, y0) = (lx - BEVEL, ly - BEVEL);
    let (x1, y1) = (lx + lw + BEVEL, ly + lh + BEVEL);
    let mut put = |x: u32, y: u32, c: [u8; 3]| {
        if x >= w || y >= h {
            return;
        }
        let d = ((y * w + x) * 4) as usize;
        face.rgba[d] = c[0];
        face.rgba[d + 1] = c[1];
        face.rgba[d + 2] = c[2];
    };
    for y in y0..y1 {
        for x in x0..x1 {
            let inside = (lx..lx + lw).contains(&x) && (ly..ly + lh).contains(&y);
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
fn paste_label(s: &Spec, face: &mut CartFace, label: &[u8]) {
    let (lx, ly, lw, lh) = s.label;
    for y in 0..lh {
        for x in 0..lw {
            let src = ((y * lw + x) * 4) as usize;
            let a = label[src + 3] as u32;
            if a == 0 {
                continue;
            }
            let d = (((y + ly) * s.w + x + lx) * 4) as usize;
            for c in 0..3 {
                face.rgba[d + c] =
                    ((label[src + c] as u32 * a + face.rgba[d + c] as u32 * (255 - a) + 127) / 255)
                        as u8;
            }
        }
    }
}

fn generated_label(s: &Spec, title: &str) -> Vec<u8> {
    let (_, _, lw, lh) = s.label;
    let bg = label_colour(title);
    let mut rgba = Vec::with_capacity((lw * lh * 4) as usize);
    for _ in 0..lw * lh {
        rgba.extend_from_slice(&[bg[0], bg[1], bg[2], 255]);
    }

    if let Some(font) = text::label_font() {
        let layout = text::fit_box(
            font,
            title,
            (lw - 2 * PAD) as f32,
            s.max_h,
            MAX_LINES,
            s.max_px,
            MIN_PX,
        );
        text::draw_centred(&mut rgba, lw, lh, &layout, ink(bg));
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

use slot_store::gb::Class;
use slot_store::{Cart, Outline, Platform};

use crate::art;
use crate::shell::{shell_for, Finish, Shell};
use crate::silhouette::{
    cart_depth, cart_mask, detail_mask, gb_cart_depth, gb_cart_mask, gb_detail_mask, Detail,
    GbShell,
};
use crate::text;

pub const CART_W: u32 = 360;
pub const CART_H: u32 = (CART_W * SEATED_H + SEATED_W / 2) / SEATED_W;

pub const GB_CART_W: u32 = 312;

pub const DISC_W: u32 = 312;
pub const DISC_H: u32 = 312;

pub const SNES_W: u32 = 360;
pub const SNES_H: u32 = 246;
pub const SNES_LABEL_X: u32 = 32;
pub const SNES_LABEL_Y: u32 = 12;
pub const SNES_LABEL_W: u32 = 296;
pub const SNES_LABEL_H: u32 = 100;
const SNES_SEATED_H: u32 = 164;

pub const GB_CART_H: u32 = (GB_CART_W * GB_SEATED_H + SEATED_W / 2) / SEATED_W;

pub const fn label_panel(w: u32, h: u32) -> (u32, u32, u32, u32) {
    (
        (w * 135 + 500) / 1000,
        (h * 231 + 500) / 1000,
        (w * 875 + 500) / 1000,
        (h * 882 + 500) / 1000,
    )
}

pub const LABEL_X: u32 = label_panel(CART_W, CART_H).0;
pub const LABEL_Y: u32 = label_panel(CART_W, CART_H).1;
pub const LABEL_W: u32 = label_panel(CART_W, CART_H).2 - LABEL_X;
pub const LABEL_H: u32 = label_panel(CART_W, CART_H).3 - LABEL_Y;

pub const fn gb_label_panel(w: u32, h: u32) -> (u32, u32, u32, u32) {
    (
        (w * 147 + 500) / 1000,
        (h * 291 + 500) / 1000,
        (w * 854 + 500) / 1000,
        (h * 868 + 500) / 1000,
    )
}

pub const GB_LABEL_X: u32 = gb_label_panel(GB_CART_W, GB_CART_H).0;
pub const GB_LABEL_Y: u32 = gb_label_panel(GB_CART_W, GB_CART_H).1;
pub const GB_LABEL_W: u32 = gb_label_panel(GB_CART_W, GB_CART_H).2 - GB_LABEL_X;
pub const GB_LABEL_H: u32 = gb_label_panel(GB_CART_W, GB_CART_H).3 - GB_LABEL_Y;

const PAD: u32 = 10;
const MAX_LINES: usize = 3;
const MAX_PX: f32 = LABEL_H as f32 / (MAX_LINES as f32 * 1.36);
const GB_MAX_PX: f32 = (GB_LABEL_H - 2 * PAD) as f32 / MAX_LINES as f32;
const MIN_PX: f32 = 10.0;

const RIM: u32 = (4 * CART_W + SEATED_W / 2) / SEATED_W;
const GB_RIM: u32 = (7 * GB_CART_W + SEATED_W / 2) / SEATED_W;

pub struct CartFace {
    pub rgba: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

struct Spec {
    w: u32,
    h: u32,
    label: (u32, u32, u32, u32),
    mask: &'static [u8],
    depth: &'static [u8],
    detail: &'static Detail,
    max_px: f32,
    max_h: f32,
    rim: u32,
    board: Board,
}

struct Board {
    x: (f32, f32),
    top: f32,
    pins: (f32, f32),
    contacts_from: f32,
    traces_from: f32,
}

enum Shape {
    Gba,
    Gb(GbShell),
}

pub fn gb_shell_of(cart: &Cart) -> Option<GbShell> {
    match cart.platform {
        Platform::Gba | Platform::Ps1 | Platform::Snes => None,
        Platform::Gb | Platform::Gbc => Some(match cart.shell.map(|c| c.outline) {
            Some(Outline::Notched) => GbShell::Notched,
            Some(Outline::Rounded) => GbShell::Rounded,
            _ => match slot_store::gb::class(&cart.rom) {
                Class::ColourOnly => GbShell::Rounded,
                Class::Original | Class::DualMode => GbShell::Notched,
            },
        }),
    }
}

fn shape_of(cart: &Cart) -> Shape {
    match gb_shell_of(cart) {
        None => Shape::Gba,
        Some(shell) => Shape::Gb(shell),
    }
}

fn spec(shape: Shape) -> Spec {
    match shape {
        Shape::Gba => Spec {
            w: CART_W,
            h: CART_H,
            label: (LABEL_X, LABEL_Y, LABEL_W, LABEL_H),
            mask: cart_mask(),
            depth: cart_depth(),
            detail: detail_mask(),
            max_px: MAX_PX,
            max_h: f32::INFINITY,
            rim: RIM,
            board: Board {
                x: (0.135, 0.875),
                top: LABEL_Y as f32 / CART_H as f32,
                pins: (0.17, 0.84),
                contacts_from: 0.85,
                traces_from: 0.76,
            },
        },
        Shape::Gb(shell) => Spec {
            w: GB_CART_W,
            h: GB_CART_H,
            label: (GB_LABEL_X, GB_LABEL_Y, GB_LABEL_W, GB_LABEL_H),
            mask: gb_cart_mask(shell),
            depth: gb_cart_depth(shell),
            detail: gb_detail_mask(shell),
            max_px: GB_MAX_PX,
            max_h: (GB_LABEL_H - 2 * PAD) as f32,
            rim: GB_RIM,
            board: Board {
                x: (0.075, 0.925),
                top: 0.07,
                pins: (0.1, 0.9),
                contacts_from: 0.905,
                traces_from: 0.84,
            },
        },
    }
}

pub const SEATED_W: u32 = 240;
const SEATED_H: u32 = 141;
const GB_SEATED_H: u32 = 259;

pub fn seated_box(platform: Platform) -> (u32, u32) {
    match platform {
        Platform::Gba => (SEATED_W, SEATED_H),
        Platform::Gb | Platform::Gbc => (SEATED_W, GB_SEATED_H),
        Platform::Ps1 => (SEATED_W, SEATED_W),
        Platform::Snes => (SEATED_W, SNES_SEATED_H),
    }
}

pub fn cart_box(platform: Platform) -> (u32, u32) {
    let shape = match platform {
        Platform::Gba => Shape::Gba,
        Platform::Gb | Platform::Gbc => Shape::Gb(GbShell::Notched),
        Platform::Ps1 => return (DISC_W, DISC_H),
        Platform::Snes => return (SNES_W, SNES_H),
    };
    let s = spec(shape);
    (s.w, s.h)
}

pub fn cart_shadow() -> CartFace {
    shadow(CART_W, CART_H, cart_mask())
}

pub fn gb_cart_shadow(shell: GbShell) -> CartFace {
    shadow(GB_CART_W, GB_CART_H, gb_cart_mask(shell))
}

fn shadow(w: u32, h: u32, mask: &[u8]) -> CartFace {
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for cover in mask {
        rgba.extend_from_slice(&[0, 0, 0, *cover]);
    }
    CartFace { rgba, w, h }
}

pub fn cart_face(cart: &Cart) -> CartFace {
    let (lw, lh) = label_size(cart);
    let art = cart.label.as_deref().and_then(|p| art::cover(p, lw, lh));
    cart_face_with(cart, art)
}

pub fn label_size(cart: &Cart) -> (u32, u32) {
    if cart.platform == Platform::Ps1 {
        return (DISC_W, DISC_H);
    }
    if cart.platform == Platform::Snes {
        return (SNES_LABEL_W, SNES_LABEL_H);
    }
    let (_, _, lw, lh) = spec(shape_of(cart)).label;
    (lw, lh)
}

pub fn cart_face_with(cart: &Cart, art: Option<Vec<u8>>) -> CartFace {
    if cart.platform == Platform::Ps1 {
        return disc_face(cart, art);
    }
    if cart.platform == Platform::Snes {
        return snes_face(cart, art);
    }
    let s = spec(shape_of(cart));
    let shell = shell_for(cart);
    let mut face = shell_face(&s, &shell);
    let label = match art {
        Some(rgba) => rgba,
        None => generated_label(&s, &label_text(cart)),
    };
    mould_detail(&s, &mut face);
    recess_label(&s, &mut face, &shell);
    paste_label(&s, &mut face, &label);
    clip_to_silhouette(&s, &mut face);
    face
}

fn clip_to_silhouette(s: &Spec, face: &mut CartFace) {
    for (px, cover) in face.rgba.chunks_exact_mut(4).zip(s.mask) {
        px[3] = ((px[3] as u32 * *cover as u32 + 127) / 255) as u8;
    }
}

pub fn label_colour(title: &str) -> [u8; 3] {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in title.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hsv_to_rgb((h % 360) as f32, 0.52, 0.74)
}

pub fn label_text(cart: &Cart) -> String {
    clean_label(&cart.stem)
}

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
    let edge = rim_colour(through(shell.colour, shell.colour));
    for (i, depth) in s.depth.iter().enumerate() {
        let (x, y) = (i as u32 % s.w, i as u32 / s.w);
        let c = match shell.finish {
            Finish::Solid => shell.colour,
            Finish::Translucent | Finish::Glitter => {
                let body = through(shell.colour, inside(s, x, y).unwrap_or(shell.colour));
                let c = lerp(edge, body, (*depth as u32).min(s.rim), s.rim);
                if shell.finish == Finish::Glitter && fleck(x, y) {
                    lerp(c, [0xff; 3], 100, 255)
                } else {
                    c
                }
            }
        };
        rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
    }
    CartFace {
        rgba,
        w: s.w,
        h: s.h,
    }
}

const SURFACE: u32 = 150;

const BOARD: [u8; 3] = [0x2c, 0x96, 0x52];
const TRACE: [u8; 3] = [0x5a, 0xb4, 0x74];
const GOLD: [u8; 3] = [0xe6, 0xb4, 0x46];

fn through(plastic: [u8; 3], behind: [u8; 3]) -> [u8; 3] {
    let filter =
        |a: [u8; 3], b: [u8; 3]| [0, 1, 2].map(|c| (a[c] as u32 * b[c] as u32 / 255) as u8);
    lerp(
        filter(plastic, behind),
        filter(plastic, plastic),
        SURFACE,
        255,
    )
}

fn inside(s: &Spec, x: u32, y: u32) -> Option<[u8; 3]> {
    let b = &s.board;
    let (fx, fy) = (x as f32 / s.w as f32, y as f32 / s.h as f32);
    if !(b.x.0..b.x.1).contains(&fx) || fy < b.top {
        return None;
    }
    let (c0, c1) = b.pins;
    let pitch = (c1 - c0) / 32.0;
    let on_contact = (c0..c1).contains(&fx) && ((fx - c0) % pitch) < pitch * 0.6;
    if on_contact && fy >= b.contacts_from {
        return Some(GOLD);
    }
    if on_contact && ((fx - c0) % pitch) < pitch * 0.2 && fy >= b.traces_from {
        Some(TRACE)
    } else {
        Some(BOARD)
    }
}

fn fleck(x: u32, y: u32) -> bool {
    let mut h = x.wrapping_mul(0x9e37_79b9) ^ y.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h.is_multiple_of(100)
}

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

const BEVEL: u32 = 3;

fn mould_detail(s: &Spec, face: &mut CartFace) {
    let mix = |px: &mut [u8], to: [u8; 3], a: u32| {
        for c in 0..3 {
            px[c] = ((to[c] as u32 * a + px[c] as u32 * (255 - a) + 127) / 255) as u8;
        }
    };
    let sides = s.detail.shadow.iter().zip(&s.detail.highlight);
    for (px, (shade, light)) in face.rgba.chunks_exact_mut(4).zip(sides) {
        let under = [px[0], px[1], px[2]];
        if *shade > 0 {
            mix(px, under.map(|c| (c as f32 * 0.62) as u8), *shade as u32);
        }
        if *light > 0 {
            mix(
                px,
                under.map(|c| c + ((255 - c) as f32 * 0.24) as u8),
                *light as u32,
            );
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
    let bevel = (BEVEL * w + SEATED_W / 2) / SEATED_W;
    let (x0, y0) = (lx - bevel, ly - bevel);
    let (x1, y1) = (lx + lw + bevel, ly + lh + bevel);
    let clear = shell.finish != Finish::Solid;
    let mut put = |x: u32, y: u32, upper: bool| {
        if x >= w || y >= h {
            return;
        }
        let d = ((y * w + x) * 4) as usize;
        let c = if clear {
            let under = [face.rgba[d], face.rgba[d + 1], face.rgba[d + 2]];
            shade(under, if upper { 0.7 } else { 1.18 })
        } else if upper {
            dark
        } else {
            lit
        };
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
            let from_top = y.saturating_sub(y0);
            let from_left = x.saturating_sub(x0);
            let from_bottom = y1.saturating_sub(y + 1);
            let from_right = x1.saturating_sub(x + 1);
            let upper = from_top.min(from_left);
            let lower = from_bottom.min(from_right);
            put(x, y, upper <= lower);
        }
    }
}

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

const DISC_RIM: f32 = 3.0;
const DISC_RIM_INK: [u8; 3] = [0x30, 0x32, 0x38];
const DISC_RING_NEAR: [u8; 3] = [0xea, 0xed, 0xf1];
const DISC_RING_FAR: [u8; 3] = [0xb2, 0xb7, 0xc0];

/// A PlayStation disc: the cover art printed on a round face with a silver hub and a hole in the
/// middle. Drawn per pixel, so it needs no silhouette asset and has no cartridge shell.
fn disc_face(cart: &Cart, art: Option<Vec<u8>>) -> CartFace {
    let (w, h) = (DISC_W, DISC_H);
    let label = match art {
        Some(rgba) if rgba.len() == (w * h * 4) as usize => rgba,
        _ => generated_disc_label(&label_text(cart), w, h),
    };
    let backing = label_colour(&label_text(cart));
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let outer = w.min(h) as f32 / 2.0 - 2.0;
    let hole = outer * 0.125;
    let print_from = outer * 0.40;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            let d = (dx * dx + dy * dy).sqrt();
            let cover = (outer - d + 0.5).clamp(0.0, 1.0) * (d - hole + 0.5).clamp(0.0, 1.0);
            if cover <= 0.0 {
                continue;
            }
            let i = ((y * w + x) * 4) as usize;
            let mut c = if d >= print_from {
                let a = label[i + 3] as u32;
                let mut c = [0u8; 3];
                for k in 0..3 {
                    c[k] = ((label[i + k] as u32 * a + backing[k] as u32 * (255 - a) + 127) / 255)
                        as u8;
                }
                c
            } else {
                let t = ((d - hole) / (print_from - hole)).clamp(0.0, 1.0);
                lerp(DISC_RING_NEAR, DISC_RING_FAR, (t * 255.0) as u32, 255)
            };
            let sheen = (-((dx - dy) / outer * 2.6).powi(2)).exp() * 0.14;
            c = c.map(|v| v + ((255 - v) as f32 * sheen) as u8);
            if (d - print_from).abs() < 0.9 {
                c = c.map(|v| (v as f32 * 0.72) as u8);
            }
            if d > outer - DISC_RIM {
                c = lerp(c, DISC_RIM_INK, 100, 255);
            }
            rgba[i] = c[0];
            rgba[i + 1] = c[1];
            rgba[i + 2] = c[2];
            rgba[i + 3] = (cover * 255.0).round() as u8;
        }
    }
    CartFace { rgba, w, h }
}

fn generated_disc_label(title: &str, w: u32, h: u32) -> Vec<u8> {
    let bg = label_colour(title);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..w * h {
        rgba.extend_from_slice(&[bg[0], bg[1], bg[2], 255]);
    }
    if let Some(font) = text::label_font() {
        let layout = text::fit_box(
            font,
            title,
            w as f32 * 0.56,
            h as f32 * 0.30,
            MAX_LINES,
            34.0,
            MIN_PX,
        );
        text::draw_centred(&mut rgba, w, h, &layout, ink(bg));
    }
    rgba
}

const SNES_BODY: [u8; 3] = [0xac, 0xb8, 0xbb];
const SNES_BRASS: [u8; 3] = [0xa6, 0x8e, 0x46];
const SNES_GROOVES: [f32; 2] = [58.0, 302.0];

/// A rounded rectangle with its own radius on the top and the bottom corners.
fn rounded(x: f32, y: f32, rect: (f32, f32, f32, f32), radii: (f32, f32)) -> bool {
    let (x0, y0, x1, y1) = rect;
    if x < x0 || x >= x1 || y < y0 || y >= y1 {
        return false;
    }
    let (r, cy) = if y < y0 + radii.0 {
        (radii.0, y0 + radii.0)
    } else if y > y1 - radii.1 {
        (radii.1, y1 - radii.1)
    } else {
        return true;
    };
    let dx = if x < x0 + r {
        x0 + r - x
    } else if x > x1 - r {
        x - (x1 - r)
    } else {
        return true;
    };
    dx * dx + (y - cy) * (y - cy) <= r * r
}

/// The cartridge of the European and Japanese Super Nintendo seen from the front: one rounded
/// rectangle with a small notch at the top where each side column meets the middle.
fn snes_inside(x: f32, y: f32) -> bool {
    let notch = y < 3.0 && SNES_GROOVES.iter().any(|g| (x - g).abs() < 1.2);
    !notch && rounded(x, y, (0.0, 0.0, SNES_W as f32, SNES_H as f32), (9.0, 12.0))
}

fn snes_slot(x: f32, y: f32) -> bool {
    rounded(x, y, (69.0, 147.0, 291.0, 171.0), (12.0, 12.0))
}

fn snes_sticker(x: f32, y: f32) -> bool {
    let (x0, y0) = (SNES_LABEL_X as f32, SNES_LABEL_Y as f32);
    let rect = (x0, y0, x0 + SNES_LABEL_W as f32, y0 + SNES_LABEL_H as f32);
    rounded(x, y, rect, (3.0, 3.0))
}

fn shade(c: [u8; 3], k: f32) -> [u8; 3] {
    c.map(|v| (v as f32 * k).clamp(0.0, 255.0) as u8)
}

/// The cartridge of the European and Japanese Super Nintendo, drawn per pixel after a photo of the
/// real thing: light grey shell, two side columns with five grip grooves each and a screw at the
/// foot, a middle block with a long recess in three fields, and the sticker with the cover art.
fn snes_face(cart: &Cart, art: Option<Vec<u8>>) -> CartFace {
    let (w, h) = (SNES_W, SNES_H);
    let (lw, lh) = (SNES_LABEL_W, SNES_LABEL_H);
    let title = label_text(cart);
    let label = match art {
        Some(rgba) if rgba.len() == (lw * lh * 4) as usize => rgba,
        _ => generated_plain_label(&title, lw, lh),
    };
    let backing = label_colour(&title);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let (wf, hf) = (w as f32, h as f32);
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32, y as f32);
            let mut hit = 0;
            for (ox, oy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                if snes_inside(fx + ox, fy + oy) {
                    hit += 1;
                }
            }
            if hit == 0 {
                continue;
            }
            let (cx, cy) = (fx + 0.5, fy + 0.5);
            let mut c = shade(SNES_BODY, 1.05 - 0.10 * cy / hf);
            if cx >= SNES_GROOVES[0] && cx < SNES_GROOVES[1] {
                c = shade(c, 1.03);
            }

            for g in SNES_GROOVES {
                if (cx - g).abs() < 1.0 {
                    c = shade(c, 0.80);
                } else if (cx - g - 1.5).abs() < 0.8 || (cx - g + 1.5).abs() < 0.8 {
                    c = shade(c, 1.05);
                }
            }

            for k in 0..5 {
                let yc = 20.0 + 10.8 * k as f32;
                let in_left = (5.0..16.0).contains(&cx);
                let in_right = (344.0..355.0).contains(&cx);
                if in_left || in_right {
                    if (cy - yc).abs() < 1.4 {
                        c = shade(c, 0.60);
                    } else if (cy - yc + 2.4).abs() < 1.0 {
                        c = shade(c, 1.08);
                    }
                }
            }

            if snes_slot(cx, cy) {
                c = shade(SNES_BODY, 0.86);
                if !snes_slot(cx, cy - 3.0) {
                    c = shade(SNES_BODY, 0.42);
                } else if !snes_slot(cx, cy + 1.5) || !snes_slot(cx - 1.5, cy) {
                    c = shade(SNES_BODY, 1.10);
                }
                if (cx - 140.0).abs() < 1.0 || (cx - 217.0).abs() < 1.0 {
                    c = shade(SNES_BODY, 0.70);
                }
            }

            for sx in [20.0, wf - 20.0] {
                let d = ((cx - sx).powi(2) + (cy - 224.0).powi(2)).sqrt();
                if d < 7.5 {
                    c = shade(SNES_BODY, 0.55);
                }
                if d < 5.5 {
                    let spot = ((cx - sx + 2.0).powi(2) + (cy - 224.0 + 2.0).powi(2)).sqrt();
                    c = shade(SNES_BRASS, if spot < 2.5 { 1.35 } else { 1.0 - 0.04 * d });
                }
            }

            if snes_sticker(cx, cy) {
                let (lx, ly) = (x.saturating_sub(SNES_LABEL_X), y.saturating_sub(SNES_LABEL_Y));
                if x >= SNES_LABEL_X && y >= SNES_LABEL_Y && lx < lw && ly < lh {
                    let i = ((ly * lw + lx) * 4) as usize;
                    let a = label[i + 3] as u32;
                    for k in 0..3 {
                        c[k] = ((label[i + k] as u32 * a + backing[k] as u32 * (255 - a) + 127)
                            / 255) as u8;
                    }
                }
                if !snes_sticker(cx - 1.2, cy)
                    || !snes_sticker(cx + 1.2, cy)
                    || !snes_sticker(cx, cy - 1.2)
                    || !snes_sticker(cx, cy + 1.2)
                {
                    c = shade(c, 0.86);
                }
            }

            if !snes_inside(cx, cy - 2.5) {
                c = shade(c, 1.08);
            } else if !snes_inside(cx - 2.5, cy)
                || !snes_inside(cx + 2.5, cy)
                || !snes_inside(cx, cy + 2.5)
            {
                c = shade(c, 0.80);
            }

            let i = ((y * w + x) * 4) as usize;
            rgba[i] = c[0];
            rgba[i + 1] = c[1];
            rgba[i + 2] = c[2];
            rgba[i + 3] = (hit * 255 / 4) as u8;
        }
    }
    CartFace { rgba, w, h }
}

fn generated_plain_label(title: &str, w: u32, h: u32) -> Vec<u8> {
    let bg = label_colour(title);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..w * h {
        rgba.extend_from_slice(&[bg[0], bg[1], bg[2], 255]);
    }
    if let Some(font) = text::label_font() {
        let layout = text::fit_box(
            font,
            title,
            w as f32 * 0.88,
            h as f32 * 0.70,
            MAX_LINES,
            30.0,
            MIN_PX,
        );
        text::draw_centred(&mut rgba, w, h, &layout, ink(bg));
    }
    rgba
}

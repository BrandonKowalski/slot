//! Rasterise the original SVGs. Slot loads the pictures in the repo's `assets/` folder, not this output.

use slot_ui::{neutral_shell, Mould};

use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

const SHELL: [u8; 3] = [0x35, 0x35, 0x3a];

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let assets = root.join("assets");
    let out = root.join("../../templates");
    std::fs::create_dir_all(&out).unwrap();

    let save = |name: &str, rgba: Vec<u8>, w: u32, h: u32| {
        write_png(&out.join(name), &rgba, w, h);
        println!("{name} {w}x{h}");
    };

    save(
        "gba_cart.png",
        render(&assets.join("cart.svg"), 360, 212),
        360,
        212,
    );
    let gb = render(&assets.join("gb_cart.svg"), 312, 337);
    let mut gbc = render(&assets.join("gbc_cart.svg"), 312, 337);
    // The two moulds share one body below the shoulder. Antialiasing of that
    // shared edge has to agree, or the shelf treats them as different widths.
    for y in 30..337 {
        for x in 0..312 {
            let i = ((y * 312 + x) * 4 + 3) as usize;
            gbc[i] = gb[i];
        }
    }
    save("gb_cart.png", gb, 312, 337);
    save("gbc_cart.png", gbc, 312, 337);

    let board_src = board_svg(&assets.join("board.svg"));
    save("board.png", render_text(&board_src, 360, 212), 360, 212);
    save(
        "socket.png",
        render(&assets.join("socket.svg"), 64, 46),
        64,
        46,
    );
    save("chip.png", render(&assets.join("chip.svg"), 59, 41), 59, 41);
    save(
        "link_port.png",
        render(&assets.join("link_port.svg"), 720, 58),
        720,
        58,
    );
    // Size template only. The live host plug is assets/link_plug_host.png.
    save(
        "link_plug_host.png",
        render(&assets.join("link_plug_host.svg"), 92, 244),
        92,
        244,
    );
    save(
        "link_plug_join.png",
        render(&assets.join("link_plug_join.svg"), 92, 244),
        92,
        244,
    );
    // Size template only. The live adapter is assets/link_adapter.png.
    save(
        "link_adapter.png",
        render(&assets.join("link_adapter.svg"), 268, 182),
        268,
        182,
    );
    // Size template only. The live sticker is assets/sticker.png.
    save(
        "sticker.png",
        render(&assets.join("sticker.svg"), 660, 228),
        660,
        228,
    );

    let pcb = with_viewbox(&board_src, "16 26 208 98");
    // Size template only. The live interior is assets/gba_interior.png.
    // Cart, interior and board share 360×212 so the carousel can stack them 1:1.
    save(
        "gba_interior.png",
        render_text(&board_src, 360, 212),
        360,
        212,
    );
    // assets/gb_board.png is the pak PCB, edited by hand. Do not overwrite it.
    let gb_mask = render(&assets.join("gb_cart.svg"), 312, 337);
    let gbc_mask = render(&assets.join("gbc_cart.svg"), 312, 337);
    save(
        "gb_interior.png",
        place_board(
            &pcb,
            312,
            337,
            Rect::new(24, 24, 289, 337),
            &union_alpha(&gb_mask, &gbc_mask),
        ),
        312,
        337,
    );

    let shells = out.join("shells");
    std::fs::create_dir_all(&shells).unwrap();
    for (prefix, mould, w, h) in [
        ("gba", Mould::Gba, 360, 212),
        ("gb", Mould::Notched, 312, 337),
        ("gbc", Mould::Rounded, 312, 337),
    ] {
        for (kind, clear) in [("neutral", false), ("clear", true)] {
            let name = format!("{prefix}_{kind}.png");
            write_png(&shells.join(&name), &neutral_shell(mould, clear), w, h);
            println!("shells/{name} {w}x{h}");
        }
    }

    let (back, front) = slot_bands();
    save("slot_back.png", back, 720, 58);
    save("slot_front.png", front, 720, 58);
}

fn with_viewbox(svg: &str, view_box: &str) -> String {
    let key = "viewBox=\"";
    let start = svg.find(key).expect("viewBox") + key.len();
    let end = start + svg[start..].find('"').expect("viewBox end");
    format!("{}{view_box}{}", &svg[..start], &svg[end..])
}

fn board_svg(path: &std::path::Path) -> String {
    let plastic = hex(SHELL);
    let floor = hex(shade(SHELL, 0.62));
    let deep = hex(shade(SHELL, 0.35));
    std::fs::read_to_string(path)
        .unwrap()
        .replace("#ff00ff", &plastic)
        .replace("#FF00FF", &plastic)
        .replace("#800080", &floor)
        .replace("#400040", &deep)
}

fn shade(c: [u8; 3], f: f32) -> [u8; 3] {
    c.map(|v| (v as f32 * f).round().clamp(0.0, 255.0) as u8)
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn render(path: &std::path::Path, w: u32, h: u32) -> Vec<u8> {
    render_text(&std::fs::read_to_string(path).unwrap(), w, h)
}

fn render_text(svg: &str, w: u32, h: u32) -> Vec<u8> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).expect("svg");
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).expect("pixmap");
    let size = tree.size();
    let scale =
        resvg::tiny_skia::Transform::from_scale(w as f32 / size.width(), h as f32 / size.height());
    resvg::render(&tree, scale, &mut pixmap.as_mut());
    let mut rgba = pixmap.data().to_vec();
    for px in rgba.chunks_exact_mut(4) {
        let a = px[3];
        if a == 0 || a == 255 {
            continue;
        }
        for c in &mut px[..3] {
            *c = ((*c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
        }
    }
    rgba
}

struct Rect {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
}

impl Rect {
    fn new(x0: u32, y0: u32, x1: u32, y1: u32) -> Self {
        Self { x0, y0, x1, y1 }
    }
}

fn place_board(svg: &str, w: u32, h: u32, at: Rect, mask: &[u8]) -> Vec<u8> {
    let (x0, y0, x1, y1) = (at.x0, at.y0, at.x1, at.y1);
    let (dw, dh) = (x1 - x0, y1 - y0);
    let board = render_text(svg, dw, dh);
    let mut out = vec![0u8; (w * h * 4) as usize];
    for y in 0..dh {
        for x in 0..dw {
            let s = ((y * dw + x) * 4) as usize;
            let d = (((y + y0) * w + x + x0) * 4) as usize;
            let a = (board[s + 3] as u32 * mask[d + 3] as u32 / 255) as u8;
            out[d..d + 3].copy_from_slice(&board[s..s + 3]);
            out[d + 3] = a;
        }
    }
    out
}

fn union_alpha(a: &[u8], b: &[u8]) -> Vec<u8> {
    a.iter()
        .zip(b)
        .enumerate()
        .map(|(i, (p, q))| if i % 4 == 3 { *p.max(q) } else { *p })
        .collect()
}

fn write_png(path: &std::path::Path, rgba: &[u8], w: u32, h: u32) {
    let file = File::create(path).unwrap();
    let mut enc = png::Encoder::new(BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(rgba).unwrap();
}

fn slot_bands() -> (Vec<u8>, Vec<u8>) {
    const W: u32 = 720;
    const H: u32 = 58;
    const BAND_Y: f32 = 422.0;
    const MOUTH_W: f32 = 254.0;
    const MOUTH_X: f32 = (720.0 - MOUTH_W) / 2.0;
    const LIP_H: f32 = 2.0;
    const BAY_W: f32 = MOUTH_W + 18.0;
    const BAY_X: f32 = (720.0 - BAY_W) / 2.0;
    const BAY_Y: f32 = BAND_Y + LIP_H;
    const SLIT_H: f32 = 9.0;
    const SLIT_Y: f32 = BAY_Y + 5.0;
    const RECESS_H: f32 = 42.0;
    const SCOOP_W: f32 = MOUTH_W * 0.88;
    const SCOOP_Y: f32 = SLIT_Y + SLIT_H;
    const SCOOP_D: f32 = RECESS_H - (SCOOP_Y - BAY_Y);
    const RIM_W: f32 = 2.0;
    const CX: f32 = 360.0;
    const SCOOP_FLAT: f32 = 4.0;
    let housing = [0x24, 0x24, 0x29, 255];
    let recess = [0x1a, 0x1a, 0x1d, 255];
    let opening = [0x05, 0x05, 0x08, 255];
    let edge = [0x4d, 0x4d, 0x57, 255];

    let mut back = vec![0u8; (W * H * 4) as usize];
    let mut front = vec![0u8; (W * H * 4) as usize];
    let paint = |buf: &mut [u8], x: f32, y: f32, w: f32, h: f32, c: [u8; 4]| {
        let x0 = x.round() as i32;
        let y0 = (y - BAND_Y).round() as i32;
        let x1 = (x + w).round() as i32;
        let y1 = (y - BAND_Y + h).round() as i32;
        for yy in y0.max(0)..y1.min(H as i32) {
            for xx in x0.max(0)..x1.min(W as i32) {
                let i = ((yy as u32 * W + xx as u32) * 4) as usize;
                buf[i..i + 4].copy_from_slice(&c);
            }
        }
    };
    let scoop = |buf: &mut [u8], c: [u8; 4], rim: bool| {
        let hw = SCOOP_W / 2.0;
        let depth = |x: f32| SCOOP_D * (1.0 - (x.abs() / hw).powf(SCOOP_FLAT)).max(0.0);
        let mut x = -hw;
        while x < hw {
            let d = depth(x + 0.5).round();
            let start = x;
            while x < hw && depth(x + 0.5).round() == d {
                x += 1.0;
            }
            if rim {
                paint(buf, CX + start, SCOOP_Y + d, x - start, RIM_W, c);
            } else {
                paint(buf, CX + start, SCOOP_Y, x - start, d, c);
            }
        }
    };

    paint(&mut back, BAY_X, BAND_Y, BAY_W, LIP_H, housing);
    paint(&mut back, MOUTH_X, BAND_Y, MOUTH_W, LIP_H, edge);
    paint(&mut back, BAY_X, BAY_Y, BAY_W, RECESS_H, recess);
    paint(&mut back, MOUTH_X, SLIT_Y, MOUTH_W, SLIT_H, opening);
    scoop(&mut back, opening, false);

    let right = BAY_X + BAY_W;
    let floor = SCOOP_Y + SCOOP_D + RIM_W;
    paint(&mut front, 0.0, BAND_Y, BAY_X, 58.0, housing);
    paint(&mut front, right, BAND_Y, 720.0 - right, 58.0, housing);
    let near = CX - SCOOP_W / 2.0;
    paint(
        &mut front,
        BAY_X,
        SCOOP_Y,
        near - BAY_X,
        floor - SCOOP_Y,
        housing,
    );
    let far = CX + SCOOP_W / 2.0;
    paint(
        &mut front,
        far,
        SCOOP_Y,
        right - far,
        floor - SCOOP_Y,
        housing,
    );
    let hw = SCOOP_W / 2.0;
    let depth = |x: f32| SCOOP_D * (1.0 - (x.abs() / hw).powf(SCOOP_FLAT)).max(0.0);
    let mut x = -hw;
    while x < hw {
        let d = depth(x + 0.5).round();
        let start = x;
        while x < hw && depth(x + 0.5).round() == d {
            x += 1.0;
        }
        let top = SCOOP_Y + d + RIM_W;
        paint(&mut front, CX + start, top, x - start, floor - top, housing);
        paint(&mut front, CX + start, SCOOP_Y + d, x - start, RIM_W, edge);
    }
    paint(&mut front, 0.0, floor, 720.0, 480.0 - floor, housing);
    (back, front)
}

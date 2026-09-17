//! The Game Boy paks as they stand on the row, rasterised to a PNG so the drawing can be looked
//! at rather than asserted about. Four carts on one screen-high ground: a GBA cart for scale and
//! all three Game Boy classes — 0x00 grey in the notched shell, 0x80 black in the same shell,
//! 0xc0 clear in the rounded one — each with its foot on the row floor, under a band the height
//! of the HUD plate. Does nothing unless `SCRATCH_PNG` names an output file:
//!
//! `SCRATCH_PNG=/tmp/gb-carts.png cargo test -p slot-ui --test render_gb_cart -- --nocapture`

use slot_store::scan;
use slot_ui::{cart_face, CartFace, CART_W, FOOT_Y, OUT_H, PLATE_H};
use tempfile::TempDir;

/// Four carts side by side, which is wider than the screen. The row is a contact sheet rather
/// than a screenshot — what is being judged is how the carts relate to each other.
const COLS: u32 = 4;
const SHEET_W: u32 = COLS * CART_W;

/// The ground the shelf draws over, near enough the wallpaper's own darkness that a shell reads
/// against it the way it will on the device.
const GROUND: [u8; 3] = [0x14, 0x15, 0x1a];
const PLATE: [u8; 3] = [0x25, 0x27, 0x2e];
const FLOOR: [u8; 3] = [0x3a, 0x3d, 0x46];

fn write_rom(d: &TempDir, dir: &str, name: &str, cgb: u8) {
    let mut rom = vec![0u8; 0x150];
    rom[0x143] = cgb;
    let games = d.path().join("Games").join(dir);
    std::fs::create_dir_all(&games).expect("create games dir");
    std::fs::write(games.join(name), rom).expect("write rom");
}

fn write_gba_rom(d: &TempDir, name: &str, code: &str) {
    let mut rom = vec![0u8; 0x100];
    rom[0xac..0xac + code.len()].copy_from_slice(code.as_bytes());
    let games = d.path().join("Games/GBA");
    std::fs::create_dir_all(&games).expect("create games dir");
    std::fs::write(games.join(name), rom).expect("write rom");
}

/// Source over, on an opaque ground, which is what the compositor does with a cart face.
fn paste(frame: &mut [u8], face: &CartFace, left: u32) {
    for y in 0..face.h {
        for x in 0..face.w {
            let s = ((y * face.w + x) * 4) as usize;
            let a = face.rgba[s + 3] as u32;
            if a == 0 {
                continue;
            }
            let top = (FOOT_Y - face.h as f32) as u32;
            let d = (((y + top) * SHEET_W + x + left) * 3) as usize;
            for c in 0..3 {
                frame[d + c] =
                    ((face.rgba[s + c] as u32 * a + frame[d + c] as u32 * (255 - a) + 127) / 255)
                        as u8;
            }
        }
    }
}

#[test]
fn render_gb_cart() {
    let Ok(out) = std::env::var("SCRATCH_PNG") else {
        return;
    };
    let d = tempfile::tempdir().expect("tempdir");
    write_gba_rom(&d, "Metroid Fusion.gba", "AMTE");
    write_rom(&d, "GB", "Tetris.gb", 0x00);
    write_rom(&d, "GB", "Wario Land 3.gb", 0x80);
    write_rom(&d, "GBC", "Tetris Chromatic.gbc", 0xc0);
    let carts = scan(d.path()).expect("scan");

    let mut frame = Vec::with_capacity((SHEET_W * OUT_H * 3) as usize);
    for y in 0..OUT_H {
        for _ in 0..SHEET_W {
            let c = if (y as f32) < PLATE_H { PLATE } else { GROUND };
            frame.extend_from_slice(&c);
        }
    }
    // The line the carts stand on, drawn so a cart that floats off it is visible rather than
    // inferred.
    for x in 0..SHEET_W {
        let d = ((FOOT_Y as u32 * SHEET_W + x) * 3) as usize;
        frame[d..d + 3].copy_from_slice(&FLOOR);
    }

    for (i, cart) in carts.iter().enumerate() {
        paste(&mut frame, &cart_face(cart), i as u32 * CART_W);
        println!("{} at column {}", cart.stem, i as u32 * CART_W);
    }

    let f = std::fs::File::create(&out).expect("create png");
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), SHEET_W, OUT_H);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    e.write_header()
        .expect("png header")
        .write_image_data(&frame)
        .expect("png data");
    println!("wrote {out}");
}

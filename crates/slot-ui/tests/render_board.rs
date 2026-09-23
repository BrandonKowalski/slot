//! The open cart's face, rasterised to a PNG so the art can be set beside the mockup and the
//! photograph it follows. Does nothing unless `SCRATCH_PNG` names an output file:
//!
//! `SCRATCH_PNG=/tmp/board.png cargo test -p slot-ui --test render_board -- --nocapture`

use slot_store::Cart;
use slot_ui::board_face;

#[test]
fn render_board() {
    let Ok(out) = std::env::var("SCRATCH_PNG") else {
        return;
    };
    let face = board_face(&Cart {
        stem: "Pokemon - Emerald Version (USA, Europe)".into(),
        rom: "Games/GBA/Emerald.gba".into(),
        label: None,
        code: "BPEE".into(),
        title: "POKEMON EMER".into(),
    });
    let f = std::fs::File::create(&out).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), face.w, face.h);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header()
        .unwrap()
        .write_image_data(&face.rgba)
        .unwrap();
    println!("wrote {out}");
}

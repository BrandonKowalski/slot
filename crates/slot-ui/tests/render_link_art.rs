//! `SCRATCH_PNG=/tmp/link-art.png cargo test -p slot-ui --test render_link_art -- --nocapture`

use slot_ui::{link_art, CartFace};

#[test]
fn render_link_art() {
    let Ok(out) = std::env::var("SCRATCH_PNG") else {
        return;
    };
    let a = link_art();
    let faces: [&CartFace; 4] = [&a.port, &a.plug_host, &a.plug_join, &a.adapter];
    let (w, h) = (760u32, faces.iter().map(|f| f.h + 10).sum::<u32>());
    let mut sheet = vec![0x05u8; (w * h * 4) as usize];
    let mut top = 0;
    for f in faces {
        for y in 0..f.h {
            for x in 0..f.w.min(w) {
                let s = ((y * f.w + x) * 4) as usize;
                let d = (((top + y) * w + x) * 4) as usize;
                let alpha = f.rgba[s + 3] as u32;
                for k in 0..3 {
                    sheet[d + k] = ((f.rgba[s + k] as u32 * alpha
                        + sheet[d + k] as u32 * (255 - alpha))
                        / 255) as u8;
                }
                sheet[d + 3] = 255;
            }
        }
        top += f.h + 10;
    }
    let file = std::fs::File::create(&out).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&sheet).unwrap();
    println!("wrote {out}");
}

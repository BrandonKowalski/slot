//! The only check that proves the Code 39 table: render the encoder's output as an image and
//! hand it to a real reader. The structural tests in `barcode.rs` catch a pattern with the
//! wrong number of wide elements, but a table that is internally consistent and still wrong
//! encodes happily and scans as nothing — which is exactly what a barcode is for.
//!
//! Needs `zbarimg` (`brew install zbar`). Without it the test says so and stops, rather than
//! passing quietly and claiming to have checked something.

use std::process::Command;

use slot_ui::{code39, CODE39_NARROW, CODE39_WIDE};

/// Four pixels per narrow element puts the wide one at ten, inside the 2:1 to 3:1 the
/// symbology allows and well clear of any resampling.
const SCALE: f32 = 4.0;

fn render(payload: &str, to: &std::path::Path) {
    let run = code39(&format!("*{payload}*")).expect("payload is in the alphabet");
    let narrow = (CODE39_NARROW * SCALE) as usize;
    let wide = (CODE39_WIDE * SCALE) as usize;
    // Code 39 wants a quiet zone of at least ten narrow elements either side. A reader that
    // cannot find one reports nothing at all, which would look like a bad table.
    let quiet = narrow * 12;
    let elements: usize = run.iter().map(|w| if *w { wide } else { narrow }).sum();
    // One narrow space between characters, part of the symbology rather than decoration:
    // without it the last bar of one character runs into the first of the next.
    let gaps = (run.len() / 9) * narrow;
    let (w, h) = (quiet * 2 + elements + gaps, 120usize);

    let mut px = vec![255u8; w * h];
    let mut x = quiet;
    for chunk in run.chunks_exact(9) {
        for (n, is_wide) in chunk.iter().enumerate() {
            let run_w = if *is_wide { wide } else { narrow };
            // Even elements are bars, odd are spaces.
            if n.is_multiple_of(2) {
                for row in 0..h {
                    px[row * w + x..row * w + x + run_w].fill(0);
                }
            }
            x += run_w;
        }
        x += narrow;
    }

    let f = std::fs::File::create(to).expect("create png");
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), w as u32, h as u32);
    e.set_color(png::ColorType::Grayscale);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&px).unwrap();
}

#[test]
fn the_barcode_reads_back_as_what_was_encoded() {
    if Command::new("zbarimg").arg("--version").output().is_err() {
        eprintln!("skipped: zbarimg not installed (brew install zbar)");
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    // The whole alphabet, a hash-shaped payload, and one character: the sentinel is the same
    // pattern in all three, so a wrong one fails every case rather than an awkward edge.
    for payload in ["0123456789ABCDEF", "0473885", "DEADBEEF", "F"] {
        let png = dir.path().join(format!("{payload}.png"));
        render(payload, &png);
        let out = Command::new("zbarimg")
            .args(["--quiet", "--raw"])
            .arg(&png)
            .output()
            .expect("run zbarimg");
        let got = String::from_utf8_lossy(&out.stdout).trim().to_string();
        assert_eq!(got, payload, "the reader disagrees with the encoder");
    }
}

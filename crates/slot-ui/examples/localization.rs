//! Render representative localized UI assets without a window or GPU.
//! cargo run -p slot-ui --example localization -- /tmp/slot-ja.png ja
use slot_ui::{menu_face, quick_label_face, quick_value_face, title_line_face, QuickRow, UndoFace};

fn main() {
    let path = std::env::args().nth(1).expect("output PNG path");
    let language = std::env::args().nth(2).unwrap_or_else(|| "ja".into());
    slot_i18n::set_language(slot_i18n::Language::parse(&language));
    let (w, h) = (640u32, 960u32);
    let mut rgba = [28, 29, 32, 255].repeat((w * h) as usize);
    let mut put = |face: UndoFace, x: u32, y: u32| {
        assert!(x + face.w <= w && y + face.h <= h);
        for row in 0..face.h {
            for col in 0..face.w {
                let src = ((row * face.w + col) * 4) as usize;
                let dst = (((y + row) * w + x + col) * 4) as usize;
                let a = face.rgba[src + 3] as u32;
                for c in 0..3 {
                    rgba[dst + c] = ((face.rgba[src + c] as u32 * a
                        + rgba[dst + c] as u32 * (255 - a))
                        / 255) as u8;
                }
            }
        }
    };
    let rows = [
        (QuickRow::Language, slot_i18n::language().name()),
        (QuickRow::Theme, "Dark"),
        (QuickRow::ShowTitles, "Show"),
        (QuickRow::AutoLoad, "On"),
        (QuickRow::LeftStick, "Off"),
        (QuickRow::LidOff, "5 min"),
    ];
    for (i, (row, value)) in rows.into_iter().enumerate() {
        let label = quick_label_face(row, true);
        let value = quick_value_face(value, true);
        assert!(
            35 + label.w + 12 <= w - 14 - value.w,
            "{row:?} overlaps its value"
        );
        put(label, 35, 30 + i as u32 * 54);
        let x = w - 14 - value.w;
        put(value, x, 30 + i as u32 * 54);
    }
    for (i, face) in [
        title_line_face("ポケットモンスター 金・銀"),
        title_line_face("ゼルダの伝説 ふしぎのぼうし"),
        menu_face("Delete this save state?"),
        menu_face("It cannot be brought back."),
        menu_face("Looking for the other player"),
        menu_face("State Saved"),
        menu_face(&slot_ui::date_time_text(1_782_940_500, false)),
    ]
    .into_iter()
    .enumerate()
    {
        let x = (w - face.w) / 2;
        put(face, x, 390 + i as u32 * 65);
    }
    let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&rgba)
        .unwrap();
}

use slot_ui::{sticker_lines, StickerFields};

fn fields() -> StickerFields<'static> {
    StickerFields {
        battery: Some(87),
        serial: "04738857",
        version: "0.1.0",
    }
}

#[test]
fn the_headline_rows_read_as_a_device_plate() {
    let all = sticker_lines(&fields()).join("\n");
    assert!(all.contains("AGS-102"), "{all}");
    assert!(all.contains("5V"), "{all}");
    assert!(all.contains("1.5A"), "{all}");
    assert!(all.contains("87"), "the gauge reading is missing: {all}");
    assert!(all.contains("0473885"), "the serial went missing: {all}");
}

#[test]
fn the_input_row_carries_the_real_dc_symbol() {
    let all = sticker_lines(&fields()).join("\n");
    assert!(all.contains(slot_ui::DC), "the rating row lost its symbol");
    assert_eq!(slot_ui::DC, '\u{2393}');
}

#[test]
fn a_missing_gauge_is_not_drawn_as_empty() {
    let mut f = fields();
    f.battery = None;
    let all = sticker_lines(&f).join("\n");
    assert!(
        !all.contains("0%"),
        "no gauge was drawn as a flat battery: {all}"
    );
    assert!(
        all.contains("BATTERY"),
        "the row should still be there: {all}"
    );
}

#[test]
fn the_compliance_block_is_the_credits() {
    let all = sticker_lines(&fields()).join("\n").to_uppercase();
    for owed in [
        "MGBA",
        "GPSP",
        "BASEOS BY PVAIBHAV",
        "OPEN SANS",
        "NERD",
        "LCD3X",
        "MARKUS",
    ] {
        assert!(all.contains(owed), "the credits do not mention {owed}");
    }
}

#[test]
fn the_serial_row_matches_the_encoded_hash() {
    let all = sticker_lines(&fields()).join("\n");
    assert!(all.contains("0473885 7"), "{all}");
}

#[test]
fn every_line_is_already_upper_case() {
    for line in sticker_lines(&fields()) {
        assert_eq!(line, line.to_uppercase(), "{line}");
    }
}

#[test]
fn the_wordmarks_top_edge_is_antialiased_not_a_hard_step() {
    use slot_ui::sticker_face;
    const INK: [u8; 3] = [0xff, 0xff, 0xff];
    let face = sticker_face(&fields());
    let get = |x: u32, y: u32| -> [u8; 3] {
        let i = ((y * face.w + x) * 4) as usize;
        [face.rgba[i], face.rgba[i + 1], face.rgba[i + 2]]
    };
    let dark = |c: [u8; 3]| (c[0] as u32 + c[1] as u32 + c[2] as u32) < 120;
    let edges: Vec<[u8; 3]> = (400..413)
        .filter_map(|x| {
            (129..142).find_map(|y| {
                let above = get(x, y - 1);
                let here = get(x, y);
                (dark(above) && !dark(here)).then_some(here)
            })
        })
        .collect();
    assert!(
        edges.iter().any(|&e| e != INK),
        "every sampled column's top edge jumps straight from the ground to the ink: {edges:?}"
    );
}

#[test]
fn the_version_rides_above_the_readme_line() {
    let lines = sticker_lines(&fields());
    let home = lines.iter().position(|l| l == "SEE README.").unwrap();
    assert_eq!(lines[home - 1], "VERSION 0.1.0");
}

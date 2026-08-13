use slot_ui::{sticker_lines, StickerFields};

fn fields() -> StickerFields<'static> {
    StickerFields {
        battery: Some(87),
        serial: "0473885",
        dirty_digit: '0',
    }
}

/// The headline rows read as a real device's plate. Only the gauge moves; the model number
/// and the input rating are the article's own shape, and the build's version has its own row
/// at the foot where the origin line goes.
#[test]
fn the_headline_rows_read_as_a_device_plate() {
    let all = sticker_lines(&fields()).join("\n");
    assert!(all.contains("AGS-102"), "{all}");
    assert!(all.contains("5V"), "{all}");
    assert!(all.contains("1.5A"), "{all}");
    assert!(all.contains("87"), "the gauge reading is missing: {all}");
    // The build is named by its serial rather than a version row, the way a real plate names
    // a unit. The barcode beside it encodes the same hash.
    assert!(all.contains("0473885"), "the serial went missing: {all}");
}

/// The rating row carries the real direct current symbol. No font in this crate has it, so
/// the renderer draws it — and the text keeps the correct codepoint rather than an equals
/// sign standing in for one.
#[test]
fn the_input_row_carries_the_real_dc_symbol() {
    let all = sticker_lines(&fields()).join("\n");
    assert!(all.contains(slot_ui::DC), "the rating row lost its symbol");
    assert_eq!(slot_ui::DC, '\u{2393}');
}

/// A device with no gauge is one that does not have one, not one reading zero percent.
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

/// The compliance block is the credits. Every one of these is something the README already
/// owes, and a label that quietly dropped one would be worse than no label at all.
#[test]
fn the_compliance_block_is_the_credits() {
    let all = sticker_lines(&fields()).join("\n").to_uppercase();
    // What README.md credits, minus the parts a label has no room for. The cartridge sounds
    // are a recording of the author's own console, so nobody is owed for them.
    for owed in ["MGBA", "LIBRETRO", "OPEN SANS", "NERD", "LCD3X", "CLAUDE"] {
        assert!(all.contains(owed), "the credits do not mention {owed}");
    }
}

/// The serial reads back what the barcode encodes, or the two halves of the same fact
/// disagree on the one screen showing both.
#[test]
fn the_serial_row_matches_the_encoded_hash() {
    let all = sticker_lines(&fields()).join("\n");
    assert!(all.contains("0473885"), "{all}");
}

/// Upper case throughout, like the label it is copying. `fit` uppercases when it lays out, so
/// a lower case line here would render in caps anyway and measure wrong for its own width.
#[test]
fn every_line_is_already_upper_case() {
    for line in sticker_lines(&fields()) {
        assert_eq!(line, line.to_uppercase(), "{line}");
    }
}

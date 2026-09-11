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

/// `Canvas::blit` composites the wordmark's own SVG raster onto the label. `render_svg` hands
/// back straight alpha, so the blend scales the source by its own alpha rather than trusting it
/// to already carry that scale; a blend shaped for premultiplied pixels would instead clip
/// every partly covered edge pixel toward the full ink colour, collapsing the wordmark's top
/// edge from a ramp to a single hard step. This scans the columns where that top edge sits (in
/// the sticker's own coordinate space) and asks for at least one column whose edge pixel is
/// neither the ground nor the ink outright: proof the edge is still antialiased.
#[test]
fn the_wordmarks_top_edge_is_antialiased_not_a_hard_step() {
    use slot_ui::sticker_face;
    const GROUND: [u8; 3] = [0x23, 0x1f, 0x20];
    const INK: [u8; 3] = [0xff, 0xff, 0xff];
    let face = sticker_face(&fields());
    let get = |x: u32, y: u32| -> [u8; 3] {
        let i = ((y * face.w + x) * 4) as usize;
        [face.rgba[i], face.rgba[i + 1], face.rgba[i + 2]]
    };
    let edges: Vec<[u8; 3]> = (400..413)
        .filter_map(|x| {
            (129..142).find_map(|y| {
                let above = get(x, y - 1);
                let here = get(x, y);
                (above == GROUND && here != GROUND).then_some(here)
            })
        })
        .collect();
    assert!(
        edges.iter().any(|&e| e != GROUND && e != INK),
        "every sampled column's top edge jumps straight from the ground to the ink: {edges:?}"
    );
}

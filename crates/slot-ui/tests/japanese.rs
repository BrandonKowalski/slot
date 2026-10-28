use slot_i18n::Language;
use slot_ui::{menu_face, quick_label_face, text, title_line_face, QuickRow, Toast};

#[test]
fn japanese_ui_and_rom_titles_render_with_real_glyphs() {
    let font = text::label_font().unwrap();
    for ch in "ポケットモンスター金銀日本語あいうえおガギグゲゴｶﾀｶﾅ漢字".chars()
    {
        let face = text::glyph_font(font, ch);
        assert_ne!(face.lookup_glyph_index(ch), 0, "missing glyph: {ch}");
        let (_, bitmap) = face.rasterize(ch, 24.0);
        assert!(bitmap.iter().any(|a| *a != 0), "empty glyph: {ch}");
    }
    let title = "ポケットモンスター 金・銀";
    slot_i18n::set_language(Language::English);
    let english = title_line_face(title);
    let literal = title_line_face("Settings");
    slot_i18n::set_language(Language::Japanese);
    let japanese = title_line_face(title);
    assert_eq!(
        english.rgba, japanese.rgba,
        "ROM rendering must not depend on locale"
    );
    assert_eq!(
        literal.rgba,
        title_line_face("Settings").rgba,
        "ROM names are not translation keys"
    );
    assert_ne!(title_line_face("日").rgba, title_line_face("本").rgba);
    assert_eq!(
        title_line_face("ガ").rgba,
        title_line_face("カ\u{3099}").rgba
    );
    let timestamp = slot_store::parse_stamp("2026-10-28_15-05-00").unwrap();
    let picker = slot_ui::ClockPicker::new(timestamp, slot_store::Zone::default(), false);
    assert_eq!(picker.value_text(slot_ui::ClockRow::Date), "2026年10月28日");
    assert_eq!(picker.value_text(slot_ui::ClockRow::Time), "午後3:05");
    for row in QuickRow::ALL {
        assert_ne!(
            slot_i18n::tr(row.label()),
            row.label(),
            "untranslated menu row: {row:?}"
        );
        let face = quick_label_face(row, true);
        assert!(face.w <= 640, "{row:?}: width {}", face.w);
        assert!(face.rgba.chunks_exact(4).any(|p| p[3] != 0));
    }
    for zone in slot_store::Zone::ALL {
        if zone.name() != "UTC" {
            assert_ne!(slot_i18n::tr(zone.name()), zone.name(), "untranslated city");
        }
    }
    // Numbered Super Game Boy palettes are technical identifiers.
    for palette in slot_store::GbColors::all().filter(|p| !p.label().starts_with("SGB ")) {
        assert_ne!(
            slot_i18n::tr(palette.label()),
            palette.label(),
            "untranslated palette"
        );
    }
    let catalog = include_str!("../../slot-i18n/locales/ja.tsv");
    for (_, value) in catalog.lines().filter_map(|line| line.split_once('\t')) {
        for ch in value.chars().filter(|c| !c.is_whitespace()) {
            assert_ne!(
                text::glyph_font(font, ch).lookup_glyph_index(ch),
                0,
                "{ch}: {value}"
            );
        }
    }
    assert_eq!(Toast::StateSaved.text(), "ステートを保存しました");
    let prompt = menu_face("Reset all shortcuts?");
    assert!(prompt.w <= 608);
    let layout = text::fit(
        font,
        "日本語の長いゲームタイトルを折り返して表示",
        150.0,
        5,
        24.0,
        12.0,
    );
    assert!(layout.lines.len() > 1);
    assert_eq!(
        layout.lines.concat(),
        "日本語の長いゲームタイトルを折り返して表示"
    );
    assert!(layout.lines.iter().all(|line| text::line_width(
        font,
        line,
        layout.px,
        layout.tracking
    ) <= 150.0));
    slot_i18n::set_language(Language::English);
}

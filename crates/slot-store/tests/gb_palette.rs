use slot_store::{GbColors, GbPalette, SlotState};

#[test]
fn the_list_is_mgba_s_48_palettes_in_its_order() {
    let names: Vec<&str> = GbPalette::all().map(GbPalette::core_name).collect();
    assert_eq!(names.len(), GbPalette::COUNT);
    assert_eq!(GbPalette::COUNT, 48);
    assert_eq!(names[0], "Grayscale");
    assert_eq!(names[1], "DMG Green");
    assert_eq!(names[14], "GBC Dark Green →A");
    assert_eq!(names[16], "SGB 1-A");
    assert_eq!(names[47], "SGB 4-H");
}

#[test]
fn labels_drop_the_boot_combo_arrows_and_stay_unique() {
    let p = GbPalette::parse("GBC Dark Green →A").unwrap();
    assert_eq!(p.label(), "GBC Dark Green");
    assert_eq!(
        GbPalette::parse("GBC Brown ↑").unwrap().label(),
        "GBC Brown"
    );
    assert_eq!(GbPalette::parse("SGB 2-C").unwrap().label(), "SGB 2-C");
    let mut labels: Vec<&str> = GbPalette::all().map(GbPalette::label).collect();
    labels.sort();
    labels.dedup();
    assert_eq!(
        labels.len(),
        GbPalette::COUNT,
        "two palettes print the same name"
    );
    assert!(
        GbPalette::all().all(|p| !p.label().contains(['↑', '↓', '←', '→'])),
        "an arrow survived into a label"
    );
}

#[test]
fn next_walks_the_list_and_wraps() {
    assert_eq!(GbPalette::DEFAULT.core_name(), "DMG Green");
    assert_eq!(GbPalette::DEFAULT.next().core_name(), "GB Pocket");
    let last = GbPalette::all().last().unwrap();
    assert_eq!(last.next().core_name(), "Grayscale");
    assert_eq!(GbPalette::parse("not a palette"), None);
}

#[test]
fn prev_walks_back_and_wraps() {
    assert_eq!(GbPalette::DEFAULT.prev().core_name(), "Grayscale");
    let first = GbPalette::all().next().unwrap();
    assert_eq!(first.prev().core_name(), "SGB 4-H");
    for p in GbPalette::all() {
        assert_eq!(p.next().prev(), p);
    }
}

fn named(name: &str) -> GbPalette {
    GbPalette::parse(name).unwrap()
}

#[test]
fn colours_run_auto_then_every_palette_and_wrap() {
    let all: Vec<GbColors> = GbColors::all().collect();
    assert_eq!(all.len(), 49);
    assert_eq!(all[0], GbColors::Auto);
    assert_eq!(all[1], GbColors::Palette(named("Grayscale")));
    assert_eq!(GbColors::Auto.prev(), GbColors::Palette(named("SGB 4-H")));
    assert_eq!(GbColors::Palette(named("SGB 4-H")).next(), GbColors::Auto);
    assert_eq!(GbColors::Auto.next(), GbColors::Palette(named("Grayscale")));
    assert_eq!(GbColors::Auto.label(), "Auto");
    assert_eq!(GbColors::Auto.core_name(), "Auto");
    assert_eq!(
        GbColors::Palette(named("GBC Dark Green →A")).label(),
        "GBC Dark Green"
    );
    for (i, c) in all.iter().enumerate() {
        assert_eq!(c.index(), i, "{c:?}");
    }
}

#[test]
fn palettes_off_reads_as_auto_and_auto_keeps_the_palette_underneath() {
    let off = SlotState {
        gb_palettes: false,
        gb_palette: named("DMG Green"),
        ..SlotState::default()
    };
    assert_eq!(off.gb_colors(), GbColors::Auto);

    let mut on = SlotState {
        gb_palettes: true,
        gb_palette: named("SGB 1-A"),
        ..SlotState::default()
    };
    assert_eq!(on.gb_colors(), GbColors::Palette(named("SGB 1-A")));
    on.set_gb_colors(GbColors::Auto);
    assert!(!on.gb_palettes);
    assert_eq!(on.gb_palette, named("SGB 1-A"));
    on.set_gb_colors(GbColors::Palette(named("GB Pocket")));
    assert!(on.gb_palettes);
    assert_eq!(on.gb_palette, named("GB Pocket"));
}

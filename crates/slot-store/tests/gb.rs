use std::path::PathBuf;

fn card_rom(rel: &str) -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../sdcard/Games")
        .join(rel);
    p.is_file().then_some(p)
}

/// A plain Game Boy cart: CGB flag 0x00, and a title in the old 11-byte field.
#[test]
fn a_dmg_cart_reads_its_title_and_flag() {
    let Some(rom) = card_rom("GB/Tetris Rosy Retrospection.gb") else {
        eprintln!("no Game Boy ROM on the card: skipping");
        return;
    };
    assert_eq!(slot_store::gb::title(&rom).as_deref(), Some("TETRIS"));
    assert_eq!(slot_store::gb::cgb_flag(&rom), Some(0x00));
    assert!(!slot_store::gb::is_colour(&rom));
}

/// A Colour-only cart: flag 0xc0, and a title field that is **entirely zero**. An empty title
/// is not a malformed ROM and must not be treated as one — the shelf names a cart from its
/// filename anyway.
#[test]
fn a_colour_only_cart_has_no_title_and_that_is_fine() {
    let Some(rom) = card_rom("GBC/Tetris Chromatic.gbc") else {
        eprintln!("no Game Boy Color ROM on the card: skipping");
        return;
    };
    assert_eq!(
        slot_store::gb::title(&rom),
        None,
        "an empty title must read as none"
    );
    assert_eq!(slot_store::gb::cgb_flag(&rom), Some(0xc0));
    assert!(slot_store::gb::is_colour(&rom));
}

/// Nobody has an 0x80 cart, so the middle of the three-way flag is synthesised. Colour-enhanced
/// but DMG-compatible still counts as Colour for the shell finish.
#[test]
fn a_colour_enhanced_cart_counts_as_colour() {
    let d = tempfile::tempdir().unwrap();
    let rom = d.path().join("Enhanced.gb");
    let mut bytes = vec![0u8; 0x150];
    bytes[0x134..0x134 + 6].copy_from_slice(b"ZELDA\0");
    bytes[0x143] = 0x80;
    std::fs::write(&rom, bytes).unwrap();

    assert_eq!(slot_store::gb::title(&rom).as_deref(), Some("ZELDA"));
    assert_eq!(slot_store::gb::cgb_flag(&rom), Some(0x80));
    assert!(slot_store::gb::is_colour(&rom));
}

/// The title field shortened to 11 bytes on later carts to make room for a manufacturer code
/// and the flag itself. Reading 16 from 0x134 swallows both.
#[test]
fn the_title_read_does_not_swallow_the_manufacturer_code_or_the_flag() {
    let d = tempfile::tempdir().unwrap();
    let rom = d.path().join("Long.gbc");
    let mut bytes = vec![0u8; 0x150];
    bytes[0x134..0x134 + 11].copy_from_slice(b"ABCDEFGHIJK");
    bytes[0x13f..0x143].copy_from_slice(b"WXYZ"); // manufacturer code
    bytes[0x143] = 0xc0;
    std::fs::write(&rom, bytes).unwrap();

    assert_eq!(slot_store::gb::title(&rom).as_deref(), Some("ABCDEFGHIJK"));
    assert_eq!(slot_store::gb::cgb_flag(&rom), Some(0xc0));
}

#[test]
fn a_truncated_rom_is_not_a_panic() {
    let d = tempfile::tempdir().unwrap();
    let rom = d.path().join("Short.gb");
    std::fs::write(&rom, [0u8; 0x50]).unwrap();
    assert_eq!(slot_store::gb::title(&rom), None);
    assert_eq!(slot_store::gb::cgb_flag(&rom), None);
}

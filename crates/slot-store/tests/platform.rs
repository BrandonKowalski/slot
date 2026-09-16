use slot_store::{Platform, ShelfKind};

/// Every platform has a directory, GBA included. Nothing stays loose, so there is no variant
/// that means "the root" — an earlier design had one and it was the source of a whole class of
/// asymmetry in the card layout.
#[test]
fn every_platform_has_a_directory() {
    let names: Vec<&str> = Platform::ALL.iter().map(|p| p.dir_name()).collect();
    assert_eq!(names, vec!["GBA", "GB", "GBC"]);
}

/// Three platforms on the card, two shelves on the carousel. A Game Boy and a Game Boy Color
/// cartridge are the same object at the same size, so they share a silhouette and a shelf;
/// only the plastic differs.
#[test]
fn game_boy_and_colour_share_one_shelf() {
    assert_eq!(Platform::Gb.shelf(), ShelfKind::GameBoy);
    assert_eq!(Platform::Gbc.shelf(), ShelfKind::GameBoy);
    assert_eq!(Platform::Gba.shelf(), ShelfKind::Gba);
}

/// The extension a folder will take. A `.gba` in `GB/` is not a Game Boy cart and must not be
/// scanned as one.
#[test]
fn each_platform_takes_only_its_own_extensions() {
    assert!(Platform::Gba.accepts(std::path::Path::new("Metroid Fusion.gba")));
    assert!(!Platform::Gba.accepts(std::path::Path::new("Tetris.gb")));
    assert!(Platform::Gb.accepts(std::path::Path::new("Tetris.gb")));
    assert!(Platform::Gb.accepts(std::path::Path::new("Tetris.gbc")));
    assert!(!Platform::Gb.accepts(std::path::Path::new("Metroid Fusion.gba")));
    // Case is the dumper's business, not ours.
    assert!(Platform::Gba.accepts(std::path::Path::new("Shrek.GBA")));
}

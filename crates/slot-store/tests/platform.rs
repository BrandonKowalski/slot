use slot_store::Platform;

/// Every platform has a directory, GBA included. Nothing stays loose, so there is no variant
/// that means "the root" — an earlier design had one and it was the source of a whole class of
/// asymmetry in the card layout.
#[test]
fn every_platform_has_a_directory() {
    let names: Vec<&str> = Platform::ALL.iter().map(|p| p.dir_name()).collect();
    assert_eq!(names, vec!["GBA", "GB", "GBC"]);
}

/// One shelf per platform, and the carousel keys its shelves on `Platform` itself. There is no
/// second grouping type to map through any more — a Game Boy and a Game Boy Color cart stand on
/// shelves of their own — so `ALL` *is* the ring, in the order the shoulders walk it, and every
/// platform in it is its own stop.
#[test]
fn every_platform_is_a_shelf_of_its_own() {
    assert_eq!(Platform::ALL, [Platform::Gba, Platform::Gb, Platform::Gbc]);
    for (i, p) in Platform::ALL.iter().enumerate() {
        assert!(
            !Platform::ALL[..i].contains(p),
            "{p:?} is named twice in the ring, so two shelves would hold one platform"
        );
    }
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

mod common;

use common::tmp_root;
use slot_store::{scan, Platform};
use tempfile::TempDir;

/// `rel` is a path relative to `Games/`, e.g. `"GBA/Pokemon Emerald.gba"`.
fn write_rom(d: &TempDir, rel: &str, title: &str) {
    let mut rom = vec![0u8; 0x100];
    rom[0xa0..0xa0 + title.len()].copy_from_slice(title.as_bytes());
    std::fs::write(d.path().join("Games").join(rel), rom).expect("write rom");
}

/// `rel` is a path relative to `Labels/`, e.g. `"GBA/Pokemon Emerald.png"`.
fn write_png(d: &TempDir, rel: &str) {
    let path = d.path().join("Labels").join(rel);
    std::fs::create_dir_all(path.parent().expect("label has a parent")).expect("create dir");
    std::fs::write(path, b"\x89PNG\r\n\x1a\n").expect("write png");
}

#[test]
fn a_png_in_labels_is_paired_to_its_rom_by_stem() {
    let d = tmp_root();
    write_rom(&d, "GBA/Pokemon Emerald.gba", "POKEMON EMER");
    write_png(&d, "GBA/Pokemon Emerald.png");
    write_rom(&d, "GBA/Advance Wars.gba", "ADVANCEWARS");
    let carts = scan(d.path()).unwrap();
    assert_eq!(carts.len(), 2);
    assert_eq!(carts[0].stem, "Advance Wars");
    assert!(carts[0].label.is_none());
    assert!(
        carts[1].label.is_some(),
        "a label in Labels/ was not picked up"
    );
    assert_eq!(carts[1].title, "POKEMON EMER");
}

#[test]
fn scan_ignores_non_gba_files() {
    let d = tmp_root();
    write_rom(&d, "GBA/Real.gba", "REAL");
    std::fs::write(d.path().join("Games/GBA/notes.txt"), "hi").unwrap();
    assert_eq!(scan(d.path()).unwrap().len(), 1);
}

#[test]
fn an_appledouble_sidecar_is_not_shelved_as_a_cart() {
    let d = tmp_root();
    write_rom(&d, "GBA/Metroid Fusion.gba", "METROID");
    // Copying a rom onto a FAT card from macOS leaves this beside it, carrying the same
    // extension and the same stem, so only the leading dot tells the two apart.
    write_rom(&d, "GBA/._Metroid Fusion.gba", "METROID");
    let carts = scan(d.path()).unwrap();
    assert_eq!(carts.len(), 1, "an AppleDouble sidecar reached the shelf");
    assert_eq!(carts[0].stem, "Metroid Fusion");
}

#[test]
fn header_title_of_a_truncated_rom_is_none_not_a_panic() {
    let d = tmp_root();
    std::fs::write(d.path().join("Games/GBA/Tiny.gba"), [0u8; 8]).unwrap();
    assert!(scan(d.path()).unwrap()[0].title.is_empty());
}

#[test]
fn a_header_title_that_is_not_text_is_dropped_rather_than_mangled() {
    let d = tmp_root();
    let mut rom = vec![0u8; 0x100];
    rom[0xa0..0xac].copy_from_slice(&[0xffu8; 12]);
    std::fs::write(d.path().join("Games/GBA/Garbage.gba"), rom).unwrap();
    assert!(scan(d.path()).unwrap()[0].title.is_empty());
}

#[test]
fn a_root_with_no_games_directory_scans_as_empty() {
    let d = tempfile::tempdir().unwrap();
    assert!(scan(d.path()).unwrap().is_empty());
}

/// Location decides the platform. Nothing is read out of the ROM to work it out.
#[test]
fn a_cart_takes_the_platform_of_the_folder_it_is_in() {
    let d = tmp_root();
    write_rom(&d, "GBA/Metroid Fusion.gba", "METROID");
    std::fs::write(d.path().join("Games/GB/Tetris.gb"), vec![0u8; 0x150]).unwrap();
    std::fs::write(d.path().join("Games/GBC/Chromatic.gbc"), vec![0u8; 0x150]).unwrap();

    let carts = scan(d.path()).unwrap();

    let by_stem = |s: &str| carts.iter().find(|c| c.stem == s).unwrap().platform;
    assert_eq!(by_stem("Metroid Fusion"), Platform::Gba);
    assert_eq!(by_stem("Tetris"), Platform::Gb);
    assert_eq!(by_stem("Chromatic"), Platform::Gbc);
}

/// A `.gba` filed under `GB/` is not a Game Boy cart. The folder says where a cart's files go;
/// it cannot make a GBA ROM into a Game Boy game, and running it as one would be a broken
/// screen. It simply does not appear, and slot says nothing.
#[test]
fn a_gba_rom_in_the_game_boy_folder_does_not_appear() {
    let d = tmp_root();
    std::fs::write(d.path().join("Games/GB/Wrong.gba"), vec![0u8; 0x150]).unwrap();
    assert!(scan(d.path()).unwrap().is_empty());
}

/// Two carts of the same name on different platforms are two carts, and their labels are two
/// files. This is the collision the whole layout exists to close.
#[test]
fn the_same_stem_on_two_platforms_is_two_carts_with_two_labels() {
    let d = tmp_root();
    write_rom(&d, "GBA/Tetris.gba", "TETRIS");
    std::fs::write(d.path().join("Games/GB/Tetris.gb"), vec![0u8; 0x150]).unwrap();
    std::fs::create_dir_all(d.path().join("Labels/GB")).unwrap();
    std::fs::write(d.path().join("Labels/GB/Tetris.png"), b"\x89PNG\r\n\x1a\n").unwrap();

    let carts = scan(d.path()).unwrap();

    assert_eq!(carts.len(), 2);
    let gb = carts.iter().find(|c| c.platform == Platform::Gb).unwrap();
    let gba = carts.iter().find(|c| c.platform == Platform::Gba).unwrap();
    assert!(gb.label.is_some(), "the Game Boy label was not found");
    assert!(
        gba.label.is_none(),
        "the GBA cart borrowed the Game Boy label"
    );
}

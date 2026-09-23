mod common;

use common::tmp_root;
use slot_store::scan;
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

/// slot runs Game Boy Advance carts and nothing else. A card from the builds that ran Game Boy
/// carts too keeps its `GB/` and `GBC/` folders untouched, and nothing in them reaches the shelf;
/// nor does a Game Boy rom filed under `GBA/`.
#[test]
fn only_gba_roms_under_gba_are_carts() {
    let d = tmp_root();
    write_rom(&d, "GBA/Metroid Fusion.gba", "METROID");
    for dir in ["Games/GB", "Games/GBC"] {
        std::fs::create_dir_all(d.path().join(dir)).unwrap();
    }
    std::fs::write(d.path().join("Games/GB/Tetris.gb"), vec![0u8; 0x150]).unwrap();
    std::fs::write(d.path().join("Games/GBC/Chromatic.gbc"), vec![0u8; 0x150]).unwrap();
    std::fs::write(d.path().join("Games/GBA/Stray.gb"), vec![0u8; 0x150]).unwrap();

    let carts = scan(d.path()).unwrap();

    let stems: Vec<_> = carts.iter().map(|c| c.stem.as_str()).collect();
    assert_eq!(stems, ["Metroid Fusion"]);
}

/// `App::boot` does `scan(root).unwrap_or_default()`, so an `Err` out of `scan` is not a message
/// anywhere. A `GBA` folder that will not open is an empty shelf, said once on stderr.
#[test]
fn an_unreadable_games_folder_is_an_empty_shelf() {
    let d = tmp_root();
    // A corrupted card: a plain file standing where the folder belongs, so `read_dir` answers
    // ENOTDIR rather than "nothing here".
    std::fs::remove_dir(d.path().join("Games/GBA")).unwrap();
    std::fs::write(d.path().join("Games/GBA"), b"not a directory").unwrap();

    assert!(scan(d.path())
        .expect("an unreadable folder is not an error")
        .is_empty());
}

mod common;

use common::tmp_root;
use slot_store::scan;
use tempfile::TempDir;

fn write_rom(d: &TempDir, name: &str, title: &str) {
    let mut rom = vec![0u8; 0x100];
    rom[0xa0..0xa0 + title.len()].copy_from_slice(title.as_bytes());
    std::fs::write(d.path().join("Games").join(name), rom).expect("write rom");
}

fn write_png(d: &TempDir, name: &str) {
    std::fs::write(d.path().join("Labels").join(name), b"\x89PNG\r\n\x1a\n").expect("write png");
}

#[test]
fn a_png_in_labels_is_paired_to_its_rom_by_stem() {
    let d = tmp_root();
    write_rom(&d, "Pokemon Emerald.gba", "POKEMON EMER");
    write_png(&d, "Pokemon Emerald.png");
    write_rom(&d, "Advance Wars.gba", "ADVANCEWARS");
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
    write_rom(&d, "Real.gba", "REAL");
    std::fs::write(d.path().join("Games/notes.txt"), "hi").unwrap();
    assert_eq!(scan(d.path()).unwrap().len(), 1);
}

#[test]
fn an_appledouble_sidecar_is_not_shelved_as_a_cart() {
    let d = tmp_root();
    write_rom(&d, "Metroid Fusion.gba", "METROID");
    // Copying a rom onto a FAT card from macOS leaves this beside it, carrying the same
    // extension and the same stem, so only the leading dot tells the two apart.
    write_rom(&d, "._Metroid Fusion.gba", "METROID");
    let carts = scan(d.path()).unwrap();
    assert_eq!(carts.len(), 1, "an AppleDouble sidecar reached the shelf");
    assert_eq!(carts[0].stem, "Metroid Fusion");
}

#[test]
fn header_title_of_a_truncated_rom_is_none_not_a_panic() {
    let d = tmp_root();
    std::fs::write(d.path().join("Games/Tiny.gba"), [0u8; 8]).unwrap();
    assert!(scan(d.path()).unwrap()[0].title.is_empty());
}

#[test]
fn a_header_title_that_is_not_text_is_dropped_rather_than_mangled() {
    let d = tmp_root();
    let mut rom = vec![0u8; 0x100];
    rom[0xa0..0xac].copy_from_slice(&[0xffu8; 12]);
    std::fs::write(d.path().join("Games/Garbage.gba"), rom).unwrap();
    assert!(scan(d.path()).unwrap()[0].title.is_empty());
}

#[test]
fn a_root_with_no_games_directory_scans_as_empty() {
    let d = tempfile::tempdir().unwrap();
    assert!(scan(d.path()).unwrap().is_empty());
}

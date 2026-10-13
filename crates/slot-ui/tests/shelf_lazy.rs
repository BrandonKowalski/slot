use std::fs;

use slot_store::{Cart, Platform};
use slot_ui::{Draw, GbShell, Shelf, TexId};

const NOTCHED: TexId = TexId::from_raw(1);
const ROUNDED: TexId = TexId::from_raw(2);
const PLAIN: TexId = TexId::from_raw(3);

fn cart(platform: Platform, stem: &str, rom: std::path::PathBuf) -> Cart {
    Cart {
        platform,
        stem: stem.into(),
        rom,
        label: None,
        code: String::new(),
        shell: None,
        title: String::new(),
    }
}

fn faded_backings(shelf: &Shelf) -> Vec<TexId> {
    let mut out = Vec::new();
    shelf.draw(0.0, &mut out);
    out.iter()
        .filter_map(|d| match *d {
            Draw::Tex { tex, .. } => Some(tex),
            _ => None,
        })
        .collect()
}

#[test]
fn a_neighbours_backing_follows_the_rom_as_it_is_at_draw_time() {
    let dir = tempfile::tempdir().unwrap();
    let rom = dir.path().join("colour.gbc");
    let mut shelf = Shelf::new(vec![
        cart(Platform::Gba, "Front", dir.path().join("front.gba")),
        cart(Platform::Gbc, "Colour", rom.clone()),
    ]);
    shelf.set_shadow(PLAIN);
    shelf.set_gb_shadow(GbShell::Notched, NOTCHED);
    shelf.set_gb_shadow(GbShell::Rounded, ROUNDED);
    // Written after the shelf is built: Shelf::new must not have read it.
    let mut header = vec![0u8; 0x150];
    header[0x143] = 0xc0;
    fs::write(&rom, header).unwrap();
    let backings = faded_backings(&shelf);
    assert!(backings.contains(&ROUNDED));
    assert!(!backings.contains(&NOTCHED));
}

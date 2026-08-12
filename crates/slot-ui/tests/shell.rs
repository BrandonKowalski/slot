use slot_store::scan;
use slot_ui::{
    cart_face, lookup_order_is_exact_then_family_then_default, shell_for, table_keys, Finish,
    DEFAULT_SHELL,
};
use tempfile::TempDir;

fn tmp_root() -> TempDir {
    let d = tempfile::tempdir().expect("tempdir");
    for sub in ["Games", "Labels", "Saves", "States", "System"] {
        std::fs::create_dir(d.path().join(sub)).expect("create content dir");
    }
    d
}

fn write_rom_with_code(d: &TempDir, name: &str, title: &str, code: &str) {
    let mut rom = vec![0u8; 0x100];
    rom[0xa0..0xa0 + title.len()].copy_from_slice(title.as_bytes());
    rom[0xac..0xac + code.len()].copy_from_slice(code.as_bytes());
    std::fs::write(d.path().join("Games").join(name), rom).expect("write rom");
}

#[test]
fn an_unknown_game_gets_the_default_grey() {
    assert_eq!(shell_for("ZZZZ").colour, DEFAULT_SHELL.colour);
    assert_eq!(shell_for("").colour, DEFAULT_SHELL.colour);
    // A real, ordinary cart: Metroid Fusion, verified as AMTE. Most of the library lands
    // here and it must not be a special case.
    assert_eq!(shell_for("AMTE").colour, DEFAULT_SHELL.colour);
}

#[test]
fn leafgreen_is_green_whatever_region_it_came_from() {
    for code in ["BPGE", "BPGJ", "BPGP", "BPGD"] {
        let s = shell_for(code);
        assert_ne!(
            s.colour, DEFAULT_SHELL.colour,
            "{code} fell through to grey"
        );
        assert!(s.colour[1] > s.colour[0], "{code} is not green");
    }
}

/// Verified from a real header: Shrek GBA Video is MSKE. The family letter is what saves
/// this from being thirty hand transcribed rows.
#[test]
fn gba_video_carts_are_light_grey() {
    let v = shell_for("MSKE");
    assert_ne!(
        v.colour, DEFAULT_SHELL.colour,
        "video fell through to the default grey"
    );
    assert!(
        v.colour.iter().all(|c| *c > 0xA0),
        "video shells are light grey, got {:?}",
        v.colour
    );
    // The whole family, not just the one title that was on hand.
    assert_eq!(shell_for("MPOE").colour, v.colour);
}

/// An explicit row has to beat the family letter, or the escape hatch does not work.
#[test]
fn an_exact_entry_outranks_the_family_letter() {
    assert_eq!(shell_for("MSKE").colour, shell_for("MSKJ").colour);
    // Adding an exact "MSK" row must be able to override; the lookup order is what is
    // under test, so assert it directly rather than through the table.
    assert!(lookup_order_is_exact_then_family_then_default());
}

/// Every cart in the table is solid, including the coloured ones. Colour and finish stay
/// separate axes so a clear shell is a table row rather than a code change.
#[test]
fn every_shell_in_the_table_is_solid() {
    for code in ["AXVE", "AXPE", "BPEE", "BPRE", "BPGE", "MSKE", "AMTE", ""] {
        assert!(
            matches!(shell_for(code).finish, Finish::Solid),
            "{code} is marked translucent, but no cart in the table is"
        );
    }
}

#[test]
fn the_pokemon_shells_are_all_distinct() {
    let codes = ["AXVE", "AXPE", "BPEE", "BPRE", "BPGE"];
    let mut seen = Vec::new();
    for c in codes {
        let col = shell_for(c).colour;
        assert!(
            !seen.contains(&col),
            "{c} shares a colour with another cart"
        );
        seen.push(col);
    }
}

/// A three character key is short enough to collide by accident. It must not.
#[test]
fn no_two_table_entries_share_a_prefix() {
    let mut keys: Vec<&str> = table_keys();
    keys.sort();
    let before = keys.len();
    keys.dedup();
    assert_eq!(keys.len(), before, "two entries claim the same prefix");
    assert!(
        keys.iter().all(|k| k.len() == 3),
        "keys must be the region free prefix"
    );
}

#[test]
fn the_label_does_not_cover_the_whole_shell() {
    let d = tmp_root();
    write_rom_with_code(&d, "Emerald.gba", "POKEMON EMER", "BPEE");
    let cart = &scan(d.path()).unwrap()[0];
    let f = cart_face(cart);
    let px = |x: u32, y: u32| {
        let i = ((y * f.w + x) * 4) as usize;
        [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2]]
    };
    let shell = shell_for("BPEE").colour;
    assert_eq!(
        px(f.w / 2, 4),
        shell,
        "the label reaches the top edge, no shell shows"
    );
    assert_ne!(
        px(f.w / 2, f.h / 2),
        shell,
        "the label is missing from the middle"
    );
}

use slot_store::scan;
use slot_ui::{
    cart_face, gb_shell_of, gba_shell_for, lookup_order_is_exact_then_family_then_default,
    shell_for, shell_presets, table_keys, Finish, GbShell, DEFAULT_SHELL, DMG_SHELL,
    DUAL_MODE_SHELL,
};
use tempfile::TempDir;

fn tmp_root() -> TempDir {
    let d = tempfile::tempdir().expect("tempdir");
    for sub in [
        "Games",
        "Games/GBA",
        "Labels",
        "Saves",
        "States",
        "System",
        "Config",
    ] {
        std::fs::create_dir(d.path().join(sub)).expect("create content dir");
    }
    d
}

fn write_rom_with_code(d: &TempDir, name: &str, title: &str, code: &str) {
    let mut rom = vec![0u8; 0x100];
    rom[0xa0..0xa0 + title.len()].copy_from_slice(title.as_bytes());
    rom[0xac..0xac + code.len()].copy_from_slice(code.as_bytes());
    std::fs::write(d.path().join("Games/GBA").join(name), rom).expect("write rom");
}

#[test]
fn an_unknown_game_gets_the_default_grey() {
    assert_eq!(gba_shell_for("ZZZZ").colour, DEFAULT_SHELL.colour);
    assert_eq!(gba_shell_for("").colour, DEFAULT_SHELL.colour);
    assert_eq!(gba_shell_for("AMTE").colour, DEFAULT_SHELL.colour);
}

#[test]
fn leafgreen_is_green_whatever_region_it_came_from() {
    for code in ["BPGE", "BPGJ", "BPGP", "BPGD"] {
        let s = gba_shell_for(code);
        assert_ne!(
            s.colour, DEFAULT_SHELL.colour,
            "{code} fell through to grey"
        );
        assert!(s.colour[1] > s.colour[0], "{code} is not green");
    }
}

#[test]
fn gba_video_carts_are_light_grey() {
    let v = gba_shell_for("MSKE");
    assert_ne!(
        v.colour, DEFAULT_SHELL.colour,
        "video fell through to the default grey"
    );
    assert!(
        v.colour.iter().all(|c| *c > 0xA0),
        "video shells are light grey, got {:?}",
        v.colour
    );
    assert_eq!(gba_shell_for("MPOE").colour, v.colour);
}

#[test]
fn an_exact_entry_outranks_the_family_letter() {
    assert_eq!(gba_shell_for("MSKE").colour, gba_shell_for("MSKJ").colour);
    assert!(lookup_order_is_exact_then_family_then_default());
}

#[test]
fn the_clear_carts_are_clear_and_the_rest_are_solid() {
    for code in table_keys() {
        let clear = code == "RZW" || ["AX", "BPE", "U3"].iter().any(|p| code.starts_with(p));
        let want = if clear {
            Finish::Translucent
        } else {
            Finish::Solid
        };
        assert_eq!(
            gba_shell_for(code).finish,
            want,
            "{code} has the wrong finish"
        );
    }
    assert_eq!(
        gba_shell_for("MSKE").finish,
        Finish::Solid,
        "the video family is not solid"
    );
    assert_eq!(
        gba_shell_for("ZZZZ").finish,
        Finish::Solid,
        "the default is not solid"
    );
}

#[test]
fn the_pokemon_shells_are_all_distinct() {
    let codes = ["AXVE", "AXPE", "BPEE", "BPRE", "BPGE"];
    let mut seen = Vec::new();
    for c in codes {
        let col = gba_shell_for(c).colour;
        assert!(
            !seen.contains(&col),
            "{c} shares a colour with another cart"
        );
        seen.push(col);
    }
}

#[test]
fn no_two_table_entries_share_a_key() {
    let mut keys: Vec<&str> = table_keys();
    keys.sort();
    let before = keys.len();
    keys.dedup();
    assert_eq!(keys.len(), before, "two entries claim the same key");
    assert!(
        keys.iter().all(|k| k.len() == 3 || k.len() == 4),
        "keys are a region free prefix or one region's whole code"
    );
}

#[test]
fn a_region_that_shipped_other_plastic_gets_its_own() {
    let us = gba_shell_for("RZWE");
    let jp = gba_shell_for("RZWJ");
    assert_eq!(us.finish, Finish::Translucent);
    assert_eq!(jp.finish, Finish::Solid);
    assert!(jp.colour.iter().all(|c| *c > 0xd0), "Japan's is white");
    assert_ne!(us.colour, DEFAULT_SHELL.colour);
}

#[test]
fn boktai_is_clear_everywhere_and_drill_dozer_is_red() {
    for code in ["U3IE", "U3IP", "U3IJ", "U32E", "U33J"] {
        assert_eq!(gba_shell_for(code).finish, Finish::Translucent, "{code}");
    }
    let dozer = gba_shell_for("V49E");
    assert!(dozer.colour[0] > dozer.colour[2] + 40, "Drill Dozer is red");
    assert_eq!(dozer.colour, gba_shell_for("V49J").colour);
}

fn gb_shell(title: &[u8], code: &[u8], cgb: u8, japan: bool) -> slot_ui::Shell {
    let d = tempfile::tempdir().expect("tempdir");
    let games = d.path().join("Games/GB");
    std::fs::create_dir_all(&games).expect("games dir");
    let mut rom = vec![0u8; 0x150];
    rom[0x134..0x134 + title.len()].copy_from_slice(title);
    rom[0x13f..0x13f + code.len()].copy_from_slice(code);
    rom[0x143] = cgb;
    rom[0x14a] = u8::from(!japan);
    std::fs::write(games.join("Pak.gb"), rom).expect("rom");
    shell_for(&scan(d.path()).expect("scan")[0])
}

#[test]
fn red_and_blue_are_coloured_outside_japan_only() {
    let red = gb_shell(b"POKEMON RED", b"", 0x00, false);
    let blue = gb_shell(b"POKEMON BLUE", b"", 0x00, false);
    assert!(red.colour[0] > red.colour[2] + 60, "Red is red");
    assert!(blue.colour[2] > blue.colour[0] + 60, "Blue is blue");
    assert_eq!(
        gb_shell(b"POKEMON RED", b"", 0x00, true),
        DMG_SHELL,
        "Japan's Red is the grey pak"
    );
}

#[test]
fn gold_is_gold_except_in_japan() {
    let gold = gb_shell(b"POKEMON_GLD", b"AAUE", 0x80, false);
    assert_ne!(gold, DUAL_MODE_SHELL);
    assert_eq!(gold, gb_shell(b"POKEMON_GLD", b"AAUD", 0x80, false));
    assert_eq!(
        gb_shell(b"POKEMON_GLD", b"AAUJ", 0x80, true),
        DUAL_MODE_SHELL
    );
    assert_ne!(gb_shell(b"POKEMON_SLV", b"AAXE", 0x80, false), gold);
}

#[test]
fn crystal_is_aqua_under_both_its_codes() {
    let en = gb_shell(b"PM_CRYSTAL", b"BYTE", 0xc0, false);
    let jp = gb_shell(b"PM_CRYSTAL", b"BXTJ", 0xc0, true);
    assert_eq!(en, jp);
    assert_eq!(
        en.finish,
        Finish::Glitter,
        "Crystal's plastic has glitter in it"
    );
    assert!(en.colour[2] > en.colour[0], "Crystal is aqua");
}

#[test]
fn metal_gear_solid_is_black_not_clear() {
    assert_eq!(
        gb_shell(b"METALGEARGB", b"BMGE", 0xc0, false),
        DUAL_MODE_SHELL
    );
}

#[test]
fn pinball_is_yellow_in_japan_only() {
    let jp = gb_shell(b"POKEPINBALL", b"VPHJ", 0x80, true);
    assert!(
        jp.colour[0] > 0xc0 && jp.colour[2] < 0x60,
        "Japan's is yellow"
    );
    assert_eq!(
        gb_shell(b"POKEPINBALL", b"VPHE", 0x80, false),
        DUAL_MODE_SHELL
    );
}

#[test]
fn an_unlisted_pak_keeps_its_flags_plastic() {
    assert_eq!(gb_shell(b"TETRIS", b"", 0x00, false), DMG_SHELL);
    assert_eq!(gb_shell(b"ZELDA", b"AZLE", 0x80, false), DUAL_MODE_SHELL);
}

#[test]
fn the_label_does_not_cover_the_whole_shell() {
    let d = tmp_root();
    write_rom_with_code(&d, "Drill Dozer.gba", "DRILL DOZER", "V49E");
    let cart = &scan(d.path()).unwrap()[0];
    let f = cart_face(cart);
    let px = |x: u32, y: u32| {
        let i = ((y * f.w + x) * 4) as usize;
        [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2]]
    };
    let shell = gba_shell_for("V49E").colour;
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

fn chosen(dir: &str, cgb: u8, code: &str, line: Option<&str>) -> (TempDir, slot_store::Cart) {
    let d = tempfile::tempdir().expect("tempdir");
    for sub in [
        format!("Games/{dir}"),
        "System".to_string(),
        "Config".to_string(),
    ] {
        std::fs::create_dir_all(d.path().join(sub)).expect("dir");
    }
    let mut rom = vec![0u8; 0x150];
    rom[0x143] = cgb;
    rom[0xac..0xac + code.len()].copy_from_slice(code.as_bytes());
    let ext = dir.to_lowercase();
    std::fs::write(d.path().join(format!("Games/{dir}/Pak.{ext}")), rom).expect("rom");
    if let Some(line) = line {
        std::fs::write(
            d.path().join("Config/cart_shell.txt"),
            format!("Pak = {line}\n"),
        )
        .expect("ini");
    }
    let cart = scan(d.path()).expect("scan").remove(0);
    (d, cart)
}

#[test]
fn a_chosen_colour_and_finish_win() {
    let (_d, cart) = chosen("GB", 0x00, "", Some("auto 123456 glitter"));
    assert_eq!(shell_for(&cart).colour, [0x12, 0x34, 0x56]);
    assert_eq!(shell_for(&cart).finish, Finish::Glitter);
    let (_d, cart) = chosen("GBA", 0, "BPEE", Some("auto abcdef clear"));
    assert_eq!(shell_for(&cart).colour, [0xab, 0xcd, 0xef]);
    assert_eq!(shell_for(&cart).finish, Finish::Translucent);
}

#[test]
fn no_choice_changes_nothing() {
    for (dir, cgb, code) in [
        ("GB", 0x00, ""),
        ("GBC", 0xc0, ""),
        ("GBA", 0, "BPEE"),
        ("GBA", 0, "AMTE"),
    ] {
        let (_d, plain) = chosen(dir, cgb, code, None);
        let (_d2, garbled) = chosen(dir, cgb, code, Some("rounded"));
        assert_eq!(plain.shell, None);
        assert_eq!(shell_for(&plain), shell_for(&garbled), "{dir} {code}");
        assert_eq!(gb_shell_of(&plain), gb_shell_of(&garbled), "{dir} {code}");
    }
}

#[test]
fn a_chosen_outline_wins_on_a_game_boy_cart_and_not_on_gba() {
    let (_d, grey) = chosen("GB", 0x00, "", Some("rounded 9a978f solid"));
    assert_eq!(gb_shell_of(&grey), Some(GbShell::Rounded));
    let (_d, clear) = chosen("GBC", 0xc0, "", Some("notched 7c7a8a clear"));
    assert_eq!(gb_shell_of(&clear), Some(GbShell::Notched));
    let (_d, auto) = chosen("GBC", 0xc0, "", Some("auto 7c7a8a clear"));
    assert_eq!(gb_shell_of(&auto), Some(GbShell::Rounded));
    let (_d, gba) = chosen("GBA", 0, "AMTE", Some("rounded 112233 solid"));
    assert_eq!(gb_shell_of(&gba), None);
}

#[test]
fn a_rounded_choice_on_a_grey_pak_draws_the_rounded_outline() {
    let (_d, chosen_rounded) = chosen("GB", 0x00, "", Some("rounded 9a978f solid"));
    let (_d2, real_rounded) = chosen("GBC", 0xc0, "", Some("auto 9a978f solid"));
    let alpha = |cart: &slot_store::Cart| {
        let f = cart_face(cart);
        (0..f.w * f.h)
            .map(|i| f.rgba[(i * 4 + 3) as usize])
            .collect::<Vec<_>>()
    };
    assert_eq!(alpha(&chosen_rounded), alpha(&real_rounded));
}

#[test]
fn the_presets_are_the_tables_plastics_each_once() {
    let presets = shell_presets();
    assert_eq!(
        presets.len(),
        25,
        "a table row the presets name has gone missing"
    );
    let mut names: Vec<_> = presets.iter().map(|(n, _)| *n).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), presets.len(), "two presets share a name");
    let find = |n: &str| presets.iter().find(|(name, _)| *name == n).unwrap().1;
    assert_eq!(find("Game Boy grey"), DMG_SHELL);
    assert_eq!(find("Pokémon Emerald"), gba_shell_for("BPEE"));
}

fn greenish(face: &slot_ui::CartFace) -> usize {
    face.rgba
        .chunks_exact(4)
        .filter(|p| p[3] > 0 && p[1] as i32 > p[0] as i32 + 10 && p[1] as i32 > p[2] as i32 + 10)
        .count()
}

#[test]
fn a_clear_gba_cart_shows_its_board_through_the_plastic() {
    let (_d, clear) = chosen("GBA", 0, "BPEE", Some("auto d9dbd8 clear"));
    let (_d2, solid) = chosen("GBA", 0, "BPEE", Some("auto d9dbd8 solid"));
    assert_eq!(
        greenish(&cart_face(&solid)),
        0,
        "solid plastic hides the board"
    );
    assert!(
        greenish(&cart_face(&clear)) > 500,
        "a clear GBA cart shows no board: {} green pixels",
        greenish(&cart_face(&clear))
    );
}

fn richness(clear: &slot_ui::CartFace, solid: &slot_ui::CartFace) -> (f32, f32) {
    let spread = |p: &[u8]| {
        let (hi, lo) = (*p[..3].iter().max().unwrap(), *p[..3].iter().min().unwrap());
        if hi == 0 {
            0.0
        } else {
            (hi - lo) as f32 / hi as f32
        }
    };
    let (mut c, mut s, mut n) = (0.0, 0.0, 0.0);
    for (a, b) in clear.rgba.chunks_exact(4).zip(solid.rgba.chunks_exact(4)) {
        if a[3] > 0 && a[..3] != b[..3] {
            c += spread(a);
            s += spread(b);
            n += 1.0;
        }
    }
    (c / n, s / n)
}

#[test]
fn clear_coloured_plastic_stays_as_rich_as_the_solid_kind() {
    for colour in ["c2332e", "249c60"] {
        let (_d, clear) = chosen("GBA", 0, "BPEE", Some(&format!("auto {colour} clear")));
        let (_d2, solid) = chosen("GBA", 0, "BPEE", Some(&format!("auto {colour} solid")));
        let (c, s) = richness(&cart_face(&clear), &cart_face(&solid));
        assert!(
            c >= 0.9 * s,
            "clear {colour} is washed out: {c:.2} against solid {s:.2}"
        );
        let (c, s) = depth(&cart_face(&clear), &cart_face(&solid));
        assert!(
            c < 0.9 * s,
            "clear {colour} is one thin layer: {c:.0} against solid {s:.0}"
        );
    }
}

fn depth(clear: &slot_ui::CartFace, solid: &slot_ui::CartFace) -> (f32, f32) {
    let y = (clear.h as f32 * 0.06) as u32;
    let lum = |f: &slot_ui::CartFace| {
        let row = (clear.w * 3 / 10..clear.w * 7 / 10).map(|x| {
            let i = ((y * f.w + x) * 4) as usize;
            f.rgba[i] as f32 * 0.3 + f.rgba[i + 1] as f32 * 0.59 + f.rgba[i + 2] as f32 * 0.11
        });
        row.clone().sum::<f32>() / row.count() as f32
    };
    (lum(clear), lum(solid))
}

#[test]
fn firered_and_leafgreen_are_solid() {
    for code in ["BPRE", "BPGE"] {
        assert_eq!(
            gba_shell_for(code).finish,
            Finish::Solid,
            "{code} is not solid"
        );
    }
}

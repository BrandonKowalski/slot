mod common;

use slot::link_kind::{serial_option, LinkKind};
use slot_retro::LibretroCore;
use slot_store::Core;

fn dylib_for(core: Core) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor")
        .join(slot::core::dylib_name(core))
}

fn every_serial_mode() -> Vec<&'static str> {
    let carts = [
        ("BPEE", "POKEMON EMER"),
        ("AWRE", "ADVANCEWARS"),
        ("AW2E", "ADVANCE WARS2"),
        ("AMFE", "METROID4"),
    ];
    let mut modes = Vec::new();
    for (code, title) in carts {
        for auto in [LinkKind::Cable, LinkKind::Wireless] {
            for chosen in [LinkKind::Cable, LinkKind::Wireless] {
                let mode = serial_option(chosen, auto, code, title);
                if !modes.contains(&mode) {
                    modes.push(mode);
                }
            }
        }
    }
    modes
}

fn every_option_is_one_the_core_has(core: &LibretroCore, what: &str) {
    let declared = core.declared_options();
    assert!(
        !declared.is_empty(),
        "{what}: the core declared no options at all, so this test would pass on anything"
    );
    let set = core.options();
    assert!(!set.is_empty(), "{what}: no options were set");
    for (key, value) in set {
        let Some(values) = declared.get(&key) else {
            let mut known: Vec<_> = declared.keys().cloned().collect();
            known.sort();
            panic!("{what}: the core declares no option {key:?}. It declares: {known:?}");
        };
        assert!(
            values.contains(&value),
            "{what}: the core declares {key:?} as {values:?}, and slot set it to {value:?}"
        );
    }
}

#[test]
fn every_option_slot_sets_is_one_the_core_declares() {
    let _g = common::core_lock();
    let mut ran = 0;
    for which in Core::ALL {
        let path = dylib_for(which);
        if !path.exists() {
            eprintln!("no {} dylib on this host, skipping", which.as_str());
            continue;
        }
        for serial in every_serial_mode() {
            for bios in [false, true] {
                for colour in [false, true] {
                    for palette in
                        std::iter::once(None).chain(slot_store::GbPalette::all().map(Some))
                    {
                        let mut core = LibretroCore::open(&path).expect("open core");
                        slot::core::apply_core_options(
                            &mut core, which, serial, bios, colour, palette,
                        );
                        every_option_is_one_the_core_has(
                            &core,
                            &format!(
                                "{} serial={serial} bios={bios} colour={colour} palette={palette:?}",
                                which.as_str()
                            ),
                        );
                        ran += 1;
                    }
                }
            }
        }
    }
    if ran == 0 {
        eprintln!("no cores on this host, nothing was checked");
    }
}

#[test]
fn both_cores_declare_a_colour_correction_option() {
    let _g = common::core_lock();
    for (which, key) in [
        (Core::Mgba, "mgba_color_correction"),
        (Core::Gpsp, "gpsp_color_correction"),
    ] {
        let path = dylib_for(which);
        if !path.exists() {
            eprintln!("no {} dylib on this host, skipping", which.as_str());
            continue;
        }
        let core = LibretroCore::open(&path).expect("open core");
        assert!(
            core.declared_options().contains_key(key),
            "{} declares no {key}",
            which.as_str()
        );
    }
}

#[test]
fn a_cable_session_leaves_the_bios_to_the_card() {
    let _g = common::core_lock();
    let path = dylib_for(Core::Mgba);
    if !path.exists() {
        eprintln!("no mgba dylib on this host, skipping");
        return;
    }
    let mut core = LibretroCore::open(&path).expect("open core");
    slot::core::apply_link_options(&mut core, Core::Mgba, 1);
    let set: std::collections::HashMap<String, String> = core.options().into_iter().collect();
    assert_eq!(
        set.get("mgba_use_bios").map(String::as_str),
        None,
        "link mode overrode the card's BIOS"
    );
    assert_eq!(set.get("mgba_link").map(String::as_str), Some("on"));
    assert_eq!(
        set.get("mgba_link_player").map(String::as_str),
        Some("1"),
        "the port this device drives did not reach the core"
    );
    every_option_is_one_the_core_has(&core, "mgba cable session");
}

#[test]
fn a_cable_session_sets_nothing_on_gpsp() {
    let _g = common::core_lock();
    let path = dylib_for(Core::Gpsp);
    if !path.exists() {
        eprintln!("no gpsp dylib on this host, skipping");
        return;
    }
    let mut core = LibretroCore::open(&path).expect("open core");
    slot::core::apply_link_options(&mut core, Core::Gpsp, 0);
    assert!(
        core.options().is_empty(),
        "a cable session reached into gpSP, which has no cable to offer"
    );
}

#[test]
fn a_named_palette_forces_a_plain_game_boy_without_hardware_presets() {
    let _g = common::core_lock();
    let path = dylib_for(Core::Mgba);
    if !path.exists() {
        eprintln!("no mgba dylib on this host, skipping");
        return;
    }
    let p = slot_store::GbPalette::parse("SGB 1-A").unwrap();
    let mut core = LibretroCore::open(&path).expect("open core");
    slot::core::apply_core_options(&mut core, Core::Mgba, "auto", false, false, Some(p));
    let set: std::collections::HashMap<String, String> = core.options().into_iter().collect();
    assert_eq!(
        set.get("mgba_gb_model").map(String::as_str),
        Some("Game Boy")
    );
    assert_eq!(
        set.get("mgba_gb_colors_preset").map(String::as_str),
        Some("0")
    );
    assert_eq!(
        set.get("mgba_gb_colors").map(String::as_str),
        Some("SGB 1-A")
    );
    drop(core);

    let mut core = LibretroCore::open(&path).expect("open core");
    slot::core::apply_core_options(&mut core, Core::Mgba, "auto", false, false, None);
    let set: std::collections::HashMap<String, String> = core.options().into_iter().collect();
    assert_eq!(
        set.get("mgba_gb_model").map(String::as_str),
        Some("Autodetect")
    );
    assert_eq!(
        set.get("mgba_gb_colors_preset").map(String::as_str),
        Some("1")
    );
    assert_eq!(
        set.get("mgba_gb_colors").map(String::as_str),
        Some("GBC Dark Green →A"),
        "palettes off changed what a Game Boy cart gets today"
    );
}

#[test]
fn only_a_cart_without_the_colour_flag_is_game_boy_only() {
    let d = tempfile::tempdir().unwrap();
    let rom = |name: &str, cgb: u8| {
        let mut bytes = vec![0u8; 0x8000];
        bytes[0x143] = cgb;
        let p = d.path().join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    };
    assert!(slot::core::dmg_only(&rom("dmg.gb", 0x00)));
    assert!(!slot::core::dmg_only(&rom("dual.gbc", 0x80)));
    assert!(!slot::core::dmg_only(&rom("cgb.gbc", 0xC0)));
    assert!(!slot::core::dmg_only(&d.path().join("missing.gb")));
}

#[test]
fn a_linked_game_never_takes_a_named_palette() {
    let d = tempfile::tempdir().unwrap();
    let mut bytes = vec![0u8; 0x8000];
    bytes[0x146] = 0x03;
    let rom = d.path().join("sgb.gb");
    std::fs::write(&rom, bytes).unwrap();
    let p = slot_store::GbPalette::DEFAULT;
    let gb = slot_store::Platform::Gb;
    assert_eq!(slot::core::palette_for(Some(p), gb, &rom, None), Some(p));
    for player in [0, 1] {
        assert_eq!(
            slot::core::palette_for(Some(p), gb, &rom, Some(player)),
            None,
            "player {player}: a linked pair would run two models of one cart"
        );
    }
    assert_eq!(slot::core::palette_for(None, gb, &rom, None), None);
    assert_eq!(
        slot::core::palette_for(Some(p), slot_store::Platform::Gba, &rom, None),
        None
    );
}

#[test]
fn mgba_always_answers_the_game_boy_player_logo() {
    let _g = common::core_lock();
    let path = dylib_for(Core::Mgba);
    if !path.exists() {
        eprintln!("no mgba dylib on this host, skipping");
        return;
    }
    for palette in [None, slot_store::GbPalette::parse("SGB 1-A")] {
        let mut core = LibretroCore::open(&path).expect("open core");
        slot::core::apply_core_options(&mut core, Core::Mgba, "auto", false, false, palette);
        let set: std::collections::HashMap<String, String> = core.options().into_iter().collect();
        assert_eq!(
            set.get("mgba_force_gbp").map(String::as_str),
            Some("ON"),
            "Pokemon Pinball Ruby and Sapphire only rumbles on a Game Boy Player"
        );
    }
}

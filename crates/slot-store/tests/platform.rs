use slot_store::Platform;

#[test]
fn every_platform_has_a_directory() {
    let names: Vec<&str> = Platform::ALL.iter().map(|p| p.dir_name()).collect();
    assert_eq!(names, vec!["GBA", "GB", "GBC", "PS1", "SNES"]);
}

#[test]
fn every_platform_is_a_shelf_of_its_own() {
    assert_eq!(
        Platform::ALL,
        [
            Platform::Gba,
            Platform::Gb,
            Platform::Gbc,
            Platform::Ps1,
            Platform::Snes
        ]
    );
    for (i, p) in Platform::ALL.iter().enumerate() {
        assert!(
            !Platform::ALL[..i].contains(p),
            "{p:?} is named twice in the ring, so two shelves would hold one platform"
        );
    }
}

#[test]
fn each_platform_takes_only_its_own_extensions() {
    assert!(Platform::Gba.accepts(std::path::Path::new("Metroid Fusion.gba")));
    assert!(!Platform::Gba.accepts(std::path::Path::new("Tetris.gb")));
    assert!(Platform::Gb.accepts(std::path::Path::new("Tetris.gb")));
    assert!(Platform::Gb.accepts(std::path::Path::new("Tetris.gbc")));
    assert!(!Platform::Gb.accepts(std::path::Path::new("Metroid Fusion.gba")));
    assert!(Platform::Gba.accepts(std::path::Path::new("Shrek.GBA")));
}

#[test]
fn the_playstation_takes_disc_images_and_nothing_else() {
    for name in ["Crash.cue", "Crash.CHD", "Final Fantasy VII.m3u", "Game.pbp", "Game.iso"] {
        assert!(Platform::Ps1.accepts(std::path::Path::new(name)), "{name}");
    }
    for name in ["Crash.bin", "Metroid Fusion.gba", "Tetris.gb"] {
        assert!(!Platform::Ps1.accepts(std::path::Path::new(name)), "{name}");
    }
    assert!(!Platform::Gba.accepts(std::path::Path::new("Crash.cue")));
}

#[test]
fn the_playstation_runs_on_pcsx_and_no_game_boy_core_runs_it() {
    use slot_store::Core;
    assert_eq!(Core::default_for(Platform::Ps1), Core::Pcsx);
    assert!(Core::Pcsx.runs(Platform::Ps1));
    assert!(!Core::Pcsx.runs(Platform::Gba));
    assert!(!Core::Mgba.runs(Platform::Ps1));
    assert!(!Core::Gpsp.runs(Platform::Ps1));
    assert_eq!(Core::Pcsx.as_str(), "pcsx_rearmed");
    assert_eq!(Core::parse("pcsx_rearmed"), Some(Core::Pcsx));
}

#[test]
fn the_super_nintendo_takes_its_rom_extensions_and_runs_on_snes9x() {
    use slot_store::Core;
    for name in ["Zelda.sfc", "Mario World.SMC", "Game.swc", "Game.fig"] {
        assert!(Platform::Snes.accepts(std::path::Path::new(name)), "{name}");
    }
    assert!(!Platform::Snes.accepts(std::path::Path::new("Zelda.gba")));
    assert_eq!(Core::default_for(Platform::Snes), Core::Snes9x);
    assert!(Core::Snes9x.runs(Platform::Snes));
    assert!(!Core::Snes9x.runs(Platform::Gba));
    assert!(!Core::Mgba.runs(Platform::Snes));
    assert_eq!(Core::Snes9x.as_str(), "snes9x");
}

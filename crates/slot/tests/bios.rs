mod common;

use std::path::PathBuf;

use common::core_lock;
use slot::core::open_core_for;
use slot_retro::{ButtonMask, LibretroCore};
use slot_store::Core;
use tempfile::tempdir;

fn vendored_core_paths() -> Vec<PathBuf> {
    common::vendored_core().into_iter().collect()
}

#[test]
fn a_missing_bios_folder_still_boots_a_core() {
    let _g = core_lock();
    let d = common::tmp_root_with_real_carts(&["Emerald"]);
    std::fs::remove_dir_all(d.path().join("BIOS")).ok();
    let mut core = open_core_for(d.path(), Core::Mgba, "auto", &vendored_core_paths());
    core.load(&d.path().join("Games/GBA/Emerald.gba")).unwrap();
    core.run_frame(ButtonMask::default());
}

#[test]
fn an_empty_bios_folder_still_boots_a_core() {
    let _g = core_lock();
    let d = common::tmp_root_with_real_carts(&["Emerald"]);
    std::fs::create_dir_all(d.path().join("BIOS")).unwrap();
    let mut core = open_core_for(d.path(), Core::Mgba, "auto", &vendored_core_paths());
    core.load(&d.path().join("Games/GBA/Emerald.gba")).unwrap();
    core.run_frame(ButtonMask::default());
}

#[test]
fn the_core_is_told_the_bios_folder_not_the_dylib_folder() {
    let _g = core_lock();
    let d = tempdir().unwrap();
    let bios = d.path().join("BIOS");
    let saves = d.path().join("Saves");
    std::fs::create_dir_all(&bios).unwrap();
    std::fs::create_dir_all(&saves).unwrap();
    let Some(dylib) = common::vendored_core() else {
        return;
    };
    let core = LibretroCore::open_with(&dylib, &bios, &saves).unwrap();
    assert_eq!(core.reported_system_dir(), bios.to_string_lossy());
    assert_ne!(
        core.reported_system_dir(),
        dylib.parent().unwrap().to_string_lossy()
    );
}

/// The save directory is the other half of the same wiring, and pointing it at the dylib
/// would scatter `.sav` files next to the core instead of into the content root.
#[test]
fn the_core_is_told_the_saves_folder_too() {
    let _g = core_lock();
    let d = tempdir().unwrap();
    let bios = d.path().join("BIOS");
    let saves = d.path().join("Saves");
    std::fs::create_dir_all(&bios).unwrap();
    std::fs::create_dir_all(&saves).unwrap();
    let Some(dylib) = common::vendored_core() else {
        return;
    };
    let core = LibretroCore::open_with(&dylib, &bios, &saves).unwrap();
    assert_eq!(core.reported_save_dir(), saves.to_string_lossy());
}

/// What turns the boot splash on, and what must not. gpSP reads exactly 16 KB into its BIOS
/// image without checking the length, then rejects the result only on its first byte — so a
/// file that fails either test is one gpSP would quietly replace with its built-in BIOS,
/// leaving a cart booting through a BIOS with no logo and no chime to play.
///
/// The "real" image here is 16 KB of nothing with the one byte set that every dump starts
/// with. Nothing copyrighted is needed to prove the frontend asks the right question, and
/// nothing copyrighted may be checked in.
#[test]
fn only_a_real_bios_image_turns_the_boot_splash_on() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    let bios = d.path().join("BIOS").join("gba_bios.bin");

    assert!(
        !slot::root::has_real_bios(d.path()),
        "an empty BIOS folder counted as a BIOS"
    );

    for (bytes, what) in [
        (vec![], "a zero byte file"),
        (vec![0x18u8; 1024], "a truncated image, right first byte"),
        (vec![0x18u8; 16 * 1024 - 1], "one byte short of an image"),
        (vec![0x18u8; 16 * 1024 + 1], "one byte past an image"),
        (vec![0u8; 16 * 1024], "16 KB that starts like nothing"),
        (vec![0xffu8; 16 * 1024], "16 KB of erased flash"),
    ] {
        std::fs::write(&bios, &bytes).unwrap();
        assert!(
            !slot::root::has_real_bios(d.path()),
            "{what} counted as a BIOS: gpSP would fall back to its built-in one and boot \
             through a splash that does not exist"
        );
    }

    let mut real = vec![0u8; 16 * 1024];
    real[0] = 0x18;
    std::fs::write(&bios, &real).unwrap();
    assert!(
        slot::root::has_real_bios(d.path()),
        "a 16 KB image starting 0x18 is what gpSP itself accepts, and it was refused"
    );
}

/// The card a cart was inserted from may have no BIOS folder at all — `ensure` makes one, but
/// a card pulled mid-session or mounted read only need not have it.
#[test]
fn a_missing_bios_folder_turns_the_boot_splash_off() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    std::fs::remove_dir_all(d.path().join("BIOS")).unwrap();
    assert!(
        !slot::root::has_real_bios(d.path()),
        "a missing BIOS folder counted as a BIOS"
    );
}

/// A card that has never held slot. has none of the six folders, and every write path
/// below assumes its own is already there.
#[test]
fn boot_creates_the_six_content_folders() {
    let d = tempdir().unwrap();
    let _ = slot::app::App::boot(d.path());
    for sub in ["BIOS", "Games", "Labels", "Saves", "States", "System"] {
        assert!(d.path().join(sub).is_dir(), "{sub} was not created");
    }
}

#[test]
fn the_content_root_has_no_art_directory() {
    assert!(slot::root::DIRS.contains(&"Labels"));
    assert!(
        !slot::root::DIRS.contains(&"Art"),
        "Art survived the rename"
    );
}

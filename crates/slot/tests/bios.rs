mod common;

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use slot::core::open_core_for;
use slot_retro::{ButtonMask, MgbaCore};
use tempfile::tempdir;

/// A libretro core keeps its machine in dylib globals, so two live cores is not a
/// configuration the tests may reach.
static CORE_LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    CORE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn vendored_core_paths() -> Vec<PathBuf> {
    common::vendored_core().into_iter().collect()
}

#[test]
fn a_missing_bios_folder_still_boots_a_core() {
    let _g = lock();
    let d = common::tmp_root_with_real_carts(&["Emerald"]);
    std::fs::remove_dir_all(d.path().join("BIOS")).ok();
    let mut core = open_core_for(d.path(), &vendored_core_paths());
    core.load(&d.path().join("Games/Emerald.gba")).unwrap();
    core.run_frame(ButtonMask::default());
}

#[test]
fn an_empty_bios_folder_still_boots_a_core() {
    let _g = lock();
    let d = common::tmp_root_with_real_carts(&["Emerald"]);
    std::fs::create_dir_all(d.path().join("BIOS")).unwrap();
    let mut core = open_core_for(d.path(), &vendored_core_paths());
    core.load(&d.path().join("Games/Emerald.gba")).unwrap();
    core.run_frame(ButtonMask::default());
}

#[test]
fn the_core_is_told_the_bios_folder_not_the_dylib_folder() {
    let _g = lock();
    let d = tempdir().unwrap();
    let bios = d.path().join("BIOS");
    let saves = d.path().join("Saves");
    std::fs::create_dir_all(&bios).unwrap();
    std::fs::create_dir_all(&saves).unwrap();
    let Some(dylib) = common::vendored_core() else {
        return;
    };
    let core = MgbaCore::open_with(&dylib, &bios, &saves).unwrap();
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
    let _g = lock();
    let d = tempdir().unwrap();
    let bios = d.path().join("BIOS");
    let saves = d.path().join("Saves");
    std::fs::create_dir_all(&bios).unwrap();
    std::fs::create_dir_all(&saves).unwrap();
    let Some(dylib) = common::vendored_core() else {
        return;
    };
    let core = MgbaCore::open_with(&dylib, &bios, &saves).unwrap();
    assert_eq!(core.reported_save_dir(), saves.to_string_lossy());
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

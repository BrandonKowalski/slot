use std::path::{Path, PathBuf};

/// The six top level folders of a content root. A card that has never held slot. has none
/// of them, and every write path below assumes its own is already there.
pub const DIRS: [&str; 7] = [
    "BIOS",
    "Games",
    "Labels",
    "Saves",
    "States",
    "System",
    "Wallpapers",
];

/// Best effort: an unmounted or read only card is an empty shelf, not a boot failure.
pub fn ensure(root: &Path) {
    for sub in DIRS {
        let _ = std::fs::create_dir_all(root.join(sub));
    }
}

/// Reported to the core as the libretro system directory. `gba_bios.bin` present means the
/// real BIOS, absent means mGBA's HLE BIOS. Neither is an error.
pub fn bios_dir(root: &Path) -> PathBuf {
    root.join("BIOS")
}

pub fn saves_dir(root: &Path) -> PathBuf {
    root.join("Saves")
}

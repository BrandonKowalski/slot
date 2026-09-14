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

/// Bring a card written before states were namespaced up to the current layout. Best
/// effort on purpose: a read only or half mounted card is an empty shelf, not a boot
/// failure, exactly as `ensure` treats it.
///
/// A per-entry failure does not stop the sweep — the rest of the shelf still gets a chance —
/// but silently eating every one of them would leave a cart stuck pre-migration forever with
/// nothing on the card to say so. Logged here, once per boot, rather than inside
/// `migrate_states` itself, which only counts and has no read on where "once per boot" ends.
pub fn migrate(root: &Path) {
    match slot_store::migrate_states(root) {
        Ok(report) if report.failed > 0 => {
            eprintln!(
                "slot: migrate: {} of {} state director{} did not move",
                report.failed,
                report.moved + report.failed,
                if report.moved + report.failed == 1 {
                    "y"
                } else {
                    "ies"
                }
            );
        }
        _ => {}
    }
}

/// Reported to the core as the libretro system directory. `gba_bios.bin` present means the
/// real BIOS, absent means mGBA's HLE BIOS. Neither is an error.
pub fn bios_dir(root: &Path) -> PathBuf {
    root.join("BIOS")
}

/// What the card calls the BIOS. Both cores look for this name and nothing else.
const BIOS_FILE: &str = "gba_bios.bin";

/// A GBA BIOS is 16 KB, and its first byte is the low byte of the entry branch every dump of
/// it opens with (`EA000018`, little endian, so 0x18 first).
const BIOS_BYTES: u64 = 16 * 1024;
const BIOS_FIRST_BYTE: u8 = 0x18;

/// Whether the card carries a real GBA BIOS, rather than nothing or merely a file by that
/// name. What decides whether gpSP is asked to boot through it (see `core::apply_core_options`).
///
/// Cheap on purpose — one open, one stat, one byte — because this is asked on every core load,
/// which is every insert and every reload for a link.
///
/// The two things checked are the two gpSP itself depends on. It reads exactly 16 KB into its
/// BIOS image with no length check of its own, so a short file leaves the rest of that image
/// as whatever was there; and it then rejects the image outright, falling back to its built-in
/// BIOS, when the first byte is not 0x18. Asking the same question here is what keeps "slot
/// turned the splash on" and "gpSP actually booted the official BIOS" from disagreeing: when
/// they disagree the player gets the built-in BIOS booted through, which is a blank pause
/// rather than the logo they were promised.
///
/// Deliberately not a checksum. It would read all 16 KB on every insert to buy no more
/// certainty than gpSP itself demands, and it would turn the feature off for anyone holding a
/// regional dump other than whichever hash got written down here.
pub fn has_real_bios(root: &Path) -> bool {
    let Ok(mut f) = std::fs::File::open(bios_dir(root).join(BIOS_FILE)) else {
        return false;
    };
    if !f.metadata().is_ok_and(|m| m.len() == BIOS_BYTES) {
        return false;
    }
    let mut first = [0u8; 1];
    std::io::Read::read_exact(&mut f, &mut first).is_ok() && first[0] == BIOS_FIRST_BYTE
}

pub fn saves_dir(root: &Path) -> PathBuf {
    root.join("Saves")
}

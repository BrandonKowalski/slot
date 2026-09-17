use std::path::{Path, PathBuf};

/// The folders of a content root, including a platform subdirectory under each of the four
/// folders whose contents are filed per platform, so `ensure` creates them and the card teaches
/// its own layout to someone dropping files in over USB.
///
/// The card teaching its own layout is now the whole of the guidance there is. slot used to
/// sweep loose files into these folders on every boot; it does not any more, so an empty folder
/// with the right name is the only thing on the card that says where a file belongs, and every
/// folder a person has to put something in has to be here.
///
/// `Saves/` and `States/` are on that list for the first time, and the reason they were kept off
/// it went out with the sweep. The old rule was who places a file: a person places a rom and a
/// piece of label art by hand and needs somewhere to put each that names a platform — a `.gb` and
/// a `.gba` cart may share a stem, so `Tetris.png` alone does not say which cart it is the face
/// of — whereas nobody hand-placed a battery save or a save state, because slot wrote both and
/// the sweep moved whatever was already there. Now that a card is organised by hand, a person
/// bringing an old card across carries their own `.sav` files and their own `States/<core>/`
/// trees over, and those two folders need to say where they go as much as `Games/` does.
///
/// `States/` goes one level deeper than this — `States/<platform>/<core>/<stem>/` — and the core
/// level is deliberately not scaffolded: slot creates it on first write, and someone moving an
/// old card's `States/mgba/` wholesale into `States/GBA/` lands on exactly the right shape
/// without having to be told the core's spelling.
///
/// A card that has never held slot. has none of them, and every write path below assumes its
/// own is already there.
///
/// Parents come before their children: `ensure` creates each in turn, and so does the test
/// harness's own root.
pub const DIRS: [&str; 19] = [
    "BIOS",
    "Games",
    "Games/GBA",
    "Games/GB",
    "Games/GBC",
    "Labels",
    "Labels/GBA",
    "Labels/GB",
    "Labels/GBC",
    "Saves",
    "Saves/GBA",
    "Saves/GB",
    "Saves/GBC",
    "States",
    "States/GBA",
    "States/GB",
    "States/GBC",
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

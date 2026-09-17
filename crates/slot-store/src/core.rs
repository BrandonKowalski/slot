use std::collections::HashMap;
use std::path::Path;

pub const SELECTED_CORE_FILE: &str = "System/selected_core.ini";

/// Which emulator runs a cart. mGBA is the whole product's default; gpSP exists for the
/// serial hardware mGBA's libretro build does not carry.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Core {
    #[default]
    Mgba,
    Gpsp,
}

impl Core {
    /// Every variant, once. The single source of truth for "is this name a core directory" —
    /// `migrate_states` walks this rather than spelling the variant list out a second time,
    /// so a third core added here does not also have to be remembered at every call site
    /// that needs to tell a core's own directory apart from a cart's.
    pub const ALL: [Core; 2] = [Core::Mgba, Core::Gpsp];

    pub fn as_str(&self) -> &'static str {
        match self {
            Core::Mgba => "mgba",
            Core::Gpsp => "gpsp",
        }
    }

    /// Position in `ALL`, which is the order the picker's rows and their faces are in.
    pub fn index(self) -> usize {
        self as usize
    }

    /// What the picker calls it. Not `as_str`: that is the ini's spelling, meant to be typed
    /// by hand into a text editor on a computer, and this is the player's, meant to be read
    /// off a panel. The two are free to differ, and already do.
    pub fn text(self) -> &'static str {
        match self {
            Core::Mgba => "mGBA",
            Core::Gpsp => "gpSP",
        }
    }

    pub fn parse(s: &str) -> Option<Core> {
        match s.trim().to_ascii_lowercase().as_str() {
            "mgba" => Some(Core::Mgba),
            "gpsp" => Some(Core::Gpsp),
            _ => None,
        }
    }
}

/// `<rom stem> = <core>`, one per line — `crate::ini`'s shape, and every rule about hand-edited
/// files that goes with it lives there. This is only the value type on top: a name we do not
/// know is dropped rather than raised, because it is a card written for a newer build, or a
/// typo, and either way the default is the safe reading.
pub fn read_selected_cores(root: &Path) -> HashMap<String, Core> {
    crate::ini::read(root, SELECTED_CORE_FILE)
        .into_iter()
        .filter_map(|(stem, name)| Core::parse(&name).map(|core| (stem, core)))
        .collect()
}

/// The core one cart wants, or the default for a cart the file does not name — which is also
/// what a cart whose line nobody can parse gets.
pub fn core_for(root: &Path, stem: &str) -> Core {
    crate::ini::value(root, SELECTED_CORE_FILE, stem)
        .as_deref()
        .and_then(Core::parse)
        .unwrap_or_default()
}

/// Set one cart's core, leaving the rest of the file exactly as it was.
pub fn write_selected_core(root: &Path, stem: &str, core: Core) -> std::io::Result<()> {
    crate::ini::write(root, SELECTED_CORE_FILE, stem, core.as_str())
}

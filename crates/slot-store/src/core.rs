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
    pub fn as_str(&self) -> &'static str {
        match self {
            Core::Mgba => "mgba",
            Core::Gpsp => "gpsp",
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

/// `<rom stem> = <core>`, one per line. Keyed on the stem because that is already the key
/// for `Labels/`, `Saves/` and `States/`; a card stays consistent with itself.
///
/// Every malformed line is skipped rather than raised. This file is edited by hand on a
/// card, and the cost of a typo must be that one cart opens with the default core, never
/// that the shelf fails to load.
pub fn read_selected_cores(root: &Path) -> HashMap<String, Core> {
    let mut out = HashMap::new();
    let Ok(text) = std::fs::read_to_string(root.join(SELECTED_CORE_FILE)) else {
        return out;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with(';')
            || line.starts_with('[')
        {
            continue;
        }
        let Some((stem, core)) = line.split_once('=') else {
            continue;
        };
        let stem = stem.trim();
        if stem.is_empty() {
            continue;
        }
        // A name we do not know is not a failure: it is a card written for a newer build,
        // or a typo. Either way the default is the safe reading.
        if let Some(core) = Core::parse(core) {
            out.insert(stem.to_string(), core);
        }
    }
    out
}

/// The core one cart wants. Reads the file each time: it is a few lines on a card that a
/// person edits between boots, and caching it would only create a staleness question
/// nobody asked for.
pub fn core_for(root: &Path, stem: &str) -> Core {
    read_selected_cores(root)
        .get(stem)
        .copied()
        .unwrap_or_default()
}

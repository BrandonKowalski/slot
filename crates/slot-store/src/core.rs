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

/// Set one cart's core, leaving the rest of the file exactly as it was.
///
/// The line is replaced in place, or appended when the cart has no line yet. The file is
/// never rebuilt from `read_selected_cores`' map: it is meant to be opened in a text editor
/// on a computer, and a rebuild would quietly drop every comment, blank line and unparsed
/// line in it — including the note somebody wrote to themselves above a cart.
pub fn write_selected_core(root: &Path, stem: &str, core: Core) -> std::io::Result<()> {
    let path = root.join(SELECTED_CORE_FILE);
    let existing = std::fs::read_to_string(&path).unwrap_or_default();

    let entry = format!("{stem} = {}", core.as_str());
    let mut out = String::with_capacity(existing.len() + entry.len() + 1);
    let mut replaced = false;

    for line in existing.lines() {
        let is_this_cart = line
            .split_once('=')
            .map(|(k, _)| k.trim() == stem)
            .unwrap_or(false);
        if is_this_cart && !replaced {
            out.push_str(&entry);
            replaced = true;
        } else if is_this_cart {
            // A duplicate for the same cart: the later line already won when read, so
            // dropping it keeps the file saying one thing per cart.
            continue;
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    if !replaced {
        out.push_str(&entry);
        out.push('\n');
    }

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::atomic::atomic_write(&path, out.as_bytes())
}

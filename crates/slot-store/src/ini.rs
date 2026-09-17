//! `<key> = <value>`, one per line, under `System/`. The shape every per-cart preference the
//! card keeps is written in, at the layer that knows nothing about what a value means.
//!
//! Keyed on the rom stem, because that is already the key for `Labels/`, `Saves/` and
//! `States/`; a card stays consistent with itself. Nothing here requires that, though — the
//! key is whatever string the caller hands over.
//!
//! Two rules, and both exist because a person edits these files in a text editor on a card:
//!
//! - Every malformed line is skipped rather than raised. The cost of a typo must be that one
//!   entry falls back to its default, never that the shelf fails to load.
//! - A write replaces one line in place and never rebuilds the file from the map, so every
//!   comment, blank line and unparsed line survives — including the note somebody wrote to
//!   themselves above a cart.
//!
//! `selected_core.ini` had all of this to itself and `video_mode.ini` is the second file to
//! want it. The two were within a value type of being the same eighty lines, and two
//! hand-copied parsers is how two files meant to behave identically start to differ.

use std::collections::HashMap;
use std::path::Path;

/// Every key the file names, with its value trimmed. Read fresh on every call: these are a few
/// lines on a card that a person edits between boots, and caching them would only create a
/// staleness question nobody asked for.
///
/// An empty value is kept rather than dropped. What an empty value means — a default, a
/// deliberate blank, a typo — is a question about the value's type, and this layer does not
/// know the type.
pub fn read(root: &Path, file: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Ok(text) = std::fs::read_to_string(root.join(file)) else {
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
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        // A later line for the same key replaces the earlier one, which is what makes the
        // file say one thing per key however many times it was written by hand.
        out.insert(key.to_string(), value.trim().to_string());
    }
    out
}

/// One key's value, or `None` when the file does not name it. The whole file is read, for the
/// same reason `read` is: it is a few lines, and a second reading rule would be a second thing
/// to keep in step.
pub fn value(root: &Path, file: &str, key: &str) -> Option<String> {
    read(root, file).remove(key)
}

/// Set one key, leaving the rest of the file exactly as it was.
///
/// The line is replaced in place, or appended when the key has none yet. See the module's own
/// comment for why the file is never rebuilt from `read`'s map.
pub fn write(root: &Path, file: &str, key: &str, value: &str) -> std::io::Result<()> {
    let path = root.join(file);
    let existing = std::fs::read_to_string(&path).unwrap_or_default();

    let entry = format!("{key} = {value}");
    let mut out = String::with_capacity(existing.len() + entry.len() + 1);
    let mut replaced = false;

    for line in existing.lines() {
        let is_this_key = line
            .split_once('=')
            .map(|(k, _)| k.trim() == key)
            .unwrap_or(false);
        if is_this_key && !replaced {
            out.push_str(&entry);
            replaced = true;
        } else if is_this_key {
            // A duplicate for the same key: the later line already won when read, so
            // dropping it keeps the file saying one thing per key.
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

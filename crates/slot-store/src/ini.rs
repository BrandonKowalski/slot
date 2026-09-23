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
        let Some((key, value)) = entry(line) else {
            continue;
        };
        // A later line for the same key replaces the earlier one, which is what makes the
        // file say one thing per key however many times it was written by hand.
        out.insert(key.to_string(), value.to_string());
    }
    out
}

/// What one line names, or `None` for a line that names nothing: blank, a comment, a section
/// header, anything with no `=`, and anything whose key is empty.
///
/// The one rule, read by both `read` and `write`, so the two cannot come to disagree about
/// which line belongs to which key. They did: `write` matched a line by splitting on `=` with
/// none of the rules above applied, so it could claim a line `read` would never hand back, and
/// could append a line it would then never find again on the next write.
fn entry(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') || line.starts_with(';') || line.starts_with('[') {
        return None;
    }
    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    (!key.is_empty()).then_some((key, value.trim()))
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
///
/// A key this format cannot say is refused rather than written. Not every string survives a
/// trip through `entry` above: a stem with a space at either end comes back trimmed, one with
/// an `=` in it comes back cut at the `=`, and one starting `#`, `;` or `[` comes back as a
/// comment — and every one of those is a filename somebody can really put in `Games/`. Writing
/// them anyway did three things, and only the first was harmless. The preference never
/// persisted, because the line could not be found again. Every write appended another copy, so
/// the card's file grew by a line on every press with nothing ever reading any of them. And a
/// cart named `Cheats` claimed the line belonging to a cart named `Cheats = On` and destroyed
/// it — one cart's preference deleting another's.
///
/// So the caller gets an error, which every one of them already logs, and the file on the card
/// stays exactly as it was. What such a cart cannot do is keep a preference; making it able to
/// would mean quoting or escaping, and that changes the shape of a file people hand-edit.
pub fn write(root: &Path, file: &str, key: &str, value: &str) -> std::io::Result<()> {
    let line = format!("{key} = {value}");
    // Asked of `entry` itself rather than by restating its rules here, which is what stops this
    // check and the parser it is checking against drifting apart. `lines` catches the one thing
    // `entry` cannot see: a newline anywhere in either half would make this one entry two.
    if line.lines().count() != 1 || entry(&line) != Some((key, value)) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{file} cannot hold the entry {line:?}"),
        ));
    }

    let path = root.join(file);
    // A file that is not there is an empty one; a file that is there and will not read is not.
    // `unwrap_or_default` treated the two the same, so a card whose ini had been saved in
    // anything but UTF-8 — Notepad's ANSI default is enough, and a European rom set puts an
    // accent in a stem sooner or later — had the whole file replaced by this one line on the
    // next press. Every other write on the card refuses to destroy what it cannot account for:
    // `write_sav` will not shrink a save and `retire_resume` renames rather than deletes. This
    // one now does too, and the caller, which already logs a failed write, is told why.
    let existing = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };

    let entry_line = line;
    let mut out = String::with_capacity(existing.len() + entry_line.len() + 1);
    let mut replaced = false;

    for line in existing.lines() {
        let is_this_key = entry(line).is_some_and(|(k, _)| k == key);
        if is_this_key && !replaced {
            out.push_str(&entry_line);
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
        out.push_str(&entry_line);
        out.push('\n');
    }

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::atomic::atomic_write(&path, out.as_bytes())
}

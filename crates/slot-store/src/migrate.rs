use std::path::Path;

use crate::core::Core;

/// Move pre-namespacing state directories under `States/mgba/`.
///
/// States used to live at `States/<stem>/`, from before a card could hold more than one
/// core. Anything directly under `States/` that is not itself a core directory is one of
/// those, and belongs to mGBA because mGBA is what wrote it.
///
/// Safe to call on every boot: once a card is migrated there is nothing left that matches,
/// so the second call walks the same directory and moves nothing. Safe to call after an
/// interrupted run for the same reason — the carts that already moved no longer match.
pub fn migrate_states(root: &Path) -> std::io::Result<usize> {
    let states = root.join("States");
    let dir = match std::fs::read_dir(&states) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e),
    };

    let known: Vec<&str> = [Core::Mgba, Core::Gpsp]
        .iter()
        .map(|c| c.as_str())
        .collect();
    let mut moved = 0;

    // Collected before anything moves. Renaming entries out of a directory while iterating
    // that same directory is unspecified, and this one is on a user's card.
    let entries: Vec<_> = dir.collect::<Result<Vec<_>, _>>()?;

    for entry in entries {
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if known.contains(&name) {
            continue;
        }

        let dest = states.join(Core::Mgba.as_str()).join(name);
        // Never clobber. A collision means someone has already played this cart under the
        // new layout, and their newer states outrank the old ones; leaving the bare copy
        // in place loses nothing and keeps the situation visible on the card.
        if dest.exists() {
            continue;
        }
        std::fs::create_dir_all(dest.parent().expect("dest has a parent"))?;
        std::fs::rename(entry.path(), &dest)?;
        moved += 1;
    }
    Ok(moved)
}

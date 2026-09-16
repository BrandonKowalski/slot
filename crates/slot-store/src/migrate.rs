use std::path::Path;

use crate::core::Core;
use crate::platform::Platform;

/// What one `migrate_states` call did to a card. `failed` is what lets a caller decide
/// whether there is anything worth logging: an ordinary boot sees `moved == 0, failed == 0`
/// and has nothing to say, but a nonzero `failed` means some directory under `States/` needs
/// a person's attention — the card is read-only, or something under it is not what this
/// expects — and the boot call site is the only place that can put that on the record.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MigrationReport {
    pub moved: usize,
    pub failed: usize,
}

/// Move pre-namespacing state directories under `States/mgba/`.
///
/// States used to live at `States/<stem>/`, from before a card could hold more than one
/// core. Anything directly under `States/` that is neither a core directory nor a **platform**
/// directory is one of those, and belongs to mGBA because mGBA is what wrote it.
///
/// The platform half of that test is not decoration. `States/GB/` is a directory at exactly
/// this level whose name is not a core, so without it the sweep renames every Game Boy save
/// state into `States/mgba/GB/` on the first boot after Game Boy support ships — silently,
/// because this function is best-effort and reports only a count.
///
/// Safe to call on every boot: once a card is migrated there is nothing left that matches,
/// so the second call walks the same directory and moves nothing. Safe to call after an
/// interrupted run for the same reason — the carts that already moved no longer match.
pub fn migrate_states(root: &Path) -> std::io::Result<MigrationReport> {
    let states = root.join("States");
    let dir = match std::fs::read_dir(&states) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(MigrationReport::default()),
        Err(e) => return Err(e),
    };

    let known: Vec<&str> = Core::ALL
        .iter()
        .map(|c| c.as_str())
        .chain(Platform::ALL.iter().map(|p| p.dir_name()))
        .collect();
    let mut report = MigrationReport::default();

    // Collected before anything moves. Renaming entries out of a directory while iterating
    // that same directory is unspecified, and this one is on a user's card.
    let entries: Vec<_> = dir.collect::<Result<Vec<_>, _>>()?;

    for entry in entries {
        // Every fallible step from here on is isolated to this one entry rather than
        // propagated with `?`. `read_dir` above is the one place a hard `Err` is right,
        // because there is nothing left to iterate at all. Once inside the loop, `read_dir`
        // order is stable, so letting one entry's failure — a corrupt subdirectory, a
        // permission bit, `States/mgba` existing as a plain file — abort the whole call
        // would strand every cart that sorts after it on every future boot, which is
        // exactly the "safe to run on every boot" guarantee this function exists to keep.
        // A cart this loop cannot move this boot is still there, unharmed, to try again on
        // the next one. Each isolated failure still counts, though: unlike a deliberate skip
        // (not a directory, already a core directory, a name collision), it is a cart that
        // should have moved and did not.
        let Ok(file_type) = entry.file_type() else {
            report.failed += 1;
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            report.failed += 1;
            continue;
        };
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
        let dest_parent = dest.parent().expect("dest has a parent");
        if std::fs::create_dir_all(dest_parent).is_err() {
            report.failed += 1;
            continue;
        }
        if std::fs::rename(entry.path(), &dest).is_ok() {
            report.moved += 1;
        } else {
            report.failed += 1;
        }
    }
    Ok(report)
}

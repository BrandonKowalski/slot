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

impl MigrationReport {
    fn add(&mut self, other: MigrationReport) {
        self.moved += other.moved;
        self.failed += other.failed;
    }
}

/// Sweep everything loose into its platform folder. Nothing stays loose: after this runs,
/// `Games/`, `Saves/` and `Labels/` hold platform directories and no files of their own, and
/// `States/` holds platform directories each holding the core directories that used to sit at
/// its top level.
///
/// No per-file classification, no header sniffing, no matching of saves to ROMs: every card
/// written before Game Boy support is entirely loose and entirely GBA, because no shipped build
/// of slot could run anything else. Loose goes to `GBA/`, and that is the whole rule.
///
/// **Must run after `migrate_states`.** Reversed, this would sweep a pre-namespacing
/// `States/<stem>/` into `States/GBA/<stem>/`, where it is a cart folder sitting where a core
/// folder belongs and the other sweep will never look at it again.
///
/// Safe on every boot, for the same reason `migrate_states` is: once an entry has moved it no
/// longer matches, so a second call walks the same directories and moves nothing. Safe after an
/// interrupted run too — `rename` within a filesystem either moves an entry or does not, so
/// there is no state in which a save is half-moved.
pub fn migrate_platforms(root: &Path) -> std::io::Result<MigrationReport> {
    let mut report = MigrationReport::default();
    for dir in ["Games", "Saves", "Labels"] {
        report.add(sweep_files(&root.join(dir))?);
    }
    report.add(sweep_state_cores(&root.join("States"))?);
    Ok(report)
}

/// Loose files in one directory into its `GBA/` subdirectory. Directories at this level are the
/// platform folders themselves and are left alone.
fn sweep_files(dir: &Path) -> std::io::Result<MigrationReport> {
    let mut report = MigrationReport::default();
    let entries = match std::fs::read_dir(dir) {
        Ok(d) => d.collect::<Result<Vec<_>, _>>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(report),
        Err(e) => return Err(e),
    };
    let dest_dir = dir.join(Platform::Gba.dir_name());
    for entry in entries {
        let Ok(file_type) = entry.file_type() else {
            report.failed += 1;
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name();
        // A leading dot is card metadata rather than content, and every folder on the card is
        // read through this rule already.
        if crate::is_hidden(Path::new(&name)) {
            continue;
        }
        let dest = dest_dir.join(&name);
        // Never clobber. A destination that exists is a file an earlier run already moved, or
        // one the user put there; either outranks the loose copy, and leaving that copy in
        // place loses nothing and keeps the situation visible on the card.
        if dest.exists() {
            continue;
        }
        if std::fs::create_dir_all(&dest_dir).is_err() {
            report.failed += 1;
            continue;
        }
        match std::fs::rename(entry.path(), &dest) {
            Ok(()) => report.moved += 1,
            Err(_) => report.failed += 1,
        }
    }
    Ok(report)
}

/// The core directories under `States/` into `States/GBA/`, which is what produces the
/// `States/<platform>/<core>/<stem>/` shape.
fn sweep_state_cores(states: &Path) -> std::io::Result<MigrationReport> {
    let mut report = MigrationReport::default();
    let entries = match std::fs::read_dir(states) {
        Ok(d) => d.collect::<Result<Vec<_>, _>>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(report),
        Err(e) => return Err(e),
    };
    let dest_dir = states.join(Platform::Gba.dir_name());
    for entry in entries {
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
        // Only a core directory moves. A platform directory is already where it belongs, and
        // anything else is `migrate_states`' business — it runs first, so by the time this is
        // reached there is nothing else left at this level.
        if !Core::ALL.iter().any(|c| c.as_str() == name) {
            continue;
        }
        let dest = dest_dir.join(name);
        if dest.exists() {
            continue;
        }
        if std::fs::create_dir_all(&dest_dir).is_err() {
            report.failed += 1;
            continue;
        }
        match std::fs::rename(entry.path(), &dest) {
            Ok(()) => report.moved += 1,
            Err(_) => report.failed += 1,
        }
    }
    Ok(report)
}

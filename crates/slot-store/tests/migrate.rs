use slot_store::{migrate_states, Core, StateRing};
use tempfile::tempdir;

fn card() -> tempfile::TempDir {
    let d = tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("States")).unwrap();
    d
}

fn bare_state(root: &std::path::Path, stem: &str) {
    let dir = root.join("States").join(stem);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("2026-08-01_00-00-00.state"), b"old").unwrap();
    std::fs::write(dir.join("resume.state"), b"resume").unwrap();
}

fn set_mode(dir: &std::path::Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode)).expect("chmod");
}

/// Root ignores the permission bits `set_mode` relies on to make a directory unwritable, so
/// a CI container running as root would pass the rename-failure test below without ever
/// exercising the failure it exists to catch.
fn running_as_root() -> bool {
    #[cfg(unix)]
    {
        extern "C" {
            fn geteuid() -> u32;
        }
        unsafe { geteuid() == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

#[test]
fn a_pre_migration_card_moves_under_mgba() {
    let d = card();
    bare_state(d.path(), "Emerald");
    bare_state(d.path(), "Metroid Fusion");

    assert_eq!(migrate_states(d.path()).unwrap().moved, 2);

    assert!(!d.path().join("States/Emerald").exists());
    assert_eq!(
        std::fs::read(d.path().join("States/mgba/Emerald/resume.state")).unwrap(),
        b"resume"
    );
    // And the ring can see them, which is the only reason to move them at all.
    assert_eq!(
        StateRing::new(d.path(), Core::Mgba, "Emerald")
            .list()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn running_twice_changes_nothing() {
    let d = card();
    bare_state(d.path(), "Emerald");
    assert_eq!(migrate_states(d.path()).unwrap().moved, 1);
    assert_eq!(migrate_states(d.path()).unwrap().moved, 0, "not idempotent");
    assert_eq!(
        std::fs::read(d.path().join("States/mgba/Emerald/resume.state")).unwrap(),
        b"resume"
    );
}

/// An interrupted first run leaves some carts moved and some not. The second run has to
/// finish the job rather than trip over the half that is already done.
#[test]
fn a_half_finished_migration_resumes() {
    let d = card();
    bare_state(d.path(), "Emerald");
    std::fs::create_dir_all(d.path().join("States/mgba/Metroid Fusion")).unwrap();
    std::fs::write(
        d.path().join("States/mgba/Metroid Fusion/resume.state"),
        b"already",
    )
    .unwrap();

    assert_eq!(migrate_states(d.path()).unwrap().moved, 1);
    assert!(d.path().join("States/mgba/Emerald/resume.state").exists());
    assert_eq!(
        std::fs::read(d.path().join("States/mgba/Metroid Fusion/resume.state")).unwrap(),
        b"already",
        "an already migrated cart was disturbed"
    );
}

/// A cart whose name collides with one already under mgba must not clobber it. Keeping the
/// bare copy in place loses nothing and leaves the situation visible.
#[test]
fn a_collision_leaves_both_alone() {
    let d = card();
    bare_state(d.path(), "Emerald");
    std::fs::create_dir_all(d.path().join("States/mgba/Emerald")).unwrap();
    std::fs::write(d.path().join("States/mgba/Emerald/resume.state"), b"kept").unwrap();

    let report = migrate_states(d.path()).unwrap();
    assert_eq!(report.moved, 0);
    assert_eq!(report.failed, 0, "a collision is expected, not a failure");
    assert_eq!(
        std::fs::read(d.path().join("States/mgba/Emerald/resume.state")).unwrap(),
        b"kept"
    );
    assert!(
        d.path().join("States/Emerald").exists(),
        "the bare copy was destroyed"
    );
}

#[test]
fn a_card_with_no_states_dir_is_fine() {
    let d = tempdir().unwrap();
    assert_eq!(migrate_states(d.path()).unwrap().moved, 0);
}

#[test]
fn core_directories_are_not_themselves_migrated() {
    let d = card();
    std::fs::create_dir_all(d.path().join("States/gpsp/Emerald")).unwrap();
    assert_eq!(migrate_states(d.path()).unwrap().moved, 0);
    assert!(d.path().join("States/gpsp/Emerald").is_dir());
}

/// I1: `known` used to spell out `[Core::Mgba, Core::Gpsp]` by hand inside `migrate_states`.
/// A third core added later without also touching that line would have its own
/// `States/<newcore>/` folded into `States/mgba/<newcore>/` on the very first boot that knew
/// the variant, hiding every cart on that core until someone went looking for it there.
/// `Core::ALL` makes the list derive from the enum, so this walks `Core::ALL` rather than
/// repeating it, and covers whatever core is registered without being edited again.
#[test]
fn every_registered_cores_directory_is_left_alone() {
    let d = card();
    for core in Core::ALL {
        std::fs::create_dir_all(d.path().join("States").join(core.as_str()).join("Emerald"))
            .unwrap();
    }

    let report = migrate_states(d.path()).unwrap();

    assert_eq!(report.moved, 0);
    for core in Core::ALL {
        assert!(
            d.path()
                .join("States")
                .join(core.as_str())
                .join("Emerald")
                .is_dir(),
            "{}'s own directory was migrated",
            core.as_str()
        );
    }
}

/// `.DS_Store` is not hypothetical: these cards get edited on a Mac, and Finder drops one
/// into every directory it visits, including `States/`. A stray file must not stop the
/// carts that migrate fine from migrating. Created before the cart so that, if the
/// implementation ever regresses to a `?` per entry, the stray sorts first and aborts the
/// whole call rather than the failure being hidden by iteration order alone.
#[test]
fn a_stray_file_does_not_stop_a_real_cart_migrating() {
    let d = card();
    std::fs::write(d.path().join("States/.DS_Store"), b"finder junk").unwrap();
    bare_state(d.path(), "Emerald");

    assert_eq!(migrate_states(d.path()).unwrap().moved, 1);
    assert_eq!(
        std::fs::read(d.path().join("States/mgba/Emerald/resume.state")).unwrap(),
        b"resume"
    );
    assert_eq!(
        std::fs::read(d.path().join("States/.DS_Store")).unwrap(),
        b"finder junk",
        "the stray file was moved or deleted"
    );
}

/// `States/mgba` existing as a plain file is not something a healthy card produces, but a
/// corrupted one is not impossible, and it must not abort the call or destroy the cart it
/// was about to move. `create_dir_all` fails because a non-directory sits where a directory
/// is wanted; that failure is isolated to the entry that hit it, the same as any other.
#[test]
fn mgba_as_a_plain_file_fails_soft_and_leaves_the_source_alone() {
    let d = card();
    bare_state(d.path(), "Emerald");
    std::fs::write(d.path().join("States/mgba"), b"not a directory").unwrap();

    let report = migrate_states(d.path()).unwrap();
    assert_eq!(report.moved, 0);
    assert_eq!(
        report.failed, 1,
        "the blocked entry must count against the boot log"
    );
    assert_eq!(
        std::fs::read(d.path().join("States/Emerald/resume.state")).unwrap(),
        b"resume",
        "the source was disturbed"
    );
}

/// I7: only the `create_dir_all` leg had coverage before this. `create_dir_all` on a
/// directory that already exists is a bare stat and never needs write permission on its
/// parent, so making `States/mgba` itself read-only — rather than replacing it with a file —
/// leaves `create_dir_all` untouched and exercises the `rename` leg on its own: the exact
/// leg Task 3's per-entry isolation exists to cover, and the one this suite had not reached.
#[test]
fn a_read_only_mgba_directory_fails_the_rename_leg_softly() {
    if running_as_root() {
        eprintln!("running as root, where permission bits do not block a rename: skipping");
        return;
    }
    let d = card();
    bare_state(d.path(), "Emerald");
    let mgba = d.path().join("States/mgba");
    std::fs::create_dir_all(&mgba).unwrap();
    set_mode(&mgba, 0o555);

    let result = migrate_states(d.path());

    // Restored before any assertion can panic and skip it — the temp dir has to be
    // deletable when it drops, on the failure path as much as the success path.
    set_mode(&mgba, 0o755);

    let report = result.unwrap();
    assert_eq!(report.moved, 0, "the rename must not have gone through");
    assert_eq!(report.failed, 1);
    assert_eq!(
        std::fs::read(d.path().join("States/Emerald/resume.state")).unwrap(),
        b"resume",
        "the source was disturbed"
    );
}

/// The guard. `States/GB/` is a directory directly under `States/` whose name is not a core,
/// which is exactly what `migrate_states` takes for a pre-namespacing cart folder — so without
/// a skip it renames it into `States/mgba/GB/` and every Game Boy save state, resume state and
/// polaroid thumb disappears into a core directory where nothing will look for it again. No
/// error, no log, no failed boot: the sweep is best-effort and reports only a count.
///
/// This test fails loudly if the skip is ever removed while tidying. That is the point of it.
#[test]
fn platform_directories_are_not_mistaken_for_carts() {
    let d = card();
    for p in slot_store::Platform::ALL {
        let dir = d
            .path()
            .join("States")
            .join(p.dir_name())
            .join("mgba")
            .join("Tetris");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("resume.state"), b"keep").unwrap();
    }

    let report = migrate_states(d.path()).unwrap();

    assert_eq!(report.moved, 0, "a platform directory was migrated");
    for p in slot_store::Platform::ALL {
        let kept = d
            .path()
            .join("States")
            .join(p.dir_name())
            .join("mgba/Tetris/resume.state");
        assert_eq!(
            std::fs::read(&kept).unwrap(),
            b"keep",
            "{}'s states were swept into a core directory",
            p.dir_name()
        );
        assert!(
            !d.path().join("States/mgba").join(p.dir_name()).exists(),
            "{} was renamed under mgba",
            p.dir_name()
        );
    }
}

/// The skip must not cost the sweep its original job: a genuine pre-namespacing cart still
/// moves. Without this, "fix the guard" could be satisfied by disabling the sweep entirely.
#[test]
fn the_platform_skip_does_not_stop_a_real_cart_migrating() {
    let d = card();
    bare_state(d.path(), "Emerald");
    std::fs::create_dir_all(d.path().join("States/GB/mgba/Tetris")).unwrap();

    assert_eq!(migrate_states(d.path()).unwrap().moved, 1);
    assert!(d.path().join("States/mgba/Emerald/resume.state").exists());
    assert!(d.path().join("States/GB/mgba/Tetris").is_dir());
}

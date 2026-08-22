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

#[test]
fn a_pre_migration_card_moves_under_mgba() {
    let d = card();
    bare_state(d.path(), "Emerald");
    bare_state(d.path(), "Metroid Fusion");

    assert_eq!(migrate_states(d.path()).unwrap(), 2);

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
    assert_eq!(migrate_states(d.path()).unwrap(), 1);
    assert_eq!(migrate_states(d.path()).unwrap(), 0, "not idempotent");
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

    assert_eq!(migrate_states(d.path()).unwrap(), 1);
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

    assert_eq!(migrate_states(d.path()).unwrap(), 0);
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
    assert_eq!(migrate_states(d.path()).unwrap(), 0);
}

#[test]
fn core_directories_are_not_themselves_migrated() {
    let d = card();
    std::fs::create_dir_all(d.path().join("States/gpsp/Emerald")).unwrap();
    assert_eq!(migrate_states(d.path()).unwrap(), 0);
    assert!(d.path().join("States/gpsp/Emerald").is_dir());
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

    assert_eq!(migrate_states(d.path()).unwrap(), 1);
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

    assert_eq!(migrate_states(d.path()).unwrap(), 0);
    assert_eq!(
        std::fs::read(d.path().join("States/Emerald/resume.state")).unwrap(),
        b"resume",
        "the source was disturbed"
    );
}

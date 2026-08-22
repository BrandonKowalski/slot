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

use slot_store::{migrate_states, Core, Platform, StateRing};
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
    // And the ring can see them once `migrate_platforms` has also run — the same order
    // `root::migrate` always calls the two sweeps in — which is the only reason to move them
    // at all.
    migrate_platforms(d.path()).unwrap();
    assert_eq!(
        StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald")
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

use slot_store::migrate_platforms;

fn loose_card() -> tempfile::TempDir {
    let d = tempdir().unwrap();
    for sub in ["Games", "Saves", "Labels", "States"] {
        std::fs::create_dir_all(d.path().join(sub)).unwrap();
    }
    std::fs::write(d.path().join("Games/Metroid Fusion.gba"), b"rom").unwrap();
    std::fs::write(d.path().join("Saves/Metroid Fusion.sav"), b"save").unwrap();
    std::fs::write(d.path().join("Saves/LeafGreen.srm"), b"srm").unwrap();
    std::fs::write(d.path().join("Labels/Metroid Fusion.png"), b"png").unwrap();
    let ring = d.path().join("States/gpsp/Advance Wars");
    std::fs::create_dir_all(&ring).unwrap();
    std::fs::write(ring.join("resume.state"), b"state").unwrap();
    d
}

/// Every card in existence is entirely loose and entirely GBA, so the sweep needs no per-file
/// classification. Contents must survive untouched: this moves a player's saves.
#[test]
fn a_loose_card_is_swept_into_gba() {
    let d = loose_card();

    migrate_platforms(d.path()).unwrap();

    assert_eq!(
        std::fs::read(d.path().join("Games/GBA/Metroid Fusion.gba")).unwrap(),
        b"rom"
    );
    assert_eq!(
        std::fs::read(d.path().join("Saves/GBA/Metroid Fusion.sav")).unwrap(),
        b"save"
    );
    assert_eq!(
        std::fs::read(d.path().join("Saves/GBA/LeafGreen.srm")).unwrap(),
        b"srm"
    );
    assert_eq!(
        std::fs::read(d.path().join("Labels/GBA/Metroid Fusion.png")).unwrap(),
        b"png"
    );
    assert_eq!(
        std::fs::read(d.path().join("States/GBA/gpsp/Advance Wars/resume.state")).unwrap(),
        b"state"
    );
    assert!(!d.path().join("Games/Metroid Fusion.gba").exists());
    assert!(!d.path().join("States/gpsp").exists());
}

#[test]
fn sweeping_twice_moves_nothing_the_second_time() {
    let d = loose_card();
    assert!(migrate_platforms(d.path()).unwrap().moved > 0);
    assert_eq!(
        migrate_platforms(d.path()).unwrap().moved,
        0,
        "not idempotent"
    );
    assert_eq!(
        std::fs::read(d.path().join("Saves/GBA/Metroid Fusion.sav")).unwrap(),
        b"save"
    );
}

/// An interrupted sweep leaves some entries moved and some loose. The next run must finish the
/// job rather than compound it, and must never overwrite what is already in place.
#[test]
fn a_half_finished_sweep_resumes_without_clobbering() {
    let d = loose_card();
    std::fs::create_dir_all(d.path().join("Saves/GBA")).unwrap();
    std::fs::write(d.path().join("Saves/GBA/Metroid Fusion.sav"), b"newer").unwrap();

    let report = migrate_platforms(d.path()).unwrap();

    assert_eq!(report.failed, 0, "a collision is expected, not a failure");
    assert_eq!(
        std::fs::read(d.path().join("Saves/GBA/Metroid Fusion.sav")).unwrap(),
        b"newer",
        "an already migrated save was clobbered"
    );
    assert!(
        d.path().join("Saves/Metroid Fusion.sav").exists(),
        "the loose copy was destroyed rather than left visible"
    );
    // The rest of the card still went.
    assert!(d.path().join("Games/GBA/Metroid Fusion.gba").exists());
}

/// Finder drops `.DS_Store` and `._` sidecars onto every FAT volume it touches. Those are card
/// metadata, not content, and sweeping them would be noise.
#[test]
fn dotfiles_are_left_where_they_are() {
    let d = loose_card();
    std::fs::write(d.path().join("Games/.DS_Store"), b"junk").unwrap();
    std::fs::write(d.path().join("Games/._Metroid Fusion.gba"), b"sidecar").unwrap();

    migrate_platforms(d.path()).unwrap();

    assert_eq!(
        std::fs::read(d.path().join("Games/.DS_Store")).unwrap(),
        b"junk"
    );
    assert!(d.path().join("Games/._Metroid Fusion.gba").exists());
}

/// Game Boy folders the user staged by hand are already where they belong, and the sweep must
/// not take them for loose files.
#[test]
fn platform_folders_are_not_swept_into_gba() {
    let d = loose_card();
    std::fs::create_dir_all(d.path().join("Games/GB")).unwrap();
    std::fs::write(d.path().join("Games/GB/Tetris.gb"), b"gb").unwrap();

    migrate_platforms(d.path()).unwrap();

    assert_eq!(
        std::fs::read(d.path().join("Games/GB/Tetris.gb")).unwrap(),
        b"gb"
    );
    assert!(!d.path().join("Games/GBA/GB").exists());
}

#[test]
fn a_card_with_no_directories_is_fine() {
    let d = tempdir().unwrap();
    assert_eq!(migrate_platforms(d.path()).unwrap().moved, 0);
}

/// I1: `Games/`, `Saves/`, `Labels/` and `States/` are four independent directories, so one of
/// them being stuck must not cost the other three their turn, and must not drop the count of
/// what they did move on the floor. `Saves/GBA` existing as a plain file is not something a
/// healthy card produces, but a corrupted one is not impossible, and `create_dir_all` failing on
/// it must not abort the whole sweep.
#[test]
fn a_blocked_directory_fails_soft_and_does_not_stop_the_others() {
    let d = tempdir().unwrap();
    for sub in ["Games", "Saves", "Labels"] {
        std::fs::create_dir_all(d.path().join(sub)).unwrap();
    }
    std::fs::write(d.path().join("Games/Metroid Fusion.gba"), b"rom").unwrap();
    std::fs::write(d.path().join("Saves/Metroid Fusion.sav"), b"save").unwrap();
    std::fs::write(d.path().join("Labels/Metroid Fusion.png"), b"png").unwrap();
    std::fs::write(d.path().join("Saves/GBA"), b"not a directory").unwrap();

    let report = migrate_platforms(d.path()).unwrap();

    // Two, not one: the blocking file is itself a loose file named `GBA` sitting directly in
    // `Saves/`, so `sweep_files` finds it too and fails to move it into `Saves/GBA/GBA` for the
    // same reason. Both the real save and this namesake count against the boot log.
    assert_eq!(
        report.failed, 2,
        "the blocked save and the blocking file itself must both count"
    );
    assert_eq!(
        std::fs::read(d.path().join("Saves/Metroid Fusion.sav")).unwrap(),
        b"save",
        "the source was disturbed"
    );
    // `Labels/` still went: one stuck directory does not abort the sweep of the others.
    assert_eq!(
        std::fs::read(d.path().join("Labels/GBA/Metroid Fusion.png")).unwrap(),
        b"png"
    );
}

/// The rom is what puts a cart on the shelf, so it is the one thing that must not overtake the
/// player's save. With `Saves/` stuck, a rom in `Games/GBA/` is a cart that opens on a blank
/// battery while its real save sits loose in `Saves/`, which is where the game announces the
/// save file is corrupt and writes a fresh one over the top of the reader's path. This test used
/// to assert the opposite — that `Games/` swept anyway — which is how the hole got in.
#[test]
fn a_stuck_saves_sweep_holds_the_roms_back_rather_than_shelving_them_saveless() {
    let d = tempdir().unwrap();
    for sub in ["Games", "Saves"] {
        std::fs::create_dir_all(d.path().join(sub)).unwrap();
    }
    std::fs::write(d.path().join("Games/Metroid Fusion.gba"), b"rom").unwrap();
    std::fs::write(d.path().join("Saves/Metroid Fusion.sav"), b"save").unwrap();
    std::fs::write(d.path().join("Saves/GBA"), b"not a directory").unwrap();

    migrate_platforms(d.path()).unwrap();

    assert!(
        !d.path().join("Games/GBA/Metroid Fusion.gba").exists(),
        "the rom reached the shelf while its save was still loose in Saves/"
    );
    assert_eq!(
        std::fs::read(d.path().join("Games/Metroid Fusion.gba")).unwrap(),
        b"rom",
        "the rom was moved somewhere else instead of being left alone"
    );

    // And the moment the card is fixed, the same call finishes the job.
    std::fs::remove_file(d.path().join("Saves/GBA")).unwrap();
    migrate_platforms(d.path()).unwrap();
    assert_eq!(
        std::fs::read(d.path().join("Games/GBA/Metroid Fusion.gba")).unwrap(),
        b"rom"
    );
    assert_eq!(
        std::fs::read(d.path().join("Saves/GBA/Metroid Fusion.sav")).unwrap(),
        b"save"
    );
}

/// `States/` is the player's data too: a rom shelved while its save states are stranded at the
/// old `States/<core>/` is a cart whose ten polaroids and whose resume have all vanished.
#[test]
fn a_stuck_states_sweep_holds_the_roms_back_too() {
    let d = tempdir().unwrap();
    for sub in ["Games", "States"] {
        std::fs::create_dir_all(d.path().join(sub)).unwrap();
    }
    std::fs::write(d.path().join("Games/Metroid Fusion.gba"), b"rom").unwrap();
    std::fs::create_dir_all(d.path().join("States/mgba/Metroid Fusion")).unwrap();
    std::fs::write(d.path().join("States/GBA"), b"not a directory").unwrap();

    migrate_platforms(d.path()).unwrap();

    assert!(
        !d.path().join("Games/GBA/Metroid Fusion.gba").exists(),
        "the rom reached the shelf while its states were still under States/mgba/"
    );
}

/// The ordering guard. Deferring the `root::migrate` call site to Task 3 left the two sweeps'
/// required order expressed nowhere but a doc comment; this is where it is proven end to end.
/// Reversed, a pre-namespacing `States/<stem>/` would be taken for a loose top level directory
/// and land at `States/GBA/<stem>/` — a cart folder sitting where a core folder belongs, which
/// `sweep_state_cores` (looking only for `Core::ALL` names) will never look inside again.
#[test]
fn migrate_states_then_migrate_platforms_lands_a_pre_namespacing_state_under_its_platform() {
    let d = tempdir().unwrap();
    let dir = d.path().join("States/Emerald");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("resume.state"), b"resume").unwrap();

    migrate_states(d.path()).unwrap();
    migrate_platforms(d.path()).unwrap();

    assert_eq!(
        std::fs::read(d.path().join("States/GBA/mgba/Emerald/resume.state")).unwrap(),
        b"resume"
    );
}

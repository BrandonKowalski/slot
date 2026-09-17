use slot_store::{Core, Platform, StateRing, RING_MAX};
use tempfile::tempdir;

#[test]
fn ring_evicts_the_oldest_beyond_ten() {
    let d = tempdir().unwrap();
    let r = StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald");
    for i in 0..14 {
        r.push(&[i as u8; 64], b"png", &format!("2026-08-09_00-00-{i:02}"))
            .unwrap();
    }
    let l = r.list().unwrap();
    assert_eq!(l.len(), RING_MAX);
    assert_eq!(l[0].stamp, "2026-08-09_00-00-13");
    assert_eq!(l[9].stamp, "2026-08-09_00-00-04");
    let orphans = std::fs::read_dir(d.path().join("States/GBA/mgba/Emerald"))
        .unwrap()
        .count();
    assert_eq!(orphans, RING_MAX * 2, "evicted thumbnails were not deleted");
}

#[test]
fn resume_is_not_part_of_the_ring() {
    let d = tempdir().unwrap();
    let r = StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald");
    for i in 0..12 {
        r.write_resume(&[i; 8]).unwrap();
    }
    assert!(
        r.list().unwrap().is_empty(),
        "resume writes leaked into the ring"
    );
    assert_eq!(r.read_resume().unwrap().unwrap(), vec![11u8; 8]);
}

#[test]
fn a_cart_with_no_saves_lists_empty_rather_than_erroring() {
    let d = tempdir().unwrap();
    let r = StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Never Played");
    assert!(r.list().unwrap().is_empty());
    assert!(r.read_resume().unwrap().is_none());
}

#[test]
fn eviction_is_by_stamp_not_by_write_order() {
    let d = tempdir().unwrap();
    let r = StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald");
    for i in [9, 3, 7, 1, 5, 11, 0, 8, 2, 10, 6, 4] {
        r.push(&[i as u8; 8], b"png", &format!("2026-08-09_00-00-{i:02}"))
            .unwrap();
    }
    let l = r.list().unwrap();
    assert_eq!(l.len(), RING_MAX);
    assert_eq!(l[0].stamp, "2026-08-09_00-00-11");
    assert_eq!(l[RING_MAX - 1].stamp, "2026-08-09_00-00-02");
}

#[test]
fn files_that_are_not_stamped_states_are_neither_listed_nor_evicted() {
    let d = tempdir().unwrap();
    let r = StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald");
    r.push(&[1u8; 8], b"png", "2026-08-09_00-00-01").unwrap();
    let dir = d.path().join("States/GBA/mgba/Emerald");
    let strays = [
        dir.join("2026-08-09_09-09-09.png"),
        dir.join("notes.txt"),
        dir.join("notes.state"),
    ];
    for s in &strays {
        std::fs::write(s, b"stray").unwrap();
    }

    let l = r.list().unwrap();
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].stamp, "2026-08-09_00-00-01");

    for i in 0..RING_MAX + 4 {
        r.push(&[0u8; 8], b"png", &format!("2026-08-09_01-00-{i:02}"))
            .unwrap();
    }
    for s in &strays {
        assert!(s.exists(), "eviction deleted {s:?}");
    }
}

/// The two halves of an entry are read back together, which is what makes an undo of a save
/// a restore rather than a reconstruction.
#[test]
fn read_pairs_the_state_with_its_thumbnail() {
    let d = tempdir().unwrap();
    let r = StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald");
    r.push(&[1u8; 8], b"first", "2026-08-09_00-00-01").unwrap();
    r.push(&[2u8; 8], b"second", "2026-08-09_00-00-02").unwrap();
    let (state, thumb) = r.read("2026-08-09_00-00-01").unwrap();
    assert_eq!(state, [1u8; 8]);
    assert_eq!(thumb, b"first");
}

/// A push cut between the two halves leaves a state with no picture. It is still a state,
/// and refusing to read it would make it the one entry an undo could not put back.
#[test]
fn read_of_an_entry_with_no_thumbnail_is_the_state_and_nothing_else() {
    let d = tempdir().unwrap();
    let r = StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald");
    r.push(&[1u8; 8], b"png", "2026-08-09_00-00-01").unwrap();
    std::fs::remove_file(
        d.path()
            .join("States/GBA/mgba/Emerald/2026-08-09_00-00-01.png"),
    )
    .unwrap();
    let (state, thumb) = r.read("2026-08-09_00-00-01").unwrap();
    assert_eq!(state, [1u8; 8]);
    assert!(thumb.is_empty());
}

/// Otherwise a stray thumbnail becomes the picture of whatever is saved next into the same
/// second, which is a polaroid showing a frame from another session.
#[test]
fn remove_takes_the_thumbnail_with_the_state() {
    let d = tempdir().unwrap();
    let r = StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald");
    r.push(&[1u8; 8], b"png", "2026-08-09_00-00-01").unwrap();
    r.remove("2026-08-09_00-00-01").unwrap();
    assert!(r.list().unwrap().is_empty());
    assert_eq!(
        std::fs::read_dir(d.path().join("States/GBA/mgba/Emerald"))
            .unwrap()
            .count(),
        0
    );
}

/// `remove` names a file, so it is the one place the ring could be talked into deleting the
/// session. Only a stamp is a name it accepts.
#[test]
fn remove_refuses_anything_that_is_not_a_stamp() {
    let d = tempdir().unwrap();
    let r = StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald");
    r.write_resume(&[7u8; 8]).unwrap();
    assert!(r.remove("resume").is_err());
    assert!(r.read("resume").is_err());
    assert_eq!(r.read_resume().unwrap().unwrap(), [7u8; 8]);
}

/// The stamp shape already refuses one, but the card is loaded from a Mac and the ring is
/// walked to decide what a cart can resume from, so it is worth holding to.
#[test]
fn a_sidecar_is_not_listed_as_a_state() {
    let d = tempdir().unwrap();
    let r = StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald");
    r.push(b"state", b"png", "2026-08-09_00-00-01").unwrap();
    let dir = d.path().join("States/GBA/mgba/Emerald");
    std::fs::write(dir.join("._2026-08-09_00-00-01.state"), b"sidecar").unwrap();
    let l = r.list().unwrap();
    assert_eq!(l.len(), 1, "a sidecar was listed as a save state");
    assert_eq!(l[0].stamp, "2026-08-09_00-00-01");
}

#[test]
fn each_core_keeps_its_own_states() {
    let d = tempdir().unwrap();
    let mgba = StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald");
    let gpsp = StateRing::new(d.path(), Platform::Gba, Core::Gpsp, "Emerald");

    mgba.push(&[1u8; 64], b"png", "2026-08-21_00-00-01")
        .unwrap();

    assert_eq!(mgba.list().unwrap().len(), 1);
    assert_eq!(
        gpsp.list().unwrap().len(),
        0,
        "a save state is not portable between cores and must not be offered as if it were"
    );
    assert!(d.path().join("States/GBA/mgba/Emerald").is_dir());
    assert!(!d.path().join("States/Emerald").exists());
}

#[test]
fn resume_is_core_private_too() {
    let d = tempdir().unwrap();
    StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald")
        .write_resume(&[9u8; 16])
        .unwrap();
    assert!(
        StateRing::new(d.path(), Platform::Gba, Core::Gpsp, "Emerald")
            .read_resume()
            .unwrap()
            .is_none()
    );
}

/// The collision, closed. A `.gb` and a `.gba` cart of the same stem are not the same cart,
/// and a state saved for one must not be offered to the other.
#[test]
fn each_platform_keeps_its_own_states() {
    let d = tempdir().unwrap();
    let gba = StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Tetris");
    let gb = StateRing::new(d.path(), Platform::Gb, Core::Mgba, "Tetris");

    gba.push(&[1u8; 64], b"png", "2026-08-21_00-00-01").unwrap();

    assert_eq!(gba.list().unwrap().len(), 1);
    assert_eq!(
        gb.list().unwrap().len(),
        0,
        "a save state is not portable between platforms and must not be offered as if it were"
    );
    assert!(d.path().join("States/GBA/mgba/Tetris").is_dir());
    assert!(!d.path().join("States/GB/mgba/Tetris").exists());
}

/// The move that ends "offered forever": after it, `read_resume` has nothing to hand the core,
/// so the next open starts the cart rather than failing on the same bytes again.
///
/// The retired file is still on the card and still exactly what was refused, because a state
/// one core will not read is a real session to the core that wrote it.
#[test]
fn a_retired_resume_stops_being_read_back_but_is_still_there() {
    let d = tempdir().unwrap();
    let r = StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald");
    r.write_resume(&[7u8; 64]).unwrap();

    let to = r
        .retire_resume("2026-09-16_23-30-00")
        .unwrap()
        .expect("nothing was moved");

    assert!(r.read_resume().unwrap().is_none(), "still offered");
    assert_eq!(std::fs::read(&to).unwrap(), [7u8; 64], "the bytes changed");
    assert_eq!(
        to.file_name().unwrap().to_str().unwrap(),
        "resume-refused-2026-09-16_23-30-00.state"
    );
    assert_eq!(
        to.parent().unwrap(),
        d.path().join("States/GBA/mgba/Emerald"),
        "the retired state left the cart's own directory"
    );
}

/// A cart with nothing to retire is the ordinary case on every open after the first, since
/// `App` calls this on every frame of the insert and only the first one finds a file.
#[test]
fn retiring_a_cart_with_no_resume_does_nothing() {
    let d = tempdir().unwrap();
    let r = StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald");
    assert!(r.retire_resume("2026-09-16_23-30-00").unwrap().is_none());
}

/// The retired name must never come back as something the player can be offered or the ring
/// can evict. `list` is what the switcher, `load_newest` and `evict` all read.
#[test]
fn a_retired_resume_is_not_a_ring_entry() {
    let d = tempdir().unwrap();
    let r = StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald");
    r.push(b"state", b"png", "2026-08-09_00-00-01").unwrap();
    r.write_resume(&[7u8; 64]).unwrap();
    r.retire_resume("2026-09-16_23-30-00").unwrap();

    let l = r.list().unwrap();
    assert_eq!(l.len(), 1, "the retired state was listed as a save state");
    assert_eq!(l[0].stamp, "2026-08-09_00-00-01");
}

/// Moving rather than deleting is worth nothing if the second move overwrites what the first
/// one saved. Two retirements inside one wall-clock second is not a thing a player can
/// produce, but the whole point of the rename is that no state is ever destroyed.
#[test]
fn a_second_retirement_in_the_same_second_does_not_overwrite_the_first() {
    let d = tempdir().unwrap();
    let r = StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald");

    r.write_resume(&[1u8; 8]).unwrap();
    let first = r.retire_resume("2026-09-16_23-30-00").unwrap().unwrap();
    r.write_resume(&[2u8; 8]).unwrap();
    let second = r.retire_resume("2026-09-16_23-30-00").unwrap().unwrap();

    assert_ne!(first, second, "the second retirement took the same name");
    assert_eq!(std::fs::read(&first).unwrap(), [1u8; 8], "the first went");
    assert_eq!(std::fs::read(&second).unwrap(), [2u8; 8]);
}

use slot_store::{StateRing, RING_MAX};
use tempfile::tempdir;

#[test]
fn ring_evicts_the_oldest_beyond_ten() {
    let d = tempdir().unwrap();
    let r = StateRing::new(d.path(), "Emerald");
    for i in 0..14 {
        r.push(&[i as u8; 64], b"png", &format!("2026-08-09_00-00-{i:02}"))
            .unwrap();
    }
    let l = r.list().unwrap();
    assert_eq!(l.len(), RING_MAX);
    assert_eq!(l[0].stamp, "2026-08-09_00-00-13");
    assert_eq!(l[9].stamp, "2026-08-09_00-00-04");
    let orphans = std::fs::read_dir(d.path().join("States/Emerald"))
        .unwrap()
        .count();
    assert_eq!(orphans, RING_MAX * 2, "evicted thumbnails were not deleted");
}

#[test]
fn resume_is_not_part_of_the_ring() {
    let d = tempdir().unwrap();
    let r = StateRing::new(d.path(), "Emerald");
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
    let r = StateRing::new(d.path(), "Never Played");
    assert!(r.list().unwrap().is_empty());
    assert!(r.read_resume().unwrap().is_none());
}

#[test]
fn eviction_is_by_stamp_not_by_write_order() {
    let d = tempdir().unwrap();
    let r = StateRing::new(d.path(), "Emerald");
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
    let r = StateRing::new(d.path(), "Emerald");
    r.push(&[1u8; 8], b"png", "2026-08-09_00-00-01").unwrap();
    let dir = d.path().join("States/Emerald");
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
    let r = StateRing::new(d.path(), "Emerald");
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
    let r = StateRing::new(d.path(), "Emerald");
    r.push(&[1u8; 8], b"png", "2026-08-09_00-00-01").unwrap();
    std::fs::remove_file(d.path().join("States/Emerald/2026-08-09_00-00-01.png")).unwrap();
    let (state, thumb) = r.read("2026-08-09_00-00-01").unwrap();
    assert_eq!(state, [1u8; 8]);
    assert!(thumb.is_empty());
}

/// Otherwise a stray thumbnail becomes the picture of whatever is saved next into the same
/// second, which is a polaroid showing a frame from another session.
#[test]
fn remove_takes_the_thumbnail_with_the_state() {
    let d = tempdir().unwrap();
    let r = StateRing::new(d.path(), "Emerald");
    r.push(&[1u8; 8], b"png", "2026-08-09_00-00-01").unwrap();
    r.remove("2026-08-09_00-00-01").unwrap();
    assert!(r.list().unwrap().is_empty());
    assert_eq!(
        std::fs::read_dir(d.path().join("States/Emerald"))
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
    let r = StateRing::new(d.path(), "Emerald");
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
    let r = StateRing::new(d.path(), "Emerald");
    r.push(b"state", b"png", "2026-08-09_00-00-01").unwrap();
    let dir = d.path().join("States/Emerald");
    std::fs::write(dir.join("._2026-08-09_00-00-01.state"), b"sidecar").unwrap();
    let l = r.list().unwrap();
    assert_eq!(l.len(), 1, "a sidecar was listed as a save state");
    assert_eq!(l[0].stamp, "2026-08-09_00-00-01");
}

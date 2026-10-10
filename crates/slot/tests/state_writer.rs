use std::sync::atomic::Ordering;

use slot::state_writer::StateWriter;
use slot_store::{read_slot_state, SlotState};

fn volume(v: u8) -> SlotState {
    SlotState {
        volume: v,
        ..Default::default()
    }
}

#[test]
fn a_flush_leaves_the_newest_state_on_the_card() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("Config")).unwrap();
    let w = StateWriter::spawn();
    for v in 0..50 {
        w.save(d.path(), &volume(v));
    }
    w.flush();
    assert_eq!(read_slot_state(d.path()).volume, 49);
}

#[test]
fn a_dropped_writer_finishes_what_it_was_given() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("Config")).unwrap();
    let w = StateWriter::spawn();
    w.save(d.path(), &volume(37));
    drop(w);
    assert_eq!(read_slot_state(d.path()).volume, 37);
}

#[test]
fn the_writer_says_the_card_is_failing_until_a_write_lands() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("Config"), b"").unwrap();
    let w = StateWriter::spawn();
    let failing = w.card_failing();
    w.save(d.path(), &volume(5));
    w.flush();
    assert!(
        failing.load(Ordering::Relaxed),
        "the refused write went unreported"
    );
    std::fs::remove_file(d.path().join("Config")).unwrap();
    std::fs::create_dir_all(d.path().join("Config")).unwrap();
    w.save(d.path(), &volume(6));
    w.flush();
    assert!(
        !failing.load(Ordering::Relaxed),
        "a landed write left it failing"
    );
}

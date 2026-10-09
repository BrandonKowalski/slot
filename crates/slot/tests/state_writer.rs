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

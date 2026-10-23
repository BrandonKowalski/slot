use std::sync::Arc;

use slot::card_log::CardWrites;
use slot::shot_writer::ShotWriter;

#[test]
fn a_failed_delete_raises_the_card_alarm() {
    let d = tempfile::tempdir().unwrap();
    let writes = Arc::new(CardWrites::default());
    let writer = ShotWriter::spawn(writes.clone());
    writer.remove(d.path().join("gone.png"));
    writer.flush();
    assert!(writes.failing(), "a delete that failed went unreported");
}

#[test]
fn a_finished_delete_is_no_longer_pending() {
    let d = tempfile::tempdir().unwrap();
    let shot = d.path().join("2026-10-22_17-30-05.png");
    std::fs::write(&shot, b"png").unwrap();
    let writer = ShotWriter::spawn(Arc::new(CardWrites::default()));
    writer.remove(shot.clone());
    writer.flush();
    assert!(!shot.exists());
    assert!(writer.removing().is_empty());
}

#[test]
fn a_failed_write_is_counted_once() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("blocked"), b"a file, not a folder").unwrap();
    let writer = ShotWriter::spawn(Arc::new(CardWrites::default()));
    writer.write(slot::shot_writer::Shot {
        path: d.path().join("blocked/shot.png"),
        w: 2,
        h: 2,
        rgba: vec![0; 16],
    });
    writer.flush();
    assert_eq!(writer.take_failures(), 1);
    assert_eq!(writer.take_failures(), 0);
}

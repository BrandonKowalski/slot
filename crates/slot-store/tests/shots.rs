use std::collections::HashSet;

use slot_store::{shot_stamp, Platform, ShotDir};
use tempfile::tempdir;

#[test]
fn a_missing_folder_lists_no_shots() {
    let d = tempdir().unwrap();
    let s = ShotDir::new(d.path(), Platform::Gb, "Tetris");
    assert!(s.list().unwrap().is_empty());
}

#[test]
fn shots_list_newest_first_and_skip_strangers() {
    let d = tempdir().unwrap();
    let dir = d.path().join("Screenshots/GB/Tetris");
    std::fs::create_dir_all(&dir).unwrap();
    for name in [
        "2026-10-22_17-30-05-120.png",
        "2026-10-22_17-30-05-981.png",
        "2026-10-22_17-30-06.png",
        "notes.png",
        "2026-10-22_17-30-07-000.txt",
    ] {
        std::fs::write(dir.join(name), b"png").unwrap();
    }
    let l = ShotDir::new(d.path(), Platform::Gb, "Tetris")
        .list()
        .unwrap();
    let stamps: Vec<&str> = l.iter().map(|e| e.stamp.as_str()).collect();
    assert_eq!(
        stamps,
        ["2026-10-22_17-30-05-981", "2026-10-22_17-30-05-120"]
    );
    for e in &l {
        assert_eq!(e.path, dir.join(format!("{}.png", e.stamp)));
    }
}

#[test]
fn remove_takes_only_a_stamped_file() {
    let d = tempdir().unwrap();
    let s = ShotDir::new(d.path(), Platform::Gba, "Emerald");
    let shot = s.path("2026-10-22_17-30-05-120");
    std::fs::create_dir_all(shot.parent().unwrap()).unwrap();
    std::fs::write(&shot, b"png").unwrap();
    s.remove("2026-10-22_17-30-05-120").unwrap();
    assert!(!shot.exists());
    assert!(s.remove("../x").is_err());
}

#[test]
fn the_path_has_no_core_directory() {
    let d = tempdir().unwrap();
    assert_eq!(
        ShotDir::new(d.path(), Platform::Gbc, "Zelda").path("2026-10-22_17-30-05"),
        d.path()
            .join("Screenshots/GBC/Zelda/2026-10-22_17-30-05.png")
    );
}

#[test]
fn a_shot_stamp_carries_milliseconds() {
    assert_eq!(shot_stamp(1_792_085_405_120), "2026-10-15_17-30-05-120");
    assert_eq!(shot_stamp(1_792_085_405_007), "2026-10-15_17-30-05-007");
}

#[test]
fn any_stops_at_a_shot_and_skips_ones_being_deleted() {
    let d = tempdir().unwrap();
    let s = ShotDir::new(d.path(), Platform::Gba, "Emerald");
    assert!(!s.any(&HashSet::new()));
    let dir = d.path().join("Screenshots/GBA/Emerald");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("notes.png"), b"png").unwrap();
    assert!(!s.any(&HashSet::new()));
    let shot = s.path("2026-10-22_17-30-05-120");
    std::fs::write(&shot, b"png").unwrap();
    assert!(s.any(&HashSet::new()));
    assert!(!s.any(&HashSet::from([shot])));
}

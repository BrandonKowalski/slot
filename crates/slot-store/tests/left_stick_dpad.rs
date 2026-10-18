use slot_store::{read_slot_state, write_slot_state, SlotState};

#[test]
fn the_left_stick_starts_as_a_dpad() {
    assert!(SlotState::default().left_stick_dpad);
}

#[test]
fn turning_the_left_stick_off_survives_a_restart() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("Config")).unwrap();
    let s = SlotState {
        left_stick_dpad: false,
        ..SlotState::default()
    };
    write_slot_state(d.path(), &s).unwrap();
    assert!(!read_slot_state(d.path()).left_stick_dpad);
}

#[test]
fn a_card_written_before_the_setting_reads_with_the_stick_on() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("Config")).unwrap();
    write_slot_state(
        d.path(),
        &SlotState {
            left_stick_dpad: false,
            ..SlotState::default()
        },
    )
    .unwrap();
    let path = d.path().join("Config/slot.state");
    let old: String = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with("left_stick_dpad="))
        .map(|l| format!("{l}\n"))
        .collect();
    std::fs::write(&path, old).unwrap();
    assert!(read_slot_state(d.path()).left_stick_dpad);
}

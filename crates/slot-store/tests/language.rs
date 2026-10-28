use slot_i18n::Language;
use slot_store::{read_slot_state, write_slot_state, SlotState};
#[test]
fn language_round_trips_and_old_or_unknown_values_default_to_english() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("Config")).unwrap();
    for language in Language::ALL {
        write_slot_state(
            root.path(),
            &SlotState {
                language,
                ..SlotState::default()
            },
        )
        .unwrap();
        assert_eq!(read_slot_state(root.path()).language, language);
    }
    for contents in ["brightness=3\n", "language=unknown\n", "language=\n"] {
        std::fs::write(root.path().join("Config/slot.state"), contents).unwrap();
        assert_eq!(read_slot_state(root.path()).language, Language::English);
    }
}

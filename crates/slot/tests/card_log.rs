mod common;

use std::sync::atomic::Ordering;

use common::{app_playing_with_card_log, tmp_root_with_carts, STUB_KERNEL_LINE};
use slot::card_log::CARD_LOG_NAME;

fn lock_the_card(root: &std::path::Path) {
    let states = root.join("States");
    let _ = std::fs::remove_dir_all(&states);
    std::fs::write(&states, b"").unwrap();
}

#[test]
fn a_card_turning_read_only_is_logged_off_the_card() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let off_card = tempfile::tempdir().unwrap();
    let log = off_card.path().join(CARD_LOG_NAME);
    let (mut a, read_only) = app_playing_with_card_log(d.path(), "Emerald", Some(log.clone()));
    a.tick_ms(1_000);
    assert!(!log.exists(), "a healthy card wrote a log entry");
    read_only.store(true, Ordering::Relaxed);
    a.tick_ms(3_100);
    assert!(a.card_alarm());
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(text.contains("alarm on read-only true"), "{text}");
    assert!(text.contains("  cart: Emerald\n"), "{text}");
    assert!(
        text.contains(&format!("  kernel: {STUB_KERNEL_LINE}\n")),
        "{text}"
    );
    read_only.store(false, Ordering::Relaxed);
    a.tick_ms(5_200);
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(text.contains("alarm off read-only false"), "{text}");
    assert_eq!(
        text.matches(STUB_KERNEL_LINE).count(),
        1,
        "the alarm clearing logged the kernel again: {text}"
    );
}

#[test]
fn a_refused_autosave_logs_the_error_it_got() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let off_card = tempfile::tempdir().unwrap();
    let log = off_card.path().join(CARD_LOG_NAME);
    let (mut a, _) = app_playing_with_card_log(d.path(), "Emerald", Some(log.clone()));
    lock_the_card(d.path());
    a.tick_ms(300_000);
    a.settle_saves();
    a.tick_ms(300_010);
    assert!(a.card_alarm());
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(text.contains("alarm on read-only false"), "{text}");
    assert!(text.contains("  write failed: autosave: "), "{text}");
}

#[test]
fn the_log_is_copied_onto_the_card_at_boot() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let off_card = tempfile::tempdir().unwrap();
    let log = off_card.path().join(CARD_LOG_NAME);
    std::fs::write(
        &log,
        "2026-10-14_10-00-00 alarm on read-only true build X\n",
    )
    .unwrap();
    let _ = app_playing_with_card_log(d.path(), "Emerald", Some(log.clone()));
    assert_eq!(
        std::fs::read(d.path().join("Logs").join(CARD_LOG_NAME)).unwrap(),
        std::fs::read(&log).unwrap()
    );
}

#[test]
fn no_log_on_the_device_puts_none_on_the_card() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let off_card = tempfile::tempdir().unwrap();
    let log = off_card.path().join(CARD_LOG_NAME);
    let _ = app_playing_with_card_log(d.path(), "Emerald", Some(log));
    assert!(!d.path().join("Logs").join(CARD_LOG_NAME).exists());
}

#[test]
fn a_settings_write_that_lands_does_not_hide_a_save_that_failed() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let off_card = tempfile::tempdir().unwrap();
    let log = off_card.path().join(CARD_LOG_NAME);
    let (mut a, _) = app_playing_with_card_log(d.path(), "Emerald", Some(log));
    lock_the_card(d.path());
    a.tick_ms(300_000);
    a.settle_saves();
    a.tick_ms(300_010);
    assert!(a.card_alarm());
    a.apply(slot_input::Action::BrightnessDown);
    a.tick_ms(310_000);
    a.flush_state();
    a.tick_ms(312_100);
    assert!(
        a.card_alarm(),
        "slot.state writing fine cleared the alarm for a save that still is not"
    );
}

#[test]
fn the_card_log_rolls_over_rather_than_growing_for_ever() {
    let d = tempfile::tempdir().unwrap();
    let log = d.path().join(CARD_LOG_NAME);
    std::fs::write(&log, vec![b'x'; 300 * 1024]).unwrap();
    slot::card_log::append(&log, "alarm on\n").unwrap();
    assert_eq!(std::fs::read_to_string(&log).unwrap(), "alarm on\n");
    assert_eq!(
        std::fs::metadata(d.path().join(format!("{CARD_LOG_NAME}.1")))
            .unwrap()
            .len(),
        300 * 1024
    );
}

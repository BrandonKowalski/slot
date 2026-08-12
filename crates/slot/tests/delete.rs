mod common;

use common::{app_in_switcher, app_playing_in, tmp_root_with_carts};
use slot::app::Phase;
use slot_input::{Action, Btn};
use slot_store::StateRing;

#[test]
fn y_deletes_the_selected_state_and_nothing_else() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let r = StateRing::new(d.path(), "Emerald");
    for i in 0..3 {
        r.push(&[i as u8; 64], b"png", &format!("2026-08-09_00-00-{i:02}"))
            .unwrap();
    }
    let mut a = app_in_switcher(d.path(), "Emerald");
    a.apply(Action::ShelfRight);
    a.apply(Action::GbaDown(Btn::Y));
    let left = r.list().unwrap();
    assert_eq!(left.len(), 2, "delete removed the wrong number of states");
    assert!(
        !left.iter().any(|e| e.stamp == "2026-08-09_00-00-01"),
        "the wrong one went"
    );
}

#[test]
fn deleting_the_pending_undos_target_clears_the_offer() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply_at(Action::SaveState, 1_000);
    a.apply_at(Action::Polaroids, 1_100);
    assert!(a.undo_available(1_200));
    a.apply(Action::GbaDown(Btn::Y));
    assert!(
        !a.undo_available(1_300),
        "the offer still points at a deleted state"
    );
}

#[test]
fn deleting_the_last_state_closes_the_switcher() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let r = StateRing::new(d.path(), "Emerald");
    r.push(&[0u8; 64], b"png", "2026-08-09_00-00-00").unwrap();
    let mut a = app_in_switcher(d.path(), "Emerald");
    a.apply(Action::GbaDown(Btn::Y));
    assert!(
        matches!(a.phase(), Phase::Playing { .. }),
        "an empty switcher stayed open"
    );
}

/// The switcher holds the ring as it was when it opened, so a delete has to come out of that
/// snapshot too. Otherwise the dots still count an entry that is gone and loading it fails.
#[test]
fn the_deleted_state_leaves_the_switcher_with_it() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let r = StateRing::new(d.path(), "Emerald");
    for i in 0..3 {
        r.push(&[i as u8; 64], b"png", &format!("2026-08-09_00-00-{i:02}"))
            .unwrap();
    }
    let mut a = app_in_switcher(d.path(), "Emerald");
    a.apply(Action::GbaDown(Btn::Y));
    let stamps: Vec<&str> = a
        .polaroid_entries()
        .iter()
        .map(|e| e.stamp.as_str())
        .collect();
    assert_eq!(stamps, ["2026-08-09_00-00-01", "2026-08-09_00-00-00"]);
    assert!(
        matches!(a.phase(), Phase::Polaroids { .. }),
        "the switcher closed with states still in it"
    );
}

/// An undo that names a different state is still good. Clearing every offer on any delete
/// would take the undo away from a save the user never touched.
#[test]
fn deleting_someone_elses_state_leaves_the_offer_alone() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let r = StateRing::new(d.path(), "Emerald");
    r.push(&[0u8; 64], b"png", "2026-08-09_00-00-00").unwrap();
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply_at(Action::SaveState, 1_000);
    a.apply_at(Action::Polaroids, 1_100);
    // Newest first, so the older entry is the one the save did not make.
    a.apply(Action::ShelfRight);
    a.apply(Action::GbaDown(Btn::Y));
    assert!(
        a.undo_available(1_300),
        "an unrelated delete took the offer"
    );
}

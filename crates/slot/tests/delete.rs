mod common;

use common::{app_in_switcher, app_playing_in, tmp_root_with_carts};
use slot::app::Phase;
use slot_input::{Action, Btn};
use slot_store::{Core, Platform, StateRing};

fn delete(a: &mut slot::app::App) {
    a.apply(Action::GbaDown(Btn::Y));
    assert!(a.delete_prompt(), "Y did not ask first");
    a.apply(Action::GbaDown(Btn::X));
    assert!(!a.delete_prompt(), "X did not close the prompt");
}

#[test]
fn y_asks_first_and_only_x_deletes() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let r = StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald");
    r.push(&[0u8; 64], b"png", "2026-08-09_00-00-00").unwrap();
    let mut a = app_in_switcher(d.path(), "Emerald");
    a.apply(Action::GbaDown(Btn::Y));
    assert!(a.delete_prompt());
    for btn in [Btn::A, Btn::Y, Btn::Left, Btn::Right, Btn::Start] {
        a.apply(Action::GbaDown(btn));
        assert!(a.delete_prompt(), "{btn:?} closed the prompt");
        assert_eq!(r.list().unwrap().len(), 1, "{btn:?} deleted the state");
    }
    a.apply(Action::GbaDown(Btn::B));
    assert!(!a.delete_prompt(), "B did not back out");
    assert_eq!(r.list().unwrap().len(), 1, "backing out deleted the state");
    assert!(
        matches!(a.phase(), Phase::Polaroids { .. }),
        "B left the switcher too"
    );
    delete(&mut a);
    assert!(r.list().unwrap().is_empty(), "X did not delete");
}

#[test]
fn y_deletes_the_selected_state_and_nothing_else() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let r = StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald");
    for i in 0..3 {
        r.push(&[i as u8; 64], b"png", &format!("2026-08-09_00-00-{i:02}"))
            .unwrap();
    }
    let mut a = app_in_switcher(d.path(), "Emerald");
    a.apply(Action::ShelfRight);
    delete(&mut a);
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
    delete(&mut a);
    assert!(
        !a.undo_available(1_300),
        "the offer still points at a deleted state"
    );
}

#[test]
fn deleting_the_last_state_closes_the_switcher() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let r = StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald");
    r.push(&[0u8; 64], b"png", "2026-08-09_00-00-00").unwrap();
    let mut a = app_in_switcher(d.path(), "Emerald");
    delete(&mut a);
    assert!(
        matches!(a.phase(), Phase::Playing { .. }),
        "an empty switcher stayed open"
    );
}

#[test]
fn the_deleted_state_leaves_the_switcher_with_it() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let r = StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald");
    for i in 0..3 {
        r.push(&[i as u8; 64], b"png", &format!("2026-08-09_00-00-{i:02}"))
            .unwrap();
    }
    let mut a = app_in_switcher(d.path(), "Emerald");
    delete(&mut a);
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

#[test]
fn deleting_someone_elses_state_leaves_the_offer_alone() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let r = StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald");
    r.push(&[0u8; 64], b"png", "2026-08-09_00-00-00").unwrap();
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply_at(Action::SaveState, 1_000);
    a.apply_at(Action::Polaroids, 1_100);
    a.apply(Action::ShelfRight);
    delete(&mut a);
    assert!(
        a.undo_available(1_300),
        "an unrelated delete took the offer"
    );
}

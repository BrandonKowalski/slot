mod common;

use common::{app_playing_in, app_playing_with, tmp_root_with_carts, CoreSnapshot};
use slot_input::{Action, Btn};
use slot_store::StateRing;

#[test]
fn undoing_a_save_removes_it_and_restores_the_evicted_entry() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let r = StateRing::new(d.path(), "Emerald");
    for i in 0..10 {
        r.push(&[i as u8; 64], b"png", &format!("2026-08-09_00-00-{i:02}"))
            .unwrap();
    }
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply_at(Action::SaveState, 1_000);
    assert_eq!(r.list().unwrap().len(), 10);
    assert!(r
        .list()
        .unwrap()
        .iter()
        .all(|e| e.stamp != "2026-08-09_00-00-00"));

    a.undo(2_000);
    let l = r.list().unwrap();
    assert_eq!(l.len(), 10);
    assert!(
        l.iter().any(|e| e.stamp == "2026-08-09_00-00-00"),
        "the evicted oldest entry was not restored"
    );
}

#[test]
fn undo_expires_after_thirty_seconds() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply_at(Action::SaveState, 1_000);
    assert!(a.undo_available(30_999));
    assert!(!a.undo_available(31_001));
}

#[test]
fn a_second_save_replaces_the_undo_rather_than_stacking() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let r = StateRing::new(d.path(), "Emerald");
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply_at(Action::SaveState, 1_000);
    a.apply_at(Action::SaveState, 2_000);
    a.undo(3_000);
    assert_eq!(
        r.list().unwrap().len(),
        1,
        "only the most recent save should be undone"
    );
}

#[test]
fn undoing_a_load_returns_the_emulator_to_the_prior_state() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let core = CoreSnapshot::new();
    let mut a = app_playing_with(d.path(), "Emerald", core.boxed());
    core.run_frame();
    a.apply_at(Action::SaveState, 500);
    for _ in 0..120 {
        core.run_frame();
    }
    let before = core.bytes();
    a.apply_at(Action::LoadState, 1_000);
    assert_ne!(core.bytes(), before, "the load never moved the core");
    a.undo(2_000);
    assert_eq!(core.bytes(), before);
}

#[test]
fn undo_is_not_offered_when_nothing_is_undoable() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let a = app_playing_in(d.path(), "Emerald");
    assert!(!a.undo_available(0));
}

/// Which of the two it is has to reach the affordance, or the screen offers to undo a save
/// while it is holding a load.
#[test]
fn the_label_names_what_will_be_undone() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply_at(Action::SaveState, 1_000);
    assert_eq!(a.undo_label(), Some("undo save"));
    a.apply_at(Action::LoadState, 2_000);
    assert_eq!(a.undo_label(), Some("undo load"));
}

/// The affordance is the switcher's and nowhere else's, so the button that works it is only
/// the switcher's too. X never reaches the core, but it is still not the shelf's or the
/// game's to spend.
#[test]
fn x_undoes_from_the_switcher_and_never_from_the_game() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let r = StateRing::new(d.path(), "Emerald");
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply_at(Action::SaveState, 1_000);
    a.apply_at(Action::GbaDown(Btn::X), 1_100);
    assert_eq!(r.list().unwrap().len(), 1, "X undid from the game");

    a.apply_at(Action::Polaroids, 1_200);
    a.apply_at(Action::GbaDown(Btn::X), 1_300);
    assert!(r.list().unwrap().is_empty(), "X did not undo the save");
}

/// An undo is a one shot. A second press would otherwise put the save back, which is a redo,
/// and a redo is a knob.
#[test]
fn undoing_twice_does_not_put_the_save_back() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let r = StateRing::new(d.path(), "Emerald");
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply_at(Action::SaveState, 1_000);
    a.undo(2_000);
    a.undo(3_000);
    assert!(r.list().unwrap().is_empty());
    assert!(!a.undo_available(3_000));
}

/// The undo names a file under one cart's ring and a state only that cart's core can read.
/// Carried across an eject it would either delete the wrong save or feed the wrong machine.
#[test]
fn ejecting_the_cart_takes_its_undo_with_it() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply_at(Action::SaveState, 1_000);
    a.apply_at(Action::Eject, 1_100);
    assert!(!a.undo_available(1_200));
}

/// The offer is drawn on the switcher and nowhere else, and it goes when the grace period
/// does rather than sitting there being pressed to no effect.
#[test]
fn the_plate_is_drawn_only_over_the_switcher_and_only_while_the_offer_stands() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply_at(Action::SaveState, 1_000);
    // A save says so on screen for 1.5 s, which is not the offer. Wait it out: the offer
    // has thirty seconds and must still be standing, and must still draw nothing.
    a.tick_ms(4_000);
    let mut playing = Vec::new();
    a.draw(&mut playing);
    assert!(
        a.undo_available(4_000),
        "the offer expired before it was tested"
    );
    assert!(playing.is_empty(), "the offer reached the game");

    // Open the switcher with the toast already faded, so the only difference between the
    // two draws below is the offer itself.
    a.apply_at(Action::Polaroids, 4_000);
    let mut open = Vec::new();
    a.draw(&mut open);
    a.tick_ms(40_000);
    let mut expired = Vec::new();
    a.draw(&mut expired);
    assert_eq!(
        open.len(),
        expired.len() + 1,
        "the plate outlived the offer"
    );
}

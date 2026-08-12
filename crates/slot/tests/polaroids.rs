mod common;

use common::{app_playing_in, app_playing_with, tmp_root_with_carts, StubSnapshot};
use slot::app::{App, Phase};
use slot_input::{Action, Btn};
use slot_store::{Cart, StateRing};

/// No content root, so nothing this app does can reach a ring.
fn app_playing(stem: &str) -> App {
    let mut a = App::new(vec![Cart {
        stem: stem.to_string(),
        rom: format!("Games/{stem}.gba").into(),
        label: None,
        code: String::new(),
        title: stem.to_uppercase(),
    }]);
    a.apply(Action::Insert);
    a.on_core_ready();
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    a
}

#[test]
fn double_tap_menu_with_no_states_does_not_open_the_switcher() {
    let mut a = app_playing("Emerald");
    a.apply(Action::Polaroids);
    assert!(matches!(a.phase(), Phase::Playing { .. }));
}

/// The same no-op, but on a real card with a real cart that simply has never been saved.
/// This is the path a first-time player takes.
#[test]
fn a_cart_that_has_never_been_saved_does_not_open_the_switcher() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::Polaroids);
    assert!(matches!(a.phase(), Phase::Playing { .. }));
}

#[test]
fn saving_pushes_a_polaroid_with_its_picture() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::SaveState);
    let entries = StateRing::new(d.path(), "Emerald").list().expect("list");
    assert_eq!(entries.len(), 1);
    assert_eq!(
        std::fs::read(&entries[0].state).expect("state"),
        [9u8; 1024]
    );
    assert!(
        !std::fs::read(&entries[0].thumb).expect("thumb").is_empty(),
        "the polaroid has no picture"
    );
}

/// The stamp is the filename, so two saves inside one second would silently be one save.
#[test]
fn two_saves_in_the_same_second_are_two_entries() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::SaveState);
    a.apply(Action::SaveState);
    assert_eq!(
        StateRing::new(d.path(), "Emerald")
            .list()
            .expect("list")
            .len(),
        2
    );
}

#[test]
fn a_saved_state_opens_the_switcher_and_a_loads_it_back() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let (snapshot, loaded) = StubSnapshot::pair();
    let mut a = app_playing_with(d.path(), "Emerald", snapshot);
    a.apply(Action::SaveState);
    a.apply(Action::Polaroids);
    assert!(matches!(a.phase(), Phase::Polaroids { .. }));
    a.apply(Action::GbaDown(Btn::A));
    assert_eq!(
        loaded.lock().expect("loaded").as_deref(),
        Some(&[9u8; 1024][..])
    );
    assert!(
        matches!(a.phase(), Phase::Playing { .. }),
        "loading must hand the game back"
    );
}

/// B is the way out, not a second load button.
#[test]
fn b_dismisses_the_switcher_without_loading() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let (snapshot, loaded) = StubSnapshot::pair();
    let mut a = app_playing_with(d.path(), "Emerald", snapshot);
    a.apply(Action::SaveState);
    a.apply(Action::Polaroids);
    a.apply(Action::GbaDown(Btn::B));
    assert!(matches!(a.phase(), Phase::Playing { .. }));
    assert!(loaded.lock().expect("loaded").is_none());
}

/// A second double tap of MENU is the same dismissal, per the input map.
#[test]
fn menu_dismisses_the_switcher_it_opened() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::SaveState);
    a.apply(Action::Polaroids);
    a.apply(Action::Polaroids);
    assert!(matches!(a.phase(), Phase::Playing { .. }));
}

/// Left and right walk the ring rather than the shelf behind it, and A loads what they
/// landed on.
#[test]
fn flicking_selects_which_state_a_loads() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let ring = StateRing::new(d.path(), "Emerald");
    ring.push(&[1u8; 16], b"png", "2026-08-09_00-00-01")
        .expect("push");
    ring.push(&[2u8; 16], b"png", "2026-08-09_00-00-02")
        .expect("push");
    let (snapshot, loaded) = StubSnapshot::pair();
    let mut a = app_playing_with(d.path(), "Emerald", snapshot);
    a.apply(Action::Polaroids);
    a.apply(Action::GbaDown(Btn::Right));
    a.apply(Action::GbaDown(Btn::A));
    assert_eq!(
        loaded.lock().expect("loaded").as_deref(),
        Some(&[1u8; 16][..])
    );
}

/// `SELECT+L1` is the no-look load. It takes the newest without opening anything.
#[test]
fn load_state_takes_the_newest_entry_without_the_switcher() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let ring = StateRing::new(d.path(), "Emerald");
    ring.push(&[1u8; 16], b"png", "2026-08-09_00-00-01")
        .expect("push");
    ring.push(&[2u8; 16], b"png", "2026-08-09_00-00-02")
        .expect("push");
    let (snapshot, loaded) = StubSnapshot::pair();
    let mut a = app_playing_with(d.path(), "Emerald", snapshot);
    a.apply(Action::LoadState);
    assert_eq!(
        loaded.lock().expect("loaded").as_deref(),
        Some(&[2u8; 16][..])
    );
    assert!(matches!(a.phase(), Phase::Playing { .. }));
}

/// The switcher is not a place a cart can be ejected from, and the shelf is not underneath
/// it either. Both would leave the emulator paused with no way back.
#[test]
fn the_switcher_swallows_the_shelf_and_eject_bindings() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::SaveState);
    a.apply(Action::Polaroids);
    a.apply(Action::Eject);
    a.apply(Action::Insert);
    assert!(matches!(a.phase(), Phase::Polaroids { .. }));
}

mod common;

use common::{
    app_booting_at, app_booting_with_clock, app_playing_in, tmp_root_with_carts, CLOCK_IS_SET,
};
use slot::app::{App, Phase};
use slot_input::{Action, Btn};
use slot_store::{write_slot_state, SlotState};
use slot_ui::{QuickRow, QuickValue};
use tempfile::TempDir;

const T: u64 = 10_000;

fn on_shelf(stems: &[&str]) -> (TempDir, App) {
    let d = tmp_root_with_carts(stems);
    write_slot_state(
        d.path(),
        &SlotState {
            clock_set: true,
            ..SlotState::default()
        },
    )
    .expect("write slot.state");
    let (mut a, _) = app_booting_at(d.path(), CLOCK_IS_SET);
    assert!(matches!(a.phase(), Phase::Shelf), "{:?}", a.phase());
    a.tick_ms(T - 1_000);
    (d, a)
}

fn at(a: &mut App, ms: u64) {
    a.tick_ms(ms);
}

fn frame_at(a: &mut App, ms: u64) {
    a.tick_ms(ms - 16);
    a.update(0.016);
}

#[test]
fn a_held_down_walks_the_menu_rows_after_a_pause_and_stops_on_release() {
    let (_d, mut a) = on_shelf(&["Emerald", "Fusion"]);
    a.apply_at(Action::QuickMenu, T - 500);
    assert_eq!(a.quick_menu(), Some(QuickRow::MAIN[0]));
    a.apply_at(Action::GbaDown(Btn::Down), T);
    assert_eq!(a.quick_menu(), Some(QuickRow::MAIN[1]));
    at(&mut a, T + 399);
    assert_eq!(a.quick_menu(), Some(QuickRow::MAIN[1]), "it repeated early");
    at(&mut a, T + 400);
    assert_eq!(a.quick_menu(), Some(QuickRow::MAIN[2]));
    at(&mut a, T + 510);
    assert_eq!(a.quick_menu(), Some(QuickRow::MAIN[3]));
    a.apply_at(Action::GbaUp(Btn::Down), T + 520);
    at(&mut a, T + 5_000);
    assert_eq!(
        a.quick_menu(),
        Some(QuickRow::MAIN[3]),
        "it kept going after the release"
    );
}

#[test]
fn a_held_left_or_right_changes_a_menu_value_once() {
    let (_d, mut a) = on_shelf(&["Emerald", "Fusion"]);
    a.apply_at(Action::QuickMenu, T - 900);
    a.apply_at(Action::GbaDown(Btn::A), T - 800);
    assert_eq!(a.quick_menu(), Some(QuickRow::Theme));
    a.apply_at(Action::GbaDown(Btn::Right), T);
    assert_eq!(a.quick_value(QuickRow::Theme), Some(QuickValue::Light));
    for ms in [T + 400, T + 510, T + 600, T + 2_000] {
        at(&mut a, ms);
        assert_eq!(
            a.quick_value(QuickRow::Theme),
            Some(QuickValue::Light),
            "the value flipped again by {ms}"
        );
    }
}

#[test]
fn a_held_up_keeps_turning_the_clock() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let (mut a, _) = app_booting_with_clock(d.path());
    assert!(matches!(a.phase(), Phase::SetClock { .. }));
    a.tick_ms(T - 1_000);
    a.apply_at(Action::GbaDown(Btn::A), T - 900);
    a.apply_at(Action::GbaUp(Btn::A), T - 800);
    let read = |a: &App| a.picker().expect("on the clock").secs();
    let start = read(&a);
    a.apply_at(Action::GbaDown(Btn::Up), T);
    let mut seen = vec![read(&a)];
    for ms in [T + 400, T + 510, T + 595, T + 660] {
        at(&mut a, ms);
        seen.push(read(&a));
    }
    assert!(seen[0] != start, "the press did nothing");
    assert!(
        seen.windows(2).all(|w| w[0] != w[1]),
        "a held Up stopped turning the clock: {seen:?}"
    );
    a.apply_at(Action::GbaUp(Btn::Up), T + 670);
    let stopped = read(&a);
    at(&mut a, T + 3_000);
    assert_eq!(
        read(&a),
        stopped,
        "the clock kept turning after the release"
    );
}

#[test]
fn a_held_down_on_the_shelf_keeps_jumping_letters() {
    let (_d, mut a) = on_shelf(&["Alpha", "Bravo", "Charlie", "Delta", "Echo"]);
    let first = a.selected_stem().map(str::to_owned);
    a.apply_at(Action::GbaDown(Btn::Down), T);
    let mut seen = vec![first, a.selected_stem().map(str::to_owned)];
    for ms in [T + 400, T + 510] {
        at(&mut a, ms);
        seen.push(a.selected_stem().map(str::to_owned));
    }
    assert!(
        seen.windows(2).all(|w| w[0] != w[1]),
        "a held Down stopped jumping letters: {seen:?}"
    );
}

#[test]
fn a_held_right_on_the_shelf_still_steps_once_per_repeat() {
    let stems = ["Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot"];
    let (_d, mut a) = on_shelf(&stems);
    let index = |a: &App| {
        let stem = a.selected_stem().expect("a cart in the slot").to_owned();
        stems.iter().position(|s| *s == stem).expect("a known cart")
    };
    let start = index(&a);
    a.apply_at(Action::GbaDown(Btn::Right), T);
    frame_at(&mut a, T + 384);
    assert_eq!(index(&a), (start + 1) % stems.len());
    frame_at(&mut a, T + 400);
    assert_eq!(
        index(&a),
        (start + 2) % stems.len(),
        "the shelf and the menu repeat both stepped"
    );
}

#[test]
fn a_direction_held_through_play_does_not_scroll_a_menu_opened_over_it() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.tick_ms(T - 1_000);
    a.apply_at(Action::GbaDown(Btn::Down), T);
    at(&mut a, T + 400);
    a.apply_at(Action::QuickMenu, T + 450);
    let opened = a.in_game_menu();
    assert!(opened.is_some(), "the in-game menu did not open");
    for ms in [T + 600, T + 1_000, T + 2_000] {
        at(&mut a, ms);
        assert_eq!(
            a.in_game_menu(),
            opened,
            "the held Down scrolled it at {ms}"
        );
    }
}

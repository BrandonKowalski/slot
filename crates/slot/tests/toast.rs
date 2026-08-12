mod common;

use common::{app_playing_in, tmp_root_with_carts};
use slot_input::Action;
use slot_ui::{Toast, HUD_MS};

#[test]
fn a_save_hotkey_says_state_saved() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::SaveState);
    assert_eq!(a.toast(), Some(Toast::StateSaved));
}

/// One line for both would tell you something happened and not which.
#[test]
fn a_load_says_state_loaded() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::SaveState);
    a.apply(Action::LoadState);
    assert_eq!(a.toast(), Some(Toast::StateLoaded));
}

/// A load that could not happen must not claim it did.
#[test]
fn a_refused_load_says_nothing() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::LoadState);
    assert_eq!(a.toast(), None, "it announced a load with an empty ring");
    assert!(a.refusal_active(a.now()), "and it did not shake either");
}

/// It says so and then leaves. A line that stayed would be a status bar.
#[test]
fn a_toast_goes_away_on_its_own() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply_at(Action::SaveState, 1_000);
    assert_eq!(a.toast(), Some(Toast::StateSaved));
    a.tick_ms(1_000 + HUD_MS);
    assert_eq!(a.toast(), None);
}

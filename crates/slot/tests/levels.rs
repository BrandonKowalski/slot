mod common;

use common::{app_playing_in, app_playing_with_volume, boot, tmp_root_with_carts};
use slot::app::Phase;
use slot_input::Action;
use slot_store::read_slot_state;
use slot_ui::Icon;

#[test]
fn levels_clamp_and_persist() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = boot(d.path());
    for _ in 0..20 {
        a.apply(Action::BrightnessUp);
    }
    assert_eq!(read_slot_state(d.path()).brightness, 9);
    for _ in 0..20 {
        a.apply(Action::BrightnessDown);
    }
    assert_eq!(read_slot_state(d.path()).brightness, 0);
}

#[test]
fn volume_moves_five_per_press() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = boot(d.path());
    a.apply(Action::VolumeUp);
    assert_eq!(read_slot_state(d.path()).volume, 65);
    for _ in 0..20 {
        a.apply(Action::VolumeDown);
    }
    assert_eq!(read_slot_state(d.path()).volume, 0);
}

#[test]
fn levels_work_on_the_shelf_not_only_in_game() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    let mut a = boot(d.path());
    assert!(matches!(a.phase(), Phase::Shelf));
    a.apply(Action::BlueLightUp);
    assert_eq!(read_slot_state(d.path()).blue_light, 1);
}

/// The bar is transient chrome over a live game, per spec section 6. Anything that moved
/// the phase would have paused it.
#[test]
fn adjusting_a_level_in_game_leaves_the_game_running() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::VolumeDown);
    assert!(matches!(a.phase(), Phase::Playing { .. }));
    assert_eq!(read_slot_state(d.path()).volume, 55);
}

#[test]
fn muting_leaves_the_level_alone_even_at_the_ends() {
    for start in [0u8, 5, 60, 100] {
        let d = tmp_root_with_carts(&["Emerald"]);
        let mut a = app_playing_with_volume(d.path(), start);
        a.apply_at(Action::VolumeUp, 0);
        a.apply_at(Action::VolumeDown, 40);
        a.apply_at(Action::MuteToggle, 60);
        assert!(a.muted(), "not muted from {start}");
        a.apply_at(Action::MuteToggle, 2_000);
        assert_eq!(a.volume(), start, "unmuted to the wrong level from {start}");
    }
}

#[test]
fn muted_is_silent_but_remembers() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_with_volume(d.path(), 70);
    a.apply_at(Action::MuteToggle, 0);
    assert_eq!(a.output_volume(), 0, "muted but still audible");
    assert_eq!(a.volume(), 70, "muting forgot the level");
    assert_eq!(a.hud_icon(), Icon::VolumeMuted);
}

/// Turning it up is the obvious way to undo a mute, and it should work.
#[test]
fn adjusting_the_volume_unmutes() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_with_volume(d.path(), 70);
    a.apply_at(Action::MuteToggle, 0);
    a.apply_at(Action::VolumeUp, 1_000);
    assert!(!a.muted(), "still muted after turning it up");
    assert_eq!(a.volume(), 75);
}

/// The chord is one press away from the level keys, so unmuting has to give back the silence
/// as well as the number. Without it the second half of the pair would leave it audible.
#[test]
fn unmuting_with_the_chord_does_not_mute_again() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_with_volume(d.path(), 70);
    a.apply_at(Action::MuteToggle, 0);
    a.apply_at(Action::VolumeDown, 1_000);
    a.apply_at(Action::VolumeUp, 1_060);
    a.apply_at(Action::MuteToggle, 1_060);
    assert!(!a.muted(), "the unmute chord muted it again");
    assert_eq!(a.volume(), 70);
}

/// A device muted at bedtime is still muted in the morning.
#[test]
fn the_mute_survives_a_reboot() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_with_volume(d.path(), 70);
    a.apply_at(Action::MuteToggle, 0);
    drop(a);
    let b = boot(d.path());
    assert!(b.muted());
    assert_eq!(b.volume(), 70);
}

mod common;

use std::sync::atomic::Ordering;
use std::time::Duration;

use common::{app_playing_in, boot, panel, tmp_root_with_carts, StubSnapshot};
use slot::app::Phase;
use slot_gfx::Draw;
use slot_input::{Action, Btn};
use slot_store::{read_slot_state, write_slot_state, SlotState, StateRing};

#[test]
fn lid_close_flushes_resume_before_dozing() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::LidClose);
    let r = StateRing::new(d.path(), "Emerald");
    assert!(
        r.read_resume().unwrap().is_some(),
        "state must be durable before doze"
    );
    assert!(matches!(a.phase(), Phase::Doze { .. }));
    assert!(
        r.list().unwrap().is_empty(),
        "lid close must not create a polaroid"
    );
}

#[test]
fn lid_open_returns_to_the_game_without_a_button() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::LidClose);
    a.apply(Action::LidOpen);
    assert!(matches!(a.phase(), Phase::Playing { .. }));
}

#[test]
fn doze_timeout_powers_off_with_the_cart_still_seated() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::LidClose);
    a.on_doze_timeout();
    assert_eq!(
        read_slot_state(d.path()).cart,
        Some("Emerald".into()),
        "power off is not an eject"
    );
}

/// Closing the lid on an empty slot is still a doze. There is nothing to flush and nothing
/// to wake back into.
#[test]
fn lid_close_on_the_shelf_wakes_back_to_the_shelf() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = boot(d.path());
    a.set_snapshot(StubSnapshot::boxed());
    a.apply(Action::LidClose);
    assert!(matches!(a.phase(), Phase::Doze { cart: None }));
    assert!(StateRing::new(d.path(), "Emerald")
        .read_resume()
        .unwrap()
        .is_none());
    a.apply(Action::LidOpen);
    assert!(matches!(a.phase(), Phase::Shelf));
}

/// The switcher is a pause over the game, so the lid closes on the game underneath it and
/// opens back onto it.
#[test]
fn lid_close_over_the_switcher_wakes_into_the_game() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::SaveState);
    a.apply(Action::Polaroids);
    a.apply(Action::LidClose);
    a.apply(Action::LidOpen);
    assert!(matches!(a.phase(), Phase::Playing { .. }));
    assert_eq!(
        StateRing::new(d.path(), "Emerald").list().unwrap().len(),
        1,
        "only the deliberate save belongs in the ring"
    );
}

/// A dozing app is still ticking, which is the only clock the timeout has — and still
/// drawing 400-700 mA behind the dark panel, which is why the timeout ends in a real power
/// off rather than a sleep this board could never wake itself from.
#[test]
fn a_doze_that_outlasts_the_timeout_powers_off_by_itself() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.set_power(panel(d.path(), Duration::from_secs(2)).0);
    a.apply(Action::LidClose);
    for _ in 0..100 {
        a.update(1.0 / 60.0);
    }
    assert!(!a.powering_off(), "1.6 s is short of the 2 s timeout");
    for _ in 0..40 {
        a.update(1.0 / 60.0);
    }
    assert!(a.powering_off());
}

#[test]
fn a_stray_doze_timeout_does_not_power_off_a_running_game() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.on_doze_timeout();
    assert!(!a.powering_off());
    assert!(matches!(a.phase(), Phase::Playing { .. }));
}

/// The panel comes up at the level the card remembers, not at whatever the kernel left it.
#[test]
fn the_backlight_follows_brightness_from_boot() {
    let d = tmp_root_with_carts(&["Emerald"]);
    write_slot_state(
        d.path(),
        &SlotState {
            brightness: 3,
            clock_set: true,
            utc_offset_min: 0,
            ..Default::default()
        },
    )
    .unwrap();
    let mut a = boot(d.path());
    let (power, step) = panel(d.path(), Duration::from_secs(60));
    a.set_power(power);
    assert_eq!(step.load(Ordering::Relaxed), 3);
    a.apply(Action::BrightnessUp);
    assert_eq!(step.load(Ordering::Relaxed), 4);
}

/// rcK stops the frontend and unloads the GPU module before the kernel is allowed to halt,
/// which takes about five seconds on this hardware. A panel that simply goes black for five
/// seconds is one the user reads as hung — this device has already been opened once over
/// exactly that confusion — so the shutdown says so, ahead of every phase and over whatever
/// was on screen.
#[test]
fn a_power_off_draws_a_shutdown_screen_over_everything() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::PowerHold);

    let mut out = Vec::new();
    a.draw(&mut out);

    match out.first() {
        Some(Draw::Rect { w, h, colour, .. }) => {
            assert_eq!(*colour, [0.0, 0.0, 0.0, 1.0], "the shutdown is black");
            assert!(*w > 0.0 && *h > 0.0, "and covers the panel");
        }
        other => panic!("the shutdown drew {other:?} rather than a panel of black"),
    }
    // One draw, not two: the line itself is a texture uploaded by the binary at boot, and a
    // unit test has no compositor to upload it with. The screen is still correct without it.
    assert_eq!(
        out.len(),
        1,
        "nothing of the previous phase survives the shutdown screen"
    );
}

/// A held POWER offers a choice rather than committing to one. Everything reachable from
/// here costs the user something — an instant resume, a three second boot, or a shutdown —
/// so the button raises the question and A answers it.
#[test]
fn a_hold_opens_the_menu_and_commits_nothing() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::PowerHold);
    assert_eq!(a.power_menu(), Some(0), "the menu opens on Restart");
    assert!(!a.powering_off() && !a.restarting());
    assert!(
        StateRing::new(d.path(), "Emerald")
            .read_resume()
            .unwrap()
            .is_some(),
        "durable before the menu is even read: the user may hold on to the PMIC's own cutoff"
    );
}

#[test]
fn the_menu_moves_and_stops_at_both_ends() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::PowerHold);
    a.apply(Action::GbaDown(Btn::Up));
    assert_eq!(a.power_menu(), Some(0), "it does not wrap off the top");
    a.apply(Action::GbaDown(Btn::Down));
    a.apply(Action::GbaDown(Btn::Down));
    assert_eq!(a.power_menu(), Some(1), "nor off the bottom");
}

#[test]
fn b_leaves_the_menu_without_doing_anything() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::PowerHold);
    a.apply(Action::GbaDown(Btn::B));
    assert_eq!(a.power_menu(), None);
    assert!(!a.powering_off() && !a.restarting());
    assert!(
        matches!(a.phase(), Phase::Playing { .. }),
        "back to the game"
    );
}

#[test]
fn each_row_commits_to_its_own_outcome() {
    for (down, want) in [(0, "restart"), (1, "off")] {
        let d = tmp_root_with_carts(&["Emerald"]);
        let mut a = app_playing_in(d.path(), "Emerald");
        a.apply(Action::PowerHold);
        for _ in 0..down {
            a.apply(Action::GbaDown(Btn::Down));
        }
        a.apply(Action::GbaDown(Btn::A));
        assert_eq!(
            a.power_menu(),
            None,
            "{want}: the menu closes on the choice"
        );
        match want {
            "restart" => assert!(a.restarting() && !a.powering_off()),
            _ => assert!(a.powering_off() && !a.restarting()),
        }
    }
}

/// The decision and the moment the machine may stop are different things. Rendering the
/// shutdown screen out of band — an extra draw and swap between the choice and `poweroff` —
/// hung the device on a GPU that was about to be torn down: slot never reached `poweroff` at
/// all, init was never signalled, and it took the PMIC held down to recover. So the ordinary
/// loop draws the screen and the binary waits for it.
#[test]
fn the_shutdown_screen_is_up_before_the_machine_may_stop() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::PowerHold);
    a.apply(Action::GbaDown(Btn::Down));
    a.apply(Action::GbaDown(Btn::A));

    assert!(a.powering_off(), "the choice decides immediately");
    assert!(
        !a.ready_to_power_off(),
        "but the binary may not act until the screen has been presented"
    );

    let mut out = Vec::new();
    a.draw(&mut out);
    assert!(
        matches!(out.first(), Some(Draw::Rect { colour, .. }) if *colour == [0.0, 0.0, 0.0, 1.0]),
        "and the screen is what the loop is drawing in the meantime"
    );

    // Absolute, not a delta: `tick_ms` takes the later of the two, so a small number is a
    // no-op against whatever clock the harness already left behind.
    a.tick_ms(600_000);
    assert!(a.ready_to_power_off(), "then it may stop");
}

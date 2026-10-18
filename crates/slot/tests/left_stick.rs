mod common;

use std::path::Path;

use common::{app_booting_at, tmp_root_with_carts, CLOCK_IS_SET};
use slot::app::App;
use slot_input::{Action, Btn};
use slot_store::{read_slot_state, write_slot_state, SlotState};
use slot_ui::{QuickRow, QuickValue};

fn boot(root: &Path, stick: bool) -> App {
    write_slot_state(
        root,
        &SlotState {
            clock_set: true,
            ..SlotState::default()
        },
    )
    .expect("write slot.state");
    let (mut a, _) = app_booting_at(root, CLOCK_IS_SET);
    a.set_left_stick(stick);
    a
}

fn press(a: &mut App, btn: Btn) {
    a.apply(Action::GbaDown(btn));
    a.apply(Action::GbaUp(btn));
}

#[test]
fn a_device_without_a_stick_hides_the_row_and_never_steers() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    let a = boot(d.path(), false);
    assert!(a.hidden_rows().contains(&QuickRow::LeftStick));
    assert!(a.in_game_hidden().contains(&QuickRow::LeftStick));
    assert!(!a.left_stick_dpad());
}

#[test]
fn the_stick_is_turned_off_from_gameplay_and_saved() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    let mut a = boot(d.path(), true);
    assert!(a.left_stick_dpad());
    a.apply(Action::QuickMenu);
    for _ in 0..QuickRow::Game.position(&[]) {
        press(&mut a, Btn::Down);
    }
    press(&mut a, Btn::A);
    for _ in 0..QuickRow::LeftStick.position(&a.hidden_rows()) {
        press(&mut a, Btn::Down);
    }
    assert_eq!(a.quick_menu(), Some(QuickRow::LeftStick));
    assert_eq!(a.quick_value(QuickRow::LeftStick), Some(QuickValue::On));
    press(&mut a, Btn::Right);
    assert_eq!(a.quick_value(QuickRow::LeftStick), Some(QuickValue::Off));
    assert!(!a.left_stick_dpad());
    assert!(
        !read_slot_state(d.path()).left_stick_dpad,
        "the choice was not saved"
    );
}

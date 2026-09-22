use slot::app::{App, Phase};
use slot_input::{Action, Btn};
use slot_ui::QuickRow;

#[test]
fn wifi_opens_from_settings_and_returns_to_the_same_row() {
    let mut app = App::new(vec![]);
    app.apply(Action::QuickMenu);
    for _ in 0..QuickRow::Wifi.index() {
        app.apply(Action::GbaDown(Btn::Down));
    }
    app.apply(Action::GbaDown(Btn::A));
    assert!(matches!(app.phase(), Phase::Wifi));
    assert!(app.wifi.status.contains("BaseOS"));
    app.apply(Action::GbaDown(Btn::B));
    assert_eq!(app.quick_menu(), Some(QuickRow::Wifi));
    app.apply(Action::GbaDown(Btn::A));
    app.apply(Action::QuickMenu);
    assert_eq!(app.quick_menu(), Some(QuickRow::Wifi));
}

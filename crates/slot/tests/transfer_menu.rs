use slot::app::{App, Phase};
use slot_input::{Action, Btn};
use slot_ui::{QuickRow, OUT_H, OUT_W};

#[test]
fn transfer_menu_handles_no_wifi_and_returns_to_settings() {
    let mut app = App::new(vec![]);
    app.apply(Action::QuickMenu);
    for _ in 0..QuickRow::FileTransfer.index() {
        app.apply(Action::GbaDown(Btn::Down));
    }
    app.apply(Action::GbaDown(Btn::A));
    assert!(matches!(app.phase(), Phase::FileTransfer));
    assert!(!app.transfer.running());
    let face = app.transfer.face();
    assert_eq!((face.w, face.h), (OUT_W, OUT_H));
    app.apply(Action::GbaDown(Btn::B));
    assert_eq!(app.quick_menu(), Some(QuickRow::FileTransfer));
    app.apply(Action::GbaDown(Btn::A));
    app.apply(Action::LidClose);
    assert!(matches!(app.phase(), Phase::Doze { .. }));
    assert!(!app.transfer.running());
}

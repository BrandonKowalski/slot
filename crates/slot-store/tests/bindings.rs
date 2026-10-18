use slot_store::{
    read_carousel, read_controls, read_shortcuts, write_carousel, write_controls, write_shortcuts,
    Binding, CarouselAction, CarouselKeys, Controls, Device, GameButton, Modifier, Platform,
    Shortcut, ShortcutKeys,
};

fn card() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("Config")).unwrap();
    d
}

fn plain(button: Device) -> Option<Binding> {
    Some(Binding {
        modifier: None,
        button,
    })
}

fn with(modifier: Modifier, button: Device) -> Option<Binding> {
    Some(Binding {
        modifier: Some(modifier),
        button,
    })
}

#[test]
fn a_gba_has_twelve_buttons_and_a_game_boy_ten() {
    assert_eq!(GameButton::of(Platform::Gba).len(), 12);
    assert_eq!(GameButton::of(Platform::Gb).len(), 10);
    assert_eq!(GameButton::of(Platform::Gbc).len(), 10);
    assert!(!GameButton::of(Platform::Gb).contains(&GameButton::L));
    assert!(GameButton::of(Platform::Gb).contains(&GameButton::TurboA));
    assert!(GameButton::of(Platform::Gbc).contains(&GameButton::TurboB));
}

#[test]
fn every_console_button_starts_on_the_device_button_of_its_name() {
    let c = Controls::default();
    assert_eq!(c.game(GameButton::A), plain(Device::A));
    assert_eq!(c.game(GameButton::L), plain(Device::L1));
    assert_eq!(c.game(GameButton::R), plain(Device::R1));
    assert_eq!(c.game(GameButton::Select), plain(Device::Select));
    assert_eq!(c.game(GameButton::TurboA), plain(Device::X));
    assert_eq!(c.game(GameButton::TurboB), plain(Device::Y));
}

#[test]
fn the_shortcuts_start_where_they_are_today() {
    let c = ShortcutKeys::default();
    assert_eq!(
        c.get(Shortcut::SaveState),
        with(Modifier::Select, Device::R1)
    );
    assert_eq!(
        c.get(Shortcut::LoadState),
        with(Modifier::Select, Device::L1)
    );
    assert_eq!(
        c.get(Shortcut::BlueLightDown),
        with(Modifier::Select, Device::Left)
    );
    assert_eq!(c.get(Shortcut::FastForward), plain(Device::R2));
    assert_eq!(c.get(Shortcut::Rewind), plain(Device::L2));
}

#[test]
fn a_binding_reads_the_way_minui_prints_it() {
    assert_eq!(Binding::text(plain(Device::A)), "A");
    assert_eq!(Binding::text(with(Modifier::Menu, Device::X)), "MENU+X");
    assert_eq!(
        Binding::text(with(Modifier::Select, Device::R1)),
        "SELECT+R1"
    );
    assert_eq!(Binding::text(None), "NONE");
}

#[test]
fn the_buttons_are_one_set_written_and_read_back() {
    let d = card();
    assert_eq!(read_controls(d.path()), Controls::default());
    let mut c = Controls::default();
    c.set_game(GameButton::A, plain(Device::B));
    c.set_game(GameButton::Start, None);
    c.set_game(GameButton::L, with(Modifier::Menu, Device::X));
    write_controls(d.path(), &c).unwrap();
    assert_eq!(read_controls(d.path()), c);
    assert!(d.path().join("Config/controls.txt").is_file());
}

#[test]
fn a_hand_written_file_reads_spaces_any_case_and_skips_what_it_cannot() {
    let d = card();
    std::fs::create_dir_all(d.path().join("Config")).unwrap();
    std::fs::write(
        d.path().join("Config/controls.txt"),
        "A = Menu+X\nstart=none\nb=nonsense\nnot_a_key=a\n",
    )
    .unwrap();
    let c = read_controls(d.path());
    assert_eq!(c.game(GameButton::A), with(Modifier::Menu, Device::X));
    assert_eq!(c.game(GameButton::Start), None);
    assert_eq!(c.game(GameButton::B), plain(Device::B));
}

#[test]
fn shortcuts_are_one_set_for_every_console_and_read_back() {
    let d = card();
    assert_eq!(read_shortcuts(d.path()), ShortcutKeys::default());
    let mut keys = ShortcutKeys::default();
    keys.set(Shortcut::Rewind, None);
    keys.set(Shortcut::SaveState, with(Modifier::Menu, Device::R1));
    write_shortcuts(d.path(), &keys).unwrap();
    assert_eq!(read_shortcuts(d.path()), keys);
    assert!(d.path().join("Config/shortcuts.txt").is_file());
}

#[test]
fn the_carousel_starts_on_the_shoulders_and_y_and_reads_back() {
    let d = card();
    let keys = CarouselKeys::default();
    assert_eq!(keys.get(CarouselAction::PreviousShelf), plain(Device::L1));
    assert_eq!(keys.get(CarouselAction::NextShelf), plain(Device::R1));
    assert_eq!(keys.get(CarouselAction::Favorite), plain(Device::Y));
    assert_eq!(read_carousel(d.path()), keys);
    let mut moved = keys;
    moved.set(CarouselAction::PreviousShelf, plain(Device::Y));
    moved.set(CarouselAction::Favorite, with(Modifier::Menu, Device::Y));
    write_carousel(d.path(), &moved).unwrap();
    assert_eq!(read_carousel(d.path()), moved);
}

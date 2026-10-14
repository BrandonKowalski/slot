mod common;

use common::tmp_root;
use slot_store::{read_slot_state, write_slot_state, Platform, Recent, SlotState};

fn favorite(platform: Platform, stem: &str) -> Recent {
    Recent {
        platform,
        stem: stem.into(),
    }
}

fn clocked() -> SlotState {
    SlotState {
        clock_set: true,
        ..SlotState::default()
    }
}

#[test]
fn nothing_is_a_favorite_until_it_is_starred() {
    let s = SlotState::default();
    assert!(s.favorites.is_empty());
    assert!(!s.is_favorite(Platform::Gba, "Emerald"));
}

#[test]
fn starring_twice_takes_the_star_back_off() {
    let mut s = clocked();
    assert!(s.toggle_favorite(Platform::Gba, "Emerald"));
    assert!(s.is_favorite(Platform::Gba, "Emerald"));
    assert!(!s.toggle_favorite(Platform::Gba, "Emerald"));
    assert!(!s.is_favorite(Platform::Gba, "Emerald"));
}

#[test]
fn the_same_name_on_two_systems_is_two_favorites() {
    let mut s = clocked();
    s.toggle_favorite(Platform::Gb, "Tetris");
    assert!(!s.is_favorite(Platform::Gbc, "Tetris"));
    s.toggle_favorite(Platform::Gbc, "Tetris");
    assert_eq!(s.favorites.len(), 2);
}

#[test]
fn favorites_survive_a_restart() {
    let d = tmp_root();
    let mut s = clocked();
    s.toggle_favorite(Platform::Gba, "Pokemon - Emerald Version (USA, Europe)");
    s.toggle_favorite(Platform::Gbc, "Tetris DX");
    write_slot_state(d.path(), &s).unwrap();
    assert_eq!(read_slot_state(d.path()), s);
}

#[test]
fn a_card_written_before_favorites_reads_with_none() {
    let d = tmp_root();
    let mut s = clocked();
    s.toggle_favorite(Platform::Gba, "Emerald");
    write_slot_state(d.path(), &s).unwrap();
    let path = d.path().join("Config/slot.state");
    let text = std::fs::read_to_string(&path).unwrap();
    let old: String = text
        .lines()
        .filter(|l| !l.starts_with("favorite="))
        .map(|l| format!("{l}\n"))
        .collect();
    std::fs::write(&path, old).unwrap();
    let s = read_slot_state(d.path());
    assert!(s.clock_set, "the old settings did not read back");
    assert!(s.favorites.is_empty());
}

#[test]
fn garbled_or_repeated_favorite_lines_are_skipped() {
    let d = tmp_root();
    let mut s = clocked();
    s.toggle_favorite(Platform::Gba, "Emerald");
    write_slot_state(d.path(), &s).unwrap();
    let path = d.path().join("Config/slot.state");
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str("favorite=n64/Mario\nfavorite=gba/\nfavorite=nothing\nfavorite=gba/Emerald\n");
    std::fs::write(&path, text).unwrap();
    assert_eq!(
        read_slot_state(d.path()).favorites,
        [favorite(Platform::Gba, "Emerald")]
    );
}

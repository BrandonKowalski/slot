//! Anything that has to reach the shelf carries a second cart: one cart on the card is a
//! dedicated device and boots past the shelf entirely.

mod common;

use common::{boot, tmp_root_with_carts};
use slot::app::{App, Phase};
use slot_input::Action;
use slot_store::{read_slot_state, write_slot_state, SlotState};

fn seated(cart: &str) -> SlotState {
    SlotState {
        cart: Some(cart.into()),
        clock_set: true,
        utc_offset_min: 0,
        ..Default::default()
    }
}

#[test]
fn boot_with_a_seated_cart_never_shows_the_shelf() {
    let d = tmp_root_with_carts(&["Emerald"]);
    write_slot_state(d.path(), &seated("Emerald")).unwrap();
    let a = App::boot(d.path());
    assert!(
        matches!(a.phase(), Phase::Inserting { .. }),
        "boot must go straight to the seated cart, not the shelf"
    );
}

#[test]
fn boot_with_an_empty_slot_shows_the_shelf() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    let a = boot(d.path());
    assert!(matches!(a.phase(), Phase::Shelf));
}

#[test]
fn boot_with_a_cart_that_no_longer_exists_falls_back_to_the_shelf() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    write_slot_state(d.path(), &seated("Deleted")).unwrap();
    let a = App::boot(d.path());
    assert!(matches!(a.phase(), Phase::Shelf));
}

/// Ejecting a resumed cart has to land on it, not on the first cart in the library.
#[test]
fn boot_leaves_the_shelf_sitting_on_the_resumed_cart() {
    let d = tmp_root_with_carts(&["Advance Wars", "Emerald", "Fire Emblem"]);
    write_slot_state(d.path(), &seated("Emerald")).unwrap();
    let mut a = App::boot(d.path());
    a.on_core_ready();
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    a.apply(Action::Eject);
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    assert!(matches!(a.phase(), Phase::Shelf));
    a.apply(Action::Insert);
    let Phase::Inserting { cart, .. } = a.phase() else {
        panic!("insert after eject did nothing: {:?}", a.phase())
    };
    assert_eq!(cart, "Emerald");
}

/// Without this the resume on the next boot has nothing to read.
#[test]
fn a_seated_cart_is_recorded_so_the_next_boot_can_resume_it() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    let mut a = boot(d.path());
    a.apply(Action::Insert);
    a.on_core_ready();
    assert_eq!(
        read_slot_state(d.path()).cart,
        None,
        "a cart that has not seated yet is not in the slot"
    );
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    assert!(matches!(a.phase(), Phase::Playing { .. }));
    assert_eq!(read_slot_state(d.path()).cart, Some("Emerald".into()));
}

/// A cart taken off the card while it was the seated one, which is a USB cable and a delete, is an
/// empty slot on the next boot, and the next write says so.
#[test]
fn a_cart_that_is_gone_takes_its_shelf_out_of_the_slot_with_it() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    write_slot_state(
        d.path(),
        &SlotState {
            cart: Some("Deleted".into()),
            clock_set: true,
            ..Default::default()
        },
    )
    .unwrap();
    let mut a = App::boot(d.path());
    // Any setting at all, because every one of them writes the whole file back.
    a.apply(Action::MuteToggle);
    let s = read_slot_state(d.path());
    assert_eq!(s.cart, None);
}

/// A refusal must not leave the slot claiming a cart that never went in.
#[test]
fn a_cart_that_fails_to_load_leaves_the_slot_empty() {
    let d = tmp_root_with_carts(&["Emerald"]);
    write_slot_state(d.path(), &seated("Emerald")).unwrap();
    let mut a = App::boot(d.path());
    a.on_core_failed();
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    assert!(matches!(a.phase(), Phase::Shelf));
    assert_eq!(read_slot_state(d.path()).cart, None);
}

/// The levels are the rest of the file. Recording a cart must not reset them.
#[test]
fn seating_a_cart_preserves_the_levels_already_in_the_file() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    write_slot_state(
        d.path(),
        &SlotState {
            cart: None,
            brightness: 2,
            blue_light: 7,
            volume: 35,
            muted: false,
            clock_set: true,
            utc_offset_min: 0,
            ..SlotState::default()
        },
    )
    .unwrap();
    let mut a = App::boot(d.path());
    a.apply(Action::Insert);
    a.on_core_ready();
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    let s = read_slot_state(d.path());
    assert_eq!((s.brightness, s.blue_light, s.volume), (2, 7, 35));
}

/// Spec section 3: a cart already in the slot shows no shelf, "not even one frame of it".
/// Task 16 only asserted the phase, so a resume that slid the cart in past a receding shelf
/// passed it. What the user sees is the game selector flashing up on every boot.
///
/// Counted against a chosen insert rather than against a fixed number: with no compositor
/// there are no uploaded faces, so carts and chrome are both plain rects and an absolute
/// count would mean nothing.
#[test]
fn a_resume_draws_no_shelf_but_a_chosen_insert_does() {
    let seated = || {
        let d = common::tmp_root_with_carts(&["Emerald", "Fusion", "Wars"]);
        write_slot_state(
            d.path(),
            &SlotState {
                cart: Some("Emerald".into()),
                clock_set: true,
                utc_offset_min: 0,
                ..Default::default()
            },
        )
        .unwrap();
        (App::boot(d.path()), d)
    };
    let chosen = || {
        let d = common::tmp_root_with_carts(&["Emerald", "Fusion", "Wars"]);
        write_slot_state(
            d.path(),
            &SlotState {
                clock_set: true,
                utc_offset_min: 0,
                ..Default::default()
            },
        )
        .unwrap();
        let mut a = App::boot(d.path());
        a.apply(Action::Insert);
        (a, d)
    };

    let (resume, _d1) = seated();
    let (pick, _d2) = chosen();
    let (r, p) = (draw_count(&resume), draw_count(&pick));
    assert!(
        p > r,
        "a resume drew {r} and a chosen insert {p}: the shelf is on screen for both"
    );

    // And it stays absent for the whole insert, not just the first frame.
    let (mut resume, _d3) = seated();
    for frame in 0..90 {
        assert!(
            draw_count(&resume) <= r,
            "frame {frame}: the shelf appeared partway through a resume"
        );
        resume.update(1.0 / 60.0);
    }
}

fn draw_count(a: &App) -> usize {
    let mut out = Vec::new();
    a.draw(&mut out);
    out.len()
}

/// A card nobody has organised yet. slot reads the platform folders and nothing else, so an
/// entirely loose card is an empty shelf — the same thing an unmounted card has always been —
/// and, crucially, boot leaves every one of those files exactly where the player put them.
///
/// Boot used to sweep them into place. It does not, and this is what stands in the place of the
/// tests that asserted it did: the promise is no longer "your files will be moved for you", it is
/// "nothing of yours will be moved at all".
#[test]
fn a_loose_card_shows_an_empty_shelf_and_nothing_on_it_is_moved() {
    let d = tempfile::tempdir().unwrap();
    for sub in ["Games", "Saves", "Labels", "States"] {
        std::fs::create_dir_all(d.path().join(sub)).unwrap();
    }
    std::fs::write(d.path().join("Games/Emerald.gba"), vec![0u8; 0x100]).unwrap();
    std::fs::write(d.path().join("Saves/Emerald.sav"), vec![7u8; 0x10000]).unwrap();
    std::fs::write(d.path().join("Labels/Emerald.png"), b"png").unwrap();
    let old_states = d.path().join("States/mgba/Emerald");
    std::fs::create_dir_all(&old_states).unwrap();
    std::fs::write(old_states.join("resume.state"), b"resume").unwrap();

    let a = App::boot(d.path());

    assert_eq!(
        a.carts().count(),
        0,
        "a loose rom reached the shelf, so something is still reading outside Games/<platform>/"
    );
    assert_eq!(
        std::fs::read(d.path().join("Games/Emerald.gba"))
            .unwrap()
            .len(),
        0x100,
        "the loose rom was moved or disturbed"
    );
    assert_eq!(
        std::fs::read(d.path().join("Saves/Emerald.sav"))
            .unwrap()
            .len(),
        0x10000,
        "the loose battery save was moved or disturbed"
    );
    assert_eq!(
        std::fs::read(d.path().join("Labels/Emerald.png")).unwrap(),
        b"png",
        "the loose label was moved"
    );
    assert_eq!(
        std::fs::read(old_states.join("resume.state")).unwrap(),
        b"resume",
        "a pre-namespacing state directory was moved"
    );
}

/// The folders that say where a hand-organised card files things are created on a card that has
/// never held slot., not merely on one that already has them. They are the only guidance there
/// is now that nothing is swept, so an empty card has to come up carrying all of them.
#[test]
fn boot_creates_the_folders_a_person_has_to_file_into() {
    let d = tempfile::tempdir().unwrap();

    App::boot(d.path());

    for name in slot::root::DIRS {
        assert!(
            d.path().join(name).is_dir(),
            "{name} is missing, so nothing on the card says where its files go"
        );
    }
}

/// And it is already home rather than travelling there.
#[test]
fn a_resumed_cart_starts_seated() {
    let d = common::tmp_root_with_carts(&["Emerald", "Fusion"]);
    write_slot_state(
        d.path(),
        &SlotState {
            cart: Some("Emerald".into()),
            clock_set: true,
            utc_offset_min: 0,
            ..Default::default()
        },
    )
    .unwrap();
    let a = App::boot(d.path());
    assert_eq!(a.seat(), 1.0, "the resumed cart is still sliding in");
}

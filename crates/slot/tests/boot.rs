//! Anything that has to reach the shelf carries a second cart: one cart on the card is a
//! dedicated device and boots past the shelf entirely.

mod common;

use common::{boot, tmp_root_with_carts};
use slot::app::{App, Phase};
use slot_input::{Action, Btn};
use slot_store::{read_slot_state, write_slot_state, Platform, SlotState};

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

/// The stem and the shelf are one fact — which cartridge is in the slot — so a boot that gives
/// up on the stem has to give up the shelf with it. It did not: the platform stayed standing in
/// memory, and the next setting the player changed wrote `cart=` with `cart_platform=gbc` beside
/// it, a card naming a shelf next to a line that names no cart.
///
/// Reached by taking a Game Boy Color cart off the card while it was the seated one, which is a
/// USB cable and a delete. Nothing reads the platform without the stem today, so the pair cost
/// nobody a boot; it is the same invariant `an_empty_slot_writes_an_empty_platform` holds for
/// every other path that empties the slot.
#[test]
fn a_cart_that_is_gone_takes_its_shelf_out_of_the_slot_with_it() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    write_slot_state(
        d.path(),
        &SlotState {
            cart: Some("Deleted".into()),
            cart_platform: Some(Platform::Gbc),
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
    assert_eq!(
        s.cart_platform, None,
        "the card names a shelf beside a line that names no cart"
    );
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

/// A card holding both Tetrises: `Games/GBA/Tetris.gba` and `Games/GB/Tetris.gb`, which is the
/// only shape of card `cart_platform` exists for. `Emerald` is there so the GBA shelf is not a
/// single cart library, which boots past the shelf entirely.
fn two_tetrises() -> tempfile::TempDir {
    let d = tmp_root_with_carts(&["Tetris", "Emerald"]);
    common::write_gb_cart(&d, "Tetris", "TETRIS");
    d
}

fn resumed(d: &tempfile::TempDir, platform: Option<Platform>) -> App {
    write_slot_state(
        d.path(),
        &SlotState {
            cart: Some("Tetris".into()),
            cart_platform: platform,
            clock_set: true,
            utc_offset_min: 0,
            ..Default::default()
        },
    )
    .unwrap();
    App::boot(d.path())
}

/// The line the card writes is the line the next boot obeys. `cart=Tetris` on its own names two
/// cartridges once a card can hold both, and the platform beside it is the only thing that says
/// which of them the player was holding.
#[test]
fn the_platform_on_the_card_decides_which_tetris_comes_back() {
    let d = two_tetrises();
    for platform in [Platform::Gb, Platform::Gba] {
        let a = resumed(&d, Some(platform));
        let cart = a.seated_cart().expect("nothing seated");
        assert_eq!(
            cart.platform, platform,
            "the card named {platform:?} and a {:?} cart came back",
            cart.platform
        );
        assert!(
            cart.rom.ends_with(format!(
                "{}/Tetris.{}",
                platform.dir_name(),
                platform.extensions()[0]
            )),
            "the rom the slot is holding is {:?}",
            cart.rom
        );
    }
}

/// Every card written before this line existed held only GBA carts, so that is what a card with
/// no line means — and it is also what slot did before the line existed, so nothing about an
/// upgraded card changes.
#[test]
fn a_card_that_never_said_resumes_the_gba_cart() {
    let d = two_tetrises();
    let a = resumed(&d, None);
    assert_eq!(
        a.seated_cart().expect("nothing seated").platform,
        Platform::Gba
    );
}

/// A named shelf is the only shelf asked. The cart of the same name on another shelf is a
/// different game with a different save, so seating it would resume a session belonging to
/// something the player never put in — an empty slot is the honest answer, and it is the one an
/// unreadable stem has always got.
#[test]
fn a_named_shelf_that_no_longer_has_the_cart_is_an_empty_slot() {
    let d = two_tetrises();
    let a = resumed(&d, Some(Platform::Gbc));
    assert!(
        matches!(a.phase(), Phase::Shelf),
        "a Colour Tetris that is not on the card seated something: {:?}",
        a.phase()
    );
}

/// The other half of the round trip: what a session actually writes down. Ringing to the Game
/// Boy shelf and seating the cart standing on it has to record that shelf, or the next boot
/// resolves the same ambiguous stem all over again and lands on the GBA cart.
#[test]
fn seating_a_cart_records_the_shelf_it_came_off() {
    let d = two_tetrises();
    let mut a = boot(d.path());
    a.apply(Action::GbaDown(Btn::R1));
    assert_eq!(
        a.selected_stem(),
        Some("Tetris"),
        "the shoulder did not ring to the Game Boy shelf"
    );
    a.apply(Action::Insert);
    a.on_core_ready();
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    assert!(matches!(a.phase(), Phase::Playing { .. }));
    let s = read_slot_state(d.path());
    assert_eq!(
        (s.cart.as_deref(), s.cart_platform),
        (Some("Tetris"), Some(Platform::Gb)),
        "the card does not say which Tetris is in the slot"
    );

    // And the next boot comes back to it rather than to the GBA cart of the same name.
    let again = App::boot(d.path());
    assert_eq!(
        again.seated_cart().expect("nothing seated").platform,
        Platform::Gb
    );
}

/// An empty slot says nothing about a platform. A shelf left naming one would be read next boot
/// beside a `cart` line naming no cart.
#[test]
fn an_eject_forgets_the_platform_with_the_cart() {
    let d = two_tetrises();
    let mut a = resumed(&d, Some(Platform::Gb));
    a.on_core_ready();
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    a.apply(Action::Eject);
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    assert!(matches!(a.phase(), Phase::Shelf));
    let s = read_slot_state(d.path());
    assert_eq!((s.cart, s.cart_platform), (None, None));
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

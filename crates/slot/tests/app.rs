mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use slot::app::{App, Phase, EJECT_S, INSERT_S, SEATED_AT};
use slot::audio::Sfx;
use slot::session::Session;
use slot::video_mode::{video_mode_for, VideoMode, VIDEO_MODE_FILE};
use slot_gfx::WHOLE_TEXTURE;
use slot_input::{Action, Btn, RawEvent};
use slot_retro::ButtonMask;
use slot_store::{write_slot_state, Cart, Core, Platform, SlotState};
use slot_ui::{
    board_at, grown, lid_at, lid_from, on_board, opening, shelf_cart_at, Draw, Placed, TexId,
    BOARD_W, CART_W, CHIP_H, CHIP_U, CHIP_V, CHIP_W, HINT_EDGE, HINT_H, LID_TURN, SLIDE_UP,
    SOCKET_H, SOCKET_U, SOCKET_V, SOCKET_W, TURN_PAD,
};

/// A tap of A, which is what plays a cart. The press alone is not enough: held, it means
/// start the cart clean, and the app cannot know which until the finger comes off.
fn play(a: &mut App) {
    a.apply(Action::GbaDown(Btn::A));
    a.apply(Action::GbaUp(Btn::A));
}

fn app_with_carts(stems: &[&str]) -> App {
    App::new(
        stems
            .iter()
            .map(|stem| Cart {
                platform: Platform::Gba,
                stem: (*stem).to_string(),
                rom: format!("Games/GBA/{stem}.gba").into(),
                label: None,
                code: String::new(),
                title: stem.to_uppercase(),
            })
            .collect(),
    )
}

/// By colour. Sizing the detector to the bands meant that reshaping the slot made it match
/// nothing, which turned one test red and made its opposite pass for the wrong reason.
fn is_mouth(d: &Draw) -> bool {
    match *d {
        Draw::Rect { colour, .. } => (0..3).all(|i| (colour[i] - opening()[i]).abs() < 0.001),
        _ => false,
    }
}

/// Two carts, because a lone cart is a dedicated device and has nowhere to eject to.
fn playing(stem: &str) -> App {
    let mut a = app_with_carts(&[stem, "Zzz"]);
    a.apply(Action::Insert);
    a.on_core_ready();
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    a
}

#[test]
fn insert_waits_for_the_core_even_after_the_animation_floor() {
    let mut a = app_with_carts(&["Emerald"]);
    a.apply(Action::Insert);
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    } // 2s, well past the floor
    assert!(
        matches!(a.phase(), Phase::Inserting { .. }),
        "advanced without the core"
    );
    a.on_core_ready();
    a.update(1.0 / 60.0);
    assert!(matches!(a.phase(), Phase::Playing { .. }));
}

#[test]
fn insert_does_not_advance_before_the_animation_floor_even_if_the_core_is_instant() {
    let mut a = app_with_carts(&["Emerald"]);
    a.apply(Action::Insert);
    a.on_core_ready();
    a.update(1.0 / 60.0);
    assert!(matches!(a.phase(), Phase::Inserting { .. }));
}

/// Seconds of frames until the app gets where it is going, giving up rather than hanging.
fn seconds_until(a: &mut App, done: fn(&App) -> bool) -> f32 {
    let mut t = 0.0;
    while !done(a) && t < 5.0 {
        a.update(1.0 / 60.0);
        t += 1.0 / 60.0;
    }
    t
}

/// Long enough to read as a cart being pushed rather than a wipe, and no longer than the
/// recording of one: the travel is cut to fit the sound, not the other way round.
#[test]
fn the_insert_reads_as_a_push_and_the_eject_takes_the_same_time() {
    let mut a = app_with_carts(&["Emerald", "Zzz"]);
    a.apply(Action::Insert);
    a.on_core_ready();
    let insert = seconds_until(&mut a, |a| matches!(a.phase(), Phase::Playing { .. }));
    a.apply(Action::Eject);
    let eject = seconds_until(&mut a, |a| matches!(a.phase(), Phase::Shelf));
    assert!(insert >= 0.4, "insert is {insert}s, still a wipe");
    // Longer than the travel, because the picture has to go out and the cart waits a beat
    // after it. That the two travels match is `the_eject_is_the_insert_run_backwards`.
    assert!(
        eject > EJECT_S,
        "the eject is {eject}s, so the cart moved before the picture was out"
    );
}

#[test]
fn the_game_does_not_appear_the_instant_the_cart_seats() {
    let mut a = app_with_carts(&["Emerald"]);
    a.apply(Action::Insert);
    a.on_core_ready();
    while a.seat() < 1.0 {
        a.update(1.0 / 60.0);
    }
    assert!(
        matches!(a.phase(), Phase::Inserting { .. }),
        "revealed on the same frame it seated"
    );
    // Derived, not counted: the beat is set from the length of the sound of the cart
    // landing, so a different recording moves it.
    let beat = ((INSERT_S - SEATED_AT) * 60.0).ceil() as u32 + 1;
    for _ in 0..beat {
        a.update(1.0 / 60.0);
    }
    assert!(matches!(a.phase(), Phase::Playing { .. }));
}

/// The game must be invisible for the whole insert, not merely dimmed. Watching it play
/// behind the cart is what made the animation feel like it was covering nothing.
///
/// Two carts, so the cart actually travels: a lone cart resumes straight into the slot and
/// the only Inserting frames are the ones the core spends loading. The sleep is the worker
/// thread's, which is the other half of the race this is about.
#[test]
fn the_game_does_not_draw_during_the_insert() {
    let d = common::tmp_root_with_real_carts(&["Emerald", "Fusion"]);
    common::clocked(d.path());
    let mut s = Session::boot(d.path().to_path_buf());
    s.feed([RawEvent::Down(Btn::A), RawEvent::Up(Btn::A)], 16);
    for i in 0..120 {
        s.update(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(1));
        if matches!(s.app().phase(), Phase::Inserting { .. }) {
            assert!(
                !s.game_visible(),
                "frame {i}: the game is playing behind the cart"
            );
        }
    }
    assert!(
        s.game_visible(),
        "the core never published, so the insert proved nothing"
    );
}

#[test]
fn the_reveal_waits_for_the_power_on_to_finish() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    let mut a = App::boot(d.path());
    a.apply(Action::Insert);
    a.on_core_ready();
    while a.seat() < 1.0 {
        a.update(1.0 / 60.0);
    }
    assert!(
        a.screen_power() < 1.0,
        "the screen was already on when the cart landed"
    );
    for _ in 0..20 {
        a.update(1.0 / 60.0);
    }
    assert!((a.screen_power() - 1.0).abs() < 0.01);
}

/// The noise belongs to the contacts, not to the button. A click at the top of the travel
/// would be a cart that announced itself before it went anywhere.
#[test]
fn the_cart_sounds_when_it_reaches_the_slot_and_not_when_it_starts_moving() {
    let mut a = app_with_carts(&["Emerald", "Zzz"]);
    a.apply(Action::Insert);
    a.update(1.0 / 60.0);
    assert_eq!(a.take_sfx(), None, "it sounded before it touched anything");
    let mut heard = None;
    while a.seat() < 1.0 && heard.is_none() {
        a.update(1.0 / 60.0);
        heard = a.take_sfx();
    }
    assert_eq!(heard, Some(Sfx::Insert));
}

/// A cart already in the slot at boot never travelled, so it never touched the rails.
#[test]
fn a_resumed_cart_makes_no_sound() {
    let d = common::tmp_root_with_carts(&["Emerald", "Zzz"]);
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
    let mut a = App::boot(d.path());
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    assert_eq!(a.take_sfx(), None);
}

/// Not when the button was held: the picture has to finish going out first, and the contacts
/// letting go is the sound of the cart starting to move rather than of the decision to move
/// it.
#[test]
fn the_cart_sounds_as_it_comes_free_and_not_before_the_screen_is_out() {
    let mut a = playing("Emerald");
    a.take_sfx();
    a.apply(Action::Eject);
    assert_eq!(a.take_sfx(), None, "it sounded over a live picture");
    let mut heard = None;
    for _ in 0..120 {
        a.update(1.0 / 60.0);
        if let Some(s) = a.take_sfx() {
            heard = Some(s);
            break;
        }
    }
    assert_eq!(heard, Some(Sfx::Eject));
    assert_eq!(a.screen_power(), 0.0, "the picture was still going out");
}

#[test]
fn a_cart_that_fails_to_load_returns_to_the_shelf() {
    let mut a = app_with_carts(&["Broken"]);
    a.apply(Action::Insert);
    a.on_core_failed();
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    assert!(matches!(a.phase(), Phase::Shelf));
}

#[test]
fn a_refused_cart_pushes_back_out_from_where_it_caught() {
    let mut a = app_with_carts(&["Broken"]);
    a.apply(Action::Insert);
    for _ in 0..6 {
        a.update(1.0 / 60.0);
    }
    let caught = a.seat();
    assert!(
        caught > 0.05 && caught < 0.95,
        "test needs a part seated cart, got {caught}"
    );
    a.on_core_failed();
    assert!(
        (a.seat() - caught).abs() < 1e-3,
        "cart jumped from {caught} to {}",
        a.seat()
    );
}

#[test]
fn an_empty_shelf_has_nothing_to_insert() {
    let mut a = app_with_carts(&[]);
    a.apply(Action::Insert);
    assert!(matches!(a.phase(), Phase::Shelf));
}

#[test]
fn face_buttons_drive_the_shelf_only_while_it_is_showing() {
    let mut a = app_with_carts(&["Emerald", "Wars"]);
    a.apply(Action::GbaDown(Btn::Right));
    play(&mut a);
    let Phase::Inserting { cart, .. } = a.phase() else {
        panic!("A on the shelf did not insert: {:?}", a.phase())
    };
    assert_eq!(cart, "Wars");

    a.on_core_ready();
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    // Left belongs to the game now and must not walk the shelf out from under it.
    a.apply(Action::GbaDown(Btn::Left));
    a.apply(Action::Eject);
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    a.apply(Action::Insert);
    let Phase::Inserting { cart, .. } = a.phase() else {
        panic!("insert after eject did nothing: {:?}", a.phase())
    };
    assert_eq!(cart, "Wars", "the game's d-pad moved the shelf behind it");
}

/// The repeat is the shelf's own, but only the app sees the up edge, so a direction let go of
/// has to reach it or the row walks on by itself.
#[test]
fn a_held_direction_walks_the_shelf_and_a_release_stops_it() {
    let mut a = app_with_carts(&["A", "B", "C", "D", "E", "F", "G"]);
    a.apply(Action::GbaDown(Btn::Right));
    for _ in 0..30 {
        a.update(1.0 / 60.0);
    }
    a.apply(Action::GbaUp(Btn::Right));
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    play(&mut a);
    let Phase::Inserting { cart, .. } = a.phase() else {
        panic!("A on the shelf did not insert: {:?}", a.phase())
    };
    assert_eq!(cart, "C", "one press and one repeat, then nothing");
}

/// One cart per pair, filed under the platform named. Which folder a cart came out of is the
/// only thing that decides which shelf it stands on — there is one shelf per platform — so a
/// test wanting more than one shelf says so here.
fn app_with_platforms(carts: &[(Platform, &str)]) -> App {
    App::new(
        carts
            .iter()
            .map(|(platform, stem)| Cart {
                platform: *platform,
                stem: (*stem).to_string(),
                rom: format!(
                    "Games/{}/{stem}.{}",
                    platform.dir_name(),
                    platform.extensions()[0]
                )
                .into(),
                label: None,
                code: String::new(),
                title: stem.to_uppercase(),
            })
            .collect(),
    )
}

/// One press of a button and the release that follows. The shelf moves on the press, so this is
/// a press that has ended rather than one being held: nothing here is testing the repeat.
fn tap(a: &mut App, btn: Btn) {
    a.apply(Action::GbaDown(btn));
    a.apply(Action::GbaUp(btn));
}

/// The shelves are a ring, so with two of them either shoulder reaches the other one and a
/// second press comes back.
#[test]
fn the_shoulders_ring_over_the_shelves() {
    let mut a = app_with_platforms(&[(Platform::Gba, "Emerald"), (Platform::Gb, "Tetris")]);
    assert_eq!(a.selected_stem(), Some("Emerald"));
    tap(&mut a, Btn::R1);
    assert_eq!(
        a.selected_stem(),
        Some("Tetris"),
        "R1 did not reach the Game Boy shelf"
    );
    tap(&mut a, Btn::R1);
    assert_eq!(
        a.selected_stem(),
        Some("Emerald"),
        "the ring did not come back round"
    );
    tap(&mut a, Btn::L1);
    assert_eq!(
        a.selected_stem(),
        Some("Tetris"),
        "L1 did not reach the Game Boy shelf"
    );
}

/// One shelf per platform, so a card with all three has three stops and the shoulders walk them
/// in the order the platforms are named in. A Colour cart stands on its own shelf: identical
/// plastic to a Game Boy cart or not, it is a different system and says so.
#[test]
fn a_colour_cart_stands_on_a_shelf_of_its_own() {
    let mut a = app_with_platforms(&[
        (Platform::Gba, "Emerald"),
        (Platform::Gb, "Tetris"),
        (Platform::Gbc, "Chromatic"),
    ]);
    tap(&mut a, Btn::R1);
    assert_eq!(a.selected_stem(), Some("Tetris"));
    tap(&mut a, Btn::R1);
    assert_eq!(
        a.selected_stem(),
        Some("Chromatic"),
        "the Colour cart shares the Game Boy shelf"
    );
    tap(&mut a, Btn::R1);
    assert_eq!(
        a.selected_stem(),
        Some("Emerald"),
        "three shelves did not ring back round to the first"
    );
    // And the other way, which with three shelves is a different route rather than the same
    // one run backwards.
    tap(&mut a, Btn::L1);
    assert_eq!(a.selected_stem(), Some("Chromatic"));
}

/// Each shelf keeps its own place. Coming back to a platform on a different cart than the one
/// it was left on would be the carousel forgetting.
#[test]
fn every_shelf_keeps_the_cart_it_was_left_on() {
    let mut a = app_with_platforms(&[
        (Platform::Gba, "Emerald"),
        (Platform::Gba, "Fusion"),
        (Platform::Gb, "Tetris"),
        (Platform::Gb, "Zelda"),
    ]);
    tap(&mut a, Btn::Right);
    assert_eq!(a.selected_stem(), Some("Fusion"));
    tap(&mut a, Btn::R1);
    assert_eq!(a.selected_stem(), Some("Tetris"));
    tap(&mut a, Btn::Right);
    assert_eq!(a.selected_stem(), Some("Zelda"));

    tap(&mut a, Btn::L1);
    assert_eq!(
        a.selected_stem(),
        Some("Fusion"),
        "the Game Boy Advance shelf forgot where it was"
    );
    tap(&mut a, Btn::R1);
    assert_eq!(
        a.selected_stem(),
        Some("Zelda"),
        "the Game Boy shelf forgot where it was"
    );
}

/// A shelf keeps its spring as well as its place, so coming back to one does not restart the
/// row sliding in from the beginning. Read off where the selected cart's own face lands, since
/// the scroll is a continuous position that the index alone cannot show.
///
/// It is also the only place the faces are proved to have been split between the shelves: a
/// build that handed every face to the first shelf would leave the Game Boy cart drawn as a
/// bare rectangle, and `Ccc`'s face would belong to somebody else.
#[test]
fn a_shelf_comes_back_settled_where_it_was_left() {
    let mut a = app_with_platforms(&[
        (Platform::Gba, "Aaa"),
        (Platform::Gba, "Bbb"),
        (Platform::Gba, "Ccc"),
        (Platform::Gb, "Tetris"),
    ]);
    let faces: Vec<TexId> = (0..4).map(|i| TexId::from_raw(700 + i)).collect();
    a.set_faces(faces.clone());
    tap(&mut a, Btn::Right);
    tap(&mut a, Btn::Right);
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    let settled = tex_at(&frame(&a), faces[2])
        .expect("no face for the selected cart")
        .1;

    tap(&mut a, Btn::R1);
    assert!(
        tex_at(&frame(&a), faces[3]).is_some(),
        "the Game Boy cart has no face of its own"
    );
    tap(&mut a, Btn::L1);
    a.update(1.0 / 60.0);
    let back = tex_at(&frame(&a), faces[2])
        .expect("no face on the way back")
        .1;
    assert!(
        (back[0] - settled[0]).abs() < 1.0,
        "the row came back at {} rather than settled at {}",
        back[0],
        settled[0]
    );
}

/// One shelf with carts on it means the buttons do nothing at all: no movement, no banner, and
/// no refusal either. A dead button is the right answer to "there is nowhere to go" — a shake
/// would be slot claiming something was wrong.
#[test]
fn the_shoulders_do_nothing_when_there_is_one_shelf_to_be_on() {
    let mut a = app_with_platforms(&[(Platform::Gba, "Emerald"), (Platform::Gba, "Fusion")]);
    for btn in [Btn::L1, Btn::R1] {
        tap(&mut a, btn);
        assert_eq!(
            a.selected_stem(),
            Some("Emerald"),
            "{btn:?} moved a shelf with no neighbour"
        );
        assert_eq!(
            a.toast(),
            None,
            "{btn:?} said something about doing nothing"
        );
        assert_eq!(
            a.shelf_shake(),
            0.0,
            "{btn:?} refused where it should have done nothing"
        );
    }
}

/// The switch says which system it landed on with the mark in the top plate's corner, and says
/// it in silence: no banner goes up over the carts, then or ever, and the mark does not fade.
/// This replaced three banners that named the shelf and then went away, which meant the one fact
/// the row could not state about itself was on screen for a second and a half out of every visit.
///
/// The faces are pushed in the way the frontend pushes them at boot, in `Platform::ALL` order,
/// and the frame is read for which of the three actually reached the corner.
#[test]
fn the_switch_changes_the_mark_on_the_case_and_says_nothing() {
    let mut a = app_with_platforms(&[
        (Platform::Gba, "Emerald"),
        (Platform::Gb, "Tetris"),
        (Platform::Gbc, "Chromatic"),
    ]);
    let marks: Vec<TexId> = (0..Platform::ALL.len())
        .map(|i| TexId::from_raw(900 + i))
        .collect();
    a.set_mark_faces(marks.clone());
    let on_the_band = |a: &App| {
        let out = frame(a);
        let found: Vec<TexId> = marks
            .iter()
            .copied()
            .filter(|m| tex_at(&out, *m).is_some())
            .collect();
        assert_eq!(found.len(), 1, "the band is showing {} marks", found.len());
        found[0]
    };

    assert_eq!(
        on_the_band(&a),
        marks[0],
        "the card did not open on the GBA"
    );
    for (btn, want) in [
        (Btn::R1, marks[1]),
        (Btn::R1, marks[2]),
        (Btn::R1, marks[0]),
        (Btn::L1, marks[2]),
    ] {
        a.apply_at(Action::GbaDown(btn), 1_000);
        assert_eq!(
            on_the_band(&a),
            want,
            "{btn:?} left the wrong mark on the case"
        );
        assert_eq!(a.toast(), None, "{btn:?} put a banner up over the carts");
    }
    // And it is still there long after any banner would have faded, because it is printed in
    // the corner rather than said.
    a.update(5.0);
    assert_eq!(on_the_band(&a), marks[2], "the mark faded");
}

/// A card whose library is all on one shelf gets no mark at all. The shoulders do nothing there
/// — there is nowhere to go — and a mark naming the platform would be the device answering a
/// question the player has no way to ask: it would sit in the corner for the life of the card
/// saying the one thing that can never change.
///
/// It is the same rule L1 and R1 follow and it is asked of the same code, so this is really
/// holding that the two cannot come apart. Both cases are read here rather than only the dead
/// one: a mark that vanished whenever the shoulders were pressed would pass a test that only
/// looked at the single-shelf card.
#[test]
fn a_card_on_one_shelf_shows_no_mark_because_there_is_nowhere_to_switch_to() {
    let marks: Vec<TexId> = (0..Platform::ALL.len())
        .map(|i| TexId::from_raw(900 + i))
        .collect();
    let marked = |a: &App| {
        let out = frame(a);
        marks.iter().any(|m| tex_at(&out, *m).is_some())
    };

    let mut alone = app_with_platforms(&[(Platform::Gba, "Emerald"), (Platform::Gba, "Fusion")]);
    alone.set_mark_faces(marks.clone());
    assert!(
        !marked(&alone),
        "a card with only Game Boy Advance carts named its platform anyway"
    );
    // The shoulders and the mark are the same rule read from two sides: nothing to switch to,
    // nothing to say.
    for btn in [Btn::L1, Btn::R1] {
        alone.apply_at(Action::GbaDown(btn), 1_000);
        assert!(
            !marked(&alone),
            "{btn:?} put a mark up on a card that has one shelf"
        );
    }

    let mut two = app_with_platforms(&[(Platform::Gba, "Emerald"), (Platform::Gb, "Tetris")]);
    two.set_mark_faces(marks.clone());
    assert!(
        marked(&two),
        "a card with two shelves did not say which one it was on"
    );
}

/// A card with no Game Boy Advance carts opens on the shelf that has some. Booting onto an
/// empty shelf beside a full one would be a device that starts by showing nothing.
#[test]
fn the_carousel_opens_on_a_shelf_that_has_carts() {
    let a = app_with_platforms(&[(Platform::Gbc, "Chromatic")]);
    assert_eq!(a.selected_stem(), Some("Chromatic"));
}

/// The shoulders are the GBA's own buttons while a game is playing, so a shelf must not move
/// under one — the row would be showing a different platform when the cart comes out.
#[test]
fn the_shoulders_belong_to_the_game_while_one_is_playing() {
    let mut a = app_with_platforms(&[
        (Platform::Gba, "Emerald"),
        (Platform::Gba, "Fusion"),
        (Platform::Gb, "Tetris"),
    ]);
    a.apply(Action::Insert);
    a.on_core_ready();
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    tap(&mut a, Btn::R1);
    assert_eq!(a.toast(), None, "the game's shoulder button named a shelf");
    assert_eq!(
        a.selected_stem(),
        Some("Emerald"),
        "the game's shoulder button switched the shelf behind it"
    );
}

/// A direction still held as a shelf leaves the screen is not held when it comes back. The
/// repeat belongs to the row being looked at, and a stale one starts the moment that row
/// returns — with nothing under the player's thumb to explain it.
#[test]
fn a_held_direction_does_not_follow_the_shelf_it_was_pressed_on() {
    let mut a = app_with_platforms(&[
        (Platform::Gba, "Aaa"),
        (Platform::Gba, "Bbb"),
        (Platform::Gba, "Ccc"),
        (Platform::Gb, "Tetris"),
    ]);
    a.apply_at(Action::GbaDown(Btn::Right), 0);
    assert_eq!(a.selected_stem(), Some("Bbb"));
    a.apply_at(Action::GbaDown(Btn::R1), 10);
    a.apply_at(Action::GbaDown(Btn::L1), 20);
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    assert_eq!(
        a.selected_stem(),
        Some("Bbb"),
        "the row walked on by itself once the shelf came back"
    );
}

#[test]
fn a_cart_in_flight_is_not_also_left_standing_on_the_shelf() {
    let mut a = app_with_carts(&["Emerald"]);
    a.apply(Action::Insert);
    a.update(0.2);
    let mut out = Vec::new();
    a.draw(&mut out);
    let carts = out
        .iter()
        .filter(|d| match **d {
            Draw::Rect { w, .. } | Draw::Tex { w, .. } | Draw::Turned { w, .. } => {
                (w - CART_W as f32).abs() < 0.01
            }
            Draw::Game | Draw::Shot { .. } => false,
        })
        .count();
    assert_eq!(
        carts, 1,
        "the shelf still holds the cart the slot is taking"
    );
}

/// The slot is part of the device, not part of the animation, so it is on screen before
/// anything is pushed into it. This test used to assert the opposite: the slot was a black
/// bar against a grey backdrop then, and hiding it was the wrong fix for the wrong problem.
#[test]
fn the_shelf_shows_the_empty_slot() {
    let a = app_with_carts(&["Emerald", "Zzz"]);
    let mut out = Vec::new();
    a.draw(&mut out);
    assert!(
        out.iter().any(is_mouth),
        "the shelf has no slot, so the cart has nowhere visible to go"
    );
}

#[test]
fn inserting_still_has_a_mouth_to_go_into() {
    let mut a = app_with_carts(&["Emerald"]);
    a.apply(Action::Insert);
    a.update(1.0 / 60.0);
    let mut out = Vec::new();
    a.draw(&mut out);
    assert!(
        out.iter().any(is_mouth),
        "the cart has nothing to slide into"
    );
}

#[test]
fn eject_returns_to_the_shelf_only_once_the_cart_is_out() {
    let mut a = playing("Emerald");
    a.apply(Action::Eject);
    dark(&mut a);
    // Stated as the thing itself rather than as a duration: the shelf is not allowed back
    // while any part of the cart is still in the slot, however long the travel and the beat
    // before it happen to be.
    while a.seat() > 0.0 {
        assert!(
            matches!(a.phase(), Phase::Ejecting { .. }),
            "the shelf came back with the cart {} of the way in",
            a.seat()
        );
        a.update(1.0 / 60.0);
    }
    a.update(1.0 / 60.0);
    assert!(matches!(a.phase(), Phase::Shelf));
}

/// Frames until the panel is out, giving up rather than hanging on a screen that never goes
/// dark. The eject is two movements now and the cart's is the second of them.
fn dark(a: &mut App) {
    for _ in 0..300 {
        if a.screen_power() == 0.0 {
            return;
        }
        a.update(1.0 / 60.0);
    }
    panic!("the screen never went dark");
}

fn lists_game(a: &App) -> bool {
    let mut out = Vec::new();
    a.draw(&mut out);
    out.iter().any(|d| matches!(d, Draw::Game))
}

/// The picture is an item in the draw list rather than a pass before it, which is what puts
/// it in front of the cart. The list is therefore also where a screen that never came up at
/// all would show, and nothing else in the tree renders one.
#[test]
fn the_game_layer_is_listed_only_once_the_screen_is_up() {
    let mut a = app_with_carts(&["Emerald", "Zzz"]);
    a.set_game_ready(true);
    a.apply(Action::Insert);
    a.on_core_ready();
    while a.seat() < 1.0 {
        a.update(1.0 / 60.0);
        assert!(
            !lists_game(&a),
            "the picture is drawn while the cart is still going in"
        );
    }
    for _ in 0..30 {
        a.update(1.0 / 60.0);
    }
    assert!(lists_game(&a), "the game never reached the draw list");
}

/// Quitting runs the insert backwards. A cart travelling out across a live picture is two
/// movements at once, and the picture is the one in front.
#[test]
fn the_cart_waits_for_the_screen_to_go_dark() {
    let d = common::tmp_root_with_carts(&["Emerald", "Zzz"]);
    let mut a = common::app_playing_in(d.path(), "Emerald");
    a.apply(Action::Eject);
    while a.screen_power() > 0.0 {
        assert_eq!(
            a.seat(),
            1.0,
            "the cart started leaving while the screen was still lit"
        );
        a.update(1.0 / 60.0);
        assert!(a.now() < 5_000, "the screen never went dark");
    }
    for _ in 0..40 {
        a.update(1.0 / 60.0);
    }
    assert!(
        a.seat() < 1.0,
        "the cart never left once the screen was dark"
    );
}

#[test]
fn eject_is_the_insert_backwards() {
    let d = common::tmp_root_with_carts(&["Emerald", "Zzz"]);
    let mut a = common::app_playing_in(d.path(), "Emerald");
    a.apply(Action::Eject);
    let first = a.screen_power();
    a.update(1.0 / 60.0);
    assert!(a.screen_power() < first, "the screen is not closing");
}

/// The quick menu, and the about screen inside it, are shelf affordances. MENU means eject and
/// polaroids once a cart is in, and a menu over a running game is a pause screen nobody asked
/// for.
#[test]
fn the_quick_menu_opens_from_the_shelf_and_nowhere_else() {
    // Two carts, or `single_cart` makes this a dedicated device: one cart is seated at boot
    // whatever the state says, and the shelf is never on screen to press MENU from.
    let d = common::tmp_root_with_carts(&["Emerald", "Fusion"]);
    let (mut s, _motor) = common::session_with_platform(d.path());

    assert!(
        matches!(s.app().phase(), slot::app::Phase::Shelf),
        "not on the shelf: {:?}",
        s.app().phase()
    );
    s.app_mut().apply(slot_input::Action::QuickMenu);
    assert!(matches!(
        s.app().phase(),
        slot::app::Phase::QuickMenu { .. }
    ));

    // And a seated cart has no quick menu at all.
    s.app_mut()
        .apply(slot_input::Action::GbaDown(slot_input::Btn::B));
    s.app_mut().apply(slot_input::Action::Insert);
    s.app_mut().apply(slot_input::Action::QuickMenu);
    assert!(
        !matches!(s.app().phase(), slot::app::Phase::QuickMenu { .. }),
        "a menu opened over a seated cart"
    );
}

/// Both ways out of the label land back on the quick menu, on its About row. MENU works as well
/// as B, so the button that brought the user here gets them back.
#[test]
fn both_b_and_menu_take_the_about_screen_back_to_the_quick_menu() {
    let d = common::tmp_root_with_carts(&["Emerald", "Fusion"]);
    for out in [
        slot_input::Action::GbaDown(slot_input::Btn::B),
        slot_input::Action::QuickMenu,
    ] {
        let (mut s, _motor) = common::session_with_platform(d.path());
        s.app_mut().apply(slot_input::Action::QuickMenu);
        for _ in 0..slot_ui::QuickRow::About.index() {
            s.app_mut()
                .apply(slot_input::Action::GbaDown(slot_input::Btn::Down));
        }
        s.app_mut()
            .apply(slot_input::Action::GbaDown(slot_input::Btn::A));
        assert!(matches!(s.app().phase(), slot::app::Phase::About));
        s.app_mut().apply(out);
        assert_eq!(
            s.app().quick_menu(),
            Some(slot_ui::QuickRow::About),
            "{out:?} did not take the label back to the menu"
        );
    }
}

/// A booted app sitting on the shelf, beside the card it reads and writes. Two carts at
/// least: one cart is a dedicated device, and `App::boot` seats it rather than leaving a
/// shelf to press anything on. `clock_set` because a card that has never been asked the
/// time opens on the clock screen, which owns every button.
fn on_shelf(stems: &[&str]) -> (tempfile::TempDir, App) {
    booted_on_shelf(common::tmp_root_with_carts(stems))
}

/// The same shelf, holding Game Boy carts instead. The folder is what gives a cart its
/// platform, so this is a different card rather than the same one with different filenames.
fn on_shelf_gb(stems: &[&str]) -> (tempfile::TempDir, App) {
    booted_on_shelf(common::tmp_root_with_gb_carts(stems))
}

fn booted_on_shelf(d: tempfile::TempDir) -> (tempfile::TempDir, App) {
    write_slot_state(
        d.path(),
        &SlotState {
            clock_set: true,
            ..Default::default()
        },
    )
    .unwrap();
    let app = App::boot(d.path());
    (d, app)
}

/// Long enough for the close to put the lid back, with room to spare.
fn let_it_close(app: &mut App) {
    app.update(0.4);
}

/// Long enough for a hop to land.
fn let_it_hop(app: &mut App) {
    app.update(0.25);
}

/// What START left behind: the picker it opened, the shelf's shake, and whether the drawn shelf
/// moved at all. The twin is the same card booted a second time and advanced beside it, so
/// "nothing moved" is measured against the shelf as it stands at that instant rather than
/// against a frame taken before the press — the row is a spring, and the two are not the same
/// picture on principle.
fn start_on(mut pressed: App, mut untouched: App) -> (Option<Core>, f32, bool) {
    fake_picker_faces(&mut pressed);
    fake_picker_faces(&mut untouched);
    pressed.apply(Action::GbaDown(Btn::Start));
    let_it_hop(&mut pressed);
    let_it_hop(&mut untouched);
    let moved = frame(&pressed) != frame(&untouched);
    (pressed.core_picker(), pressed.shelf_shake(), moved)
}

/// The picker's board art is a traced GBA cartridge PCB — 32 contacts and a GBA ROM package — so
/// opening a Game Boy shell onto it would show the player hardware that is not in their hand.
/// There is no core to choose for one either: mGBA is the only core that runs a Game Boy game,
/// and the ini gets no say in it. Nothing to offer and nothing to refuse, so START does nothing
/// at all — not even the shake, on the same reasoning as inert L1/R1 where there is only one
/// shelf. A Game Boy board is on the backlog.
#[test]
fn start_opens_no_picker_on_a_game_boy_cart() {
    let (_d, gb) = on_shelf_gb(&["Tetris", "Zzz"]);
    let (_twin, gb_twin) = on_shelf_gb(&["Tetris", "Zzz"]);
    let (picker, shake, moved) = start_on(gb, gb_twin);
    assert_eq!(picker, None, "a Game Boy cart opened the GBA picker");
    assert_eq!(
        shake, 0.0,
        "START on a Game Boy cart was answered with a refusal shake"
    );
    assert!(!moved, "START changed what the shelf draws");

    // The control, without which "nothing moved" would pass just as well with START unbound
    // outright: the same press on a GBA cart does open the picker, and the shelf does move.
    let (_d, gba) = on_shelf(&["Emerald", "Zzz"]);
    let (_twin, gba_twin) = on_shelf(&["Emerald", "Zzz"]);
    let (picker, _, moved) = start_on(gba, gba_twin);
    assert_eq!(picker, Some(Core::Mgba), "START stopped opening the picker");
    assert!(moved, "the picker opened and the shelf drew the same frame");
}

/// Opening on mGBA whatever the cart runs would be a board that says every cart runs mGBA,
/// which is a lie the moment one of them does not.
#[test]
fn start_on_the_shelf_opens_the_core_picker_on_the_carts_current_core() {
    let (d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    assert_eq!(app.selected_stem(), Some("Emerald"));

    app.apply(Action::GbaDown(Btn::Start));
    assert_eq!(
        app.core_picker(),
        Some(Core::Mgba),
        "a cart with no line of its own runs the default core"
    );
    app.apply(Action::GbaDown(Btn::B));
    let_it_close(&mut app);

    slot_store::write_selected_core(d.path(), "Emerald", Core::Gpsp).unwrap();
    app.apply(Action::GbaDown(Btn::Start));
    assert_eq!(
        app.core_picker(),
        Some(Core::Gpsp),
        "the chip should start in the core the cart already uses"
    );
}

/// The write happens on the press, and the lid then takes its time going back on.
#[test]
fn choosing_a_core_writes_it_and_closes() {
    let (d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    app.apply(Action::GbaDown(Btn::Right));
    app.apply(Action::GbaDown(Btn::A));

    assert_eq!(slot_store::core_for(d.path(), "Emerald"), Core::Gpsp);
    assert_eq!(
        slot_store::core_for(d.path(), "Zzz"),
        Core::Mgba,
        "the choice landed on a cart the shelf was not on"
    );
    let_it_close(&mut app);
    assert_eq!(
        app.core_picker(),
        None,
        "the picker stayed open after a choice"
    );
}

#[test]
fn b_closes_the_picker_without_writing() {
    let (d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    app.apply(Action::GbaDown(Btn::Right));
    app.apply(Action::GbaDown(Btn::B));
    let_it_close(&mut app);

    assert_eq!(app.core_picker(), None);
    assert_eq!(
        slot_store::core_for(d.path(), "Emerald"),
        Core::Mgba,
        "backing out of the picker still changed the cart"
    );
}

/// The sockets sit left and right, so the arrows point at them: no wrapping. Toward the socket
/// the chip is already in, only the chip shakes.
#[test]
fn the_chip_goes_where_the_arrow_points_and_does_not_wrap() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));

    app.apply(Action::GbaDown(Btn::Left));
    assert_eq!(
        app.core_picker(),
        Some(Core::Mgba),
        "left from mGBA wrapped round"
    );
    assert_ne!(
        app.core_picker_chip().unwrap().shake,
        0.0,
        "a press toward the chip's own socket went unanswered"
    );
    assert_eq!(
        app.shelf_shake(),
        0.0,
        "the shelf shook as well as the chip"
    );

    app.apply(Action::GbaDown(Btn::Right));
    assert_eq!(app.core_picker(), Some(Core::Gpsp));
    let_it_hop(&mut app);
    app.apply(Action::GbaDown(Btn::Right));
    assert_eq!(
        app.core_picker(),
        Some(Core::Gpsp),
        "right from gpSP wrapped round"
    );
}

/// `the_chip_goes_where_the_arrow_points_and_does_not_wrap` asserts this too, but nothing in it
/// ever refuses the shelf, so that assertion passes whether or not `shelf_shake` still guards on
/// the picker being up. Arming a real refusal first is what makes the guard's absence show.
#[test]
fn a_shelf_refusal_does_not_shake_once_the_picker_takes_over() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    app.refuse();
    app.apply(Action::GbaDown(Btn::Start));
    assert_eq!(
        app.shelf_shake(),
        0.0,
        "the shelf shook under an open picker"
    );
}

#[test]
fn back_mid_hop_turns_round_and_onward_does_nothing() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    app.apply(Action::GbaDown(Btn::Right));
    app.update(0.05);

    app.apply(Action::GbaDown(Btn::Right));
    assert_eq!(app.core_picker(), Some(Core::Gpsp));
    assert_eq!(
        app.core_picker_chip().unwrap().shake,
        0.0,
        "onward mid-hop was refused"
    );

    app.apply(Action::GbaDown(Btn::Left));
    assert_eq!(
        app.core_picker(),
        Some(Core::Mgba),
        "back mid-hop did not turn the chip round"
    );
}

#[test]
fn a_mid_hop_writes_where_the_chip_is_heading() {
    let (d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    app.apply(Action::GbaDown(Btn::Right));
    app.update(0.05);
    app.apply(Action::GbaDown(Btn::A));
    assert_eq!(slot_store::core_for(d.path(), "Emerald"), Core::Gpsp);
}

/// A on the shelf inserts on the release of a press the shelf saw. The press that saved went
/// to the picker, so its release — however late — must not start the cart.
#[test]
fn the_a_that_saved_does_not_start_the_cart() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    app.apply(Action::GbaDown(Btn::Start));
    app.apply(Action::GbaDown(Btn::Right));
    app.apply(Action::GbaDown(Btn::A));
    let_it_close(&mut app);
    assert_eq!(app.core_picker(), None);

    app.apply(Action::GbaUp(Btn::A));
    assert!(
        matches!(app.phase(), Phase::Shelf),
        "releasing the A that saved inserted the cart: {:?}",
        app.phase()
    );
}

#[test]
fn presses_during_the_close_and_keys_the_picker_does_not_use_do_nothing() {
    let (d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    for key in [Btn::Up, Btn::Down, Btn::Start, Btn::Select] {
        app.apply(Action::GbaDown(key));
        assert_eq!(
            app.core_picker(),
            Some(Core::Mgba),
            "{key:?} moved the chip"
        );
    }

    app.apply(Action::GbaDown(Btn::B));
    app.apply(Action::GbaDown(Btn::Right));
    app.apply(Action::GbaDown(Btn::A));
    let_it_close(&mut app);
    assert_eq!(app.core_picker(), None);
    assert_eq!(
        slot_store::core_for(d.path(), "Emerald"),
        Core::Mgba,
        "a press during the close wrote a core"
    );
}

/// A shut lid is walking away. The picker goes at once and writes nothing, so waking lands on a
/// plain shelf.
#[test]
fn shutting_the_lid_closes_the_picker_without_writing() {
    let (d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    app.apply(Action::GbaDown(Btn::Right));
    app.apply(Action::LidClose);
    assert_eq!(app.core_picker(), None, "the picker survived the lid");
    assert_eq!(slot_store::core_for(d.path(), "Emerald"), Core::Mgba);
}

/// A menu that let the thing behind it move would act on a different cart than the one it
/// named when it opened.
#[test]
fn the_picker_swallows_the_shelf_arrows() {
    let (_d, mut app) = on_shelf(&["Emerald", "Metroid Fusion"]);
    app.apply(Action::GbaDown(Btn::Start));
    app.apply(Action::GbaDown(Btn::Right));
    app.apply(Action::GbaDown(Btn::B));
    assert_eq!(
        app.selected_stem(),
        Some("Emerald"),
        "the shelf moved underneath an open picker"
    );
}

/// An A pressed just before START belonged to the shelf that was showing when it went down.
/// Opening the picker has to let go of it, or its 500 ms hold still runs out underneath the
/// open cart and inserts it clean, skipping the resume the shelf would otherwise have offered.
#[test]
fn opening_the_picker_lets_go_of_a_play_hold_already_armed() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    app.apply(Action::GbaDown(Btn::A));
    app.apply(Action::GbaDown(Btn::Start));
    app.apply(Action::GbaUp(Btn::A));
    app.update(0.6);
    assert!(
        matches!(app.phase(), Phase::Shelf),
        "the held A inserted the cart under the open picker: {:?}",
        app.phase()
    );
}

/// A direction still repeating when START goes down is the shelf's, not the picker's: it must
/// stop, the same as when the shelf leaves the screen any other way, or the row keeps moving
/// underneath the cart that is supposedly open.
#[test]
fn opening_the_picker_lets_go_of_a_direction_still_held() {
    let (_d, mut app) = on_shelf(&["Emerald", "Metroid Fusion", "Zzz"]);
    app.apply(Action::GbaDown(Btn::Right));
    app.apply(Action::GbaDown(Btn::Start));
    app.update(0.6);
    assert_eq!(
        app.selected_stem(),
        Some("Metroid Fusion"),
        "the shelf moved underneath the open picker"
    );
}

/// Nothing to configure with no cart under the highlight, and a picker that wrote to an
/// empty stem would leave a line for a cart that is not there.
#[test]
fn the_picker_does_not_open_on_an_empty_shelf() {
    let (_d, mut app) = on_shelf(&[]);
    app.apply(Action::GbaDown(Btn::Start));
    assert_eq!(app.core_picker(), None);
}

/// The picker is on START because SELECT is the chord key. Held, SELECT turns Up/Down into
/// brightness and Left/Right into blue light, and `adjust` answers those on the shelf as
/// readily as in a game. A picker on SELECT would have to choose between eating the first
/// half of every one of those chords and putting the 600 ms chord window in front of the
/// menu; START is bound to nothing here and owes neither.
#[test]
fn select_on_the_shelf_leaves_the_picker_shut_so_it_can_still_chord() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);

    app.apply(Action::GbaDown(Btn::Select));
    assert_eq!(
        app.core_picker(),
        None,
        "SELECT must stay free for brightness and blue light on the shelf"
    );

    // That SELECT+Up actually yields BrightnessUp is the gesture layer's to prove, and it
    // does: see the chord table test in slot-input/tests/gesture.rs. What this layer owes is
    // only that the shelf does not intercept SELECT before the chord can form.
}

/// Stand-ins for everything the frontend uploads for the picker, so the draw can be read back
/// without a compositor. Every id distinct.
struct PickerFaces {
    board: TexId,
    lid: TexId,
    sockets: [TexId; 2],
    chips: [TexId; 2],
    blank: TexId,
    shadow: TexId,
    legend: [(TexId, u32); 3],
}

/// Only the faces the frontend uploads at boot: the sockets, the chips and the legend. The board
/// and the lid are the highlighted cart's, and are not set.
fn fake_boot_faces(app: &mut App) -> PickerFaces {
    let id = TexId::from_raw;
    let f = PickerFaces {
        board: id(900),
        lid: id(901),
        sockets: [id(902), id(903)],
        chips: [id(904), id(905)],
        blank: id(906),
        shadow: id(907),
        legend: [(id(908), 60), (id(909), 90), (id(910), 80)],
    };
    app.set_core_part_faces(f.sockets.to_vec(), f.chips.to_vec(), f.blank, f.shadow);
    app.set_core_legend_faces(f.legend.to_vec());
    f
}

/// Everything the picker draws, this cart's board and lid included, so START opens the cart at
/// once instead of leaving it waiting on the shelf, where the arrows do nothing.
fn fake_picker_faces(app: &mut App) -> PickerFaces {
    let f = fake_boot_faces(app);
    app.set_core_board_faces(f.board, f.lid);
    f
}

/// Where the row is standing the selected cart, read off the frame rather than assumed. A shelf
/// holding two carts centres the pair rather than the selection, so the middle of the screen is
/// not always the answer — and the picker and the slot both take the cart from here.
fn resting_cart(out: &[Draw]) -> f32 {
    out.iter()
        .filter_map(|d| match *d {
            Draw::Rect { x, w, .. } | Draw::Tex { x, w, .. } => Some((x, w)),
            _ => None,
        })
        .find(|(_, w)| (w - CART_W as f32).abs() < 0.01)
        .expect("no cart standing on the row")
        .0
}

fn frame(app: &App) -> Vec<Draw> {
    let mut out = Vec::new();
    app.draw(&mut out);
    out
}

/// Where a plain face landed: its place in the frame, and its rect.
fn tex_at(out: &[Draw], want: TexId) -> Option<(usize, [f32; 4])> {
    out.iter().enumerate().find_map(|(i, d)| match *d {
        Draw::Tex {
            x, y, w, h, tex, ..
        } if tex == want => Some((i, [x, y, w, h])),
        _ => None,
    })
}

/// Every place one face landed, in frame order. The chip's shadow is drawn twice while the
/// chip is in the air: once under the lid, once under the chip.
fn tex_all(out: &[Draw], want: TexId) -> Vec<(usize, [f32; 4])> {
    out.iter()
        .enumerate()
        .filter_map(|(i, d)| match *d {
            Draw::Tex {
                x, y, w, h, tex, ..
            } if tex == want => Some((i, [x, y, w, h])),
            _ => None,
        })
        .collect()
}

/// Where a turned face landed, and its turn.
fn turned_at(out: &[Draw], want: TexId) -> Option<(usize, [f32; 4], f32)> {
    out.iter().enumerate().find_map(|(i, d)| match *d {
        Draw::Turned {
            x,
            y,
            w,
            h,
            tex,
            turn,
            ..
        } if tex == want => Some((i, [x, y, w, h], turn)),
        _ => None,
    })
}

fn near(a: [f32; 4], b: [f32; 4]) -> bool {
    a.iter().zip(b).all(|(p, q)| (p - q).abs() < 0.01)
}

/// How opaque a plain face was drawn.
fn alpha_of(out: &[Draw], want: TexId) -> Option<f32> {
    out.iter().find_map(|d| match *d {
        Draw::Tex { tex, alpha, .. } if tex == want => Some(alpha),
        _ => None,
    })
}

/// The sockets and the chip seated in mGBA ride `board` wherever it is and at whatever size it
/// is: a missing `* zoom` anywhere in that chain would separate them from it well before the
/// movement settles.
fn assert_parts_on_board(out: &[Draw], f: &PickerFaces, board: Placed, when: &str) {
    let zoom = board.w / BOARD_W as f32;
    for (i, socket) in f.sockets.iter().enumerate() {
        let (x, y) = on_board(board, SOCKET_U[i], SOCKET_V);
        let (_, at) = tex_at(out, *socket).unwrap_or_else(|| panic!("a socket is missing {when}"));
        assert!(
            near(
                at,
                [
                    x.round(),
                    y.round(),
                    SOCKET_W as f32 * zoom,
                    SOCKET_H as f32 * zoom
                ]
            ),
            "socket {i} at {at:?} {when}"
        );
    }
    let (cx, cy) = on_board(board, CHIP_U[0], CHIP_V);
    let want_chip = grown(
        Placed {
            x: cx,
            y: cy,
            w: CHIP_W as f32 * zoom,
            h: CHIP_H as f32 * zoom,
        },
        TURN_PAD as f32 * zoom,
    );
    let (_, chip, _) =
        turned_at(out, f.chips[0]).unwrap_or_else(|| panic!("no seated chip {when}"));
    assert!(
        near(
            chip,
            [
                want_chip.x.round(),
                want_chip.y.round(),
                want_chip.w,
                want_chip.h
            ]
        ),
        "the chip is not riding the board {when}: {chip:?}"
    );
}

/// Quads a shelf cart's width other than the open cart's board: the highlighted cart standing in
/// the row. Its neighbours are drawn smaller.
fn carts_standing(out: &[Draw], board: TexId) -> usize {
    out.iter()
        .filter(|d| match **d {
            Draw::Rect { w, .. } => (w - CART_W as f32).abs() < 0.01,
            Draw::Tex { w, tex, .. } => tex != board && (w - CART_W as f32).abs() < 0.01,
            _ => false,
        })
        .count()
}

/// A picker still waiting on its cart's faces draws nothing of its own: the shelf is the shelf,
/// with the highlighted cart standing in the row.
fn assert_plain_shelf(out: &[Draw], f: &PickerFaces, when: &str) {
    assert!(
        !out.iter().any(|d| matches!(d, Draw::Turned { .. })),
        "a turned face is drawn {when}"
    );
    let picker = [
        f.board,
        f.lid,
        f.sockets[0],
        f.sockets[1],
        f.chips[0],
        f.chips[1],
        f.blank,
        f.shadow,
        f.legend[0].0,
        f.legend[1].0,
        f.legend[2].0,
    ];
    let drawn: Vec<TexId> = out
        .iter()
        .filter_map(|d| match *d {
            Draw::Tex { tex, .. } if picker.contains(&tex) => Some(tex),
            _ => None,
        })
        .collect();
    assert!(drawn.is_empty(), "the picker drew {drawn:?} {when}");
    assert_eq!(
        carts_standing(out, f.board),
        1,
        "the highlighted cart is not standing in the row {when}"
    );
}

/// Long enough for the lid to come off.
fn let_it_open(app: &mut App) {
    app.update(0.5);
}

/// At rest: the board where the mockup has it, both sockets on it, the chip seated in the cart's
/// own core, the lid lifted and turned, and the legend — all after the shelf's own slot, so over
/// the shelf rather than under it.
#[test]
fn the_open_cart_rests_over_the_shelf_with_its_lid_turned() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let f = fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    let_it_open(&mut app);
    let out = frame(&app);

    let rest = board_at(1.0);
    let (board_i, board) = tex_at(&out, f.board).expect("no board");
    assert!(
        near(board, [rest.x, rest.y, rest.w, rest.h]),
        "board at {board:?}"
    );
    let mouth = out
        .iter()
        .rposition(is_mouth)
        .expect("no slot on the shelf");
    assert!(board_i > mouth, "the board is drawn under the shelf");

    for (i, socket) in f.sockets.iter().enumerate() {
        let (x, y) = on_board(rest, SOCKET_U[i], SOCKET_V);
        let (_, at) = tex_at(&out, *socket).expect("a socket is missing");
        assert!(
            near(at, [x.round(), y.round(), SOCKET_W as f32, SOCKET_H as f32]),
            "socket {i} at {at:?}"
        );
    }

    let (chip_i, chip, chip_turn) =
        turned_at(&out, f.chips[0]).expect("the chip is not seated in mGBA");
    assert_eq!(chip_turn, 0.0, "a seated chip is tipped");
    let (cx, cy) = on_board(rest, CHIP_U[0], CHIP_V);
    let want_chip = grown(
        Placed {
            x: cx,
            y: cy,
            w: CHIP_W as f32,
            h: CHIP_H as f32,
        },
        TURN_PAD as f32,
    );
    assert!(
        near(
            chip,
            [
                want_chip.x.round(),
                want_chip.y.round(),
                want_chip.w,
                want_chip.h
            ]
        ),
        "the seated chip is not in mGBA's socket: {chip:?}"
    );
    assert!(turned_at(&out, f.blank).is_none(), "a blank chip at rest");

    let (lid_rest, turn) = lid_at(1.0);
    let want = grown(lid_rest, TURN_PAD as f32 * lid_rest.w / CART_W as f32);
    let (lid_i, lid, lid_turn) = turned_at(&out, f.lid).expect("no lid");
    assert!(
        near(lid, [want.x, want.y, want.w, want.h]),
        "lid at {lid:?}"
    );
    assert_eq!((lid_turn, turn), (LID_TURN, LID_TURN));
    assert!(lid_i > chip_i, "the chip is drawn over the lid");

    // The soft oval on the ground under the lid, where the mockup has it: centred on
    // (360, 140), and under the lid rather than over it. A seated chip casts none.
    let shadows = tex_all(&out, f.shadow);
    assert_eq!(
        shadows.len(),
        1,
        "expected the lid's shadow alone, got {shadows:?}"
    );
    let (shadow_i, s) = shadows[0];
    assert!(
        (s[0] + s[2] / 2.0 - 360.0).abs() < 0.01 && (s[1] + s[3] / 2.0 - 140.0).abs() < 0.01,
        "the lid's shadow is not under it: {s:?}"
    );
    assert!(shadow_i < lid_i, "the lid's shadow is drawn over the lid");

    // Cancel under the cart's left edge, Swap centred by what shows, Choose's word ending under
    // the cart's right edge. A hint face's last HINT_EDGE pixels are transparent, so they do not
    // count toward where it sits.
    let [cancel, swap, choose] = f.legend;
    let at = |tex| tex_at(&out, tex).expect("a legend hint is missing").1;
    let seen = |w: u32| (w - HINT_EDGE) as f32;
    assert_eq!(at(cancel.0), [174.0, 386.0, cancel.1 as f32, HINT_H as f32]);
    assert_eq!(
        at(swap.0)[0] + seen(swap.1) / 2.0,
        360.0,
        "Swap is off the panel's centre"
    );
    assert_eq!(
        at(choose.0)[0] + seen(choose.1),
        546.0,
        "Choose does not end at the cart's edge"
    );
    assert_eq!((at(swap.0)[1], at(choose.0)[1]), (386.0, 386.0));
}

/// A face drawn at its own size is only sharp on whole pixels. At a fractional place the linear
/// filter splits each 1 px line of a socket's silkscreen across two pixels at half strength, and
/// the empty socket's outline goes faint.
#[test]
fn the_sockets_and_the_seated_chip_rest_on_whole_pixels() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let f = fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    let_it_open(&mut app);
    let out = frame(&app);

    let whole = |r: [f32; 4]| r[0].fract() == 0.0 && r[1].fract() == 0.0;
    for (i, socket) in f.sockets.iter().enumerate() {
        let (_, at) = tex_at(&out, *socket).expect("a socket is missing");
        assert!(whole(at), "socket {i} is off the pixel grid at {at:?}");
    }
    let (_, chip, _) = turned_at(&out, f.chips[0]).expect("the chip is not seated in mGBA");
    assert!(
        whole(chip),
        "the seated chip is off the pixel grid at {chip:?}"
    );
}

/// The row parts to where the mockup stands the neighbours and dims them to a quarter, as it has
/// them. The recede alone, set by where they stand, left their faces at 0.41.
#[test]
fn the_neighbours_dim_to_a_quarter_while_a_cart_is_open() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let faces = vec![TexId::from_raw(920), TexId::from_raw(921)];
    app.set_faces(faces.clone());
    fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    let_it_open(&mut app);
    let out = frame(&app);

    let alpha = out
        .iter()
        .find_map(|d| match *d {
            Draw::Tex { tex, alpha, .. } if tex == faces[1] => Some(alpha),
            _ => None,
        })
        .expect("the neighbour is not on screen while the cart is open");
    assert!(
        (alpha - 0.25).abs() <= 0.01,
        "the neighbour's face is at {alpha}, not a quarter"
    );
}

/// Which socket the chip lands in follows the arrow. A swapped `CHIP_U` index, or an `across`
/// inverted from what the picker reports, would still draw a chip named `chips[1]` somewhere on
/// the board and pass a test that only asked whether it was there.
#[test]
fn the_seated_chip_moves_to_the_socket_it_hopped_to() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let f = fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    let_it_open(&mut app);
    app.apply(Action::GbaDown(Btn::Right));
    let_it_hop(&mut app);
    let out = frame(&app);

    let (x, y) = on_board(board_at(1.0), CHIP_U[1], CHIP_V);
    let want = grown(
        Placed {
            x,
            y,
            w: CHIP_W as f32,
            h: CHIP_H as f32,
        },
        TURN_PAD as f32,
    );
    let (_, chip, _) = turned_at(&out, f.chips[1]).expect("the chip did not land in gpSP");
    assert!(
        near(chip, [want.x.round(), want.y.round(), want.w, want.h]),
        "the gpSP chip is not in gpSP's socket: {chip:?}"
    );
    assert!(
        turned_at(&out, f.chips[0]).is_none(),
        "mGBA's chip is still drawn once the hop lands in gpSP"
    );
}

/// The lid is the highlighted cart. On the first frame it stands exactly where the shelf stood
/// it, and the row does not draw a second copy underneath.
#[test]
fn the_highlighted_cart_becomes_the_lid_rather_than_a_second_cart() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let f = fake_picker_faces(&mut app);
    let rest = resting_cart(&frame(&app));
    app.apply(Action::GbaDown(Btn::Start));
    let out = frame(&app);

    let standing = out
        .iter()
        .filter(|d| match **d {
            Draw::Rect { w, .. } => (w - CART_W as f32).abs() < 0.01,
            Draw::Tex { w, tex, .. } if tex != f.board => (w - CART_W as f32).abs() < 0.01,
            _ => false,
        })
        .count();
    assert_eq!(
        standing, 0,
        "the row still draws the cart whose lid is coming off"
    );

    let (_, lid, turn) = turned_at(&out, f.lid).expect("no lid");
    let want = grown(lid_from(shelf_cart_at(rest, 1.0), 0.0).0, TURN_PAD as f32);
    assert!(
        near(lid, [want.x, want.y, want.w, want.h]),
        "the lid starts at {lid:?}"
    );
    assert_eq!(turn, 0.0);

    // Halfway through the slide the front half has come up half its travel, level and still the
    // shelf cart's width, off a back half standing exactly where the shelf stood the cart, opaque,
    // with the sockets and the chip already on it. 80.5 ms rather than 80: the app's clock counts
    // whole milliseconds, and 0.08 s adds up to a hair under 80.
    app.update(0.0805);
    let slid = frame(&app);
    let (_, lid, turn) = turned_at(&slid, f.lid).expect("the lid vanished mid-slide");
    assert_eq!(turn, 0.0, "the lid turned while it slid");
    let shelf = shelf_cart_at(rest, 1.0);
    let want = grown(
        Placed {
            y: shelf.y - SLIDE_UP * 0.5,
            ..shelf
        },
        TURN_PAD as f32,
    );
    assert!(
        near(lid, [want.x, want.y, want.w, want.h]),
        "the lid is not half way up its slide: {lid:?}"
    );
    let (_, board) = tex_at(&slid, f.board).expect("no board mid-slide");
    assert_eq!(
        board,
        [shelf.x, shelf.y, shelf.w, shelf.h],
        "the back half moved while the front slid"
    );
    assert_eq!(
        alpha_of(&slid, f.board),
        Some(1.0),
        "the back half is not opaque under the front"
    );
    assert_parts_on_board(&slid, &f, shelf, "mid-slide");

    // Halfway through the lift, the lid is on its way up and turning, over a back half that is
    // part grown and still opaque.
    app.update(0.21);
    let lifting = frame(&app);
    let (_, lid, turn) = turned_at(&lifting, f.lid).expect("the lid vanished mid-lift");
    assert!(
        turn < 0.0 && turn > LID_TURN,
        "the lid is not turning: {turn}"
    );
    assert!(
        lid[1] < lid_at(0.0).0.y && lid[1] > lid_at(1.0).0.y,
        "the lid is not on its way up: {lid:?}"
    );
    let (_, [bx, by, bw, bh]) = tex_at(&lifting, f.board).expect("no board mid-lift");
    assert!(
        bw > board_at(0.0).w && bw < board_at(1.0).w,
        "the board is not growing: {bw}"
    );
    assert_eq!(
        alpha_of(&lifting, f.board),
        Some(1.0),
        "the back half faded mid-lift"
    );
    assert_parts_on_board(
        &lifting,
        &f,
        Placed {
            x: bx,
            y: by,
            w: bw,
            h: bh,
        },
        "mid-lift",
    );
}

#[test]
fn mid_hop_the_chip_is_blank_tipped_and_off_the_board() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let f = fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    let_it_open(&mut app);
    let (_, seated, _) = turned_at(&frame(&app), f.chips[0]).expect("no seated chip");

    app.apply(Action::GbaDown(Btn::Right));
    app.update(0.09);
    let out = frame(&app);
    let (_, flying, tip) = turned_at(&out, f.blank).expect("no chip in flight");
    assert!(tip > 0.0, "a chip heading right does not lean right");
    assert!(flying[1] < seated[1], "the chip is not off the board");
    // Between the two sockets rather than merely "somewhere off the board": an inverted
    // `across` would send it the wrong way and still clear the two checks above.
    let board = board_at(1.0);
    let left = on_board(board, CHIP_U[0], CHIP_V).0 + CHIP_W as f32 / 2.0;
    let right = on_board(board, CHIP_U[1], CHIP_V).0 + CHIP_W as f32 / 2.0;
    let mid = flying[0] + flying[2] / 2.0;
    assert!(
        mid > left && mid < right,
        "the flying chip is not between the sockets: {mid} not in ({left}, {right})"
    );
    assert_eq!(
        tex_all(&out, f.shadow).len(),
        2,
        "nothing under the chip in flight, beside the lid's own shadow"
    );
    assert!(
        f.chips.iter().all(|c| turned_at(&out, *c).is_none()),
        "a named chip is drawn mid-hop"
    );
    for socket in f.sockets {
        assert!(tex_at(&out, socket).is_some(), "a socket is hidden mid-hop");
    }
}

/// A refusal moves the chip and nothing else on the panel.
#[test]
fn the_chip_alone_shakes_when_refused() {
    let (_d, mut app) = on_shelf(&["Emerald", "Metroid Fusion", "Zzz"]);
    let f = fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    let_it_open(&mut app);
    let before = frame(&app);
    app.apply(Action::GbaDown(Btn::Left));
    let after = frame(&app);

    let (_, still, _) = turned_at(&before, f.chips[0]).unwrap();
    let (_, shaken, _) = turned_at(&after, f.chips[0]).unwrap();
    assert_ne!(still[0], shaken[0], "the chip did not move");
    let rest = |out: &[Draw]| -> Vec<Draw> {
        out.iter()
            .filter(|d| !matches!(d, Draw::Turned { tex, .. } if *tex == f.chips[0]))
            .copied()
            .collect()
    };
    assert_eq!(
        rest(&before),
        rest(&after),
        "something besides the chip moved"
    );
}

/// Backing out puts the lid back on — partway through, the lid is on its way down and turning
/// level while the board shrinks under it — and then hands the cart back to the row.
#[test]
fn closing_puts_the_cart_back_on_the_shelf() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let f = fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    let_it_open(&mut app);
    app.apply(Action::GbaDown(Btn::B));

    app.update(0.1);
    let partway = frame(&app);
    let (_, lid, turn) = turned_at(&partway, f.lid).expect("the lid vanished mid-close");
    assert!(
        turn < 0.0 && turn > LID_TURN,
        "the lid is not turning level: {turn}"
    );
    assert!(lid[1] > lid_at(1.0).0.y, "the lid is not coming back down");
    let (_, [bx, by, bw, bh]) = tex_at(&partway, f.board).expect("the board vanished mid-close");
    assert!(
        bw < board_at(1.0).w && bw > board_at(0.0).w,
        "the board is not shrinking: {bw}"
    );
    assert_eq!(
        alpha_of(&partway, f.board),
        Some(1.0),
        "the back half faded mid-close"
    );

    // The sockets and the seated chip shrink with the board rather than staying put, the same
    // check as mid-open run the other way.
    assert_parts_on_board(
        &partway,
        &f,
        Placed {
            x: bx,
            y: by,
            w: bw,
            h: bh,
        },
        "mid-close",
    );

    let_it_close(&mut app);
    let out = frame(&app);

    assert!(
        tex_at(&out, f.board).is_none(),
        "the board outlived the close"
    );
    assert!(
        turned_at(&out, f.lid).is_none(),
        "the lid outlived the close"
    );
    let standing = out
        .iter()
        .filter(|d| {
            matches!(**d, Draw::Rect { w, .. } | Draw::Tex { w, .. }
                if (w - CART_W as f32).abs() < 0.01)
        })
        .count();
    assert_eq!(standing, 1, "the cart did not go back on the shelf");
}

/// Pressed before the cart's faces are up, the cart stands on the shelf until they arrive and
/// opens from there. Drawn before them the picker had only bare sockets and a chip to show where
/// the cart had been, and an open timed from the press would have spent the wait as animation
/// that never reached the panel.
#[test]
fn the_open_waits_on_the_shelf_for_its_faces() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let f = fake_boot_faces(&mut app);
    let rest = resting_cart(&frame(&app));
    app.apply(Action::GbaDown(Btn::Start));
    app.update(0.3);
    assert_plain_shelf(&frame(&app), &f, "while its faces are built");

    app.set_core_board_faces(f.board, f.lid);
    app.update(0.016);
    let out = frame(&app);
    let shelf = grown(shelf_cart_at(rest, 1.0), TURN_PAD as f32);
    let (_, lid, _) = turned_at(&out, f.lid).expect("no lid once the faces arrived");
    assert!(
        near(lid, [shelf.x, shelf.y, shelf.w, shelf.h]),
        "the open used up the wait: the lid is at {lid:?}"
    );
    assert_eq!(
        carts_standing(&out, f.board),
        0,
        "the row still draws the cart whose lid is coming off"
    );

    let_it_open(&mut app);
    let (rest, _) = lid_at(1.0);
    let rest = grown(rest, TURN_PAD as f32 * rest.w / CART_W as f32);
    let (_, lid, _) = turned_at(&frame(&app), f.lid).expect("no lid once open");
    assert!(
        near(lid, [rest.x, rest.y, rest.w, rest.h]),
        "the lid is not at rest: {lid:?}"
    );
}

/// A face that never comes cannot freeze the picker: past `FACES_WAIT_MS` the cart opens anyway.
/// The only faces on the GPU are the other cart's, built while the caret passed over it on the
/// way back to this one, and the fallback open never wears them: it has nothing of its own to
/// show and draws nothing rather than borrow theirs.
#[test]
fn the_open_starts_anyway_when_the_faces_never_come() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    app.apply(Action::GbaDown(Btn::Right));
    app.apply(Action::GbaUp(Btn::Right));
    assert_eq!(app.selected_stem(), Some("Zzz"));
    let f = fake_picker_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Left));
    app.apply(Action::GbaUp(Btn::Left));
    assert_eq!(app.selected_stem(), Some("Emerald"));

    app.apply(Action::GbaDown(Btn::Start));
    app.update(1.499);
    assert_plain_shelf(&frame(&app), &f, "a hair under the cap");

    app.update(0.002);
    assert_eq!(
        carts_standing(&frame(&app), f.board),
        0,
        "the open never started once the cap ran out"
    );

    let_it_open(&mut app);
    let out = frame(&app);
    assert!(
        tex_at(&out, f.board).is_none(),
        "the fallback open drew the other cart's board"
    );
    assert!(
        turned_at(&out, f.lid).is_none(),
        "the fallback open drew the other cart's lid"
    );
    for tex in [f.sockets[0], f.sockets[1], f.chips[0], f.chips[1], f.blank] {
        assert!(
            tex_at(&out, tex).is_none() && turned_at(&out, tex).is_none(),
            "the fallback open drew a socket or chip quad"
        );
    }
}

/// Past the cap, with nothing of this cart's own built yet, the fallback lifts the shelf's own
/// face for the cart — the only thing on the GPU that is honestly this cart's — rather than
/// leave the lid off altogether.
#[test]
fn the_fallback_open_lifts_the_shelfs_own_face_for_the_lid() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let shelf_faces = [TexId::from_raw(950), TexId::from_raw(951)];
    app.set_faces(shelf_faces.to_vec());
    app.apply(Action::GbaDown(Btn::Start));
    app.update(1.501);
    let_it_open(&mut app);

    let (rest, _) = lid_at(1.0);
    let (_, lid, turn) =
        turned_at(&frame(&app), shelf_faces[0]).expect("no lid drawn from the shelf's own face");
    assert!(
        near(lid, [rest.x, rest.y, rest.w, rest.h]),
        "the fallback lid is not at rest: {lid:?}"
    );
    assert_eq!(turn, LID_TURN, "the fallback lid is not turned");
}

/// When this cart's own faces land mid-open, having missed the cap, the very next frame draws
/// them: the fallback was only ever a placeholder for a build the worker had not finished yet.
#[test]
fn the_real_faces_replace_the_fallback_once_they_arrive() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let shelf_faces = [TexId::from_raw(950), TexId::from_raw(951)];
    app.set_faces(shelf_faces.to_vec());
    app.apply(Action::GbaDown(Btn::Start));
    app.update(1.501);
    let_it_open(&mut app);
    assert!(
        turned_at(&frame(&app), shelf_faces[0]).is_some(),
        "the fallback lid should be up before the real faces arrive"
    );

    let f = fake_picker_faces(&mut app);
    app.update(0.016);
    let out = frame(&app);
    assert!(tex_at(&out, f.board).is_some(), "the board never arrived");
    assert!(
        turned_at(&out, f.lid).is_some(),
        "the picker's own lid never arrived"
    );
}

/// The chip is already in the cart's own socket when the cart opens. Arrows pressed while the cart
/// still stands on the shelf waiting for its faces move nothing, so the `A` that follows writes
/// only a choice the player saw made.
#[test]
fn arrows_pressed_while_the_cart_waits_move_nothing() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let f = fake_boot_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));
    app.apply(Action::GbaDown(Btn::Right));
    app.set_core_board_faces(f.board, f.lid);
    app.update(0.016);
    assert_eq!(
        app.core_picker(),
        Some(Core::Mgba),
        "a hop ran while the cart waited"
    );

    let_it_open(&mut app);
    let out = frame(&app);
    assert!(
        turned_at(&out, f.chips[0]).is_some(),
        "the chip is not seated in mGBA once the cart is open"
    );
    assert!(
        turned_at(&out, f.chips[1]).is_none(),
        "the chip opened in gpSP"
    );
}

/// The whole card, from the shelf to a cart playing, over whatever `open_core` finds — the mock
/// on a machine with no dylib anywhere it looks, which is what a test wants: these are about
/// which buttons reach the core, not about what the core does with them.
fn session_playing(root: &Path) -> Session {
    common::clocked(root);
    let mut s = Session::boot(root.to_path_buf());
    // A tap of A on the shelf. A card that resumed a cart is already on its way in and the
    // press lands on nothing, which is the same place it was going.
    s.feed([RawEvent::Down(Btn::A)], 16);
    s.feed([RawEvent::Up(Btn::A)], 32);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        s.update(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(1));
    }
    s
}

/// What `video_refresh` leaves a Game Boy picture occupying inside the 240x160 buffer, as the
/// game pass takes it: x 40..200 and y 8..152, in texture coordinates.
const GB_WINDOW: [f32; 4] = [40.0 / 240.0, 8.0 / 160.0, 160.0 / 240.0, 144.0 / 160.0];

/// What the pad is holding right now, as the core would be polled for it. `Session` hands the
/// mask to the emulator thread at the end of every `feed`, so this is the far side of the one
/// gate that decides whether a button is the game's.
fn pad(s: &Session) -> u16 {
    s.emu().expect("no core in the slot").input().0
}

/// The Game Boy and the Game Boy Color had no shoulder buttons, so on one of their carts slot
/// takes L and R for the picture. On a GBA cart they are the GBA's own and must reach the pad
/// untouched — letting them through to a Game Boy core as well would in fact be harmless,
/// since mGBA maps libretro's L and R to nothing there, but correct-by-accident is what this
/// plan has already been bitten by once.
#[test]
fn l_and_r_change_the_mode_on_a_game_boy_cart_and_not_on_a_gba_one() {
    let d = common::tmp_root_with_gb_carts(&["Tetris", "Zzz"]);
    let mut s = session_playing(d.path());
    assert_eq!(
        s.app().source_rect(),
        WHOLE_TEXTURE,
        "it did not open actual size"
    );

    s.feed([RawEvent::Down(Btn::L1)], 100);
    assert_eq!(
        s.app().source_rect(),
        GB_WINDOW,
        "L did not stretch the picture"
    );
    assert_eq!(pad(&s) & ButtonMask::L, 0, "L reached the game as well");
    s.feed([RawEvent::Up(Btn::L1)], 120);

    s.feed([RawEvent::Down(Btn::R1)], 200);
    assert_eq!(
        s.app().source_rect(),
        WHOLE_TEXTURE,
        "R did not give it back"
    );
    assert_eq!(pad(&s) & ButtonMask::R, 0, "R reached the game as well");
    s.feed([RawEvent::Up(Btn::R1)], 220);

    // The control. Without it every assertion above would pass on a build that simply stopped
    // sending the shoulders to any core at all.
    let d = common::tmp_root_with_carts(&["Emerald", "Zzz"]);
    let mut s = session_playing(d.path());
    s.feed([RawEvent::Down(Btn::L1)], 100);
    assert_ne!(pad(&s) & ButtonMask::L, 0, "the GBA lost its own L");
    s.feed([RawEvent::Down(Btn::R1)], 120);
    assert_ne!(pad(&s) & ButtonMask::R, 0, "the GBA lost its own R");
    assert_eq!(
        s.app().source_rect(),
        WHOLE_TEXTURE,
        "a GBA picture moved when its shoulders were pressed"
    );
}

/// A shoulder held across the insert. Which side of the gate a button falls on is decided on
/// the phase it lands in, and a finger can stay down across a change of phase: L pressed on the
/// shelf is nobody's and reaches the pad, and the release that follows it arrives in
/// `Phase::Playing` on a Game Boy cart, where slot owns it. Withholding that release rather than
/// acting on it leaves the bit set and the core holding L for the rest of the session.
///
/// It goes unnoticed today because mGBA maps libretro's L and R to nothing on a Game Boy, so
/// nothing in the game moves. That is the reasoning this plan has already been bitten by three
/// times, which is why it is pinned here instead of relied on.
#[test]
fn a_shoulder_held_across_the_insert_does_not_stick_on_the_pad() {
    let d = common::tmp_root_with_gb_carts(&["Tetris", "Zzz"]);
    common::clocked(d.path());
    let mut s = Session::boot(d.path().to_path_buf());
    // Down on the shelf, where nothing has taken it. The library is Game Boy only, so there is
    // no other shelf to ring to and the press moves nothing on screen — it is here to put the
    // bit on the pad, which is exactly what a player's thumb resting on L would do.
    s.feed([RawEvent::Down(Btn::L1)], 16);
    // In it goes, with L still held.
    s.feed([RawEvent::Down(Btn::A)], 32);
    s.feed([RawEvent::Up(Btn::A)], 48);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        s.update(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(1));
    }
    // And the finger comes off, now that slot owns the shoulders.
    s.feed([RawEvent::Up(Btn::L1)], 1000);
    assert_eq!(
        pad(&s) & ButtonMask::L,
        0,
        "the pad is still holding L after the finger came off it"
    );
}

/// The same shoulder, still held. The test above lets go once the cart is playing, so it is
/// answered by acting on that release — an edge. This one never releases, and there is no edge
/// to answer with: the finger goes down on the shelf, the cart seats under it, and the thumb
/// stays where it is.
///
/// Ownership is a function of the phase and the platform, and nothing about a phase changing is
/// an edge on a button. So a gate that only ever runs on an edge hands the core a pad with L
/// held for as long as the finger stays down, however long that is — the state the player is in
/// while they keep holding, not a state they pass through.
///
/// Harmless on this cart only because mGBA maps libretro's L and R to nothing on a Game Boy.
/// That is the fourth time this plan has been asked to rely on correct-by-accident, which is why
/// the answer here is to re-decide ownership wherever the answer can move rather than to add
/// another edge.
///
/// The GBA half is the control, and it is the reason this cannot be satisfied by simply never
/// sending the shoulders: on a GBA cart L is the game's own button, and a thumb resting on it
/// through the insert has to arrive in the game still holding it.
#[test]
fn a_shoulder_still_held_when_the_cart_seats_never_reaches_a_game_boy_core() {
    for (label, root, want_held) in [
        (
            "Game Boy",
            common::tmp_root_with_gb_carts(&["Tetris", "Zzz"]),
            false,
        ),
        (
            "Game Boy Advance",
            common::tmp_root_with_carts(&["Emerald", "Zzz"]),
            true,
        ),
    ] {
        common::clocked(root.path());
        let mut s = Session::boot(root.path().to_path_buf());
        // Down on the shelf, where nothing has taken it, and never let go of.
        s.feed([RawEvent::Down(Btn::L1)], 16);
        s.feed([RawEvent::Down(Btn::A)], 32);
        s.feed([RawEvent::Up(Btn::A)], 48);
        // Fed and updated every frame, in that order, which is what `Frontend::advance` does on
        // the device: a thumb held down is an absence of events, not a stream of them.
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut now = 48;
        while !matches!(s.app().phase(), Phase::Playing { .. }) {
            assert!(Instant::now() < deadline, "{label}: the cart never seated");
            now += 16;
            s.feed([], now);
            s.update(1.0 / 60.0);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            pad(&s) & ButtonMask::L != 0,
            want_held,
            "{label}: the core is holding L {} the cart seated under a thumb that never moved",
            if want_held {
                "nowhere after"
            } else {
                "ever since"
            }
        );
    }
}

/// A display preference, remembered per cart, in the shape `selected_core.ini` already has —
/// and it is the card that remembers it, which is why the second half boots the whole session
/// again rather than reading the app's own field back.
#[test]
fn the_picture_mode_is_remembered_per_cart() {
    let d = common::tmp_root_with_gb_carts(&["Tetris", "Zzz"]);
    let mut s = session_playing(d.path());
    s.feed([RawEvent::Down(Btn::L1)], 100);
    s.feed([RawEvent::Up(Btn::L1)], 120);
    assert_eq!(video_mode_for(d.path(), "Tetris"), VideoMode::Stretch);
    assert_eq!(
        video_mode_for(d.path(), "Zzz"),
        VideoMode::Actual,
        "the press was recorded against another cart as well"
    );
    drop(s);

    let mut s = session_playing(d.path());
    assert_eq!(
        s.app().source_rect(),
        GB_WINDOW,
        "the cart did not come back stretched"
    );
    s.feed([RawEvent::Down(Btn::R1)], 100);
    assert_eq!(video_mode_for(d.path(), "Tetris"), VideoMode::Actual);
    assert_eq!(s.app().source_rect(), WHOLE_TEXTURE);
}

/// A cart nobody has chosen for gets the integer-scaled, mask-aligned look — exactly as
/// `core_for` answers for a cart with no line.
#[test]
fn a_cart_with_no_line_reads_as_actual_size() {
    let d = common::tmp_root_with_gb_carts(&["Tetris", "Zzz"]);
    assert!(
        !d.path().join(VIDEO_MODE_FILE).exists(),
        "the fixture already wrote the file"
    );
    assert_eq!(video_mode_for(d.path(), "Tetris"), VideoMode::Actual);

    let s = session_playing(d.path());
    assert_eq!(
        s.app().source_rect(),
        WHOLE_TEXTURE,
        "a cart with no line did not open at actual size"
    );
}

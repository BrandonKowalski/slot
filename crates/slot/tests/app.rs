mod common;

use std::time::Duration;

use slot::app::{App, Phase, EJECT_S, INSERT_S, SEATED_AT};
use slot::audio::Sfx;
use slot::session::Session;
use slot_input::{Action, Btn, RawEvent};
use slot_store::{write_slot_state, Cart, SlotState};
use slot_ui::{edge, opening, Draw, TexId, CART_W, OUT_H, OUT_W};

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
                stem: (*stem).to_string(),
                rom: format!("Games/{stem}.gba").into(),
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
            Draw::Rect { w, .. } | Draw::Tex { w, .. } => (w - CART_W as f32).abs() < 0.01,
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

/// The about screen is a shelf affordance. MENU means eject and polaroids once a cart is in,
/// and a label over a running game is a pause screen nobody asked for.
#[test]
fn about_opens_from_the_shelf_and_nowhere_else() {
    // Two carts, or `single_cart` makes this a dedicated device: one cart is seated at boot
    // whatever the state says, and the shelf is never on screen to press MENU from.
    let d = common::tmp_root_with_carts(&["Emerald", "Fusion"]);
    let (mut s, _motor) = common::session_with_platform(d.path());

    assert!(
        matches!(s.app().phase(), slot::app::Phase::Shelf),
        "not on the shelf: {:?}",
        s.app().phase()
    );
    s.app_mut().apply(slot_input::Action::OpenAbout);
    assert!(matches!(s.app().phase(), slot::app::Phase::About));

    // And a seated cart has no about screen at all.
    s.app_mut()
        .apply(slot_input::Action::GbaDown(slot_input::Btn::B));
    s.app_mut().apply(slot_input::Action::Insert);
    s.app_mut().apply(slot_input::Action::OpenAbout);
    assert!(
        !matches!(s.app().phase(), slot::app::Phase::About),
        "a label opened over a seated cart"
    );
}

/// Both ways out land on the shelf. MENU is the one that matters: the button that opened it
/// should close it without the user having to know that B works too.
#[test]
fn both_b_and_menu_close_the_about_screen() {
    let d = common::tmp_root_with_carts(&["Emerald", "Fusion"]);
    for out in [
        slot_input::Action::GbaDown(slot_input::Btn::B),
        slot_input::Action::OpenAbout,
    ] {
        let (mut s, _motor) = common::session_with_platform(d.path());
        s.app_mut().apply(slot_input::Action::OpenAbout);
        assert!(matches!(s.app().phase(), slot::app::Phase::About));
        s.app_mut().apply(out);
        assert!(
            matches!(s.app().phase(), slot::app::Phase::Shelf),
            "{out:?} did not close the label"
        );
    }
}

/// A booted app sitting on the shelf, beside the card it reads and writes. Two carts at
/// least: one cart is a dedicated device, and `App::boot` seats it rather than leaving a
/// shelf to press anything on. `clock_set` because a card that has never been asked the
/// time opens on the clock screen, which owns every button.
fn on_shelf(stems: &[&str]) -> (tempfile::TempDir, App) {
    let d = common::tmp_root_with_carts(stems);
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

/// Opening on row zero would be a menu that says every cart runs mGBA, which is a lie the
/// moment one of them does not.
#[test]
fn start_on_the_shelf_opens_the_core_picker_on_the_carts_current_core() {
    let (d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    assert_eq!(app.selected_stem(), Some("Emerald"));

    app.apply(Action::GbaDown(Btn::Start));
    assert_eq!(
        app.core_picker(),
        Some(0),
        "a cart with no line of its own runs the default core"
    );
    app.apply(Action::GbaDown(Btn::B));

    slot_store::write_selected_core(d.path(), "Emerald", slot_store::Core::Gpsp).unwrap();
    app.apply(Action::GbaDown(Btn::Start));
    assert_eq!(
        app.core_picker(),
        Some(1),
        "the picker should open on the core the cart already uses, not on row zero"
    );
}

#[test]
fn choosing_a_core_writes_it_and_closes() {
    let (d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    app.apply(Action::GbaDown(Btn::Start));
    app.apply(Action::GbaDown(Btn::Down));
    app.apply(Action::GbaDown(Btn::A));

    assert_eq!(
        app.core_picker(),
        None,
        "the picker stayed open after a choice"
    );
    assert_eq!(
        slot_store::core_for(d.path(), "Emerald"),
        slot_store::Core::Gpsp
    );
    assert_eq!(
        slot_store::core_for(d.path(), "Zzz"),
        slot_store::Core::Mgba,
        "the choice landed on a cart the shelf was not on"
    );
}

#[test]
fn b_closes_the_picker_without_writing() {
    let (d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    app.apply(Action::GbaDown(Btn::Start));
    app.apply(Action::GbaDown(Btn::Down));
    app.apply(Action::GbaDown(Btn::B));

    assert_eq!(app.core_picker(), None);
    assert_eq!(
        slot_store::core_for(d.path(), "Emerald"),
        slot_store::Core::Mgba,
        "backing out of the picker still changed the cart"
    );
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

/// Nothing to configure with no cart under the highlight, and a picker that wrote to an
/// empty stem would leave a line for a cart that is not there.
#[test]
fn the_picker_does_not_open_on_an_empty_shelf() {
    let (_d, mut app) = on_shelf(&[]);
    app.apply(Action::GbaDown(Btn::Start));
    assert_eq!(app.core_picker(), None);
}

/// Up from the top and down from the bottom wrap. Two rows makes either arrow the other's
/// undo, and a picker that stuck at an end would need the player to know which.
#[test]
fn the_picker_wraps_at_both_ends() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    app.apply(Action::GbaDown(Btn::Start));
    app.apply(Action::GbaDown(Btn::Up));
    assert_eq!(app.core_picker(), Some(slot_store::Core::ALL.len() - 1));
    app.apply(Action::GbaDown(Btn::Down));
    assert_eq!(app.core_picker(), Some(0));
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

/// The faces the binary uploads at boot, without a compositor to upload them with: one per
/// core, in `Core::ALL` order, each a different width so a test can tell which row a bar the
/// width of its own words is sitting behind.
fn fake_core_faces(app: &mut App) -> Vec<(TexId, u32, u32)> {
    let faces: Vec<(TexId, u32, u32)> = slot_store::Core::ALL
        .iter()
        .enumerate()
        .map(|(i, _)| (TexId::from_raw(900 + i), 120 + 40 * i as u32, 40))
        .collect();
    app.set_core_picker_faces(faces.clone());
    faces
}

/// Every row's face, in the order they were uploaded in, wherever they landed in the frame.
fn core_rows(out: &[Draw], faces: &[(TexId, u32, u32)]) -> Vec<(usize, f32, f32)> {
    out.iter()
        .enumerate()
        .filter_map(|(i, d)| match *d {
            Draw::Tex { x, y, tex, .. } => faces.iter().position(|f| f.0 == tex).map(|_| (i, x, y)),
            _ => None,
        })
        .collect()
}

/// The bar behind the row in hand. Matched on the row's own geometry as well as its colour:
/// the footer is drawn in `edge` too, in dozens of one and two pixel slivers, so colour
/// alone would count the clock's glyphs as menu bars.
fn highlight_bars(out: &[Draw], faces: &[(TexId, u32, u32)]) -> Vec<(f32, f32, f32)> {
    out.iter()
        .filter_map(|d| match *d {
            Draw::Rect {
                x, y, w, colour, ..
            } if colour == edge()
                && faces.iter().any(|f| {
                    w == f.1 as f32 && x == ((OUT_W as f32 - f.1 as f32) / 2.0).round()
                }) =>
            {
                Some((x, y, w))
            }
            _ => None,
        })
        .collect()
}

/// The picker changing `core_picker()` and nothing else is a menu that does not exist: on a
/// device it reads as START swallowing the arrows and drawing nothing. A row per core has to
/// reach the frame.
///
/// Over the shelf and not instead of it, which is why this asserts on where the picker's
/// draws land as well as that they are there. Copying the power menu's call site — first
/// thing in `draw`, then `return` — would put the menu under the very shelf it is a menu
/// for, and leave the frame looking exactly as broken as it does with no picker at all.
#[test]
fn the_open_picker_draws_a_row_per_core_on_top_of_the_shelf() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let faces = fake_core_faces(&mut app);

    let mut shut = Vec::new();
    app.draw(&mut shut);
    assert!(
        core_rows(&shut, &faces).is_empty(),
        "the picker drew its rows with no picker open"
    );

    app.apply(Action::GbaDown(Btn::Start));
    let mut open = Vec::new();
    app.draw(&mut open);

    let rows = core_rows(&open, &faces);
    assert_eq!(
        rows.len(),
        slot_store::Core::ALL.len(),
        "expected one row per core, got {rows:?}"
    );
    // Distinct rows down the panel, in upload order, rather than two faces stacked on the
    // same line.
    assert!(
        rows[0].2 < rows[1].2,
        "the rows are not stacked down the panel: {rows:?}"
    );
    // Centred on the panel, each to its own width, the way the power menu's are.
    for ((_, x, _), (_, w, _)) in rows.iter().zip(faces.iter()) {
        assert_eq!(*x, ((OUT_W as f32 - *w as f32) / 2.0).round());
    }

    // And the shelf is still under it. The two frames agree until the picker starts, which
    // is what puts the picker on top of the carts rather than beneath them: copied to the
    // power menu's call site — first in `draw`, then `return` — it would be painted over by
    // the very row of carts it is a menu for, and START would look inert.
    let split = shut
        .iter()
        .zip(open.iter())
        .take_while(|(a, b)| a == b)
        .count();
    assert!(
        split > 0,
        "the picker is the first thing in the frame, so the shelf is drawn over it"
    );
    assert!(
        split < rows[0].0,
        "the picker's rows land inside the shelf's own draws rather than after them"
    );
}

/// Which core the cart runs is the whole question the menu asks, and the bar is the only
/// thing on screen that answers it. A menu that highlighted row zero whatever the player
/// pressed would look identical on both cores and write the wrong one.
#[test]
fn the_bar_sits_behind_the_row_the_player_is_on() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let faces = fake_core_faces(&mut app);

    app.apply(Action::GbaDown(Btn::Start));
    let mut top = Vec::new();
    app.draw(&mut top);
    let bars = highlight_bars(&top, &faces);
    assert_eq!(bars.len(), 1, "expected one bar, got {bars:?}");
    let rows = core_rows(&top, &faces);
    assert_eq!(
        bars[0].2, faces[0].1 as f32,
        "the bar is not the width of the first row's own words"
    );
    assert!(
        (bars[0].1 - rows[0].2).abs() < (bars[0].1 - rows[1].2).abs(),
        "the bar is nearer the second row than the first: bar {bars:?}, rows {rows:?}"
    );

    app.apply(Action::GbaDown(Btn::Down));
    assert_eq!(app.core_picker(), Some(1));
    let mut moved = Vec::new();
    app.draw(&mut moved);
    let bars = highlight_bars(&moved, &faces);
    assert_eq!(bars.len(), 1, "expected one bar, got {bars:?}");
    let rows = core_rows(&moved, &faces);
    assert_eq!(
        bars[0].2, faces[1].1 as f32,
        "the bar did not follow the highlight down to the second row"
    );
    assert!(
        (bars[0].1 - rows[1].2).abs() < (bars[0].1 - rows[0].2).abs(),
        "the bar stayed nearer the first row: bar {bars:?}, rows {rows:?}"
    );
}

/// Backing out has to take the menu off the panel, not just out of the state. A picker that
/// closed but kept drawing would be a shelf the player could no longer see.
#[test]
fn closing_the_picker_takes_it_off_the_screen() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let faces = fake_core_faces(&mut app);

    app.apply(Action::GbaDown(Btn::Start));
    app.apply(Action::GbaDown(Btn::B));
    let mut out = Vec::new();
    app.draw(&mut out);

    assert!(
        core_rows(&out, &faces).is_empty(),
        "the picker's rows survived it closing"
    );
    assert!(
        highlight_bars(&out, &faces).is_empty(),
        "the bar behind a row survived the picker closing"
    );
}

/// The picker names cores and nothing else — no cart title, no "which game is this". The only
/// thing on screen saying which cart is being configured is the shelf behind it, so the ground
/// the picker lays down has to be a scrim rather than the opaque one the power menu takes.
/// Painting the shelf out would leave "mGBA / gpSP" floating with no subject.
#[test]
fn the_picker_lets_the_shelf_read_through_so_the_cart_is_still_visible() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
    let _faces = fake_core_faces(&mut app);
    app.apply(Action::GbaDown(Btn::Start));

    let mut out = Vec::new();
    app.draw(&mut out);

    let full_panel: Vec<f32> = out
        .iter()
        .filter_map(|d| match d {
            Draw::Rect { x, y, w, h, colour }
                if *x == 0.0 && *y == 0.0 && *w == OUT_W as f32 && *h == OUT_H as f32 =>
            {
                Some(colour[3])
            }
            _ => None,
        })
        .collect();

    assert!(
        !full_panel.is_empty(),
        "the picker laid down no ground at all"
    );
    assert!(
        full_panel.iter().all(|a| *a < 1.0),
        "a full-panel ground is opaque, so the shelf cannot read through it: {full_panel:?}"
    );
}

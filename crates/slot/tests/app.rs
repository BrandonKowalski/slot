mod common;

use std::time::Duration;

use slot::app::{App, Phase, EJECT_S, INSERT_S, SEATED_AT};
use slot::audio::Sfx;
use slot::session::Session;
use slot_input::{Action, Btn, RawEvent};
use slot_store::{write_slot_state, Cart, Core, SlotState};
use slot_ui::{
    board_at, grown, lid_at, on_board, opening, Draw, Placed, TexId, BOARD_W, CART_W, CHIP_H,
    CHIP_U, CHIP_V, CHIP_W, LID_TURN, SOCKET_H, SOCKET_U, SOCKET_V, SOCKET_W, TURN_PAD,
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

/// Long enough for the close to put the lid back, with room to spare.
fn let_it_close(app: &mut App) {
    app.update(0.3);
}

/// Long enough for a hop to land.
fn let_it_hop(app: &mut App) {
    app.update(0.25);
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

#[test]
fn back_mid_hop_turns_round_and_onward_does_nothing() {
    let (_d, mut app) = on_shelf(&["Emerald", "Zzz"]);
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

fn fake_picker_faces(app: &mut App) -> PickerFaces {
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
    app.set_core_board_faces(f.board, f.lid);
    app.set_core_part_faces(f.sockets.to_vec(), f.chips.to_vec(), f.blank, f.shadow);
    app.set_core_legend_faces(f.legend.to_vec());
    f
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

/// Long enough for the lid to come off.
fn let_it_open(app: &mut App) {
    app.update(0.4);
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

    for (tex, _) in f.legend {
        let (_, at) = tex_at(&out, tex).expect("a legend hint is missing");
        assert_eq!(at[1], 386.0, "the legend is off its line");
    }
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
    let want = grown(lid_at(0.0).0, TURN_PAD as f32);
    assert!(
        near(lid, [want.x, want.y, want.w, want.h]),
        "the lid starts at {lid:?}"
    );
    assert_eq!(turn, 0.0);

    // Halfway up, the lid is between the shelf and its rest and part turned, over a board
    // that is part grown and part faded in.
    app.update(0.13);
    let partway = frame(&app);
    let (_, lid, turn) = turned_at(&partway, f.lid).expect("the lid vanished mid-open");
    assert!(
        turn < 0.0 && turn > LID_TURN,
        "the lid is not turning: {turn}"
    );
    assert!(
        lid[1] < lid_at(0.0).0.y && lid[1] > lid_at(1.0).0.y,
        "the lid is not on its way up: {lid:?}"
    );
    let (_, [bx, by, bw, bh]) = tex_at(&partway, f.board).expect("no board mid-open");
    assert!(
        bw > board_at(0.0).w && bw < board_at(1.0).w,
        "the board is not growing: {bw}"
    );
    let alpha = partway
        .iter()
        .find_map(|d| match *d {
            Draw::Tex { tex, alpha, .. } if tex == f.board => Some(alpha),
            _ => None,
        })
        .expect("no board mid-open");
    assert!(
        alpha > 0.0 && alpha < 1.0,
        "the board is not fading in: {alpha}"
    );

    // The sockets and the seated chip travel with the board rather than staying where the
    // shelf drew the cart: a missing `* zoom` anywhere in that chain would separate them from
    // it here, well before the movement settles.
    let board = Placed {
        x: bx,
        y: by,
        w: bw,
        h: bh,
    };
    let zoom = bw / BOARD_W as f32;
    for (i, socket) in f.sockets.iter().enumerate() {
        let (x, y) = on_board(board, SOCKET_U[i], SOCKET_V);
        let (_, at) = tex_at(&partway, *socket).expect("a socket is missing mid-open");
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
            "socket {i} at {at:?} mid-open"
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
    let (_, chip, _) = turned_at(&partway, f.chips[0]).expect("no seated chip mid-open");
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
        "the chip is not riding the board mid-open: {chip:?}"
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

    // The sockets and the seated chip shrink with the board rather than staying put, the same
    // check as mid-open run the other way.
    let board = Placed {
        x: bx,
        y: by,
        w: bw,
        h: bh,
    };
    let zoom = bw / BOARD_W as f32;
    for (i, socket) in f.sockets.iter().enumerate() {
        let (x, y) = on_board(board, SOCKET_U[i], SOCKET_V);
        let (_, at) = tex_at(&partway, *socket).expect("a socket is missing mid-close");
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
            "socket {i} at {at:?} mid-close"
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
    let (_, chip, _) = turned_at(&partway, f.chips[0]).expect("no seated chip mid-close");
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
        "the chip is not riding the board mid-close: {chip:?}"
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

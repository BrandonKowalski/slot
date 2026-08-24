//! The in-game menu and the link it starts.
//!
//! Nothing here shells out to `ags-net` or touches a network interface: every starter is
//! built through `LinkStarter::spawn_with`, whose slow parts are injected. The one test that
//! needs a real `TcpLink` makes one over loopback, because `LinkProgress::Ready` carries a
//! transport and there is no other way to have one.

mod common;

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::{Duration, Instant};

use slot::app::{App, GameMenu, GameRow, LinkRow, Phase};
use slot::emu::Speed;
use slot::link_net::{Cancel, TcpLink};
use slot::link_radio::LinkRole;
use slot::link_start::{LinkFail, LinkStarter};
use slot::session::Session;
use slot_input::{Action, Btn, Millis, RawEvent};
use slot_retro::ButtonMask;
use slot_store::{write_slot_state, Core, SlotState};
use slot_ui::{opening, Draw, TexId, OUT_H, OUT_W};
use tempfile::TempDir;

/// How long a test waits on a real worker thread before deciding it never will answer.
const BAIL: Duration = Duration::from_secs(5);

/// A game in the slot, running on a stated core. The core is set the way `session.rs` sets
/// it — once, by whoever spawned the core — because it is the thing that decides whether the
/// Link row exists at all.
///
/// Two carts, so `single_cart` does not turn this into a dedicated device.
fn playing_on(core: Core) -> (App, TempDir) {
    let d = common::tmp_root_with_carts(&["Emerald", "Zzz"]);
    let mut app = common::boot(d.path());
    app.apply(Action::Insert);
    app.set_core(core);
    app.on_core_ready();
    for _ in 0..120 {
        app.update(1.0 / 60.0);
    }
    assert!(matches!(app.phase(), Phase::Playing { .. }), "never seated");
    (app, d)
}

/// A worker whose radio always comes up and whose socket step is whatever the test says.
fn fake_starter(
    socket: impl FnMut(u16, &Cancel) -> io::Result<TcpLink> + Send + 'static,
) -> LinkStarter {
    LinkStarter::spawn_with(
        Box::new(|_| Ok(())),
        Box::new(|| {}),
        LinkRole::Host,
        0,
        Box::new(socket),
    )
}

fn io_err(kind: io::ErrorKind) -> io::Error {
    io::Error::new(kind, "from a test")
}

/// Frames, until the overlay stops waiting on the worker. The worker is a real thread, so
/// this is a bounded wait rather than a fixed number of frames.
fn settle(app: &mut App) {
    let deadline = Instant::now() + BAIL;
    while matches!(app.game_menu(), Some(GameMenu::Working(_))) {
        app.update(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(1));
        assert!(Instant::now() < deadline, "the overlay never left Working");
    }
}

/// The faces the binary uploads at boot, without a compositor to upload them with. Each row
/// a different width, so a test can tell which one a bar the width of its own words is
/// sitting behind.
fn fake_rows(app: &mut App) -> Vec<(TexId, u32, u32)> {
    let rows: Vec<(TexId, u32, u32)> = (0..GameRow::ALL.len())
        .map(|i| (TexId::from_raw(700 + i), 120 + 40 * i as u32, 40))
        .collect();
    app.set_game_menu_faces(rows.clone());
    rows
}

#[test]
fn select_and_menu_in_game_opens_the_menu_over_the_game() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    assert!(app.game_menu_open(), "SELECT+MENU in game did nothing");
    assert!(
        matches!(app.phase(), Phase::Playing { .. }),
        "the game must still be seated behind it"
    );
}

/// The spec's most explicit UI decision. A player on an mGBA cart never learns the row
/// exists, because the fix is not on this screen — it is four steps away on the shelf, and a
/// row that explains that is a row that teaches a dead end.
#[test]
fn the_link_row_is_absent_under_mgba_not_merely_refused() {
    let (mut app, _d) = playing_on(Core::Mgba);
    app.apply(Action::GameMenu);
    assert!(
        !app.game_menu_rows().iter().any(|r| r.contains("Link")),
        "an mGBA cart was shown a row whose fix is four steps away on the shelf"
    );
    // Link is the only row this menu has today, so under mGBA there is nothing to show and
    // an empty panel over a paused game is worse than no panel at all. When a second row
    // lands, this becomes an assertion about that row instead.
    assert!(!app.game_menu_open(), "an empty menu came up over the game");
}

#[test]
fn the_link_row_is_present_under_gpsp() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    assert!(app.game_menu_rows().iter().any(|r| r.contains("Link")));
}

/// The shelf's About sticker is a different screen on a different button, and this must not
/// have replaced it.
#[test]
fn the_game_menu_does_not_open_on_the_shelf() {
    let d = common::tmp_root_with_carts(&["Emerald", "Zzz"]);
    let mut app = common::boot(d.path());
    app.apply(Action::GameMenu);
    assert!(!app.game_menu_open(), "the shelf raised the in-game menu");
    assert!(matches!(app.phase(), Phase::Shelf));
    app.apply(Action::OpenAbout);
    assert!(
        matches!(app.phase(), Phase::About),
        "the shelf lost its About screen"
    );
}

#[test]
fn the_link_row_offers_host_and_join() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    app.apply(Action::GbaDown(Btn::A));
    assert_eq!(app.game_menu_rows(), vec!["Host", "Join"]);
}

/// B backs out one step at a time. The player one press into a two-press choice is not
/// asking to leave the menu, and the last B hands the game back untouched.
#[test]
fn b_backs_out_one_step_at_a_time_and_leaves_the_game_seated() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    app.apply(Action::GbaDown(Btn::A));
    app.apply(Action::GbaDown(Btn::B));
    assert!(
        app.game_menu_rows().iter().any(|r| r.contains("Link")),
        "B left the whole menu instead of the row it was in"
    );
    app.apply(Action::GbaDown(Btn::B));
    assert!(!app.game_menu_open());
    assert!(
        matches!(app.phase(), Phase::Playing { .. }),
        "cancel has to return the player to their game"
    );
}

/// Host is libretro's client 0 and the joiner is client 1. Its numbering, not ours, and the
/// two sides must never both think they are the same one.
#[test]
fn the_host_is_client_zero_and_the_joiner_client_one() {
    assert_eq!(LinkRow::Host.client_id(), 0);
    assert_eq!(LinkRow::Join.client_id(), 1);
    assert_eq!(LinkRow::Host.role(), LinkRole::Host);
    assert_eq!(LinkRow::Join.role(), LinkRole::Join);
}

/// Picking a row has to actually start something.
///
/// This used to lean on the joiner failing in milliseconds against a host that is not there.
/// It no longer does — a joiner retries for the same thirty seconds a host waits, so that the
/// order the two players press their buttons in stops mattering — so the pick is cancelled
/// here instead. Cancelling is the honest way to end it anyway: it is what the player has,
/// and it exercises the path B takes out of `Working`.
#[test]
fn picking_a_row_puts_the_screen_on_the_working_step() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    app.apply(Action::GbaDown(Btn::A));
    app.apply(Action::GbaDown(Btn::Down));
    assert_eq!(app.game_menu(), Some(GameMenu::Link(1)));
    app.apply(Action::GbaDown(Btn::A));
    assert!(
        matches!(app.game_menu(), Some(GameMenu::Working(_))),
        "the pick did nothing at all"
    );
    // B does not close the overlay on the spot — the radio step cannot be interrupted — so
    // this is a cancel followed by a wait for the worker to answer, not an instant exit.
    app.apply(Action::GbaDown(Btn::B));
    settle(&mut app);
}

/// Three failures, three sentences. "The link failed" does not tell a player whether to try
/// again, to move closer, or to ask their friend to press something.
#[test]
fn each_failure_says_which_one_it_was() {
    for (kind, want) in [
        (io::ErrorKind::TimedOut, LinkFail::NobodyCame),
        (io::ErrorKind::ConnectionRefused, LinkFail::PeerVanished),
    ] {
        let (mut app, _d) = playing_on(Core::Gpsp);
        app.apply(Action::GameMenu);
        app.start_link(fake_starter(move |_, _| Err(io_err(kind))), 0);
        settle(&mut app);
        assert_eq!(app.game_menu(), Some(GameMenu::Failed(want)));
    }
    let lines: Vec<&str> = [
        LinkFail::Radio,
        LinkFail::NobodyCame,
        LinkFail::PeerVanished,
    ]
    .iter()
    .map(|f| f.line())
    .collect();
    assert_eq!(
        lines.len(),
        lines.iter().collect::<std::collections::HashSet<_>>().len(),
        "two failures share a sentence, which is a generic 'link failed' in disguise"
    );
}

/// A failure is a screen to read, and the way off it is back into the game that was never
/// interrupted.
#[test]
fn b_on_a_failure_puts_the_player_back_in_the_game() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    app.start_link(fake_starter(|_, _| Err(io_err(io::ErrorKind::TimedOut))), 0);
    settle(&mut app);
    assert_eq!(
        app.game_menu(),
        Some(GameMenu::Failed(LinkFail::NobodyCame))
    );
    app.apply(Action::GbaDown(Btn::B));
    assert!(!app.game_menu_open());
    assert!(
        matches!(app.phase(), Phase::Playing { .. }),
        "a failed link ate the session"
    );
    assert!(!app.link_active(), "a failed link started a session anyway");
}

/// A player who backed out is not shown a screen about the thing they just did on purpose.
#[test]
fn a_cancelled_link_says_nothing_and_returns_to_the_game() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    app.start_link(
        fake_starter(|_, cancel: &Cancel| {
            let deadline = Instant::now() + BAIL;
            while !cancel.is_cancelled() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(io_err(io::ErrorKind::Interrupted))
        }),
        0,
    );
    app.update(1.0 / 60.0);
    app.apply(Action::GbaDown(Btn::B));
    settle(&mut app);
    assert!(!app.game_menu_open(), "the cancel left a screen behind");
    assert!(matches!(app.phase(), Phase::Playing { .. }));
}

/// The one that looks exactly like success on screen if it is wrong: the overlay goes away,
/// the game comes back, and nothing is linked. `Ready` carries the transport the emulator
/// thread needs, so reaching it without starting a session — or without handing the
/// transport on — is a link that never happened behind a screen that says it did.
#[test]
fn a_link_that_comes_up_starts_the_session_and_hands_the_transport_on() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    // A real pair over loopback: `Ready` carries a `TcpLink` and there is no way to forge one.
    let port = 45907;
    let far = std::thread::spawn(move || TcpLink::host("127.0.0.1", port).expect("host"));
    std::thread::sleep(Duration::from_millis(150));
    app.start_link(
        fake_starter(move |_, _| TcpLink::join("127.0.0.1", port)),
        0,
    );
    settle(&mut app);
    let _far = far.join().expect("host thread");

    assert!(
        !app.game_menu_open(),
        "the overlay stayed up over a live session"
    );
    assert!(app.link_active(), "the link came up and no session started");
    assert_eq!(app.link_client_id(), Some(0));
    let (client_id, _transport) = app
        .take_link_transport()
        .expect("the transport never reached the emulator thread");
    assert_eq!(client_id, 0);
}

/// `link_radio::up` is an opaque blocking process spawn: nothing can interrupt it for one to
/// five seconds. B asks the worker to stop and the screen stays where it is until it
/// answers — closing here would put the player back in their game with an access point
/// still coming up behind them.
#[test]
fn b_during_the_radio_step_does_not_hand_the_game_back_early() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    let (release, held) = channel::<()>();
    app.start_link(
        LinkStarter::spawn_with(
            // Stands in for the five seconds `ags-net link` can take, and for the fact that
            // nothing may interrupt it.
            Box::new(move |_| {
                held.recv().expect("released");
                Ok(())
            }),
            Box::new(|| {}),
            LinkRole::Host,
            0,
            Box::new(|_, cancel: &Cancel| {
                Err(io_err(if cancel.is_cancelled() {
                    io::ErrorKind::Interrupted
                } else {
                    io::ErrorKind::TimedOut
                }))
            }),
        ),
        0,
    );
    for _ in 0..10 {
        app.update(1.0 / 60.0);
    }
    app.apply(Action::GbaDown(Btn::B));
    for _ in 0..10 {
        app.update(1.0 / 60.0);
    }
    assert!(
        matches!(app.game_menu(), Some(GameMenu::Working(_))),
        "B handed the game back while the radio was still coming up behind it"
    );
    release.send(()).expect("release the radio");
    settle(&mut app);
    assert!(!app.game_menu_open(), "the cancel never landed at all");
}

/// `LinkStarter` has no `Drop`: one dropped mid-wait keeps working, and a host dropped while
/// waiting leaves its access point up for up to thirty seconds with nothing on the other end
/// of it. Every path that ends the overlay has to ask it to stop first.
#[test]
fn a_shut_lid_cancels_the_link_it_interrupted() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    let cancelled = Arc::new(AtomicBool::new(false));
    let seen = cancelled.clone();
    app.start_link(
        fake_starter(move |_, cancel: &Cancel| {
            let deadline = Instant::now() + BAIL;
            while !cancel.is_cancelled() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(2));
            }
            seen.store(cancel.is_cancelled(), Ordering::SeqCst);
            Err(io_err(io::ErrorKind::Interrupted))
        }),
        0,
    );
    app.update(1.0 / 60.0);
    app.apply(Action::LidClose);
    assert!(
        !app.game_menu_open(),
        "the overlay outlived the game it was drawn over"
    );
    let deadline = Instant::now() + BAIL;
    while !cancelled.load(Ordering::SeqCst) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        cancelled.load(Ordering::SeqCst),
        "a starter dropped mid-wait leaves a host's access point up for thirty seconds"
    );
}

/// The overlay pauses the core underneath it, and pausing is one of the exact manipulations
/// libretro's netpacket contract forbids while players are connected — the same guard
/// `open_power_menu` already carries, for the same reason.
#[test]
fn the_menu_is_refused_over_a_live_session() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.begin_link(0);
    app.apply(Action::GameMenu);
    assert!(
        !app.game_menu_open(),
        "the overlay paused a session libretro forbids pausing"
    );
    assert!(app.link_active(), "the refusal ended the session instead");
}

/// A menu that changes `game_menu()` and nothing else does not exist: on a device it reads
/// as a chord that swallows the buttons and draws nothing. Over the game rather than instead
/// of it, so the scrim is what separates the two.
#[test]
fn the_open_menu_draws_its_rows_over_the_game() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    let rows = fake_rows(&mut app);
    app.apply(Action::GameMenu);
    let mut out = Vec::new();
    app.draw(&mut out);
    let scrim = out.iter().position(|d| {
        matches!(*d, Draw::Rect { w, h, colour, .. }
            if w == OUT_W as f32 && h == OUT_H as f32 && colour == opening())
    });
    let scrim = scrim.expect("the menu drew no ground of its own over the game");
    let row = out
        .iter()
        .position(|d| matches!(*d, Draw::Tex { tex, .. } if tex == rows[0].0))
        .expect("the Link row never reached the frame");
    assert!(row > scrim, "the row was drawn under its own scrim");
}

// --- the two wirings into the running game ------------------------------------------------
//
// Everything above drives `App` alone, which is where the screen lives. These two are what
// the screen is worth nothing without: the game underneath it actually stopping, and the wire
// a started link runs over actually reaching the thread the core is on. Both are invisible to
// every test above — `App` holds neither the core nor the transport, deliberately — and both
// look exactly like success from the panel when they are missing.

/// A real `Session` with a gpSP cart playing. The core falls back to the mock, as it does for
/// every test in this crate that does not fetch a real dylib; what matters here is that
/// `selected_core.ini` says gpSP, because that is what decides the Link row exists.
fn session_playing_on_gpsp() -> (Session, TempDir, Millis) {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    slot_store::write_selected_core(d.path(), "Emerald", Core::Gpsp).expect("write core");
    write_slot_state(
        d.path(),
        &SlotState {
            cart: Some("Emerald".into()),
            clock_set: true,
            utc_offset_min: 0,
            ..Default::default()
        },
    )
    .expect("write slot.state");
    let mut s = Session::boot(d.path().to_path_buf());
    let mut now: Millis = 0;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(1));
    }
    (s, d, now)
}

fn step(s: &mut Session, now: &mut Millis, events: &[RawEvent]) {
    *now += 16;
    s.feed(events.iter().copied(), *now);
    s.update(1.0 / 60.0);
}

/// Frames, until the worker thread has actually read the speed it was set to. What the
/// handle was told is not what the core is doing; `observed_speed` is the worker's own last
/// pass through its loop.
fn runs_at(s: &mut Session, now: &mut Millis, want: Speed) -> bool {
    let deadline = Instant::now() + BAIL;
    while s.observed_speed() != Some(want) {
        if Instant::now() >= deadline {
            return false;
        }
        step(s, now, &[]);
        std::thread::sleep(Duration::from_millis(1));
    }
    true
}

/// The menu is over a *paused* game, not a live one — `Session::held` is what carries that.
/// Without it the core runs on flat out behind a panel the player is reading, and the motor
/// keeps buzzing under it, which is the exact bug that put the power menu in `held` in the
/// first place.
///
/// Driven from raw button edges rather than an `Action`, so the chord this menu is opened by
/// is proven to reach the app through the real gesture layer and not only in theory.
#[test]
fn the_open_menu_pauses_the_game_underneath_it() {
    let (mut s, _d, mut now) = session_playing_on_gpsp();
    assert!(
        runs_at(&mut s, &mut now, Speed::Normal),
        "the game never started running, so pausing it proves nothing"
    );
    step(
        &mut s,
        &mut now,
        &[RawEvent::Down(Btn::Select), RawEvent::Down(Btn::Menu)],
    );
    assert!(
        s.app().game_menu_open(),
        "SELECT+MENU never reached the app through the gesture layer"
    );
    assert!(
        runs_at(&mut s, &mut now, Speed::Paused),
        "the game ran on behind the menu"
    );
}

/// The wire, not only the bookkeeping. `App` never touches a transport, so a link that marks
/// its own session live and leaves the socket on the floor is a screen saying "linked" over
/// two devices that cannot hear each other — and there is nothing on the panel to tell the
/// difference.
#[test]
fn a_started_link_reaches_the_emulator_thread_with_its_transport() {
    let (mut s, _d, mut now) = session_playing_on_gpsp();
    let port = 45911;
    let far = std::thread::spawn(move || TcpLink::host("127.0.0.1", port).expect("host"));
    std::thread::sleep(Duration::from_millis(150));
    s.app_mut().start_link(
        fake_starter(move |_, _| TcpLink::join("127.0.0.1", port)),
        1,
    );
    let deadline = Instant::now() + BAIL;
    while s.app().game_menu_open() {
        assert!(Instant::now() < deadline, "the link never came up");
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(1));
    }
    let _far = far.join().expect("host thread");
    assert!(s.app().link_active(), "no session started at all");
    assert_eq!(s.app().link_client_id(), Some(1), "the joiner is client 1");
    let deadline = Instant::now() + BAIL;
    while !s.emu().is_some_and(|e| e.net().is_active()) {
        assert!(
            Instant::now() < deadline,
            "the transport never reached the emulator thread"
        );
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// A press the menu is using is not the game's. The pause underneath (`held`) hides most of
/// it, but a pause is not a mask: A picked the Host row while the core was stopped, and if
/// the link comes up before the finger does, the game resumes with A already down and starts
/// the round by pressing it. The switcher clears the pad for exactly this reason; this menu
/// is the second screen that has to.
#[test]
fn a_button_the_menu_is_using_never_reaches_the_game() {
    let (mut s, _d, mut now) = session_playing_on_gpsp();
    step(
        &mut s,
        &mut now,
        &[RawEvent::Down(Btn::Select), RawEvent::Down(Btn::Menu)],
    );
    assert!(s.app().game_menu_open(), "the chord never reached the app");
    step(&mut s, &mut now, &[RawEvent::Down(Btn::A)]);
    assert_eq!(
        s.emu().expect("a core is running").input(),
        ButtonMask(0),
        "the press that picked a row was handed to the game as well"
    );
}

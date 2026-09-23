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

use slot::app::{
    App, GameMenu, LinkLegend, LinkRow, Phase, LINKED_HOLD_MS, LINK_LOST_MS, UNPLUG_HOLD_MS,
};
use slot::emu::{CoreState, EmuHandle, Speed};
use slot::link_kind::LinkKind;
use slot::link_net::{Cancel, TcpLink};
use slot::link_radio::{LinkRole, RadioJob, RadioJobs};
use slot::link_start::{LinkFail, LinkStarter, LinkStep};
use slot::persist::{self, Snapshot};
use slot::session::Session;
use slot_input::{Action, Btn, Millis, RawEvent};
use slot_retro::{ButtonMask, LinkChannel};
use slot_store::{write_slot_state, Core, SlotState};
use slot_ui::{arrows_hint_face, hint_face, opening, Draw, TexId, Toast, HINT_EDGE, OUT_H, OUT_W};
use tempfile::TempDir;

/// How long a test waits on a real worker thread before deciding it never will answer.
const BAIL: Duration = Duration::from_secs(5);

/// A game in the slot, running on a stated core. The core is set the way `session.rs` sets
/// it — once, by whoever spawned the core — because it is the thing that decides whether the
/// link screen exists at all.
///
/// Two carts, so `single_cart` does not turn this into a dedicated device.
///
/// The seated one is a cart gpSP can carry — Ruby's code, which puts it in the Pokémon family
/// and so on `mul_poke` — because the link screen does not open for a cart gpSP has no protocol
/// for. It is a cable cart on gpSP's own pick, which is what the hardware tests below switch
/// away from. A header with no code at all, which is what `tmp_root_with_carts` writes, is a
/// cart gpSP would take a session for and then ignore.
fn playing_on(core: Core) -> (App, TempDir) {
    let d = common::tmp_root_with_carts(&["Emerald", "Zzz"]);
    common::write_retail_header(&d, "Emerald", "POKEMON RUBY", "AXVE");
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
        Box::new(|_, _| Ok(())),
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
    while matches!(app.game_menu(), Some(GameMenu::Working { .. })) {
        app.update(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(1));
        assert!(Instant::now() < deadline, "the overlay never left Working");
    }
}

/// HOST and JOIN, each a different width so a test can tell which one is drawn.
fn fake_roles(app: &mut App) -> Vec<(TexId, u32, u32)> {
    let faces: Vec<(TexId, u32, u32)> = (0..LinkRow::ALL.len())
        .map(|i| (TexId::from_raw(700 + i), 120 + 40 * i as u32, 40))
        .collect();
    app.set_link_menu_faces(faces.clone());
    faces
}

#[test]
fn select_and_menu_open_the_link_screen_on_host() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    assert_eq!(app.game_menu(), Some(GameMenu::Pick(LinkRow::Host)));
    assert!(matches!(app.phase(), Phase::Playing { .. }));
}

/// mGBA links by running both machines in step, so the screen opens on its own cable rather
/// than sending the player to another core.
#[test]
fn the_link_screen_opens_under_mgba_on_its_own_cable() {
    let (mut app, _d) = playing_on(Core::Mgba);
    app.apply(Action::GameMenu);
    assert_eq!(
        app.game_menu(),
        Some(GameMenu::Pick(LinkRow::Host)),
        "mGBA did not offer the link it carries"
    );
    assert_eq!(
        app.toast(),
        None,
        "it opened the screen and banished it too"
    );
}

/// On gpSP the screen itself is the answer; the banner stays out of it.
#[test]
fn the_link_screen_on_gpsp_says_nothing_in_the_banner() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    assert!(app.game_menu_open());
    assert_eq!(app.toast(), None);
}

#[test]
fn left_and_right_swap_host_and_join_and_the_screen_remembers() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    app.apply(Action::GbaDown(Btn::Right));
    assert_eq!(app.game_menu(), Some(GameMenu::Pick(LinkRow::Join)));
    app.apply(Action::GbaDown(Btn::Left));
    assert_eq!(app.game_menu(), Some(GameMenu::Pick(LinkRow::Host)));
    app.apply(Action::GbaDown(Btn::Right));
    app.apply(Action::GbaDown(Btn::B));
    assert!(!app.game_menu_open());
    app.apply(Action::GameMenu);
    assert_eq!(
        app.game_menu(),
        Some(GameMenu::Pick(LinkRow::Join)),
        "the last role was forgotten"
    );
}

#[test]
fn b_on_pick_hands_the_game_back() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    app.apply(Action::GbaDown(Btn::B));
    assert!(!app.game_menu_open());
    assert!(matches!(app.phase(), Phase::Playing { .. }));
}

/// The shelf's quick menu is a different screen on a different button, and this must not have
/// replaced it.
#[test]
fn the_game_menu_does_not_open_on_the_shelf() {
    let d = common::tmp_root_with_carts(&["Emerald", "Zzz"]);
    let mut app = common::boot(d.path());
    app.apply(Action::GameMenu);
    assert!(!app.game_menu_open(), "the shelf raised the in-game menu");
    assert!(matches!(app.phase(), Phase::Shelf));
    app.apply(Action::QuickMenu);
    assert!(
        matches!(app.phase(), Phase::QuickMenu { .. }),
        "the shelf lost its quick menu"
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
    assert_eq!(LinkRow::from_client_id(0), LinkRow::Host);
    assert_eq!(LinkRow::from_client_id(1), LinkRow::Join);
    assert_eq!(LinkRow::Host.other(), LinkRow::Join);
}

#[test]
fn a_on_pick_starts_the_link_in_the_picked_role() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    app.apply(Action::GbaDown(Btn::Right));
    app.apply(Action::GbaDown(Btn::A));
    assert!(
        matches!(
            app.game_menu(),
            Some(GameMenu::Working {
                role: LinkRow::Join,
                step: LinkStep::Radio,
                ..
            })
        ),
        "A did not start a joiner: {:?}",
        app.game_menu()
    );
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
        assert!(matches!(app.game_menu(), Some(GameMenu::Failed { fail, .. }) if fail == want));
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
    assert!(matches!(
        app.game_menu(),
        Some(GameMenu::Failed {
            fail: LinkFail::NobodyCame,
            ..
        })
    ));
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
fn a_link_that_comes_up_holds_linked_for_a_second_then_hands_the_game_back() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    let port = common::free_port();
    let far = std::thread::spawn(move || TcpLink::host("127.0.0.1", port).expect("host"));
    std::thread::sleep(Duration::from_millis(150));
    app.start_link(
        fake_starter(move |_, _| TcpLink::join("127.0.0.1", port)),
        0,
    );
    settle(&mut app);
    let _far = far.join().expect("host thread");

    assert!(matches!(
        app.game_menu(),
        Some(GameMenu::Linked {
            role: LinkRow::Host,
            ..
        })
    ));
    assert!(
        app.link_active(),
        "the session waited for the screen instead of starting"
    );
    let (client_id, _transport) = app.take_link_transport().expect("no transport handed on");
    assert_eq!(client_id, 0);

    for press in [Btn::A, Btn::B] {
        app.apply(Action::GbaDown(press));
    }
    app.apply(Action::GameMenu);
    assert!(
        matches!(app.game_menu(), Some(GameMenu::Linked { .. })),
        "the hold took a press"
    );

    let frames = (LINKED_HOLD_MS as f32 / (1000.0 / 60.0)) as usize;
    for _ in 0..frames - 2 {
        app.update(1.0 / 60.0);
    }
    assert!(app.game_menu_open(), "LINKED left before its second");
    for _ in 0..4 {
        app.update(1.0 / 60.0);
    }
    assert!(!app.game_menu_open(), "LINKED never handed the game back");
    assert!(app.link_active());
}

#[test]
fn a_peer_lost_during_the_hold_closes_the_screen_and_breaks_the_badge() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    let port = common::free_port();
    let far = std::thread::spawn(move || TcpLink::host("127.0.0.1", port).expect("host"));
    std::thread::sleep(Duration::from_millis(150));
    app.start_link(
        fake_starter(move |_, _| TcpLink::join("127.0.0.1", port)),
        1,
    );
    settle(&mut app);
    let _far = far.join().expect("host thread");
    assert!(matches!(app.game_menu(), Some(GameMenu::Linked { .. })));
    app.peer_lost();
    assert!(
        !app.game_menu_open(),
        "LINKED stayed up over a link that just died"
    );
    assert_eq!(app.link_badge(), slot_ui::LinkBadge::JoinedLost);
}

/// B asks the worker to stop, and the screen stays where it is until it answers. The real
/// `up` now kills its `ags-net` child on a cancel rather than waiting out a joiner's search,
/// so that answer comes quickly — but it still comes from the worker, and closing before it
/// would put the player back in their game with an access point still coming up behind them.
/// This fake ignores the flag, which is the slowest case that shape allows.
#[test]
fn b_during_the_radio_step_does_not_hand_the_game_back_early() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    let (release, held) = channel::<()>();
    app.start_link(
        LinkStarter::spawn_with(
            // Stands in for an `ags-net link` that has not answered yet.
            Box::new(move |_, _| {
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
        matches!(app.game_menu(), Some(GameMenu::Working { .. })),
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

/// The screen opens over a live session now, which it refused to do before: a paused GBA
/// cannot hold a link open, so `Session::sync_speed` leaves the core running while one is up
/// and `Session::overlaid` keeps the menu's buttons out of the game. What the screen shows is
/// the session, with the key that ends it.
#[test]
fn the_shortcut_opens_the_connected_screen_over_a_live_session() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    let log = watched(&mut app);
    app.begin_link(0);
    app.apply(Action::GameMenu);
    assert!(
        matches!(app.game_menu(), Some(GameMenu::Linked { opened: true, .. })),
        "the shortcut did not open the connected screen"
    );
    assert!(app.link_active(), "opening the screen ended the session");
    assert_eq!(app.toast(), None, "nothing has happened to announce yet");
    assert!(
        log.jobs().is_empty(),
        "the radio was touched by a screen that only opened"
    );
}

/// B is the way out that changes nothing: the session it was opened over is still running,
/// and the radio under it is still the session's own.
#[test]
fn b_leaves_the_session_running() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.begin_link(0);
    app.apply(Action::GameMenu);
    let log = watched(&mut app);
    app.apply(Action::GbaDown(Btn::B));
    assert!(!app.game_menu_open(), "B did not leave the screen");
    assert!(app.link_active(), "B ended the session it was opened over");
    assert!(
        !log.jobs().contains(&RadioJob::Cool),
        "leaving a live session cooled the radio it runs on"
    );
}

/// A ends it, immediately: the screen that asked is the confirmation, and the far end handles
/// a partner leaving the same way it handles a flat battery over there. The banner is what
/// says it happened, since the game underneath carries straight on.
#[test]
fn a_ends_the_session_and_says_so() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.begin_link(0);
    app.apply(Action::GameMenu);
    let log = watched(&mut app);
    app.apply(Action::GbaDown(Btn::A));
    assert!(!app.link_active(), "A left the session running");
    assert_eq!(app.toast(), Some(Toast::LinkEnded));
    // The session is over on the frame the key landed. The plug coming out is the screen
    // catching up with that, not a step in it — which is why `link_active` is already false
    // here, with the animation still to play.
    assert!(
        matches!(app.game_menu(), Some(GameMenu::Unplug { .. })),
        "A did not put the plug back out: {:?}",
        app.game_menu()
    );
    for _ in 0..((UNPLUG_HOLD_MS / 16 + 4) as usize) {
        app.update(1.0 / 60.0);
    }
    assert!(!app.game_menu_open(), "the screen stayed up over the game");
    // Down ends the session's own network; the cool behind it is for a BaseOS whose down does
    // not unload the driver itself. Exactly those two: the unplug leaving must not cool a
    // second time, which it would if it closed through `close_game_menu`.
    assert_eq!(log.jobs(), vec![RadioJob::Down, RadioJob::Cool]);
}

/// The unplug has to play on the device that pressed nothing, which is the half of this the
/// control frame exists for: one cable cannot come out of one end and stay in the other.
///
/// This player is in their game with no screen up at all — the ordinary case for the far end,
/// and the one `peer_lost` never had to draw anything for because it only ever broke a badge.
#[test]
fn a_peer_ending_the_link_unplugs_on_this_device_too() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.begin_link(0);
    assert!(!app.game_menu_open(), "nothing should be on screen yet");

    app.peer_ended();

    assert!(
        !app.link_active(),
        "the ending waited on the animation instead of the other way round"
    );
    assert!(
        matches!(app.game_menu(), Some(GameMenu::Unplug { .. })),
        "the far end's ending did not unplug on this device: {:?}",
        app.game_menu()
    );
    assert_eq!(app.toast(), Some(Toast::PeerEnded));

    for _ in 0..((UNPLUG_HOLD_MS / 16 + 4) as usize) {
        app.update(1.0 / 60.0);
    }
    assert!(
        !app.game_menu_open(),
        "the unplug screen never left by itself"
    );
}

/// A menu that changes `game_menu()` and nothing else does not exist: on a device it reads
/// as a chord that swallows the buttons and draws nothing. Over the game rather than instead
/// of it, so the scrim is what separates the two.
#[test]
fn the_link_screen_draws_its_role_over_the_game() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    let roles = fake_roles(&mut app);
    app.apply(Action::GameMenu);
    let mut out = Vec::new();
    app.draw(&mut out);
    let scrim = out
        .iter()
        .position(|d| {
            matches!(*d, Draw::Rect { w, h, colour, .. }
        if w == OUT_W as f32 && h == OUT_H as f32 && colour == opening())
        })
        .expect("the screen drew no ground over the game");
    let host = out
        .iter()
        .position(|d| matches!(*d, Draw::Tex { tex, .. } if tex == roles[0].0))
        .expect("HOST never reached the frame");
    assert!(host > scrim);
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
/// `selected_core.ini` says gpSP, because that is what decides the link screen exists.
fn session_playing_on_gpsp() -> (Session, TempDir, Millis) {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    // A cart gpSP can carry, as in `playing_on`: the link screen does not open otherwise.
    common::write_retail_header(&d, "Emerald", "POKEMON RUBY", "AXVE");
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
    let port = common::free_port();
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
/// it, but a pause is not a mask: A picked Host while the core was stopped, and if the link
/// comes up before the finger does, the game resumes with A already down and starts the round
/// by pressing it. The switcher clears the pad for exactly this reason; this link screen is
/// the second one that has to.
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

/// `Session::update`'s own hop from the emulator's lost-peer flag to `App::peer_lost` (see its
/// doc comment there) has nothing watching it end to end: the flag alone is `tests/emu.rs`'s,
/// `App::peer_lost` called directly is this file's and `link_session.rs`'s, and
/// `TcpLink::is_closed` is `link_session.rs`'s again — every piece tested alone, never the
/// wire between them. This links a real session over loopback the way
/// `a_started_link_reaches_the_emulator_thread_with_its_transport` above does, drops the far
/// end, and follows the badge breaking and then the session actually ending, on both `App`
/// and the emulator thread — the same two-sided proof that test already gives the *start* of
/// a link, but for the end of one instead.
#[test]
fn a_dropped_peer_breaks_the_badge_and_ends_the_session_end_to_end() {
    let (mut s, _d, mut now) = session_playing_on_gpsp();
    let port = common::free_port();
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
    let far = far.join().expect("host thread");

    // Live on both sides before anything is dropped, the same wait
    // `a_started_link_reaches_the_emulator_thread_with_its_transport` makes for the start.
    let deadline = Instant::now() + BAIL;
    while !(s.app().link_active() && s.emu().is_some_and(|e| e.net().is_active())) {
        assert!(
            Instant::now() < deadline,
            "the link never went live on both sides"
        );
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(2));
    }

    // The peer leaving: `TcpLink`'s `Drop` shuts its socket down, which is a real FIN on the
    // wire (see `dropping_the_link_closes_the_wire` in `link_session.rs`), not merely a value
    // going out of scope.
    drop(far);

    let deadline = Instant::now() + BAIL;
    while s.app().link_badge() != slot_ui::LinkBadge::JoinedLost {
        assert!(Instant::now() < deadline, "the badge never broke");
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(2));
    }

    // The broken badge has to be seen for `LINK_LOST_MS` before the session ends on its own.
    let margin_steps = (LINK_LOST_MS / 16) as usize + 30;
    for _ in 0..margin_steps {
        step(&mut s, &mut now, &[]);
    }
    assert!(!s.app().link_active(), "the session never ended");

    // The proof this test exists for: the ending reached the emulator thread too, which only
    // happens through `Session::bridge_link` — `App`'s own bookkeeping ending is not enough.
    let deadline = Instant::now() + BAIL;
    while s.emu().is_some_and(|e| e.net().is_active()) {
        assert!(
            Instant::now() < deadline,
            "bridge_link never carried the ending to the emulator thread"
        );
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// The ending the user actually asked for: a link ended on the *far* device ends on this one
/// too, promptly, and the banner says which of the two it was.
///
/// The far end is a real `TcpLink` the test keeps hold of. It sends the control frame and then
/// stays open, deliberately — that is what isolates the message as the cause. The lost-peer
/// path cannot explain this ending: nothing is dropped, no FIN is sent, `is_closed` stays
/// false, and with the control frame removed this session would simply carry on running rather
/// than fail. It is the difference between proving the message works and proving a socket
/// closed.
///
/// Promptness is held against `LINK_LOST_MS` itself rather than a frame count, because that
/// bound *is* the claim: a deliberate ending must not sit through the broken-badge timeout a
/// peer that vanished has to.
#[test]
fn a_peer_that_ends_the_link_ends_this_session_without_waiting_out_the_timeout() {
    let (mut s, _d, mut now) = session_playing_on_gpsp();
    let port = common::free_port();
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
    let mut far = far.join().expect("host thread");

    let deadline = Instant::now() + BAIL;
    while !(s.app().link_active() && s.emu().is_some_and(|e| e.net().is_active())) {
        assert!(
            Instant::now() < deadline,
            "the link never went live on both sides"
        );
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(2));
    }

    // The far player choosing to end it. Nothing else happens to the wire: this side has not
    // stepped yet, so its own teardown cannot have run and closed anything.
    far.send_end();
    assert!(
        !far.is_closed(),
        "the far socket was already closed, so nothing below is about the message"
    );

    let began = now;
    let deadline = Instant::now() + BAIL;
    while s.app().link_active() {
        assert!(
            Instant::now() < deadline,
            "the far end's ending never reached this session"
        );
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(2));
    }

    assert!(
        now - began < LINK_LOST_MS,
        "the session took {}ms to end, which is the lost-peer timeout rather than the message",
        now - began
    );
    assert_eq!(
        s.app().toast(),
        Some(Toast::PeerEnded),
        "the banner did not say the link had been ended from the other end"
    );
    assert_eq!(
        s.app().link_badge(),
        slot_ui::LinkBadge::Off,
        "a deliberate ending broke the badge as if the peer had vanished"
    );

    // And it reached the emulator thread, which only `bridge_link` does — the same second half
    // the dropped-peer test above insists on.
    let deadline = Instant::now() + BAIL;
    while s.emu().is_some_and(|e| e.net().is_active()) {
        assert!(
            Instant::now() < deadline,
            "bridge_link never carried the ending to the emulator thread"
        );
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Sprites distinguishable only by their `TexId`, the way `a_pokemon_cart_shows_the_adapter`
/// and `a_pokemon_hack_shows_the_cable` tell which one the screen actually drew.
fn fake_link_sprites() -> slot::link_screen::LinkSprites {
    let s = |n: usize| slot::link_screen::Sprite {
        tex: TexId::from_raw(n),
        w: 10,
        h: 10,
    };
    slot::link_screen::LinkSprites {
        port: s(1),
        plug_host: s(2),
        plug_join: s(3),
        adapter: s(4),
        arcs_right: [s(7), s(8), s(9)],
        arcs_left: [s(10), s(11), s(12)],
        clicks: s(13),
        arrow_left: s(14),
        arrow_right: s(15),
    }
}

/// The first cart on the card in the slot and running on gpSP, with `fake_link_sprites`' faces
/// to tell the plug from the adapter.
fn seated_on_gpsp(d: &TempDir) -> App {
    seated_on(d, Core::Gpsp)
}

/// The same, on whichever core the test is about. The core decides whether the link screen can
/// open at all, so a test about which refusal a press earns has to be able to name it.
fn seated_on(d: &TempDir, core: Core) -> App {
    let mut app = common::boot(d.path());
    app.apply(Action::Insert);
    app.set_core(core);
    app.on_core_ready();
    for _ in 0..120 {
        app.update(1.0 / 60.0);
    }
    app.set_link_sprites(fake_link_sprites());
    app
}

/// A cart in the slot, its link screen open and drawn, with `fake_link_sprites`' faces to
/// tell the plug from the adapter.
fn open_link_screen(d: &TempDir) -> Vec<Draw> {
    let mut app = seated_on_gpsp(d);
    app.apply(Action::GameMenu);
    let mut out = Vec::new();
    app.draw(&mut out);
    out
}

/// A retail Pokémon cart links over the Wireless Adapter, so that is what its screen shows.
#[test]
fn a_pokemon_cart_shows_the_adapter() {
    let d = common::tmp_root_with_carts(&["Zzz"]);
    // Written before `boot`, so the shelf scan behind it reads this header off disk.
    // "Pokemon Emerald" still sorts before "Zzz", so `Action::Insert` seats it.
    common::write_retail_header(&d, "Pokemon Emerald", "POKEMON EMER", "BPEE");
    let out = open_link_screen(&d);
    assert!(
        out.iter()
            .any(|d| matches!(*d, Draw::Tex { tex, .. } if tex == TexId::from_raw(4))),
        "no adapter"
    );
    assert!(
        !out.iter()
            .any(|d| matches!(*d, Draw::Tex { tex, .. } if tex == TexId::from_raw(2))),
        "a plug on a wireless cart"
    );
}

/// gpSP forces a Pokémon ROM whose header is not standard to the cable, whatever its title
/// claims to be — it is a hack, not the retail game.
#[test]
fn a_pokemon_hack_shows_the_cable() {
    let d = common::tmp_root_with_carts(&["Zzz"]);
    common::write_retail_header(&d, "Pokemon Emerald", "POKEMON EMER", "BPEE");
    // Overwrite the entry branch's opcode byte gpSP checks, leaving the rest of the header
    // (title, code) looking exactly like the retail game.
    let rom = d.path().join("Games/GBA").join("Pokemon Emerald.gba");
    let mut bytes = std::fs::read(&rom).expect("read rom");
    bytes[3] = 0;
    std::fs::write(&rom, bytes).expect("rewrite rom");
    let out = open_link_screen(&d);
    assert!(
        out.iter()
            .any(|d| matches!(*d, Draw::Tex { tex, .. } if tex == TexId::from_raw(2))),
        "no plug"
    );
    assert!(
        !out.iter()
            .any(|d| matches!(*d, Draw::Tex { tex, .. } if tex == TexId::from_raw(4))),
        "the adapter on a Pokémon hack"
    );
}

// --- the cable or the adapter -------------------------------------------------------------
//
// The screen opens on the hardware gpSP would pick and SELECT switches it. gpSP reads its link
// mode only while a game loads, so linking in the mode it was not loaded with reloads the game
// behind the screen first. `App` asks for that and `Session` carries it out.

/// A tap of SELECT the way the gesture layer delivers one: on the release, with no chord.
fn select(app: &mut App) {
    app.apply(Action::GbaDown(Btn::Select));
    app.apply(Action::GbaUp(Btn::Select));
}

/// Which hardware the open screen draws, told apart by `fake_link_sprites`' faces: 2 and 3
/// are the two plugs, 4 the adapter.
fn drawn_hardware(app: &App) -> LinkKind {
    let mut out = Vec::new();
    app.draw(&mut out);
    let drew = |n: usize| {
        out.iter().any(|d| {
            matches!(*d, Draw::Tex { tex, .. } | Draw::Turned { tex, .. }
                if tex == TexId::from_raw(n))
        })
    };
    match (drew(2) || drew(3), drew(4)) {
        (true, false) => LinkKind::Cable,
        (false, true) => LinkKind::Wireless,
        other => panic!("the screen drew (plug, adapter) = {other:?}"),
    }
}

/// Frames, until the link worker has reported its socket step. Nothing but a running worker
/// moves the screen off its first step, so this is what tells a started link apart from a
/// screen still waiting on the game to reload.
fn reaches_waiting(app: &mut App) -> bool {
    let deadline = Instant::now() + BAIL;
    while !matches!(
        app.game_menu(),
        Some(GameMenu::Working {
            step: LinkStep::Waiting,
            ..
        })
    ) {
        if Instant::now() >= deadline {
            return false;
        }
        app.update(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(1));
    }
    true
}

/// Frames for a stretch of wall clock long enough for a worker, had one been started, to have
/// moved the screen on.
fn idle(app: &mut App, ms: u64) {
    let until = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < until {
        app.update(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// The link screen open as Join with the hardware switched, and A pressed. Join, so the
/// worker a test starts reaches out rather than binding the port every test would share.
fn switched_and_picked() -> (App, TempDir) {
    let (mut app, d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    app.apply(Action::GbaDown(Btn::Right));
    select(&mut app);
    app.apply(Action::GbaDown(Btn::A));
    (app, d)
}

/// SELECT swaps the cable for the adapter and the art follows on the same frame. The choice
/// belongs to the cart, so closing the screen and opening it again keeps it.
#[test]
fn select_on_pick_switches_the_hardware_and_the_cart_keeps_it() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.set_link_sprites(fake_link_sprites());
    app.apply(Action::GameMenu);
    assert_eq!(
        drawn_hardware(&app),
        LinkKind::Cable,
        "the test cart's own header links by cable"
    );
    select(&mut app);
    assert_eq!(
        drawn_hardware(&app),
        LinkKind::Wireless,
        "SELECT switched nothing"
    );
    assert_eq!(
        app.game_menu(),
        Some(GameMenu::Pick(LinkRow::Host)),
        "SELECT did more than switch the hardware"
    );
    app.apply(Action::GbaDown(Btn::B));
    app.apply(Action::GameMenu);
    assert_eq!(
        drawn_hardware(&app),
        LinkKind::Wireless,
        "the switch was forgotten when the screen closed"
    );
    select(&mut app);
    assert_eq!(
        drawn_hardware(&app),
        LinkKind::Cable,
        "SELECT only goes one way"
    );
}

/// Switched away and back again is the mode the game already runs, so there is nothing to
/// reload and A starts the link exactly as it always has.
#[test]
fn a_in_the_mode_the_game_already_runs_starts_the_link_straight_away() {
    // Only one test in this process may have a live link at a time: they all share the
    // one port the product reads, so a host here and a joiner there connect to each other.
    let _link = common::link_port_lock();
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    app.apply(Action::GbaDown(Btn::Right));
    select(&mut app);
    select(&mut app);
    app.apply(Action::GbaDown(Btn::A));
    assert_eq!(
        app.take_link_reload(),
        None,
        "a mode the game already runs asked for a reload"
    );
    assert!(reaches_waiting(&mut app), "A started no link");
    app.apply(Action::GbaDown(Btn::B));
    settle(&mut app);
}

/// A in the other mode asks for the game to be loaded again — which cart, and the
/// `gpsp_serial` to load it with — and starts nothing yet: a worker running ahead of the
/// reload would bring a link up over a game still in the old mode.
#[test]
fn a_in_a_switched_mode_asks_for_the_game_to_reload_first() {
    // Only one test in this process may have a live link at a time: they all share the
    // one port the product reads, so a host here and a joiner there connect to each other.
    let _link = common::link_port_lock();
    let (mut app, _d) = switched_and_picked();
    assert_eq!(
        app.take_link_reload(),
        Some(("Emerald".to_string(), "rfu")),
        "no reload asked for, or the wrong one"
    );
    assert_eq!(
        app.take_link_reload(),
        None,
        "the request was handed on twice"
    );
    assert!(
        matches!(
            app.game_menu(),
            Some(GameMenu::Working {
                role: LinkRow::Join,
                step: LinkStep::Radio,
                ..
            })
        ),
        "the screen did not go to its first step: {:?}",
        app.game_menu()
    );
    idle(&mut app, 150);
    assert!(
        matches!(
            app.game_menu(),
            Some(GameMenu::Working {
                step: LinkStep::Radio,
                ..
            })
        ),
        "a link started before the game reloaded: {:?}",
        app.game_menu()
    );
}

/// The reload done, the link starts in the role that was picked, and the screen carries on
/// from the step it has been showing since A rather than starting its animation again.
#[test]
fn the_reload_finishing_starts_the_link() {
    // Only one test in this process may have a live link at a time: they all share the
    // one port the product reads, so a host here and a joiner there connect to each other.
    let _link = common::link_port_lock();
    let (mut app, _d) = switched_and_picked();
    let Some(GameMenu::Working { since, .. }) = app.game_menu() else {
        panic!("A did not start working: {:?}", app.game_menu());
    };
    app.take_link_reload().expect("no reload asked for");
    idle(&mut app, 50);
    app.link_reload_done();
    assert!(
        matches!(
            app.game_menu(),
            Some(GameMenu::Working { role: LinkRow::Join, since: s, .. }) if s == since
        ),
        "the screen started over: {:?}",
        app.game_menu()
    );
    assert!(
        reaches_waiting(&mut app),
        "the reload finished and no link started"
    );
    app.apply(Action::GbaDown(Btn::B));
    settle(&mut app);
}

/// B during the reload is heard, but the game is still loading behind the screen and there
/// is no worker to stop. Once the reload finishes the screen closes and hands the game back
/// instead of linking, the way a cancelled start does.
#[test]
fn b_during_the_reload_hands_the_game_back_once_it_finishes() {
    // Only one test in this process may have a live link at a time: they all share the
    // one port the product reads, so a host here and a joiner there connect to each other.
    let _link = common::link_port_lock();
    let (mut app, _d) = switched_and_picked();
    app.take_link_reload().expect("no reload asked for");
    app.apply(Action::GbaDown(Btn::B));
    idle(&mut app, 50);
    assert!(
        matches!(app.game_menu(), Some(GameMenu::Working { .. })),
        "B handed the game back while it was still loading"
    );
    app.link_reload_done();
    assert!(!app.game_menu_open(), "the cancel left a screen behind");
    assert!(matches!(app.phase(), Phase::Playing { .. }));
    idle(&mut app, 150);
    assert!(!app.game_menu_open(), "a link started anyway");
    assert!(!app.link_active());
}

/// A game that will not load in the mode it was switched to goes back to the one it came from,
/// which loaded a moment ago from the same state. Once it has, the switch is undone, the cart's
/// choice with it, and the game is handed back with the shake every refusal gets.
#[test]
fn a_reload_that_fails_goes_back_to_the_mode_the_game_came_from() {
    // Only one test in this process may have a live link at a time: they all share the
    // one port the product reads, so a host here and a joiner there connect to each other.
    let _link = common::link_port_lock();
    let (mut app, _d) = switched_and_picked();
    app.set_link_sprites(fake_link_sprites());
    assert_eq!(app.take_link_reload(), Some(("Emerald".to_string(), "rfu")));
    app.link_reload_failed();
    assert_eq!(
        app.take_link_reload(),
        Some(("Emerald".to_string(), "auto")),
        "the reload that failed did not go back to the mode the game came from"
    );
    assert!(
        matches!(app.game_menu(), Some(GameMenu::Working { .. })),
        "the screen closed with the game still loading behind it"
    );
    app.link_reload_done();
    assert!(
        !app.game_menu_open(),
        "the screen stayed up over a switch that never happened"
    );
    assert!(matches!(app.phase(), Phase::Playing { .. }));
    assert!(
        app.refusal_active(app.now()),
        "the game came back without saying the switch was refused"
    );
    idle(&mut app, 150);
    assert!(!app.game_menu_open(), "a link started anyway");
    assert!(!app.link_active());
    app.apply(Action::GameMenu);
    assert_eq!(
        drawn_hardware(&app),
        LinkKind::Cable,
        "the cart kept the switch that failed"
    );
}

/// Neither mode loads, so there is no game left to hand back. The cart comes back out of the
/// slot carrying the alert, the way a cart the core refused on the way in does, rather than
/// sitting seated with no core behind it.
#[test]
fn a_game_that_loads_in_neither_mode_comes_back_out_refused() {
    // Only one test in this process may have a live link at a time: they all share the
    // one port the product reads, so a host here and a joiner there connect to each other.
    let _link = common::link_port_lock();
    let (mut app, _d) = switched_and_picked();
    app.take_link_reload().expect("no reload asked for");
    app.link_reload_failed();
    app.take_link_reload().expect("no way back asked for");
    app.link_reload_failed();
    assert_eq!(app.take_link_reload(), None, "a third load was asked for");
    assert!(!app.game_menu_open(), "the screen outlived the game");
    assert!(
        matches!(app.phase(), Phase::Ejecting { .. }),
        "the cart stayed seated with no game: {:?}",
        app.phase()
    );
    assert!(
        app.alert_visible(),
        "the cart came out without saying it was refused"
    );
    for _ in 0..120 {
        app.update(1.0 / 60.0);
    }
    assert!(matches!(app.phase(), Phase::Shelf), "{:?}", app.phase());
}

/// A shut lid closes the screen, but a reload already underway still has to end in a game or
/// on the shelf. If neither mode loads, opening the lid lands on the shelf rather than on a
/// seated cart with no core behind it.
#[test]
fn a_lid_shut_over_a_game_that_loads_in_neither_mode_opens_onto_the_shelf() {
    // Only one test in this process may have a live link at a time: they all share the
    // one port the product reads, so a host here and a joiner there connect to each other.
    let _link = common::link_port_lock();
    let (mut app, _d) = switched_and_picked();
    app.take_link_reload().expect("no reload asked for");
    app.apply(Action::LidClose);
    assert!(matches!(app.phase(), Phase::Doze { .. }));
    app.link_reload_failed();
    assert_eq!(
        app.take_link_reload(),
        Some(("Emerald".to_string(), "auto")),
        "the shut lid dropped the way back"
    );
    app.link_reload_failed();
    app.apply(Action::LidOpen);
    assert!(
        matches!(app.phase(), Phase::Shelf),
        "the lid opened onto a cart with no game: {:?}",
        app.phase()
    );
}

/// A game with no cable protocol of its own loads on `auto` whichever hardware is picked, and
/// gpSP links it over the adapter regardless, so a plug drawn over it would be a mode the core
/// never runs. SELECT is refused with the shake, the adapter stays, and A links straight away.
#[test]
fn select_is_refused_where_gpsp_would_link_the_same_either_way() {
    // Only one test in this process may have a live link at a time: they all share the
    // one port the product reads, so a host here and a joiner there connect to each other.
    let _link = common::link_port_lock();
    let d = common::tmp_root_with_carts(&["Zzz"]);
    // "Mario Golf" sorts before "Zzz", so `Action::Insert` seats it.
    common::write_retail_header(&d, "Mario Golf", "MARIO GOLF", "BMGE");
    let mut app = seated_on_gpsp(&d);
    app.apply(Action::GameMenu);
    assert_eq!(drawn_hardware(&app), LinkKind::Wireless);
    app.apply(Action::GbaDown(Btn::Right));
    select(&mut app);
    assert!(app.refusal_active(app.now()), "SELECT was not refused");
    assert_eq!(
        drawn_hardware(&app),
        LinkKind::Wireless,
        "a plug drawn over a game gpSP links by adapter"
    );
    assert_eq!(app.game_menu(), Some(GameMenu::Pick(LinkRow::Join)));
    app.apply(Action::GbaDown(Btn::A));
    assert_eq!(
        app.take_link_reload(),
        None,
        "a reload for a mode gpSP would not change"
    );
    assert!(reaches_waiting(&mut app), "A started no link");
    app.apply(Action::GbaDown(Btn::B));
    settle(&mut app);
}

/// A compares the mode picked with the `gpsp_serial` the running core was actually loaded with,
/// not with what the screen opened on. A core loaded on `rfu` links over the adapter straight
/// away, and has to be loaded again to link by cable.
#[test]
fn a_reloads_only_for_a_serial_the_core_was_not_loaded_with() {
    // Only one test in this process may have a live link at a time: they all share the
    // one port the product reads, so a host here and a joiner there connect to each other.
    let _link = common::link_port_lock();
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.set_link_loaded("rfu");
    app.apply(Action::GameMenu);
    app.apply(Action::GbaDown(Btn::Right));
    select(&mut app);
    app.apply(Action::GbaDown(Btn::A));
    assert_eq!(
        app.take_link_reload(),
        None,
        "the adapter, picked over a core loaded on rfu, asked for a reload"
    );
    assert!(reaches_waiting(&mut app), "A started no link");
    app.apply(Action::GbaDown(Btn::B));
    settle(&mut app);
    app.apply(Action::GameMenu);
    select(&mut app);
    app.apply(Action::GbaDown(Btn::A));
    assert_eq!(
        app.take_link_reload(),
        Some(("Emerald".to_string(), "auto")),
        "the cable, picked over a core loaded on rfu, linked without a reload"
    );
}

/// A core that refused the resume it was opened with: running, but on its own default machine,
/// which is the one thing a flush will not write back.
struct RefusedResume;

impl Snapshot for RefusedResume {
    fn state(&self) -> Option<Vec<u8>> {
        Some(vec![0; 8])
    }

    fn save_ram(&self) -> Option<Vec<u8>> {
        None
    }

    fn thumb(&self) -> Option<Vec<u8>> {
        None
    }

    fn load(&self, _state: Vec<u8>) {}

    fn resume_trusted(&self) -> bool {
        false
    }
}

/// Over a core that refused its resume, the flush keeps the player's state rather than writing
/// the core's default machine over it, so a reload would resume from the refused file again and
/// lose everything since. A in a switched mode is refused with the shake instead: the screen
/// stays on Pick, and the mode the game already runs still links.
#[test]
fn a_switch_is_refused_over_a_resume_the_core_would_not_take() {
    // Only one test in this process may have a live link at a time: they all share the
    // one port the product reads, so a host here and a joiner there connect to each other.
    let _link = common::link_port_lock();
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.set_snapshot(Box::new(RefusedResume));
    app.apply(Action::GameMenu);
    app.apply(Action::GbaDown(Btn::Right));
    select(&mut app);
    app.apply(Action::GbaDown(Btn::A));
    assert_eq!(
        app.take_link_reload(),
        None,
        "a reload over a state that cannot be saved"
    );
    assert!(app.refusal_active(app.now()), "A was not refused");
    assert_eq!(
        app.game_menu(),
        Some(GameMenu::Pick(LinkRow::Join)),
        "the refusal left Pick"
    );
    select(&mut app);
    app.apply(Action::GbaDown(Btn::A));
    assert!(
        reaches_waiting(&mut app),
        "the mode the game already runs did not link"
    );
    app.apply(Action::GbaDown(Btn::B));
    settle(&mut app);
}

/// Pick names every key it takes, the way out first and the commitment last: B, SELECT, the
/// arrows, A. The faces are the real ones, so the widths are what the device rasterises and
/// the row is proven to fit the console strip rather than assumed to.
#[test]
fn pick_names_cancel_mode_swap_and_link_across_the_strip() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    let width = |k: LinkLegend| match k {
        LinkLegend::Cancel => hint_face("B", "Cancel").w,
        LinkLegend::Mode => hint_face("SELECT", "Mode").w,
        LinkLegend::Swap => arrows_hint_face("Swap").w,
        LinkLegend::Link => hint_face("A", "Link").w,
        LinkLegend::Ok => hint_face("A", "OK").w,
        LinkLegend::Back => hint_face("B", "Back").w,
        LinkLegend::EndLink => hint_face("A", "End Link").w,
    };
    let faces: Vec<(TexId, u32)> = LinkLegend::ALL
        .iter()
        .map(|k| (TexId::from_raw(900 + k.index()), width(*k)))
        .collect();
    app.set_link_legend_faces(faces.clone());
    app.apply(Action::GameMenu);
    let mut out = Vec::new();
    app.draw(&mut out);
    let mut keys: Vec<(f32, f32, LinkLegend)> = out
        .iter()
        .filter_map(|d| match *d {
            Draw::Tex { x, w, tex, .. } => LinkLegend::ALL
                .iter()
                .find(|k| faces[k.index()].0 == tex)
                .map(|k| (x, w, *k)),
            _ => None,
        })
        .collect();
    keys.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert_eq!(
        keys.iter().map(|k| k.2).collect::<Vec<_>>(),
        [
            LinkLegend::Cancel,
            LinkLegend::Mode,
            LinkLegend::Swap,
            LinkLegend::Link
        ],
    );
    let left = keys[0].0;
    let (x, w, _) = keys[keys.len() - 1];
    let right = x + w - HINT_EDGE as f32;
    println!(
        "pick legend: faces {:?} wide, drawn from x {left} to {right} of {OUT_W}",
        keys.iter().map(|k| k.1).collect::<Vec<_>>()
    );
    assert!(
        left >= 0.0 && right <= OUT_W as f32,
        "the legend runs off the strip: {left}..{right}"
    );
}

/// The mock core's own frame counter, which is the whole of its save state.
fn counter(s: &Session) -> u64 {
    let state = s
        .emu()
        .expect("a core is running")
        .request_state()
        .recv_timeout(BAIL)
        .expect("the core gave up no state");
    u64::from_le_bytes(state.try_into().expect("mock state is 8 bytes"))
}

/// The reload end to end, through a real `Session`: SELECT and A replace the emulator with a
/// new one, that one carries on from exactly where the old one stopped, and only then does
/// the link start. `App` alone cannot show any of it — it never holds the core, so a reload
/// it asked for and nobody carried out looks exactly like one that happened.
#[test]
fn a_link_in_a_switched_mode_reloads_the_game_and_then_starts_the_link() {
    let (mut s, _d, mut now) = session_playing_on_gpsp();
    assert!(
        runs_at(&mut s, &mut now, Speed::Normal),
        "the game never started running, so a fresh core would look the same"
    );
    step(
        &mut s,
        &mut now,
        &[RawEvent::Down(Btn::Select), RawEvent::Down(Btn::Menu)],
    );
    step(
        &mut s,
        &mut now,
        &[RawEvent::Up(Btn::Menu), RawEvent::Up(Btn::Select)],
    );
    assert!(s.app().game_menu_open(), "the chord never reached the app");
    assert!(
        runs_at(&mut s, &mut now, Speed::Paused),
        "the game ran on behind the menu"
    );
    step(&mut s, &mut now, &[RawEvent::Down(Btn::Right)]);
    step(&mut s, &mut now, &[RawEvent::Up(Btn::Right)]);
    step(&mut s, &mut now, &[RawEvent::Down(Btn::Select)]);
    step(&mut s, &mut now, &[RawEvent::Up(Btn::Select)]);

    let played = counter(&s);
    assert!(
        played > 0 && s.frames_published() > 0,
        "nothing ran before the reload"
    );

    step(&mut s, &mut now, &[RawEvent::Down(Btn::A)]);
    assert_eq!(
        s.frames_published(),
        0,
        "the emulator that was running is still the one in the slot"
    );
    let deadline = Instant::now() + BAIL;
    while s.emu().map(EmuHandle::state) != Some(CoreState::Ready) {
        assert!(Instant::now() < deadline, "the reloaded game never loaded");
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        counter(&s),
        played,
        "the reloaded game did not come back where it left off"
    );

    let deadline = Instant::now() + BAIL;
    while !matches!(
        s.app().game_menu(),
        Some(GameMenu::Working {
            step: LinkStep::Waiting,
            ..
        })
    ) {
        assert!(
            Instant::now() < deadline,
            "the game reloaded and no link started: {:?}",
            s.app().game_menu()
        );
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(1));
    }

    step(&mut s, &mut now, &[RawEvent::Down(Btn::B)]);
    let deadline = Instant::now() + BAIL;
    while s.app().game_menu_open() {
        assert!(Instant::now() < deadline, "the cancel never landed");
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// A whole `Session` over a real core planted under gpSP's name, the way `gpsp.rs` plants one,
/// and a cart that is a real ROM. The mock loads anything, so only a real core can fail to load
/// a game again. `None` on a host with no core to plant; the caller holds `core_lock`.
fn session_on_a_real_core() -> Option<(Session, TempDir, Millis)> {
    let core = common::vendored_core()?;
    let d = common::tmp_root_with_real_carts(&["Emerald"]);
    // A real ROM, but carrying Ruby's identity, so the link screen this test drives will open.
    common::write_real_cart_as(&d, "Emerald", "POKEMON RUBY", "AXVE");
    slot_store::write_selected_core(d.path(), "Emerald", Core::Gpsp).expect("write core");
    std::fs::copy(
        &core,
        d.path()
            .join("System")
            .join(slot::core::dylib_name(Core::Gpsp)),
    )
    .expect("plant a core under gpSP's name");
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
    Some((s, d, now))
}

/// The failure path through a real `Session`. The ROM is taken off the card from under the
/// running game, so neither the mode it was switched to nor the one it came from can load it
/// again: the session goes back once, then gives up, and the cart comes back out of the slot
/// refused. At no point is a seated cart left playing with no core behind it.
#[test]
fn a_game_that_will_not_load_again_comes_back_out_of_the_slot() {
    let _g = common::core_lock();
    let Some((mut s, d, mut now)) = session_on_a_real_core() else {
        eprintln!("no host-openable dylib on this machine, skipping");
        return;
    };
    step(
        &mut s,
        &mut now,
        &[RawEvent::Down(Btn::Select), RawEvent::Down(Btn::Menu)],
    );
    step(
        &mut s,
        &mut now,
        &[RawEvent::Up(Btn::Menu), RawEvent::Up(Btn::Select)],
    );
    assert!(s.app().game_menu_open(), "the chord never reached the app");
    step(&mut s, &mut now, &[RawEvent::Down(Btn::Right)]);
    step(&mut s, &mut now, &[RawEvent::Up(Btn::Right)]);
    step(&mut s, &mut now, &[RawEvent::Down(Btn::Select)]);
    step(&mut s, &mut now, &[RawEvent::Up(Btn::Select)]);
    std::fs::remove_file(d.path().join("Games/GBA").join("Emerald.gba"))
        .expect("take the rom away");

    step(&mut s, &mut now, &[RawEvent::Down(Btn::A)]);
    let deadline = Instant::now() + BAIL;
    while !matches!(s.app().phase(), Phase::Ejecting { .. }) {
        assert!(
            Instant::now() < deadline,
            "the cart never came back out: {:?}",
            s.app().phase()
        );
        assert!(
            s.has_core() || s.app().game_menu_open(),
            "a seated cart was left playing with no core behind it"
        );
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        s.app().alert_visible(),
        "the cart came out without the alert"
    );
    assert!(
        persist::read_resume(d.path(), Core::Gpsp, "Emerald").is_some(),
        "the state flushed before the reload is gone"
    );
    let deadline = Instant::now() + BAIL;
    while !matches!(s.app().phase(), Phase::Shelf) {
        assert!(
            Instant::now() < deadline,
            "the refused cart never reached the shelf"
        );
        step(&mut s, &mut now, &[]);
        std::thread::sleep(Duration::from_millis(1));
    }
}

// --- the carts gpSP cannot link ----------------------------------------------------------
//
// gpSP does not emulate the link cable. It speaks the Wireless Adapter and three named cable
// protocols, and a cart it has none of is left on `SERIAL_MODE_AUTO`, which its netpacket hooks
// have no case for. The session still comes up — gpSP accepts the peer — and then every packet
// is dropped, which is a screen saying LINKED over two games that cannot hear each other.

/// Apotris is the one on the card: a real cable game, absent from gpSP's `gba_over.h`. The
/// screen stays shut and the banner says so, rather than bringing a radio up and joining two
/// devices for a game that will never see a packet.
#[test]
fn a_cart_gpsp_cannot_carry_is_refused_the_link_screen_and_told_why() {
    let d = common::tmp_root_with_carts(&["Apotris", "Zzz"]);
    // "Apotris" sorts before "Zzz", so `Action::Insert` seats it.
    common::write_retail_header(&d, "Apotris", "APOTRIS", "2ATE");
    let mut app = seated_on_gpsp(&d);
    app.apply(Action::GameMenu);
    assert!(
        !app.game_menu_open(),
        "a cart gpSP has no protocol for was offered a link screen"
    );
    assert_eq!(
        app.toast(),
        Some(slot_ui::Toast::NoLink),
        "the press did nothing and said nothing"
    );
    assert!(
        matches!(app.phase(), Phase::Playing { .. }),
        "the refusal took the game away: {:?}",
        app.phase()
    );
    assert!(
        !app.link_active(),
        "a session started for a cart gpSP will not link"
    );
}

/// The other half of it: a cart gpSP does carry still opens the screen, and says nothing in the
/// banner. Without this the refusal above passes just as well with the link screen removed.
#[test]
fn a_cart_gpsp_carries_still_opens_the_link_screen() {
    for (stem, title, code) in [
        ("Mario Golf", "MARIO GOLF", "BMGE"),    // the adapter list
        ("Emerald", "POKEMON EMER", "BPEE"),     // the Pokémon family
        ("Advance Wars", "ADVANCEWARS", "AWRE"), // Advance Wars
    ] {
        let d = common::tmp_root_with_carts(&["Zzz"]);
        common::write_retail_header(&d, stem, title, code);
        let mut app = seated_on_gpsp(&d);
        app.apply(Action::GameMenu);
        assert!(app.game_menu_open(), "{code} was refused its link screen");
        assert_eq!(
            app.toast(),
            None,
            "{code} opened the screen and said so too"
        );
    }
}

/// Which refusal wins when both apply. Apotris on mGBA is the cart the user pressed this on:
/// nothing on the card can link it, and the old order answered with the core instead, sending
/// them to gpSP for a game gpSP cannot carry either — a core swap and a reload to arrive back at
/// a refusal they could not reach from here. The cart's own answer is the one that survives a
/// switch, so it is the one the banner gives.
#[test]
fn a_cart_gpsp_cannot_link_is_refused_on_gpsp_and_carried_by_mgbas_cable() {
    let refused = |core, open: bool| {
        let d = common::tmp_root_with_carts(&["Apotris", "Zzz"]);
        common::write_retail_header(&d, "Apotris", "APOTRIS", "2ATE");
        let mut app = seated_on(&d, core);
        app.apply(Action::GameMenu);
        assert_eq!(
            app.game_menu_open(),
            open,
            "{core:?} answered Apotris with the wrong screen"
        );
        app.toast()
    };
    // gpSP speaks named protocols and has none for this cart, so there is nothing to reach.
    assert_eq!(refused(Core::Gpsp, false), Some(Toast::NoLink));
    // mGBA does not speak the game's protocol at all; it runs both machines in step, which
    // carries every cart, Apotris included.
    assert_eq!(refused(Core::Mgba, true), None);
}

/// The other half of the order, and the half that keeps "switch to gpSP" worth saying: a cart
/// gpSP really can link, sitting on mGBA, is still told which core would carry it. Without this
/// the refusal above passes just as well with `Toast::NeedsGpsp` deleted outright.
#[test]
fn a_wireless_adapter_cart_on_mgba_still_says_to_switch_to_gpsp() {
    let d = common::tmp_root_with_carts(&["Emerald", "Zzz"]);
    common::write_retail_header(&d, "Emerald", "POKEMON EMER", "BPEE");
    let mut app = seated_on(&d, Core::Mgba);
    app.apply(Action::GameMenu);
    assert!(
        !app.game_menu_open(),
        "mGBA offered a cable to a cart that talks to the Wireless Adapter"
    );
    assert_eq!(
        app.toast(),
        Some(Toast::NeedsGpsp),
        "a cart gpSP can carry was told there is no link support for it"
    );
}

/// The legend names SELECT only where SELECT does something. A game gpSP links the same way on
/// either hardware refuses the press — `select_is_refused_where_gpsp_would_link_the_same_either_way`
/// is that refusal — so a legend offering Mode over it is the screen promising a choice the core
/// will not honour.
#[test]
fn the_pick_legend_names_mode_only_where_the_hardware_can_be_switched() {
    let faces: Vec<(TexId, u32)> = LinkLegend::ALL
        .iter()
        .map(|k| (TexId::from_raw(900 + k.index()), 40))
        .collect();
    let mode = faces[LinkLegend::Mode.index()].0;
    let drawn = |app: &App| {
        let mut out = Vec::new();
        app.draw(&mut out);
        out
    };

    // A Pokémon cart: the cable is `mul_poke` and the adapter `rfu`, two modes gpSP really does
    // load it differently with, so the switch is a choice and the legend says so.
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.set_link_legend_faces(faces.clone());
    app.apply(Action::GameMenu);
    assert!(
        drawn(&app)
            .iter()
            .any(|d| matches!(*d, Draw::Tex { tex, .. } if tex == mode)),
        "a cart whose hardware can be switched did not offer SELECT"
    );

    // An adapter-list game with no cable protocol of its own: it loads on `auto` either way and
    // gpSP links it over the adapter regardless.
    let d = common::tmp_root_with_carts(&["Zzz"]);
    common::write_retail_header(&d, "Mario Golf", "MARIO GOLF", "BMGE");
    let mut app = seated_on_gpsp(&d);
    app.set_link_legend_faces(faces.clone());
    app.apply(Action::GameMenu);
    let out = drawn(&app);
    assert!(
        !out.iter()
            .any(|d| matches!(*d, Draw::Tex { tex, .. } if tex == mode)),
        "the screen offered SELECT over a game gpSP links the same either way"
    );
    for k in [LinkLegend::Cancel, LinkLegend::Swap, LinkLegend::Link] {
        let want = faces[k.index()].0;
        assert!(
            out.iter()
                .any(|d| matches!(*d, Draw::Tex { tex, .. } if tex == want)),
            "{k:?} left the legend along with Mode"
        );
    }
}

/// What the radio was asked to do, in order. `App` never waits on any of it, so the queue
/// behind these is replaceable and a test can simply read the list.
#[derive(Clone, Default)]
struct RadioLog(Arc<std::sync::Mutex<Vec<RadioJob>>>);

impl RadioLog {
    fn jobs(&self) -> Vec<RadioJob> {
        self.0.lock().expect("radio log").clone()
    }
}

impl RadioJobs for RadioLog {
    fn ask(&mut self, job: RadioJob) {
        self.0.lock().expect("radio log").push(job);
    }

    /// Asked for is not loaded. Nothing behind this log ever runs, so no warm it was handed ever
    /// finishes — which is the state these tests drive the screen in.
    fn warmed(&self) -> bool {
        false
    }
}

fn watched(app: &mut App) -> RadioLog {
    let log = RadioLog::default();
    app.set_radio_jobs(Box::new(log.clone()));
    log
}

/// The driver takes about a second to load and the player is about to spend longer than that
/// choosing a role, so the screen opening is what pays for it. Nothing waits on it: `link
/// host` loads the driver itself if this has not finished.
#[test]
fn opening_the_link_screen_warms_the_radio() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    let log = watched(&mut app);
    app.apply(Action::GameMenu);
    assert_eq!(log.jobs(), vec![RadioJob::Warm]);
}

/// Leaving without starting anything is what says the driver is not going to be used. Left
/// warm, it would sit loaded behind the game until the device powered off, which is the drain
/// the radio is kept off the boot path for.
#[test]
fn leaving_the_link_screen_without_a_session_cools_it() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    let log = watched(&mut app);
    app.apply(Action::GameMenu);
    app.apply(Action::GbaDown(Btn::B));
    assert_eq!(log.jobs(), vec![RadioJob::Warm, RadioJob::Cool]);
}

/// The other half of that rule, and the one that would break a link rather than waste a
/// battery: the screen also closes when a session starts, and cooling under one takes the
/// session's own network down with it.
#[test]
fn a_screen_that_closes_over_a_live_session_leaves_the_radio_alone() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    let log = watched(&mut app);
    app.begin_link(0);
    app.apply(Action::GbaDown(Btn::B));
    assert!(
        !log.jobs().contains(&RadioJob::Cool),
        "cooled the radio a live session was running over"
    );
}

/// The same shortcut with no session is what opens the screen, which is the behaviour it had
/// before it learned to end one.
#[test]
fn the_shortcut_still_opens_the_screen_when_nothing_is_linked() {
    let (mut app, _d) = playing_on(Core::Gpsp);
    app.apply(Action::GameMenu);
    assert!(app.game_menu_open());
    assert_eq!(app.toast(), None);
}

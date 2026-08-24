mod common;

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use common::{
    app_playing_in, app_playing_with, panel_with_battery, tmp_root_with_carts, StubSnapshot,
};
use slot::app::Phase;
use slot::emu::Speed;
use slot::link_net::{Cancel, TcpLink};
use slot::session::Session;
use slot_input::{Action, Btn, Millis, RawEvent};
use slot_power::{Battery, Charge};
use slot_retro::{LinkChannel, LoopbackLink, NETPACKET_RELIABLE};
use slot_store::{write_slot_state, Core, SlotState, StateRing};

/// Both ends on loopback: no radio, no peer device, no BaseOS. This proves the framing and
/// the threading, which is everything the transport is responsible for.
#[test]
fn a_packet_survives_the_wire_intact() {
    let port = 45881;
    let server = std::thread::spawn(move || TcpLink::host("127.0.0.1", port).expect("host"));
    std::thread::sleep(std::time::Duration::from_millis(150));
    let mut client = TcpLink::join("127.0.0.1", port).expect("join");
    let mut host = server.join().expect("host thread");

    host.send(NETPACKET_RELIABLE, b"\x01\x02\x03");
    client.send(NETPACKET_RELIABLE, b"from the other side");

    let got = wait_for(&mut client);
    assert_eq!(got.as_deref(), Some(&b"\x01\x02\x03"[..]));
    let got = wait_for(&mut host);
    assert_eq!(got.as_deref(), Some(&b"from the other side"[..]));
}

/// Two packets that arrive in the *same* read (the whole reason for the length prefix) are
/// still delivered as two packets, not one run of bytes.
///
/// Driving this through two `send()` calls and hoping they race the reader thread onto a
/// single `read()` was tried first and is flaky: on loopback the OS just as often delivers
/// them as two separate reads, which makes the test pass even with framing ripped out
/// (verified empirically: an unframed implementation still passed ~20% of runs). So this
/// writes both length-prefixed frames in one `write_all` from a raw socket, which guarantees
/// they land in the kernel's receive buffer together before `TcpLink`'s reader thread ever
/// calls `read()` on them — a deterministic reproduction of "TCP delivers in batches faster
/// than the game reads them", the exact gpSP behaviour this framing exists for.
#[test]
fn batched_writes_keep_their_boundaries() {
    let port = 45882;
    let server = std::thread::spawn(move || TcpLink::host("127.0.0.1", port).expect("host"));
    std::thread::sleep(std::time::Duration::from_millis(150));
    let mut raw = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let mut host = server.join().expect("host thread");

    let mut batch = Vec::new();
    batch.extend_from_slice(&5u16.to_be_bytes());
    batch.extend_from_slice(b"first");
    batch.extend_from_slice(&6u16.to_be_bytes());
    batch.extend_from_slice(b"second");
    raw.write_all(&batch).expect("write batch");

    assert_eq!(wait_for(&mut host).as_deref(), Some(&b"first"[..]));
    assert_eq!(wait_for(&mut host).as_deref(), Some(&b"second"[..]));
}

#[test]
fn try_recv_never_blocks_on_an_idle_link() {
    let port = 45883;
    let server = std::thread::spawn(move || TcpLink::host("127.0.0.1", port).expect("host"));
    std::thread::sleep(std::time::Duration::from_millis(150));
    let mut client = TcpLink::join("127.0.0.1", port).expect("join");
    let _host = server.join().expect("host thread");

    let start = std::time::Instant::now();
    assert_eq!(client.try_recv(), None);
    assert!(
        start.elapsed() < std::time::Duration::from_millis(5),
        "try_recv blocked, which would cost frames"
    );
}

/// The peer vanishing must end the reader thread quietly and leave `try_recv` and `send`
/// safe to keep calling. `try_recv` staying at `None` forever is the correct, boring outcome.
///
/// The "peer" here is a raw socket, not a `TcpLink`: a `TcpLink` never actually closes its
/// side of the connection when dropped, because its reader thread holds its own clone of
/// the socket (`try_clone`, a dup at the OS level) and keeps that file descriptor open even
/// after the `TcpLink` value is gone. Dropping a `TcpLink` peer would therefore leave the
/// connection fully alive underneath and prove nothing. A raw `TcpStream` with no clone
/// genuinely closes on drop, which is what a peer process disappearing looks like on the
/// wire — an actual FIN, not a no-op.
#[test]
fn peer_disconnecting_does_not_panic_or_hang() {
    let port = 45884;
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind");
    let acceptor = std::thread::spawn(move || listener.accept().expect("accept").0);
    let mut client = TcpLink::join("127.0.0.1", port).expect("join");
    let host_raw = acceptor.join().expect("accept thread");

    drop(host_raw);

    // Give the reader thread a moment to notice EOF, then poll a few more times: none of
    // this should panic, hang, or ever report a packet that didn't arrive.
    std::thread::sleep(std::time::Duration::from_millis(200));
    for _ in 0..10 {
        assert_eq!(client.try_recv(), None);
    }
    // Writing into a connection whose peer is gone must not panic either: the emulator
    // thread calls send() without knowing whether anyone is still listening. A single write
    // right after close is not a reliable trigger — TCP half-close means the very first
    // write can still succeed locally before the peer's RST comes back — so write several
    // times over a short window to make sure at least one lands after the connection is
    // fully torn down.
    for _ in 0..20 {
        client.send(NETPACKET_RELIABLE, b"into the void");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// Dropping a `TcpLink` must close its socket: the peer needs a real FIN, not a link that
/// has merely gone quiet, since silence is indistinguishable from a player still thinking.
///
/// The peer here is a raw `TcpStream`, not a `TcpLink`, for the same reason as
/// `peer_disconnecting_does_not_panic_or_hang` above: a raw socket genuinely reflects what
/// arrives on the wire. A read timeout makes the proof deterministic — if the drop doesn't
/// close the connection, this test fails on its own timeout instead of hanging the suite.
#[test]
fn dropping_the_link_closes_the_wire() {
    let port = 45885;
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind");
    let acceptor = std::thread::spawn(move || listener.accept().expect("accept").0);
    let client = TcpLink::join("127.0.0.1", port).expect("join");
    let mut host_raw = acceptor.join().expect("accept thread");
    host_raw
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .expect("set read timeout");

    drop(client);

    let mut buf = [0u8; 1];
    let n = host_raw.read(&mut buf).expect("read after drop");
    assert_eq!(
        n, 0,
        "peer should observe a clean EOF, not a hang or an error"
    );
}

/// C1: a peer that accepts and then never reads must not be able to block `send` — the
/// worker calls it from inside its own frame, every present, and `EmuHandle::drop` joins
/// that thread, so a hang here used to hang eject, cart swap and shutdown behind it.
/// Before the fix (a direct, synchronous `write_all`) this test does not fail — it hangs,
/// the same way the reviewer had to kill a hand-run reproduction of it, confirmed by hand at
/// 90 s with no end in sight.
///
/// Driven from its own thread and bounded by `recv_timeout` below, rather than calling
/// `send` straight from the test thread the way earlier versions of this test did: a
/// regression back to a direct, synchronous `write_all` would block inside the loop and
/// never send on `tx` at all, which would hang this test — and the whole suite behind it —
/// instead of failing it. `host_binds_the_address_it_is_given_not_every_interface` above
/// uses the identical shape for the identical reason: no test may be allowed to hang the
/// suite, a bound failure is strictly better than an unbounded one.
#[test]
fn send_never_blocks_on_a_peer_that_stopped_reading() {
    let port = 45886;
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind");
    let acceptor = std::thread::spawn(move || listener.accept().expect("accept").0);
    let mut client = TcpLink::join("127.0.0.1", port).expect("join");
    // Accepted, held onto, and never read from again.
    let _peer = acceptor.join().expect("accept thread");

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // Large packets, comfortably more total volume than any OS's default *or
        // auto-tuned* socket buffers would absorb before a synchronous `write_all` blocked
        // waiting for the peer to make room. A smaller burst of small packets was tried
        // first and is not reliable here: macOS's default 128 KB send buffer, with the
        // kernel free to auto-tune well past it for a fast loopback link, swallowed tens of
        // thousands of small sends without the old, unfixed synchronous `write_all` ever
        // blocking at all — the volume has to clear that headroom, not just clear a
        // headline packet count.
        let payload = vec![0u8; 60_000];
        for _ in 0..300 {
            client.send(NETPACKET_RELIABLE, &payload);
        }
        let _ = tx.send(());
    });

    assert!(
        rx.recv_timeout(Duration::from_secs(2)).is_ok(),
        "send blocked on a peer that stopped reading"
    );
}

/// I7: `set_nodelay(true)` in `wrap` had no test watching it at all — one of the four
/// wirings the reviewer's mutation run found nothing covering.
#[test]
fn wrap_disables_nagle_on_both_sockets() {
    let port = 45887;
    let server = std::thread::spawn(move || TcpLink::host("127.0.0.1", port).expect("host"));
    std::thread::sleep(Duration::from_millis(150));
    let client = TcpLink::join("127.0.0.1", port).expect("join");
    let host = server.join().expect("host thread");

    assert!(
        host.nodelay().expect("nodelay"),
        "the host socket must disable Nagle"
    );
    assert!(
        client.nodelay().expect("nodelay"),
        "the joiner socket must disable Nagle"
    );
}

/// I6: `host` used to bind `0.0.0.0` regardless of what it was asked for, reachable from
/// anything on the user's home network rather than just the private WiFi a session actually
/// runs over. Proven here by an address absent from every interface on this machine
/// (`203.0.113.1` is RFC 5737's TEST-NET-3, reserved so it can never be a real one): if
/// `host` still bound the wildcard under the hood instead of what it was actually given,
/// this bind would succeed rather than fail.
#[test]
fn host_binds_the_address_it_is_given_not_every_interface() {
    // Run on its own thread and bounded with a timeout, rather than called directly: if a
    // regression ever bound `0.0.0.0` again, `bind` would succeed here and `host` would go
    // on to block in `accept()` forever, with nothing on the other end ever going to
    // connect. No test may be allowed to hang the suite that way — a bind failure resolves
    // near instantly, so the timeout below only ever gets paid on the regression itself,
    // turning what would otherwise be a hang into a clean, deterministic failure.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let kind = TcpLink::host("203.0.113.1", 0).err().map(|e| e.kind());
        let _ = tx.send(kind);
    });
    match rx.recv_timeout(Duration::from_secs(2)) {
        Ok(Some(kind)) => assert_eq!(kind, std::io::ErrorKind::AddrNotAvailable),
        Ok(None) => panic!("must not silently bind 0.0.0.0"),
        Err(_) => panic!(
            "host() did not return within 2s -- a bind-address regression blocks forever in \
             accept() instead of failing to bind, which is exactly what this test exists to \
             catch without hanging the suite to do it"
        ),
    }
}

fn wait_for(link: &mut TcpLink) -> Option<Vec<u8>> {
    for _ in 0..200 {
        if let Some(p) = link.try_recv() {
            return Some(p);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    None
}

// --- the interlocks -----------------------------------------------------------------------
//
// libretro.h: "When two or more players are connected and this interface has been set, time
// manipulation features (such as pausing, slow motion, fast forward, rewinding, save state
// loading, etc.) are disabled to avoid interrupting communication." These tests drive `App`
// with no core, no device and no transport — `begin_link`/`end_link` are pure state, exactly
// like every other phase transition in this file — so they exercise the interlocks directly
// rather than through a live session nobody here can open a real one for.

#[test]
fn a_live_session_disables_rewind_and_state_loading() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    assert!(
        a.may_rewind() && a.may_load_state(),
        "not what a fresh app should refuse"
    );

    a.begin_link(0);
    assert!(a.link_active());
    assert!(!a.may_rewind(), "rewind interrupts communication");
    assert!(
        !a.may_load_state(),
        "a state load desynchronises the other device"
    );
}

/// The button still means something during a session — it just means "declined" rather than
/// "start rewinding" — so it shakes the screen instead of doing nothing. Silence reads as a
/// press that never landed.
#[test]
fn a_live_session_refuses_a_rewind_press_instead_of_dropping_it() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.begin_link(0);

    a.apply(Action::RewindStart);
    assert!(
        a.refusal_active(a.now()),
        "a refused rewind must shake, not vanish silently"
    );
}

/// Refused ahead of even checking whether there is a state to load, so a session with real
/// saves sitting in the ring still declines — proving the session is what refused it, not an
/// incidentally empty ring (`loading_with_no_states_shakes_as_well` in refusal.rs already
/// covers that ordinary case).
#[test]
fn a_live_session_refuses_a_state_load_even_when_one_exists() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::SaveState);
    a.begin_link(0);

    a.apply(Action::LoadState);
    assert!(
        a.refusal_active(a.now()),
        "a refused load must shake, not vanish silently"
    );
}

/// C3: `load_newest` (`Action::LoadState`, above) was the only one of the two switcher load
/// routes ever guarded. `load_selected` — the switcher's own A-button pick — funnelled into
/// the same unguarded `load_file` underneath, and reached the core with no check at all. The
/// switcher has to already be open for a pick to mean anything, so this begins the session
/// only after opening it — proving the guard `load_file` itself now carries, independent of
/// whichever caller happens to reach it.
#[test]
fn a_live_session_refuses_a_switcher_pick_even_when_one_exists() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let r = StateRing::new(d.path(), Core::Mgba, "Emerald");
    r.push(&[0u8; 64], b"png", "2026-08-09_00-00-00").unwrap();
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::Polaroids);
    a.begin_link(0);

    a.apply(Action::GbaDown(Btn::A));
    assert!(
        a.refusal_active(a.now()),
        "picking a state in the switcher must be refused during a session"
    );
    // `load_selected` used to call `close_polaroids` unconditionally, so a refused pick
    // shook the screen *and* closed the switcher out from under it — a refusal that reads
    // as accepted-and-dismissed is worse than no feedback at all.
    assert!(
        matches!(a.phase(), Phase::Polaroids { .. }),
        "a refused pick must not also close the switcher"
    );
}

/// C3: the same hole reaches undo. `load_file` guards every load read fresh off disk, but an
/// undo of a *load* replays bytes already in hand from when that load happened — it never
/// calls `load_file` at all, so it needs its own check. Priming the offer happens before the
/// session starts (a load during one is refused by the test above already); this proves the
/// undo of that earlier, legitimate load is what a session still forbids.
#[test]
fn a_live_session_refuses_to_undo_a_load() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let r = StateRing::new(d.path(), Core::Mgba, "Emerald");
    r.push(&[7u8; 64], b"png", "2026-08-09_00-00-00").unwrap();
    let (snapshot, loaded) = StubSnapshot::pair();
    let mut a = app_playing_with(d.path(), "Emerald", snapshot);

    a.apply(Action::Polaroids);
    a.apply(Action::GbaDown(Btn::A)); // load_selected: primes a Load undo
    assert!(a.undo_available(a.now()), "the load did not offer an undo");
    *loaded.lock().unwrap() = None; // clear what that priming load itself recorded

    a.begin_link(0);
    a.undo(a.now());

    assert!(
        a.refusal_active(a.now()),
        "undoing a load must be refused during a session, the same hazard load_file guards"
    );
    assert!(
        loaded.lock().unwrap().is_none(),
        "the core must not have been moved to the prior state"
    );
    assert!(
        a.undo_available(a.now()),
        "a refused undo must not consume the offer"
    );
}

/// C3: opening the switcher pauses the core (`Session::sync_speed` maps `Phase::Polaroids`
/// straight to `Speed::Paused`), one of the exact manipulations libretro's netpacket
/// contract forbids while a session is live — whether or not the player means to load
/// anything once inside.
#[test]
fn a_live_session_refuses_to_open_the_switcher() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let r = StateRing::new(d.path(), Core::Mgba, "Emerald");
    r.push(&[0u8; 64], b"png", "2026-08-09_00-00-00").unwrap();
    let mut a = app_playing_in(d.path(), "Emerald");
    a.begin_link(0);

    a.apply(Action::Polaroids);
    assert!(
        matches!(a.phase(), Phase::Playing { .. }),
        "opening the switcher pauses the core, which a session forbids"
    );
    assert!(
        a.refusal_active(a.now()),
        "a refused switcher open must shake"
    );
}

/// C3: the power menu pauses the core too (`Session::sync_speed` maps `held()`, which an
/// open menu is one of, to `Speed::Paused`) — the same hazard as the switcher, reached
/// through `PowerHold` instead.
#[test]
fn a_live_session_refuses_to_open_the_power_menu() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.begin_link(0);

    a.apply(Action::PowerHold);
    assert!(
        a.power_menu().is_none(),
        "the power menu pauses the core, which a session forbids"
    );
    assert!(
        a.refusal_active(a.now()),
        "a refused power-menu open must shake"
    );
}

/// I3: `eject` used to leave `self.link` untouched. Proven the way the reviewer proved it —
/// `link_active()` still true, both interlocks still wedged closed — with no cart left to
/// carry the session at all, and nothing short of this fix ever clearing it again.
#[test]
fn ejecting_during_a_session_ends_it() {
    let d = tmp_root_with_carts(&["Emerald", "Ruby"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.begin_link(0);
    assert!(a.link_active());

    a.apply(Action::Eject);
    assert!(
        !a.link_active(),
        "an ejected cart must not leave a phantom session behind"
    );
    assert!(a.may_rewind());
    assert!(a.may_load_state());
}

/// One button, one meaning at a time. Ending a live session is what this press is for, and
/// it must not also flush-and-continue as though nothing were open.
#[test]
fn a_power_press_ends_a_live_session_instead_of_flushing_only() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.begin_link(0);
    assert!(a.link_active());

    a.apply(Action::PowerPress);
    assert!(!a.link_active(), "a power press must end a live session");
    assert!(
        matches!(a.phase(), Phase::Playing { .. }),
        "ending the session is not an eject or a doze"
    );

    // Nothing left to end: a second press is not an error, and behaves exactly as it does
    // today outside a session (a flush, nothing else — there is no session left to end).
    a.apply(Action::PowerPress);
    assert!(!a.link_active());
}

/// `PowerTap` reaches `doze` by way of `power_press`, bypassing `PowerPress`'s own guard
/// entirely — the actual hole this closes. `doze` ends a live session and then completes the
/// doze underneath it (see `doze`'s own doc comment for why leaving the device awake behind
/// it is worse, not safer) — so one tap during a session both ends it and puts the device to
/// sleep, exactly as one tap outside a session already does.
#[test]
fn a_power_tap_ends_a_live_session_and_dozes_in_the_same_tap() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.begin_link(0);
    assert!(a.link_active());

    a.apply(Action::PowerTap);
    assert!(!a.link_active(), "a power tap must end a live session");
    assert!(
        matches!(a.phase(), Phase::Doze { .. }),
        "ending the session must not leave the device awake behind the tap that closed it"
    );

    // A second tap wakes, exactly as it does outside a session.
    a.apply(Action::PowerTap);
    assert!(matches!(a.phase(), Phase::Playing { .. }));
}

/// `LidClose` calls `doze` directly (both arms: with the power menu open and without), so it
/// needs no guard of its own — the one inside `doze` closes this path exactly the way it
/// closes `PowerTap`'s. A real lid delivers `LidClose` once per physical close
/// (`gesture.rs:198`) — there is no second press to "retry" with, the way `PowerTap` has —
/// so ending a live session here must land the device in `Doze`, not leave it awake behind a
/// shut lid at 400-700 mA waiting for a close that is not coming again. `LidOpen` is what a
/// real lid can actually deliver next, and is what wakes it back up.
#[test]
fn a_lid_close_ends_a_live_session_and_dozes() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.begin_link(0);
    assert!(a.link_active());

    a.apply(Action::LidClose);
    assert!(!a.link_active(), "closing the lid must end a live session");
    assert!(
        matches!(a.phase(), Phase::Doze { .. }),
        "ending the session must not leave the device awake behind a shut lid"
    );

    // The lid opening, not a second close, is what hardware can actually deliver next.
    a.apply(Action::LidOpen);
    assert!(matches!(a.phase(), Phase::Playing { .. }));
}

/// I1: the fifth route into `begin_power_off` — no button, no menu, and no doze anywhere
/// upstream of it to have ended a session first. Before this fix `on_battery` reached
/// `begin_power_off` with no session guard at all, so a critical reading mid-session left
/// `link_active()` true, `shutting_down()` true, and the core paused underneath both
/// (`Session::sync_speed` maps `held()`, of which `shutting_down()` is one part, straight to
/// `Speed::Paused`) — the exact hazard libretro's netpacket contract forbids, with the
/// session itself never ended and nothing ever telling the core or the peer the exchange was
/// over. `begin_power_off` now ends a live session itself, the same way `doze` already does.
#[test]
fn a_critical_battery_reading_ends_a_live_session_instead_of_pausing_it() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.begin_link(0);
    assert!(a.link_active());

    a.on_battery(Battery {
        percent: 3,
        charge: Charge::Discharging,
    });
    assert!(
        !a.link_active(),
        "a critical battery reading must end a live session, not merely pause it"
    );
    assert!(a.powering_off(), "the shutdown itself must still proceed");
}

/// The production path, and the only one: `App::update` drives `timers`, which reads
/// `doze_expired` itself rather than being told the timeout fired — `on_doze_timeout` (also
/// callable directly, which is what `power.rs`'s own timeout tests use as a stand-in for the
/// wait) carries no guard of its own, so this is what actually protects a live session. A
/// trade partner reading a menu on the other device must not have the link dropped because
/// this one sat idle behind a closed lid.
#[test]
fn doze_never_expires_while_a_session_is_live() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.set_power(common::panel(d.path(), Duration::from_secs(2)).0);
    a.apply(Action::LidClose);
    a.begin_link(0);

    // Three seconds of updates against a two second timeout: comfortably past it, the same
    // margin `power.rs`'s own timeout tests use.
    for _ in 0..180 {
        a.update(1.0 / 60.0);
    }
    assert!(
        !a.powering_off(),
        "a link session was dropped by the doze timer"
    );
    assert!(matches!(a.phase(), Phase::Doze { .. }));
}

#[test]
fn ending_a_session_restores_normal_behaviour() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.begin_link(0);

    a.end_link();
    assert!(!a.link_active());
    assert!(a.may_rewind());
    assert!(a.may_load_state());
    assert_eq!(a.link_client_id(), None);
}

#[test]
fn link_client_id_reports_which_side_of_the_session_this_device_is() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    assert_eq!(a.link_client_id(), None, "nothing to ask about yet");

    a.begin_link(1);
    assert_eq!(a.link_client_id(), Some(1));
}

// --- ending a session reaches the emulator thread, not just App's own bookkeeping ---------
//
// Every test above drives `App` alone, with no core and no `EmuHandle` — proof enough that
// `App::end_link` clears the app's own state, but not that anything downstream ever hears
// about it. `App::end_link` never touches the core (see its doc comment): `Session::act` is
// what bridges an ending onto `EmuHandle::end_link`, which is what this test drives a real
// `Session` — App plus a spawned core — to prove.

fn step(s: &mut Session, now: &mut Millis, ev: Option<RawEvent>) {
    *now += 16;
    s.feed(ev, *now);
    s.update(1.0 / 60.0);
}

fn wait_until(cond: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if cond() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Before the fix in `Session::act` this could pass forever with the fix absent: `App`'s own
/// bookkeeping already flipped correctly on a power press (see
/// `a_power_press_ends_a_live_session_instead_of_flushing_only` above), so a test that only
/// reads `App::link_active` would never notice the core was left believing a session it can
/// no longer reach was still live, still pumping, still producing packets nobody carries.
#[test]
fn ending_a_session_reaches_the_emulator_thread_too() {
    let d = tmp_root_with_carts(&["Emerald"]);
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
        step(&mut s, &mut now, None);
        std::thread::sleep(Duration::from_millis(1));
    }

    s.emu()
        .expect("a cart is seated, a core must be running")
        .begin_link(0, Box::new(LoopbackLink::default()));
    assert!(
        wait_until(|| s.emu().is_some_and(|e| e.net().is_active())),
        "begin_link never took"
    );
    s.app_mut().begin_link(0);
    assert!(s.app().link_active());

    step(&mut s, &mut now, Some(RawEvent::Down(Btn::Power)));
    assert!(
        !s.app().link_active(),
        "the app's own bookkeeping should have ended"
    );
    assert!(
        wait_until(|| s.emu().is_some_and(|e| !e.net().is_active())),
        "ending a session at the App level must reach the emulator thread too"
    );
}

/// I1: the fifth route closed at the `App` level
/// (`a_critical_battery_reading_ends_a_live_session_instead_of_pausing_it` in this file) needs
/// the same proof `ending_a_session_reaches_the_emulator_thread_too` gives `PowerPress` above
/// — that the ending mirrors onto the emulator thread too, not only `App`'s own bookkeeping.
/// This route reaches `App` through `update`/`timers`, not through `apply`, which is exactly
/// what `Session::act`'s old, `apply`-only bridge never watched; `Session::bridge_link` now
/// wraps both of `Session`'s own entry points into `App`, so an ending reached this way
/// reaches the emulator thread exactly like a button press does. Driven through a real
/// `Power`, not `on_battery` called directly: the ending has to happen *inside* the same
/// `update` call `bridge_link` wraps, exactly as a real battery poll would deliver it, or the
/// edge it watches for would never fire.
#[test]
fn a_critical_battery_reading_reaches_the_emulator_thread_too() {
    let d = tmp_root_with_carts(&["Emerald"]);
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
        step(&mut s, &mut now, None);
        std::thread::sleep(Duration::from_millis(1));
    }

    s.emu()
        .expect("a cart is seated, a core must be running")
        .begin_link(0, Box::new(LoopbackLink::default()));
    assert!(
        wait_until(|| s.emu().is_some_and(|e| e.net().is_active())),
        "begin_link never took"
    );
    s.app_mut().begin_link(0);
    assert!(s.app().link_active());

    // `set_power` primes `battery_at` to fire on the very next poll (see its own doc
    // comment), so one frame is enough for `timers` to read this critical reading and call
    // `on_battery` from inside the `update` this test's `step` drives.
    let (power, _) = panel_with_battery(d.path(), Duration::from_secs(300), 1, 3);
    s.app_mut().set_power(power);
    step(&mut s, &mut now, None);

    assert!(
        !s.app().link_active(),
        "a critical battery reading must end a live session"
    );
    assert!(
        wait_until(|| s.emu().is_some_and(|e| !e.net().is_active())),
        "ending a session from inside `update` must reach the emulator thread too"
    );
}

/// C3: fast forward had no `may_*` check at all — `session.rs:334` picked `Speed::Fast`
/// whenever R2 was held and the game was playing, session or not. Driven through a real
/// `Session` (unlike the pure-`App` interlock tests above) because the gate lives in
/// `Session::sync_speed`, not in `App`.
#[test]
fn a_live_session_refuses_fast_forward() {
    let d = tmp_root_with_carts(&["Emerald"]);
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
        step(&mut s, &mut now, None);
        std::thread::sleep(Duration::from_millis(1));
    }

    s.app_mut().begin_link(0);
    step(&mut s, &mut now, Some(RawEvent::Down(Btn::R2)));
    for _ in 0..5 {
        step(&mut s, &mut now, None);
    }

    assert!(
        wait_until(|| s.emu().is_some_and(|e| e.observed_speed() == Speed::Normal)),
        "fast forward must be refused while a session is live"
    );
}

/// I6: `sync_ff_hud` did not consult `may_fast_forward()`, so the badge kept reading Held or
/// Latched during a live session even though `sync_speed` — proven above — was already
/// withholding the speed underneath it. `sync_rewind_hud`'s own `actually_rewinding` already
/// guards the rewind bar against exactly this; showing a player their input landed when a
/// session actually withheld it is the specific lie that comment warns about, and this is the
/// fast forward badge's turn to make the same promise.
#[test]
fn a_live_session_hides_the_fast_forward_badge() {
    let d = tmp_root_with_carts(&["Emerald"]);
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
        step(&mut s, &mut now, None);
        std::thread::sleep(Duration::from_millis(1));
    }

    s.app_mut().begin_link(0);
    step(&mut s, &mut now, Some(RawEvent::Down(Btn::R2)));

    assert_eq!(
        s.app().ff_badge(),
        None,
        "the badge must not claim fast forward is happening while a session withholds it"
    );
}

// --- a bounded, cancellable accept ---------------------------------------------------------
//
// `host` used to block in `accept()` forever with nothing able to interrupt it. A real entry
// point wants both ways out, and each has to be told apart from the other on screen: a
// deadline means nobody arrived, a cancel means the player changed their mind.

/// Port 0 is "any free port", and nothing is ever told which one it got — so nothing can
/// connect, which is the point. The bound is what has to end this.
#[test]
fn a_host_that_nobody_joins_gives_up_instead_of_waiting_forever() {
    let cancel = Cancel::new();
    let started = Instant::now();
    // `let Err(..) else` rather than `expect_err`, which would want a `Debug` on `TcpLink`
    // that nothing but this line has ever asked for.
    let Err(err) = TcpLink::host_until("127.0.0.1", 0, Duration::from_millis(300), &cancel) else {
        panic!("nobody connected, so this must not succeed");
    };
    assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "waited past its bound"
    );
}

#[test]
fn a_host_can_be_cancelled_while_it_is_waiting() {
    let cancel = Cancel::new();
    let flag = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        flag.cancel();
    });
    let started = Instant::now();
    let Err(err) = TcpLink::host_until("127.0.0.1", 0, Duration::from_secs(60), &cancel) else {
        panic!("cancelled, so this must not succeed");
    };
    assert_eq!(err.kind(), std::io::ErrorKind::Interrupted);
    // The bound was a minute; cancellation is what ended this, not the deadline.
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "cancel did not take effect"
    );
}

/// The bound and the flag are both ways out, not the only ways out: a peer that does arrive
/// still has to come back as a link that carries a packet, in *both* directions.
///
/// The inbound half is the load-bearing one. `host_until` puts the listener in non-blocking
/// mode to make the wait pollable, and on this platform the accepted stream inherits that
/// mode through every `try_clone` (a `dup`, one shared file description) — so unless it is
/// taken back off before `wrap` sees it, the host's reader thread meets `WouldBlock` on its
/// first `read_exact` and gives up on the peer forever. Sending host → joiner alone cannot
/// see that: the joiner's socket came from `join`, which this function never touches.
#[test]
fn a_bounded_host_still_accepts_a_peer_that_does_arrive() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let cancel = Cancel::new();
    let peer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        TcpLink::join("127.0.0.1", port)
    });
    let mut host = TcpLink::host_until("127.0.0.1", port, Duration::from_secs(10), &cancel)
        .expect("the peer arrived inside the bound");
    let mut joiner = peer.join().unwrap().expect("joiner connected");

    host.send(0, b"ping");
    let got = wait_for(&mut joiner);
    assert_eq!(
        got.as_deref(),
        Some(&b"ping"[..]),
        "a bounded accept must yield a working link"
    );

    joiner.send(0, b"pong");
    let got = wait_for(&mut host);
    assert_eq!(
        got.as_deref(),
        Some(&b"pong"[..]),
        "the accepted socket must be blocking again, or the host never hears its peer"
    );
}

/// The order the two players press their buttons in must not decide whether they meet. The
/// host has a radio to bring up — one to five seconds on device — before it binds anything,
/// and its player pressed a button the joiner's player never saw.
#[test]
fn a_joiner_waits_for_a_host_that_is_not_listening_yet() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let cancel = Cancel::new();
    let started = Instant::now();
    // The host arrives well after the joiner has already given up under the old one-shot
    // connect, which failed in milliseconds.
    let host = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(400));
        TcpLink::host("127.0.0.1", port)
    });

    let mut joiner = TcpLink::join_until("127.0.0.1", port, Duration::from_secs(10), &cancel)
        .expect("the joiner must wait for a host that is merely late");
    let mut hosted = host.join().unwrap().expect("host bound");
    assert!(
        started.elapsed() >= Duration::from_millis(400),
        "connected before the host existed, so this proves nothing"
    );

    // A real link, not merely a connect that returned: both directions, as with the host.
    joiner.send(0, b"up");
    assert_eq!(wait_for(&mut hosted), Some(b"up".to_vec()));
    hosted.send(0, b"down");
    assert_eq!(wait_for(&mut joiner), Some(b"down".to_vec()));
}

/// A host that never comes has to become an answer rather than a wait with no end.
#[test]
fn a_joiner_gives_up_when_no_host_ever_appears() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let cancel = Cancel::new();
    let started = Instant::now();
    let Err(e) = TcpLink::join_until("127.0.0.1", port, Duration::from_millis(300), &cancel) else {
        panic!("connected to a host that does not exist");
    };
    assert_eq!(
        e.kind(),
        std::io::ErrorKind::TimedOut,
        "a host that never came is `nobody arrived`, not a peer that vanished"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "waited past its bound"
    );
}

/// The joiner spends its wait asleep between attempts, so B has to reach it there too.
#[test]
fn a_joiner_can_be_cancelled_while_it_is_retrying() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let cancel = Cancel::new();
    let flag = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        flag.cancel();
    });
    let started = Instant::now();
    let Err(e) = TcpLink::join_until("127.0.0.1", port, Duration::from_secs(60), &cancel) else {
        panic!("cancelled, so this must not succeed");
    };
    assert_eq!(e.kind(), std::io::ErrorKind::Interrupted);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the bound was a minute; cancellation is what ended this"
    );
}

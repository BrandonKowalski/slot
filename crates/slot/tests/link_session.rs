mod common;

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use common::{app_playing_in, tmp_root_with_carts};
use slot::app::Phase;
use slot::link_net::TcpLink;
use slot::session::Session;
use slot_input::{Action, Btn, Millis, RawEvent};
use slot_retro::{LinkChannel, LoopbackLink, NETPACKET_RELIABLE};
use slot_store::{write_slot_state, SlotState};

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
/// the same way the reviewer had to kill a hand-run reproduction of it.
#[test]
fn send_never_blocks_on_a_peer_that_stopped_reading() {
    let port = 45886;
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind");
    let acceptor = std::thread::spawn(move || listener.accept().expect("accept").0);
    let mut client = TcpLink::join("127.0.0.1", port).expect("join");
    // Accepted, held onto, and never read from again.
    let _peer = acceptor.join().expect("accept thread");

    let start = Instant::now();
    // Large packets, comfortably more total volume than any OS's default *or auto-tuned*
    // socket buffers would absorb before a synchronous `write_all` blocked waiting for the
    // peer to make room. A smaller burst of small packets was tried first and is not
    // reliable here: macOS's default 128 KB send buffer, with the kernel free to auto-tune
    // well past it for a fast loopback link, swallowed tens of thousands of small sends
    // without the old, unfixed synchronous `write_all` ever blocking at all — the volume has
    // to clear that headroom, not just clear a headline packet count.
    let payload = vec![0u8; 60_000];
    for _ in 0..300 {
        client.send(NETPACKET_RELIABLE, &payload);
    }
    assert!(
        start.elapsed() < Duration::from_secs(2),
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
/// entirely — the actual hole this closes. Before the fix this silently dozed a live session
/// instead of ending it, which is precisely the desync `App::may_rewind`/`may_load_state`
/// exist to prevent, just reached through a different button edge.
#[test]
fn a_power_tap_ends_a_live_session_instead_of_dozing() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.begin_link(0);
    assert!(a.link_active());

    a.apply(Action::PowerTap);
    assert!(!a.link_active(), "a power tap must end a live session");
    assert!(
        matches!(a.phase(), Phase::Playing { .. }),
        "ending the session must not also doze in the same tap"
    );

    // Nothing left to end: a second tap behaves exactly as it does outside a session.
    a.apply(Action::PowerTap);
    assert!(matches!(a.phase(), Phase::Doze { .. }));
}

/// `LidClose` calls `doze` directly (both arms: with the power menu open and without), so it
/// needs no guard of its own — the one inside `doze` closes this path exactly the way it
/// closes `PowerTap`'s. A trade partner's lid closing must not silently drop their game the
/// way it silently dozed it before this fix; ending the session here (rather than holding it
/// open through a doze, which pauses the core — see `doze`'s own comment for why that would
/// just trade one contract violation for another) is the deliberate choice.
#[test]
fn a_lid_close_ends_a_live_session_instead_of_dozing() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.begin_link(0);
    assert!(a.link_active());

    a.apply(Action::LidClose);
    assert!(!a.link_active(), "closing the lid must end a live session");
    assert!(
        matches!(a.phase(), Phase::Playing { .. }),
        "ending the session must not also doze on the same close"
    );

    // Nothing left to end: closing it again behaves exactly as it does outside a session.
    a.apply(Action::LidClose);
    assert!(matches!(a.phase(), Phase::Doze { .. }));
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

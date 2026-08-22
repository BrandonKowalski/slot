use std::io::Write;
use std::net::{TcpListener, TcpStream};

use slot::link_net::TcpLink;
use slot_retro::{LinkChannel, NETPACKET_RELIABLE};

/// Both ends on loopback: no radio, no peer device, no BaseOS. This proves the framing and
/// the threading, which is everything the transport is responsible for.
#[test]
fn a_packet_survives_the_wire_intact() {
    let port = 45881;
    let server = std::thread::spawn(move || TcpLink::host(port).expect("host"));
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
    let server = std::thread::spawn(move || TcpLink::host(port).expect("host"));
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
    let server = std::thread::spawn(move || TcpLink::host(port).expect("host"));
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

fn wait_for(link: &mut TcpLink) -> Option<Vec<u8>> {
    for _ in 0..200 {
        if let Some(p) = link.try_recv() {
            return Some(p);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    None
}

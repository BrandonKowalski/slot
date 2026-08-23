//! The worker that does the slow parts of starting a link session.
//!
//! Every test here injects fakes for the radio, so nothing in this file touches a network
//! interface or shells out to `ags-net`. What is under test is the *sequence*: which steps
//! are reported, in what order, and — the one that strands a device if it is wrong — whether
//! the radio is taken back down on the way out.

use std::io;
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use slot::link_net::{Cancel, TcpLink};
use slot::link_radio::LinkRole;
use slot::link_start::{LinkFail, LinkProgress, LinkStarter, LinkStep};

/// How long a test waits for a worker to reach an outcome before deciding it never will.
/// Generous next to anything these fakes do — they answer immediately, or the moment they
/// are cancelled — and short enough that a broken worker fails its test instead of hanging
/// the whole suite.
const BAIL: Duration = Duration::from_secs(5);

/// Poll to the end, throwing away the steps on the way. Returns the one terminal message.
fn drain(starter: &mut LinkStarter) -> LinkProgress {
    let deadline = Instant::now() + BAIL;
    loop {
        match starter.poll() {
            Some(LinkProgress::At(_)) => {}
            Some(outcome) => return outcome,
            None => std::thread::sleep(Duration::from_millis(2)),
        }
        assert!(
            Instant::now() < deadline,
            "the worker never reached an outcome"
        );
    }
}

/// Poll to the end, keeping every step seen on the way and dropping the outcome.
fn drain_steps(starter: &mut LinkStarter) -> Vec<LinkStep> {
    let deadline = Instant::now() + BAIL;
    let mut steps = Vec::new();
    loop {
        match starter.poll() {
            Some(LinkProgress::At(step)) => steps.push(step),
            Some(_) => return steps,
            None => std::thread::sleep(Duration::from_millis(2)),
        }
        assert!(
            Instant::now() < deadline,
            "the worker never reached an outcome"
        );
    }
}

#[test]
fn a_radio_that_will_not_come_up_stops_before_the_socket() {
    let tried_socket = Arc::new(AtomicBool::new(false));
    let seen = tried_socket.clone();
    let downs = Arc::new(AtomicUsize::new(0));
    let count = downs.clone();
    let mut starter = LinkStarter::spawn_with(
        Box::new(|_role| Err(io::Error::other("no ap"))),
        Box::new(move || {
            count.fetch_add(1, Ordering::SeqCst);
        }),
        LinkRole::Host,
        0,
        Box::new(move |_, _| {
            seen.store(true, Ordering::SeqCst);
            Err(io::Error::other("must not be reached"))
        }),
    );
    let outcome = drain(&mut starter);
    assert!(matches!(outcome, LinkProgress::Failed(LinkFail::Radio)));
    assert!(
        !tried_socket.load(Ordering::SeqCst),
        "opened a socket on a network that never came up"
    );
    // `up` failing does not mean nothing came up: `ags-net link host` can get the interface
    // as far as configured and still exit non-zero. The teardown runs on this path too.
    assert_eq!(
        downs.load(Ordering::SeqCst),
        1,
        "a radio that only half came up must still be taken down"
    );
}

#[test]
fn a_failure_always_takes_the_radio_back_down() {
    let downs = Arc::new(AtomicUsize::new(0));
    let count = downs.clone();
    let mut starter = LinkStarter::spawn_with(
        Box::new(|_| Ok(())),
        Box::new(move || {
            count.fetch_add(1, Ordering::SeqCst);
        }),
        LinkRole::Host,
        0,
        Box::new(|_, _| Err(io::Error::new(io::ErrorKind::TimedOut, "nobody"))),
    );
    let outcome = drain(&mut starter);
    assert!(matches!(
        outcome,
        LinkProgress::Failed(LinkFail::NobodyCame)
    ));
    assert_eq!(
        downs.load(Ordering::SeqCst),
        1,
        "a failed link must never leave the radio up"
    );
    assert!(
        starter.poll().is_none(),
        "the worker said how it ended; there is nothing after that"
    );
}

/// The other direction of the same rule, and the one with no second chance: a link that
/// *worked* needs the radio, because the session about to start runs over it.
#[test]
fn a_link_that_comes_up_leaves_the_radio_up() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let downs = Arc::new(AtomicUsize::new(0));
    let count = downs.clone();
    let peer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        TcpLink::join("127.0.0.1", port)
    });
    let mut starter = LinkStarter::spawn_with(
        Box::new(|_| Ok(())),
        Box::new(move || {
            count.fetch_add(1, Ordering::SeqCst);
        }),
        LinkRole::Host,
        port,
        // Takes the port it was handed rather than one of its own, so this also proves the
        // worker passes the port through to the socket step.
        Box::new(|port, cancel: &Cancel| {
            TcpLink::host_until("127.0.0.1", port, Duration::from_secs(10), cancel)
        }),
    );
    let outcome = drain(&mut starter);
    assert!(
        matches!(outcome, LinkProgress::Ready(_)),
        "the peer arrived inside the bound, so this is a link"
    );
    let _joiner = peer.join().unwrap().expect("joiner connected");
    assert_eq!(
        downs.load(Ordering::SeqCst),
        0,
        "tearing the radio down on success kills the session it was brought up for"
    );
    assert!(
        starter.poll().is_none(),
        "a worker that has handed over its link must not then invent a failure"
    );
}

#[test]
fn the_steps_are_reported_in_order_before_the_outcome() {
    // The screen says "bringing the radio up" then "waiting for a friend"; a worker that
    // only reports the outcome leaves 30 s of blank screen.
    let mut starter = LinkStarter::spawn_with(
        Box::new(|_| Ok(())),
        Box::new(|| {}),
        LinkRole::Host,
        0,
        Box::new(|_, _| Err(io::Error::new(io::ErrorKind::TimedOut, "nobody"))),
    );
    let steps = drain_steps(&mut starter);
    assert_eq!(steps, vec![LinkStep::Radio, LinkStep::Waiting]);
}

#[test]
fn cancelling_reports_cancelled_rather_than_a_timeout() {
    let mut starter = LinkStarter::spawn_with(
        Box::new(|_| Ok(())),
        Box::new(|| {}),
        LinkRole::Host,
        0,
        Box::new(|_, cancel: &Cancel| {
            while !cancel.is_cancelled() {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"))
        }),
    );
    starter.cancel();
    let outcome = drain(&mut starter);
    assert!(
        matches!(outcome, LinkProgress::Failed(LinkFail::Cancelled)),
        "a player who backed out is not a player nobody joined"
    );
}

/// The third kind. `host_until` reserves `Interrupted` and `TimedOut` for its two ways out
/// and hands anything else back as itself, so everything else is a real fault on the wire.
#[test]
fn a_socket_fault_that_is_neither_a_deadline_nor_a_cancel_blames_the_wire() {
    let mut starter = LinkStarter::spawn_with(
        Box::new(|_| Ok(())),
        Box::new(|| {}),
        LinkRole::Join,
        0,
        Box::new(|_, _| Err(io::Error::new(io::ErrorKind::ConnectionRefused, "refused"))),
    );
    assert!(matches!(
        drain(&mut starter),
        LinkProgress::Failed(LinkFail::PeerVanished)
    ));
}

/// A worker that dies without saying how it ended must not read as a worker still working:
/// `poll` would return `None` forever and the screen would wait for a friend until the
/// player held the power button. The panic this prints to stderr is the point of the test.
#[test]
fn a_worker_that_dies_is_reported_rather_than_polled_forever() {
    let mut starter = LinkStarter::spawn_with(
        Box::new(|_| panic!("the radio call blew up")),
        Box::new(|| {}),
        LinkRole::Host,
        0,
        Box::new(|_, _| Err(io::Error::other("must not be reached"))),
    );
    assert!(matches!(
        drain(&mut starter),
        LinkProgress::Failed(LinkFail::PeerVanished)
    ));
}

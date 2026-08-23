//! Starting a link session, off the UI thread.
//!
//! Two of the steps are slow enough to be felt: `link_radio::up` blocks for one to five
//! seconds, and `TcpLink::host_until` for up to thirty. Run either on the frame loop and the
//! device is frozen — including the cancel button, which is the one control that matters
//! while a host is waiting for a friend who is not coming. This runs both on a thread of
//! their own and turns them into messages the frame loop picks up in microseconds.
//!
//! It reports the steps as they happen, not just the outcome, for the same reason: a screen
//! that says nothing for thirty seconds and then says "nobody came" is indistinguishable
//! from a crash.
//!
//! Nothing is shared with `App`. `link_radio`'s `up`/`down` are free functions that spawn a
//! process and hold no state, so the worker calls them directly and borrows nothing.

use std::sync::mpsc::{channel, Receiver, TryRecvError};

use crate::link_net::{Cancel, TcpLink, HOST_BOUND};
use crate::link_radio::{self, LinkRole};

/// Where the host lives on the private WiFi. `link_net` deliberately refuses to know this —
/// which handheld is `10.42.0.1` is a fact about the product, not about a TCP transport — so
/// the layer that wires the transport to a session is the one that says it.
#[cfg(feature = "device")]
pub const HOST_ADDR: &str = "10.42.0.1";

/// There is no private WiFi on a host build and `link_radio::up` brings nothing up there, so
/// the host address is the loopback one: two copies of slot on one machine is a real way to
/// drive this screen. Same cfg split as `link_radio`, for the same reason.
#[cfg(not(feature = "device"))]
pub const HOST_ADDR: &str = "127.0.0.1";

/// The port a link session meets on. Fixed rather than negotiated: there is no discovery
/// protocol on this network and nothing to negotiate over. Chosen below the ephemeral range
/// so an outgoing connection on either device can never already be holding it.
pub const LINK_PORT: u16 = 7211;

/// Which slow step the worker is on. The screen says a different sentence for each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkStep {
    /// Bringing the private network up. One to five seconds.
    Radio,
    /// The socket step: a host waiting for a friend, a joiner connecting out.
    Waiting,
}

/// Why a link did not start.
///
/// Four sentences rather than one, deliberately: "the link failed" does not tell a player
/// whether to try again, to move closer, or to ask their friend to press something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkFail {
    /// The network never came up. Nothing to do with the other player.
    Radio,
    /// The host waited out its bound and nobody arrived.
    NobodyCame,
    /// A real fault on the wire — or a worker that died without saying how it ended.
    PeerVanished,
    /// The player backed out. Not a failure, but it ends the same way.
    Cancelled,
}

/// One message from the worker. `At` may arrive more than once; exactly one `Ready` or
/// `Failed` ever does, and it is the last thing the worker says.
pub enum LinkProgress {
    At(LinkStep),
    Ready(TcpLink),
    Failed(LinkFail),
}

/// The three error kinds `host_until` is careful to tell apart, turned into the three
/// sentences above. It reserves `Interrupted` for a cancel and `TimedOut` for its deadline
/// and hands anything else back as itself, so anything else is a fault on the wire.
fn classify(e: &std::io::Error) -> LinkFail {
    match e.kind() {
        std::io::ErrorKind::Interrupted => LinkFail::Cancelled,
        std::io::ErrorKind::TimedOut => LinkFail::NobodyCame,
        _ => LinkFail::PeerVanished,
    }
}

/// Bring the private network up. Injectable so tests never shell out to `ags-net`.
type RadioUp = Box<dyn FnMut(LinkRole) -> std::io::Result<()> + Send>;
/// Take it back down. Infallible, like the real one: a teardown that can fail is a teardown
/// callers skip.
type RadioDown = Box<dyn FnMut() + Send>;
/// The socket step, given the port and the flag that ends it early.
type Socket = Box<dyn FnMut(u16, &Cancel) -> std::io::Result<TcpLink> + Send>;

/// A link session being started. Poll it once a frame; cancel it whenever.
pub struct LinkStarter {
    rx: Receiver<LinkProgress>,
    cancel: Cancel,
    /// Set the moment a terminal message is handed out. Without it the very next poll would
    /// see the worker's sender drop and invent a second, contradictory outcome — a failure
    /// reported over a link that had just come up.
    done: bool,
}

impl LinkStarter {
    /// The real thing: `link_radio` for the network, `TcpLink` for the socket.
    pub fn spawn(role: LinkRole, port: u16) -> LinkStarter {
        LinkStarter::spawn_with(
            Box::new(link_radio::up),
            Box::new(link_radio::down),
            role,
            port,
            Box::new(move |port, cancel| match role {
                // Bounded and cancellable; the joiner's `connect` needs neither, because it
                // fails in milliseconds against a host that is not there.
                LinkRole::Host => TcpLink::host_until(HOST_ADDR, port, HOST_BOUND, cancel),
                LinkRole::Join => TcpLink::join(HOST_ADDR, port),
            }),
        )
    }

    /// The same worker with its slow parts injectable, so a test can drive every path
    /// without a network interface anywhere near it.
    pub fn spawn_with(
        mut radio_up: RadioUp,
        mut radio_down: RadioDown,
        role: LinkRole,
        port: u16,
        mut socket: Socket,
    ) -> LinkStarter {
        let (tx, rx) = channel();
        let cancel = Cancel::new();
        let flag = cancel.clone();
        std::thread::spawn(move || {
            // Every send is `let _ =`. A receiver dropped mid-flight means the screen that
            // asked for this is already gone, which is not this thread's problem to report —
            // but finishing the teardown still is.
            let _ = tx.send(LinkProgress::At(LinkStep::Radio));
            if radio_up(role).is_err() {
                // `up` failing is no promise that nothing came up: `ags-net link` can get an
                // interface as far as configured and still exit non-zero.
                radio_down();
                let _ = tx.send(LinkProgress::Failed(LinkFail::Radio));
                return;
            }
            let _ = tx.send(LinkProgress::At(LinkStep::Waiting));
            match socket(port, &flag) {
                // No teardown here, and that is the point of the whole module: the session
                // this just handed over runs over that network.
                Ok(link) => {
                    let _ = tx.send(LinkProgress::Ready(link));
                }
                Err(e) => {
                    // Down before the message, on this path and the one above it. The
                    // message is what unblocks whoever is watching, so anything after it can
                    // be observed as not having happened — and a failed link that leaves the
                    // access point running strands the device on a network with nothing on
                    // the other end of it.
                    radio_down();
                    let _ = tx.send(LinkProgress::Failed(classify(&e)));
                }
            }
        });
        LinkStarter {
            rx,
            cancel,
            done: false,
        }
    }

    /// `None` means still working. This is a queue poll, not a syscall, so the frame loop
    /// can afford it every frame.
    pub fn poll(&mut self) -> Option<LinkProgress> {
        if self.done {
            return None;
        }
        match self.rx.try_recv() {
            Ok(progress) => {
                self.done = matches!(progress, LinkProgress::Ready(_) | LinkProgress::Failed(_));
                Some(progress)
            }
            Err(TryRecvError::Empty) => None,
            // The worker is gone without having said how it ended, which means it panicked.
            // Saying so is the difference between a screen that reports a fault and one that
            // waits for a friend until the player holds the power button.
            Err(TryRecvError::Disconnected) => {
                self.done = true;
                Some(LinkProgress::Failed(LinkFail::PeerVanished))
            }
        }
    }

    /// Ask the worker to give up. A waiting host notices within 50 ms and reports
    /// `Cancelled` rather than a timeout: a player who backed out is not a player nobody
    /// joined.
    pub fn cancel(&mut self) {
        self.cancel.cancel();
    }
}

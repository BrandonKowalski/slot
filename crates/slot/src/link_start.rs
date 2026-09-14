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
use crate::link_radio::{self, LinkRole, RadioFail};

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

/// The port a link session meets on. Not negotiated: there is no discovery protocol on this
/// network and nothing to negotiate over, so both ends have to arrive at the same number
/// independently. Chosen below the ephemeral range so an outgoing connection on either device
/// can never already be holding it.
pub const DEFAULT_LINK_PORT: u16 = 7211;

/// Overrides it. Two devices only ever meet on the default; this exists because off-device
/// `HOST_ADDR` is loopback, so two copies of slot on one machine differ in nothing but the
/// port — and without a way to move one of them, a live link cannot be driven anywhere but on
/// hardware.
const PORT_ENV: &str = "SLOT_LINK_PORT";

/// Both ends read this, so anything that makes them disagree makes the link fail as "nobody
/// arrived" — the one failure that reads as the other player's fault. A value that will not
/// serve is therefore refused out loud and the default used, rather than quietly halving the
/// pair.
pub fn link_port() -> u16 {
    let Some(raw) = std::env::var_os(PORT_ENV) else {
        return DEFAULT_LINK_PORT;
    };
    // `export SLOT_LINK_PORT=` is how a shell clears one, so an empty value is an unset value
    // and not worth a complaint.
    if raw.to_str().is_some_and(|s| s.trim().is_empty()) {
        return DEFAULT_LINK_PORT;
    }
    match raw.to_str().and_then(|s| s.trim().parse::<u16>().ok()) {
        // 0 asks the OS for whatever is free. Everywhere else that is the useful answer; here
        // it is exactly wrong, because the joiner has to name the port from its own side and
        // cannot be told one the host only learns after binding.
        None | Some(0) => {
            eprintln!(
                "slot: {PORT_ENV}={:?} is not a port a link can meet on, using {DEFAULT_LINK_PORT}",
                raw.to_string_lossy()
            );
            DEFAULT_LINK_PORT
        }
        Some(port) => port,
    }
}

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

impl LinkStep {
    /// Every step, in the order the worker reports them, which is also the order their faces
    /// are uploaded in.
    pub const ALL: [LinkStep; 2] = [LinkStep::Radio, LinkStep::Waiting];

    /// Position in `ALL`, so a step is a face without a lookup.
    pub fn index(self) -> usize {
        self as usize
    }

    /// What the screen says while the worker is here. Thirty seconds of a screen saying
    /// nothing and then saying "nobody came" is indistinguishable from a crash, which is the
    /// whole reason the worker reports steps rather than only outcomes.
    ///
    /// One sentence covers both ends of the socket step. A host is waiting for its friend
    /// and a joiner is reaching for its host; which of those two the machine is doing is not
    /// a thing the player has to be told.
    pub fn line(self) -> &'static str {
        match self {
            LinkStep::Radio => "Bringing the radio up",
            LinkStep::Waiting => "Looking for the other player",
        }
    }

    /// Which step's sentence to show for this one, given whether the driver is already loaded.
    ///
    /// Only the first step moves, and only when the radio is warm. The link screen asks for a
    /// `RadioJob::Warm` the moment it opens, and that takes about 1.1 s — the load
    /// `ags-net link host|join` would otherwise pay for inside this step. A player who spent
    /// longer than that choosing a role never waits for it, so the screen stops narrating a
    /// wait that has already happened and says what the step is now doing. The work underneath
    /// is unchanged: a host still spends about 1.6 s bringing its access point up, and looking
    /// for the other player is true for every second of it.
    ///
    /// `warm` must be a warm that finished, not one that was asked for — see
    /// `RadioJobs::warmed`, which is where that promise is kept.
    ///
    /// Mapped onto the other step rather than given a third of its own, so `ALL` stays every
    /// sentence this screen can say and the faces are still uploaded once at boot.
    pub fn shown(self, warm: bool) -> LinkStep {
        match self {
            LinkStep::Radio if warm => LinkStep::Waiting,
            step => step,
        }
    }
}

impl LinkFail {
    /// The failures that reach the screen, in the order their faces are uploaded.
    ///
    /// `Cancelled` is not one of them: a player who backed out is put straight back in their
    /// game, not shown a panel telling them what they just did on purpose.
    pub const SHOWN: [LinkFail; 3] = [
        LinkFail::Radio,
        LinkFail::NobodyCame,
        LinkFail::PeerVanished,
    ];

    /// Which of `SHOWN` this is, and `None` for the one that is never drawn.
    pub fn shown(self) -> Option<usize> {
        LinkFail::SHOWN.iter().position(|f| *f == self)
    }

    /// One sentence each, never a single generic "link failed": the three say whether to try
    /// again, to move closer, or to ask the other player to press something. `Cancelled` has
    /// a line only because this is total over the enum — it is never drawn, since the
    /// overlay closes on it.
    pub fn line(self) -> &'static str {
        match self {
            LinkFail::Radio => "The radio did not come up",
            LinkFail::NobodyCame => "Nobody arrived",
            LinkFail::PeerVanished => "The other player vanished",
            LinkFail::Cancelled => "Cancelled",
        }
    }
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

/// Bring the private network up. Injectable so tests never shell out to `ags-net`. It takes
/// the cancel flag because the real one is a child process that can be killed: a joiner's
/// search runs for half a minute, and a player who has backed out should not sit through it.
type RadioUp = Box<dyn FnMut(LinkRole, &Cancel) -> Result<(), RadioFail> + Send>;
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
                // Both bounded, both cancellable, and on the same bound: whichever player
                // presses first is the one that waits, and neither should give up while the
                // other is still there.
                LinkRole::Host => TcpLink::host_until(HOST_ADDR, port, HOST_BOUND, cancel),
                LinkRole::Join => TcpLink::join_until(HOST_ADDR, port, HOST_BOUND, cancel),
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
            if let Err(e) = radio_up(role, &flag) {
                // Three outcomes, and only one of them is the radio. A joiner that searched
                // its whole window without finding a host is reported as nobody arriving,
                // which is what happened and what the other player can act on; a kill is the
                // player's own cancel coming back.
                let fail = match &e {
                    RadioFail::NoHost => LinkFail::NobodyCame,
                    RadioFail::Cancelled => LinkFail::Cancelled,
                    RadioFail::Radio(why) => {
                        eprintln!("slot: link: {role:?} could not bring the radio up: {why}");
                        LinkFail::Radio
                    }
                };
                // `up` failing is no promise that nothing came up: `ags-net link` can get an
                // interface as far as configured and still exit non-zero.
                radio_down();
                let _ = tx.send(LinkProgress::Failed(fail));
                return;
            }
            let _ = tx.send(LinkProgress::At(LinkStep::Waiting));
            // Which end, where, and on what port — printed before the attempt rather than
            // after it, so a hang shows the address it is hanging on. The two ends must agree
            // on this port and nothing reconciles them if they do not, so it is the first
            // thing worth being able to compare between two logs.
            eprintln!("slot: link: {role:?} using {HOST_ADDR}:{port}");
            match socket(port, &flag) {
                // No teardown here, and that is the point of the whole module: the session
                // this just handed over runs over that network.
                Ok(link) => {
                    // Handing the link over is what transfers the radio with it, so the send
                    // failing means there is nobody to transfer it to: the player left between
                    // the socket coming up and this line, and the receiver went with them. A
                    // joiner reaches here even after a cancel, because `TcpLink::join` is a
                    // plain `connect` that never looks at the flag. Nothing else owns the
                    // network at that point, so this thread is the one that has to put it back.
                    eprintln!("slot: link: {role:?} connected on {HOST_ADDR}:{port}");
                    if tx.send(LinkProgress::Ready(link)).is_err() {
                        eprintln!("slot: link: nobody left to hand it to, radio back down");
                        radio_down();
                    }
                }
                Err(e) => {
                    // `LinkFail` has three words for every way this can go wrong, and the
                    // screen only has room for those three. The kind is what separates
                    // "nothing was listening" from "it went away mid-handshake", and both
                    // arrive on screen as PeerVanished.
                    eprintln!(
                        "slot: link: {role:?} failed on {HOST_ADDR}:{port}: {e} (kind {:?})",
                        e.kind()
                    );
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

#[cfg(test)]
mod port_tests {
    use super::*;

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn unset_means_the_number_both_devices_already_agree_on() {
        let _g = lock();
        std::env::remove_var(PORT_ENV);
        assert_eq!(link_port(), DEFAULT_LINK_PORT);
    }

    #[test]
    fn a_port_in_the_environment_wins() {
        let _g = lock();
        std::env::set_var(PORT_ENV, "7300");
        assert_eq!(link_port(), 7300);
        std::env::remove_var(PORT_ENV);
    }

    /// Surrounding whitespace is what a shell export picks up by accident, and it is not a
    /// reason to send the two ends to different ports.
    #[test]
    fn a_padded_port_is_still_a_port() {
        let _g = lock();
        std::env::set_var(PORT_ENV, "  7300 ");
        assert_eq!(link_port(), 7300);
        std::env::remove_var(PORT_ENV);
    }

    /// Falling back rather than failing: the pair still meets, just not where it was asked to.
    #[test]
    fn a_value_that_is_not_a_port_falls_back() {
        let _g = lock();
        for bad in ["banana", "-1", "70000", "7300x"] {
            std::env::set_var(PORT_ENV, bad);
            assert_eq!(
                link_port(),
                DEFAULT_LINK_PORT,
                "{bad:?} should not be taken"
            );
        }
        std::env::remove_var(PORT_ENV);
    }

    /// 0 parses and is a legal u16, so it slips past a plain parse check. It cannot serve
    /// here: the host would bind whatever is free and the joiner has no way to learn it.
    /// The way a shell clears a variable, which is a request for the default rather than a
    /// mistake worth printing about.
    #[test]
    fn an_empty_value_is_an_unset_value() {
        let _g = lock();
        std::env::set_var(PORT_ENV, "   ");
        assert_eq!(link_port(), DEFAULT_LINK_PORT);
        std::env::remove_var(PORT_ENV);
    }

    #[test]
    fn zero_is_refused_even_though_it_parses() {
        let _g = lock();
        std::env::set_var(PORT_ENV, "0");
        assert_eq!(link_port(), DEFAULT_LINK_PORT);
        std::env::remove_var(PORT_ENV);
    }
}

#[cfg(test)]
mod step_tests {
    use super::*;

    /// The sentence is about a wait, so it goes when the wait does. With the driver already
    /// loaded the step runs `ags-net link host|join` with nothing to load, and what it is doing
    /// from its first frame is looking for the other player.
    #[test]
    fn a_warm_radio_captions_the_first_step_as_the_search() {
        assert_eq!(LinkStep::Radio.shown(true), LinkStep::Waiting);
        assert_eq!(
            LinkStep::Radio.shown(true).line(),
            "Looking for the other player"
        );
    }

    /// Cold, the load is still ahead of the player and the screen still says so: about 1.1 s of
    /// silence is what the step reports exist to prevent.
    #[test]
    fn a_cold_radio_still_says_it_is_bringing_the_radio_up() {
        assert_eq!(LinkStep::Radio.shown(false), LinkStep::Radio);
        assert_eq!(LinkStep::Radio.shown(false).line(), "Bringing the radio up");
    }

    /// The socket step never waited on the driver, so nothing about it changes either way.
    #[test]
    fn the_socket_step_says_the_same_thing_in_both_states() {
        assert_eq!(LinkStep::Waiting.shown(true), LinkStep::Waiting);
        assert_eq!(LinkStep::Waiting.shown(false), LinkStep::Waiting);
    }

    /// Both sentences stay in `ALL`, which is what the faces are built from: the warm screen
    /// borrows the other step's face rather than needing one of its own.
    #[test]
    fn every_sentence_a_step_can_show_is_still_one_of_the_faces() {
        for warm in [true, false] {
            for step in LinkStep::ALL {
                assert!(
                    LinkStep::ALL.contains(&step.shown(warm)),
                    "{step:?} at warm={warm} shows a sentence with no face uploaded for it"
                );
            }
        }
    }
}

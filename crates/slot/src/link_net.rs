//! The TCP transport for a link session: two handhelds on the OS's private WiFi, `10.42.0.1`
//! and `10.42.0.2`, ~2 ms round trip. This module owns exactly one job — carry a core's serial
//! packets over that link intact and promptly — and nothing else. Wiring it to `Link`, the
//! queue the core side actually touches, is the next layer's job, not this one's — which is
//! also why `host` takes the address to bind rather than assuming it: knowing the device is
//! always `10.42.0.1` on that network is a fact about the product, not about a TCP transport,
//! and belongs to whoever wires this to a real session.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use slot_retro::LinkChannel;

/// A shared "stop waiting" flag. Cloned to whoever might press cancel; checked by whoever is
/// blocking. Separate from a deadline because the two failures need different words on
/// screen: a deadline means nobody arrived, a cancel means the player changed their mind.
#[derive(Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Cancel {
        Cancel::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// How often a waiting host looks up from the socket to ask whether it should still be
/// waiting. Well under a frame's worth of perceived lag on a cancel, and costs nothing
/// meaningful spread over a 30 s wait.
const POLL_MS: u64 = 50;

/// What a host waits for a friend before deciding nobody is coming.
pub const HOST_BOUND: Duration = Duration::from_secs(30);

/// One TCP connection carrying a core's serial traffic.
///
/// Reads run on their own thread into a queue, so `try_recv` is a queue poll rather than a
/// syscall: the emulator thread calls it every frame and cannot afford to block. Writes run
/// on a second thread of their own, for the same reason in the other direction: `send` only
/// ever queues the packet onto a channel, which never blocks the caller regardless of what
/// the peer is doing, and the writer thread is what actually makes the blocking `write_all`
/// syscall. A peer that stops reading blocks that thread, not the emulator's — the frame
/// budget is what this exists to protect, not the writer thread's own backlog, which a
/// permanently stalled peer would grow without bound. That corner is deliberately not closed
/// in this wave, the same way `Link`'s own inbound queue is not: a peer stalled forever is
/// the same failure `TcpLink::Drop` (via the session that notices and ends it) is the actual
/// answer to, not a bound on how much can pile up first.
pub struct TcpLink {
    /// The writer thread's queue. `send` never touches the socket itself.
    outbox: Sender<Vec<u8>>,
    inbox: Receiver<Vec<u8>>,
    /// Kept so `Drop` can shut the socket down directly, and so a test can ask what was
    /// actually set on it (`nodelay`) — the reader and writer threads each hold their own
    /// `try_clone` of the same socket, a dup at the OS level, so a shutdown through this
    /// handle reaches both of theirs too.
    stream: TcpStream,
    /// Set by the reader thread the moment a read on the socket fails — the peer is gone, not
    /// merely quiet. `try_recv` alone cannot tell the two apart: both look like `None` forever.
    closed: Arc<AtomicBool>,
}

impl TcpLink {
    /// Wait for the other handheld, but not forever and not uninterruptibly.
    ///
    /// `accept()` cannot be cancelled, so the listener goes non-blocking and the wait becomes
    /// a poll: every 50 ms, ask whether a peer has arrived, whether the player has given up,
    /// and whether the bound has passed. The three outcomes get three different error kinds
    /// because the screen above says a different sentence for each — `Interrupted` is the
    /// player pressing B, `TimedOut` is nobody coming, and anything else is a real socket
    /// fault worth saying so about.
    ///
    /// Binds to `addr` specifically rather than `0.0.0.0`: a listener open on every
    /// interface is reachable from anything on the user's home network, not just the private
    /// WiFi a link session actually runs over, with no handshake and no ROM check to turn
    /// away whatever finds it — see the module doc for why this transport does not hardcode
    /// which address that is.
    pub fn host_until(
        addr: &str,
        port: u16,
        bound: Duration,
        cancel: &Cancel,
    ) -> std::io::Result<TcpLink> {
        let listener = TcpListener::bind((addr, port))?;
        listener.set_nonblocking(true)?;
        let deadline = Instant::now() + bound;
        loop {
            if cancel.is_cancelled() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "cancelled while waiting for a peer",
                ));
            }
            match listener.accept() {
                Ok((stream, _peer)) => {
                    // Back to blocking before `wrap` sees it. The mode rides along through
                    // `try_clone` (a dup: one shared file description), and `read_exact`
                    // hands WouldBlock straight back to a caller that treats any error as
                    // "peer gone" — so a stream left non-blocking kills the host's reader
                    // thread on its first read and the host never hears its peer again.
                    // `a_bounded_host_still_accepts_a_peer_that_does_arrive` is what holds
                    // this line in place; it sends in both directions for exactly this.
                    stream.set_nonblocking(false)?;
                    return TcpLink::wrap(stream);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e),
            }
            if Instant::now() >= deadline {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "no peer arrived",
                ));
            }
            std::thread::sleep(Duration::from_millis(POLL_MS));
        }
    }

    /// The unbounded-looking spelling, kept for the tests that spawn their own peer. It is
    /// bounded now too: there is no caller anywhere that genuinely wants to wait forever.
    pub fn host(addr: &str, port: u16) -> std::io::Result<TcpLink> {
        TcpLink::host_until(addr, port, HOST_BOUND, &Cancel::new())
    }

    /// Reach a host, retrying until it is there, the player gives up, or the bound passes.
    ///
    /// A single `connect` is what this used to be, justified by "it fails in milliseconds
    /// against a host that is not there". That is true and it is the problem: it fails in
    /// milliseconds against a host that is not there *yet*. The host has to finish bringing
    /// its radio up — one to five seconds on device — and then bind, all after its player
    /// pressed a button that the joiner's player cannot see. A joiner with one attempt only
    /// works if the two presses happen in the right order, close together, and it reports
    /// the failure as `ConnectionRefused`, which the screen shows as the peer vanishing —
    /// blaming the other player for being early.
    ///
    /// Retrying makes the order of the two presses stop mattering, which is the whole of it.
    /// The same `bound` as the host's wait, deliberately: a joiner that gave up while the
    /// host was still listening would be a pair that never meets for no reason either player
    /// could see.
    pub fn join_until(
        addr: &str,
        port: u16,
        bound: Duration,
        cancel: &Cancel,
    ) -> std::io::Result<TcpLink> {
        let deadline = Instant::now() + bound;
        loop {
            if cancel.is_cancelled() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "cancelled while reaching for the host",
                ));
            }
            // Every error is worth another try, which is why none of them is inspected: the
            // host may not have bound yet (`ConnectionRefused`), its interface may not be
            // configured yet (`NetworkUnreachable`, and on some systems a plain `Other`), or
            // the address may not be assigned yet (`AddrNotAvailable`). None of those tells
            // "not yet" from "never" at the moment it happens. The deadline below is what
            // turns that difference into an answer.
            if let Ok(stream) = TcpStream::connect((addr, port)) {
                return TcpLink::wrap(stream);
            }
            if Instant::now() >= deadline {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "never reached the host",
                ));
            }
            std::thread::sleep(Duration::from_millis(POLL_MS));
        }
    }

    /// Connect to a host that is already waiting, once. Kept for tests that stand their own
    /// listener up first and have nothing to wait for.
    pub fn join(addr: &str, port: u16) -> std::io::Result<TcpLink> {
        TcpLink::wrap(TcpStream::connect((addr, port))?)
    }

    fn wrap(stream: TcpStream) -> std::io::Result<TcpLink> {
        // Serial traffic is small and latency sensitive: Nagle would hold a packet back
        // waiting for company it will not get.
        stream.set_nodelay(true)?;
        let mut reader = stream.try_clone()?;
        let mut writer = stream.try_clone()?;
        let (rtx, inbox) = channel();
        let (wtx, wrx) = channel::<Vec<u8>>();
        let closed = Arc::new(AtomicBool::new(false));
        let reader_closed = closed.clone();

        std::thread::spawn(move || {
            let mut header = [0u8; 2];
            loop {
                if reader.read_exact(&mut header).is_err() {
                    // The peer is gone. Said out loud now, so the session can end the link
                    // instead of starving on it.
                    reader_closed.store(true, Ordering::Release);
                    return;
                }
                let len = u16::from_be_bytes(header) as usize;
                let mut buf = vec![0u8; len];
                if len > 0 && reader.read_exact(&mut buf).is_err() {
                    reader_closed.store(true, Ordering::Release);
                    return;
                }
                if rtx.send(buf).is_err() {
                    return; // our own end hung up
                }
            }
        });

        std::thread::spawn(move || {
            // Ends when `wrx.iter()` sees the channel disconnect, which happens the moment
            // `TcpLink::outbox` (the `Sender` half) drops — i.e. when the `TcpLink` itself
            // does. A `write_all` blocked on a stalled peer at that moment is unblocked by
            // that same drop's `shutdown(Both)` on a clone of this socket, exactly like the
            // reader thread's blocked `read_exact` is.
            for buf in wrx.iter() {
                // Framed here rather than in `send`: the boundary is TCP's problem to solve,
                // not the caller's, and a packet longer than u16 cannot come from GBA serial
                // hardware, so refusing one is better than truncating it.
                let Ok(len) = u16::try_from(buf.len()) else {
                    continue;
                };
                if writer.write_all(&len.to_be_bytes()).is_err() {
                    return;
                }
                if writer.write_all(&buf).is_err() {
                    return;
                }
            }
        });

        Ok(TcpLink {
            outbox: wtx,
            inbox,
            stream,
            closed,
        })
    }

    /// Whether Nagle's algorithm is disabled on the wrapped socket. Nothing in this crate
    /// reads it outside `link_session.rs`'s own coverage of `wrap`'s `set_nodelay` call —
    /// mutation testing proved that call had no test watching it at all before this existed.
    pub fn nodelay(&self) -> std::io::Result<bool> {
        self.stream.nodelay()
    }
}

impl Drop for TcpLink {
    /// Shutting the socket down does two jobs at once, and both are why this exists, not
    /// just the second one: it makes the reader thread's blocked `read_exact` — and a writer
    /// thread mid-`write_all` against a stalled peer — return an error so each thread
    /// actually exits instead of leaking for the life of the process, and it sends the peer
    /// a FIN so their side sees a real close rather than a link that has simply gone quiet —
    /// which, without this, is indistinguishable from a player who is still thinking. A
    /// `shutdown` error here almost always means the peer tore the connection down first,
    /// which is the ordinary case, not a fault, so it's ignored.
    fn drop(&mut self) {
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

impl LinkChannel for TcpLink {
    fn send(&mut self, _flags: i32, buf: &[u8]) {
        // Queued for the writer thread, never written here: this is called from inside the
        // worker's own frame, and a peer that stops reading must not be able to block it —
        // proven by `send_never_blocks_on_a_peer_that_stopped_reading` in `link_session.rs`,
        // which the old direct `write_all` here could hang on indefinitely. `Sender::send`
        // on this unbounded channel never blocks its caller, whatever the writer thread is
        // doing.
        let _ = self.outbox.send(buf.to_vec());
    }

    fn try_recv(&mut self) -> Option<Vec<u8>> {
        match self.inbox.try_recv() {
            Ok(p) => Some(p),
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => None,
        }
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

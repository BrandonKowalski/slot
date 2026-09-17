use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// Where a core's serial traffic goes: the link cable and wireless adapter packets that
/// gpSP hands the frontend through libretro's netpacket interface. The real transport in
/// `slot` implements this over the OS's private WiFi link; `LoopbackLink` implements it in
/// memory, so everything above this line can be exercised with no network, no peer and no
/// device.
///
/// `Send` because the core runs on the emulator thread: `send` and `try_recv` are called
/// from there, not from wherever the transport itself does its I/O.
pub trait LinkChannel: Send {
    /// `flags` is libretro's own — `NETPACKET_RELIABLE`, `_UNSEQUENCED`, `_FLUSH_HINT` — so a
    /// transport that cannot honour one (e.g. no unreliable channel available) should fall
    /// back to reliable delivery rather than drop silently.
    fn send(&mut self, flags: i32, buf: &[u8]);

    /// Must never block: this is called once a frame, and a blocking read would eat straight
    /// into the 16 ms budget. `None` means nothing has arrived yet, never an error — a peer
    /// that has gone quiet is indistinguishable from one that is still thinking, and neither
    /// is a reason to stop the frame.
    fn try_recv(&mut self) -> Option<Vec<u8>>;

    /// Whether the other end is known to have gone. Only a transport that can tell a closed
    /// wire from a quiet one says yes; the default is a transport that never closes by itself.
    fn is_closed(&self) -> bool {
        false
    }

    /// Tell the far end this session is over, before the wire goes.
    ///
    /// A transport with a control channel of its own puts a word on it and waits, briefly, for
    /// it to actually leave; the default is a transport with nothing to say it with, for which
    /// dropping the wire is the only message there is. Called on the emulator thread with the
    /// drop immediately behind it, so an implementation must be bounded — never "until the peer
    /// answers", which is a peer that stopped reading holding a teardown open forever.
    fn send_end(&mut self) {}

    /// Whether the far end said it was ending the session, as opposed to merely vanishing.
    ///
    /// The two are a different sentence on screen and a different speed: a peer that said so
    /// ends the session now, a peer that went quiet ends it once the broken badge has been
    /// seen. The default is a transport with no control channel, which can only ever be the
    /// second — which is why the timeout stays underneath this rather than being replaced by
    /// it.
    fn peer_ended(&self) -> bool {
        false
    }
}

/// Hands back whatever was put in, in the order it was sent. No network, no peer, no
/// device: every layer built on `LinkChannel` can be driven against this instead.
#[derive(Default)]
pub struct LoopbackLink {
    queue: VecDeque<Vec<u8>>,
}

impl LinkChannel for LoopbackLink {
    fn send(&mut self, _flags: i32, buf: &[u8]) {
        self.queue.push_back(buf.to_vec());
    }

    fn try_recv(&mut self) -> Option<Vec<u8>> {
        self.queue.pop_front()
    }
}

/// The core's end of a link session, shared with whoever drives the transport, the same way
/// `Rumble` is shared with whoever drives the motor. Unlike rumble this is bidirectional and
/// needs more than a store/load pair: packets have to queue in both directions without being
/// dropped, and something has to be able to ask whether a session is live at all.
///
/// A `Mutex<VecDeque<_>>` per direction rather than something lock-free: this is touched once
/// a frame, from `pump_link`, and once per packet from a trampoline the core calls — nowhere
/// near the once-per-sample rate that makes rumble's motors worth keeping lock-free. The
/// simplicity of a mutex is worth more here than the throughput a lock-free queue would buy.
#[derive(Clone, Default)]
pub struct Link(Arc<LinkState>);

#[derive(Default)]
struct LinkState {
    /// Packets that arrived from the peer, waiting to reach the core.
    inbound: Mutex<VecDeque<Vec<u8>>>,
    /// Packets the core produced, waiting to reach the peer.
    outbound: Mutex<VecDeque<Vec<u8>>>,
    /// Whether a session is actually live, as opposed to a core merely having registered the
    /// netpacket interface. Whoever starts and stops the session is what sets this.
    active: AtomicBool,
}

/// Takes one of the two queues, taking it back from a panic rather than passing that panic on.
///
/// A poisoned lock means whoever last held it panicked between taking it and dropping it. The
/// queue is still a queue: no guard ever leaves this module, so the only code that can run
/// while one is held is a `push_back`, a `pop_front` or a `clear` over whole packets that were
/// built before the lock was taken, and none of those can unwind halfway through and leave a
/// deque that is no longer a deque. What is at stake is therefore at most one packet either
/// pushed or not, against a running game that would otherwise be taken down by a fault in a
/// link it does not even need — the same trade `Frames::lock` and the audio ring's `lock` are
/// written on.
fn lock(queue: &Mutex<VecDeque<Vec<u8>>>) -> MutexGuard<'_, VecDeque<Vec<u8>>> {
    queue.lock().unwrap_or_else(|e| e.into_inner())
}

impl Link {
    /// A packet that arrived from the peer. The transport calls this; `pump_link` and the
    /// core's `poll_receive` trampoline are what drain it back out.
    pub fn push_inbound(&self, packet: Vec<u8>) {
        lock(&self.0.inbound).push_back(packet);
    }

    /// Pop the next packet waiting for the core, in the order it arrived.
    pub fn take_inbound(&self) -> Option<Vec<u8>> {
        lock(&self.0.inbound).pop_front()
    }

    /// The core handed this to the `send` trampoline. The transport is what actually puts it
    /// on the wire.
    pub fn push_outbound(&self, packet: Vec<u8>) {
        lock(&self.0.outbound).push_back(packet);
    }

    /// Pop the next packet the core produced, in the order it was sent.
    pub fn take_outbound(&self) -> Option<Vec<u8>> {
        lock(&self.0.outbound).pop_front()
    }

    /// Whether a session is actually live right now. `Acquire`, paired with `set_active`'s
    /// `Release`: a caller who observes this flip to `false` is guaranteed to also see
    /// whatever the writer did *before* that store — which `Cmd::EndLink` (`slot`'s `emu.rs`)
    /// relies on by calling `clear` first and flipping the flag second, so a reader never has
    /// to bridge the gap between the two with a sleep of its own (see
    /// `ending_a_link_clears_stale_packets_for_the_next_session` in `emu.rs`).
    pub fn is_active(&self) -> bool {
        self.0.active.load(Ordering::Acquire)
    }

    /// Mark the session live or ended. Ending it does not clear either queue itself — see
    /// `clear` for that — so a transport that is winding down may still flush what is left
    /// before its caller gets around to calling it. `Release`, so that whatever a caller did
    /// before this call (`clear`, in `Cmd::EndLink`'s case) is visible to anyone who observes
    /// the flip through `is_active`'s matching `Acquire` load.
    pub fn set_active(&self, active: bool) {
        self.0.active.store(active, Ordering::Release);
    }

    /// Empties both queues. A packet that arrived — or was produced — before a session ended
    /// must not be sitting here waiting for the next one: `Cmd::EndLink` (`slot`'s `emu.rs`)
    /// is the only caller, right after it marks the session inactive, so a stale packet from
    /// session one is never mistaken for traffic belonging to session two.
    ///
    /// The strongest case for `lock` taking a poisoned queue back rather than refusing it is
    /// here: this call exists to restore an invariant, so declining to empty a queue because
    /// the last holder panicked would leave behind exactly the stale packets it is called to
    /// remove — and hand them to the next session.
    pub fn clear(&self) {
        lock(&self.0.inbound).clear();
        lock(&self.0.outbound).clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `crates/slot-retro/tests/netpacket.rs` covers what `Link` is for — packets in and out, in
    // order, across clones. These cover what happens to it when a thread holding one of its
    // queues dies, which no caller outside this crate can arrange: a guard never leaves this
    // module, so poisoning a queue means reaching the `Mutex` itself, and that is only
    // reachable from inside. Same reason `libretro.rs` keeps its ABI tests in the file.

    /// Poisons one queue the way a thread driving the transport dying mid packet would: take
    /// the lock, then panic while still holding it. The panic is on a thread of its own so the
    /// unwind is contained, and the queue is checked to be genuinely poisoned afterwards —
    /// without that check every test below would still pass against a plain `unwrap`.
    fn poison(link: &Link, queue: fn(&LinkState) -> &Mutex<VecDeque<Vec<u8>>>) {
        let state = link.0.clone();
        std::thread::spawn(move || {
            let _held = queue(&state)
                .lock()
                .expect("the queue was poisoned already");
            panic!("a thread driving the transport died mid packet");
        })
        .join()
        .expect_err("the poisoning thread was supposed to panic");
        assert!(
            queue(&link.0).lock().is_err(),
            "the queue was not left poisoned, so this proves nothing"
        );
    }

    /// The one that matters: a link fault must cost the link, not the game. `pump_link` runs on
    /// the emulator thread every frame, so an `unwrap` on a poisoned queue ends *that* thread —
    /// which on the device is the running game disappearing because a packet queue went wrong.
    #[test]
    fn a_dead_transport_does_not_take_the_emulator_thread_with_it() {
        let link = Link::default();
        link.set_active(true);
        link.push_inbound(b"arrived before the fault".to_vec());
        poison(&link, |s| &s.inbound);
        poison(&link, |s| &s.outbound);

        // What one frame of `pump_link`/`flush_outbound` does, on a thread of its own so the
        // test can see whether that thread is still alive at the end of it.
        let worker = link.clone();
        let frame = std::thread::spawn(move || {
            let got = worker.take_inbound();
            worker.push_outbound(b"this frame's traffic".to_vec());
            (got, worker.take_outbound())
        })
        .join();

        let (got, sent) = frame.expect("the emulator thread died with the transport");
        assert_eq!(
            got.as_deref(),
            Some(&b"arrived before the fault"[..]),
            "a packet queued before the fault is still the core's to read"
        );
        assert_eq!(
            sent.as_deref(),
            Some(&b"this frame's traffic"[..]),
            "the core must still be able to send after the fault"
        );
    }

    #[test]
    fn a_poisoned_inbound_queue_keeps_carrying_packets_to_the_core() {
        let link = Link::default();
        poison(&link, |s| &s.inbound);

        link.push_inbound(b"first".to_vec());
        link.push_inbound(b"second".to_vec());

        assert_eq!(link.take_inbound().as_deref(), Some(&b"first"[..]));
        assert_eq!(
            link.take_inbound().as_deref(),
            Some(&b"second"[..]),
            "order is not something a poisoned lock can disturb"
        );
        assert_eq!(link.take_inbound(), None);
    }

    #[test]
    fn a_poisoned_outbound_queue_keeps_carrying_packets_to_the_wire() {
        let link = Link::default();
        poison(&link, |s| &s.outbound);

        link.push_outbound(b"first".to_vec());
        link.push_outbound(b"second".to_vec());

        assert_eq!(link.take_outbound().as_deref(), Some(&b"first"[..]));
        assert_eq!(link.take_outbound().as_deref(), Some(&b"second"[..]));
        assert_eq!(link.take_outbound(), None);
    }

    /// `clear` is the one call that must go through a poisoned lock: refusing would leave the
    /// stale packets it exists to remove waiting for the next session to pick up.
    #[test]
    fn ending_a_session_still_empties_both_poisoned_queues() {
        let link = Link::default();
        link.push_inbound(b"stale".to_vec());
        link.push_outbound(b"stale".to_vec());
        poison(&link, |s| &s.inbound);
        poison(&link, |s| &s.outbound);

        link.clear();

        assert_eq!(link.take_inbound(), None, "session one's inbound survived");
        assert_eq!(
            link.take_outbound(),
            None,
            "session one's outbound survived"
        );
    }
}

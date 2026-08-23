use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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

impl Link {
    /// A packet that arrived from the peer. The transport calls this; `pump_link` and the
    /// core's `poll_receive` trampoline are what drain it back out.
    pub fn push_inbound(&self, packet: Vec<u8>) {
        self.0.inbound.lock().unwrap().push_back(packet);
    }

    /// Pop the next packet waiting for the core, in the order it arrived.
    pub fn take_inbound(&self) -> Option<Vec<u8>> {
        self.0.inbound.lock().unwrap().pop_front()
    }

    /// The core handed this to the `send` trampoline. The transport is what actually puts it
    /// on the wire.
    pub fn push_outbound(&self, packet: Vec<u8>) {
        self.0.outbound.lock().unwrap().push_back(packet);
    }

    /// Pop the next packet the core produced, in the order it was sent.
    pub fn take_outbound(&self) -> Option<Vec<u8>> {
        self.0.outbound.lock().unwrap().pop_front()
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
    pub fn clear(&self) {
        self.0.inbound.lock().unwrap().clear();
        self.0.outbound.lock().unwrap().clear();
    }
}

use std::collections::VecDeque;

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

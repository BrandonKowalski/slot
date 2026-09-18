//! The emulated cable's frame clock: whose buttons go into which frame, and when a frame may run.
//!
//! The netpacket route in `emu.rs` lets the core emit packets and posts them as they appear. The
//! in-core lockstep route cannot: `run_frame_linked` needs *both* players' buttons before it can
//! step either console, and a frame that steps on a guess cannot be taken back.
//!
//! So input is delayed. A mask sampled now is stamped for a frame `DELAY` ahead and sent at once,
//! which buys the wire that many frames to deliver it. Both ends seed the first `DELAY` frames as
//! idle by convention rather than sending them, so neither waits on the other to start.
//!
//! Transport-free on purpose: `Cable` never touches a socket, so the whole protocol is testable
//! without one.

use std::collections::BTreeMap;

use slot_retro::ButtonMask;

/// Frames between sampling a mask and running it. Two at 60 Hz is about 33 ms of wire, against a
/// link measured at about 2 ms, so it absorbs a stalled present rather than only a slow packet.
pub const DELAY: u64 = 2;

/// One player's buttons for one frame: frame index then mask, little endian.
const PACKET: usize = 10;

pub fn encode(frame: u64, mask: ButtonMask) -> [u8; PACKET] {
    let mut out = [0u8; PACKET];
    out[..8].copy_from_slice(&frame.to_le_bytes());
    out[8..].copy_from_slice(&mask.0.to_le_bytes());
    out
}

pub fn decode(buf: &[u8]) -> Option<(u64, ButtonMask)> {
    if buf.len() != PACKET {
        return None;
    }
    let frame = u64::from_le_bytes(buf[..8].try_into().ok()?);
    let mask = u16::from_le_bytes(buf[8..].try_into().ok()?);
    Some((frame, ButtonMask(mask)))
}

pub struct Cable {
    /// 0 or 1, which is libretro's client id and also which port this device drives.
    player: u8,
    /// The frame `step` will run next.
    frame: u64,
    local: BTreeMap<u64, ButtonMask>,
    remote: BTreeMap<u64, ButtonMask>,
    /// Frames asked for and not run because the peer's mask had not arrived.
    stalled: u32,
}

impl Cable {
    pub fn new(player: u8) -> Self {
        let idle = ButtonMask::default();
        let seed: BTreeMap<u64, ButtonMask> = (0..DELAY).map(|f| (f, idle)).collect();
        Cable {
            player,
            frame: 0,
            local: seed.clone(),
            remote: seed,
            stalled: 0,
        }
    }

    pub fn player(&self) -> u8 {
        self.player
    }

    /// The frame about to run. Read by tests holding the delay to a constant.
    pub fn frame(&self) -> u64 {
        self.frame
    }

    pub fn stalled(&self) -> u32 {
        self.stalled
    }

    /// This device's buttons for the frame `DELAY` ahead of the one about to run.
    ///
    /// Decided once and never revised. Asking twice for the same frame, which is what a stalled
    /// present does, returns the mask already decided rather than replacing it. Two reasons, and
    /// the second is the serious one: stamping a new frame per present would add a frame of input
    /// delay for every stall, without bound and worst on whichever device stalls more; and
    /// revising a mask the peer may already have run that frame with would desync the pair
    /// outright.
    pub fn sample(&mut self, mask: ButtonMask) -> [u8; PACKET] {
        let frame = self.frame + DELAY;
        let mask = *self.local.entry(frame).or_insert(mask);
        encode(frame, mask)
    }

    /// A packet off the wire. Unparseable or already-known frames are dropped rather than raised:
    /// a duplicate is not an error and a short read is not worth ending a session over.
    pub fn accept(&mut self, buf: &[u8]) -> bool {
        let Some((frame, mask)) = decode(buf) else {
            return false;
        };
        if frame < self.frame {
            return false;
        }
        self.remote.insert(frame, mask);
        true
    }

    /// The two masks for the frame about to run, in port order, or `None` while the peer's has
    /// not arrived. Port order rather than local-first: the core drives port 0 with the first,
    /// so handing them over the wrong way round swaps the two consoles.
    pub fn ready(&self) -> Option<(ButtonMask, ButtonMask)> {
        let mine = *self.local.get(&self.frame)?;
        let theirs = *self.remote.get(&self.frame)?;
        match self.player {
            0 => Some((mine, theirs)),
            _ => Some((theirs, mine)),
        }
    }

    /// Consumes the frame `ready` answered for, dropping what neither side needs again.
    pub fn advance(&mut self) {
        self.local.remove(&self.frame);
        self.remote.remove(&self.frame);
        self.frame += 1;
        self.stalled = 0;
    }

    /// The frame could not run: the peer is late. Counted so a caller can tell a hiccup from a
    /// peer that has stopped talking altogether.
    pub fn stall(&mut self) {
        self.stalled = self.stalled.saturating_add(1);
    }
}

/// Stalled presents before a peer is called lost rather than slow. About a second at 60 Hz, which
/// is long enough to ride out a WiFi hiccup and short enough to notice a device that walked away.
pub const QUIET_FRAMES: u32 = 60;

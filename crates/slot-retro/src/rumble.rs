use std::ffi::c_uint;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;

use crate::ffi::{RUMBLE_STRONG, RUMBLE_WEAK};

/// The core's end of the vibration motor, shared with whoever drives the hardware. The core
/// writes it from the emulator thread, so setting it is an atomic store and nothing else: a
/// motor write that waited on a device would cost the frame it happened in.
#[derive(Clone, Default)]
pub struct Rumble(Arc<Motors>);

#[derive(Default)]
struct Motors {
    strong: AtomicU16,
    weak: AtomicU16,
}

impl Rumble {
    /// The body of libretro's `set_rumble_state`, refusals and all, so the callback above it
    /// is one line and everything that can be got wrong is reachable without a core.
    pub fn set(&self, port: c_uint, effect: c_uint, strength: u16) -> bool {
        // One pad, soldered in.
        if port != 0 {
            return false;
        }
        let motor = match effect {
            RUMBLE_STRONG => &self.0.strong,
            RUMBLE_WEAK => &self.0.weak,
            _ => return false,
        };
        motor.store(strength, Ordering::Relaxed);
        true
    }

    /// One motor for the two effects, so it takes the louder and stopping either one leaves
    /// the other running.
    pub fn strength(&self) -> u16 {
        self.0
            .strong
            .load(Ordering::Relaxed)
            .max(self.0.weak.load(Ordering::Relaxed))
    }
}

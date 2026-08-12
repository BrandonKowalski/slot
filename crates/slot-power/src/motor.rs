use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;

/// What the vibration motor was last set to. A shared handle rather than a field because the
/// platform it belongs to is boxed into `Power` and never comes back out, and on a host with
/// no motor the recorded value is the whole of the implementation.
#[derive(Clone, Default)]
pub struct Motor(Arc<AtomicU16>);

impl Motor {
    pub fn set(&self, strength: u16) {
        self.0.store(strength, Ordering::Relaxed);
    }

    pub fn last(&self) -> u16 {
        self.0.load(Ordering::Relaxed)
    }
}

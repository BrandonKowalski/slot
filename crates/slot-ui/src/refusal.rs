use crate::hud::Millis;

/// Long enough to read as a movement, short enough to be over before it looks like
/// something the player has to dismiss.
const REFUSAL_MS: Millis = 300;

const SHAKE_PX: f32 = 6.0;
const SHAKE_HZ: f32 = 14.0;

/// The whole of the error UI, per spec section 12: no words, just a shake. One curve for
/// every refusal, so a cart that will not seat and an action that will not happen are
/// answered the same way. What moves is the caller's: a cart on its way back out, or the
/// whole presented image when there is nothing else on screen to flinch.
#[derive(Copy, Clone)]
pub struct Refusal {
    started: Millis,
}

impl Refusal {
    pub fn started(now: Millis) -> Self {
        Refusal { started: now }
    }

    pub fn active(&self, now: Millis) -> bool {
        now.saturating_sub(self.started) < REFUSAL_MS
    }

    /// Horizontal pixels off centre, decaying to nothing. Cosine rather than sine, so the
    /// first frame is already at full throw: a flinch is a knock and everything after it is
    /// settling. Starting at zero would make the frame the action was refused on the one
    /// frame that did not move.
    pub fn offset(&self, now: Millis) -> f32 {
        let age = now.saturating_sub(self.started);
        if age >= REFUSAL_MS {
            return 0.0;
        }
        let decay = 1.0 - age as f32 / REFUSAL_MS as f32;
        let secs = age as f32 / 1000.0;
        (secs * SHAKE_HZ * std::f32::consts::TAU).cos() * SHAKE_PX * decay
    }
}

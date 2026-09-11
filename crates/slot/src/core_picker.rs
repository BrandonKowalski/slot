//! The core picker's own clock: how open the cart is, where the chip is, and what a press does
//! to either. No drawing and no card — `App` asks it what to show and what to write, and the
//! whole of its behaviour can be stated against a number of milliseconds.

use slot_store::Core;
use slot_ui::{ease, Millis, Refusal, CHIP_TIP};

/// The front half slides off the back, then lifts away; the close runs the same progress back,
/// quicker. `slot_ui::SLIDE_SHARE` is the slide's share of the open, held to these by a test.
pub const SLIDE_MS: Millis = 160;
pub const LIFT_MS: Millis = 260;
pub const OPEN_MS: Millis = SLIDE_MS + LIFT_MS;
pub const CLOSE_MS: Millis = 320;
pub const HOP_MS: Millis = 180;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Press {
    Left,
    Right,
    Keep,
    Back,
}

/// What a press asks of `App`, beyond the picker's own state.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Outcome {
    Nothing,
    Refused,
    Write(Core),
}

/// The chip's pose for one frame.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Chip {
    /// 0.0 over the mGBA socket, 1.0 over the gpSP socket.
    pub across: f32,
    /// 0.0 seated, 1.0 at the top of the hop.
    pub lift: f32,
    /// Radians, clockwise, leaning into the direction of travel.
    pub tip: f32,
    /// The socket it sits in, and so the name it wears. `None` in flight.
    pub seated: Option<Core>,
    /// Panel pixels the refusal puts on its x.
    pub shake: f32,
}

#[derive(Copy, Clone)]
struct Hop {
    from: Core,
    started: Millis,
}

#[derive(Copy, Clone)]
struct Close {
    started: Millis,
    /// How open the cart was when the close began.
    from: f32,
}

#[derive(Copy, Clone)]
pub struct CorePicker {
    seat: Core,
    /// When SELECT + START was pressed.
    opened: Millis,
    /// When the open's clock began, once the cart's faces were ready.
    started: Option<Millis>,
    hop: Option<Hop>,
    close: Option<Close>,
    /// The chip's own. The shelf does not shake while the picker is up.
    refusal: Option<Refusal>,
}

impl CorePicker {
    pub fn open(seat: Core, now: Millis) -> Self {
        CorePicker {
            seat,
            opened: now,
            started: None,
            hop: None,
            close: None,
            refusal: None,
        }
    }

    /// Starts the open's clock. The first start is the one that counts.
    pub fn start(&mut self, now: Millis) {
        self.started.get_or_insert(now);
    }

    /// Up, but not yet moving: the cart's faces are not on the GPU yet.
    pub fn waiting(&self) -> bool {
        self.started.is_none()
    }

    /// How long ago SELECT + START was pressed.
    pub fn waited(&self, now: Millis) -> Millis {
        now.saturating_sub(self.opened)
    }

    /// Where the chip is, or where it is going: the core `A` writes.
    pub fn seat(&self) -> Core {
        self.seat
    }

    pub fn closing(&self) -> bool {
        self.close.is_some()
    }

    /// 0.0 is the cart standing on the shelf, 1.0 open at rest. Linear progress through both
    /// beats, which `slot_ui::slide_of` and `lift_of` ease on their own shares.
    pub fn openness(&self, now: Millis) -> f32 {
        match (self.close, self.started) {
            // Reversed from wherever the open had got to, at the close's speed.
            (Some(Close { started, from }), _) => {
                (from - now.saturating_sub(started) as f32 / CLOSE_MS as f32).max(0.0)
            }
            (None, Some(started)) => (now.saturating_sub(started) as f32 / OPEN_MS as f32).min(1.0),
            (None, None) => 0.0,
        }
    }

    /// The close has run out, and `App` can let the picker go.
    pub fn finished(&self, now: Millis) -> bool {
        self.close.is_some() && self.openness(now) <= 0.0
    }

    /// What a press does to the chip or the lid, and what it asks of `App`. While the cart still
    /// waits on its faces the arrows do nothing: the chip is not on screen, and a hop run unseen
    /// would open the cart with the chip already in the other socket, for an `A` to write a core
    /// the player never saw chosen. `A` and `B` still close it.
    pub fn press(&mut self, press: Press, now: Millis) -> Outcome {
        if self.close.is_some() {
            return Outcome::Nothing;
        }
        let target = match press {
            Press::Keep => {
                self.begin_close(now);
                return Outcome::Write(self.seat);
            }
            Press::Back => {
                self.begin_close(now);
                return Outcome::Nothing;
            }
            Press::Left => Core::Mgba,
            Press::Right => Core::Gpsp,
        };
        if self.waiting() {
            return Outcome::Nothing;
        }
        if let Some(progress) = self.hop_progress(now) {
            if target == self.seat {
                return Outcome::Nothing;
            }
            // Back toward the socket it is leaving: the new hop starts as far through as the
            // old one had left to go, which puts the chip exactly where it already was.
            let done = ((1.0 - progress) * HOP_MS as f32) as Millis;
            self.hop = Some(Hop {
                from: self.seat,
                started: now.saturating_sub(done),
            });
            self.seat = target;
            // A hop is happening now, so a shake left over from an earlier refusal is not
            // this chip's any more.
            self.refusal = None;
            return Outcome::Nothing;
        }
        if target == self.seat {
            self.refusal = Some(Refusal::started(now));
            return Outcome::Refused;
        }
        self.hop = Some(Hop {
            from: self.seat,
            started: now,
        });
        self.seat = target;
        // Same rule as the turn above: a hop starting now has nothing to do with whatever was
        // refused before it, however recently.
        self.refusal = None;
        Outcome::Nothing
    }

    pub fn chip(&self, now: Millis) -> Chip {
        let shake = self.refusal.map_or(0.0, |r| r.offset(now));
        match (self.hop, self.hop_progress(now)) {
            (Some(hop), Some(q)) => {
                let rightward = hop.from == Core::Mgba;
                let lean = if rightward { 1.0 } else { -1.0 };
                let arc = (std::f32::consts::PI * q).sin();
                Chip {
                    across: if rightward { ease(q) } else { 1.0 - ease(q) },
                    lift: arc,
                    tip: lean * CHIP_TIP * arc,
                    seated: None,
                    shake,
                }
            }
            _ => Chip {
                across: self.seat.index() as f32,
                lift: 0.0,
                tip: 0.0,
                seated: Some(self.seat),
                shake,
            },
        }
    }

    fn begin_close(&mut self, now: Millis) {
        self.close = Some(Close {
            started: now,
            from: self.openness(now),
        });
    }

    /// How far through a hop in flight, or `None` once the chip is seated.
    fn hop_progress(&self, now: Millis) -> Option<f32> {
        let hop = self.hop?;
        let q = now.saturating_sub(hop.started) as f32 / HOP_MS as f32;
        (q < 1.0).then_some(q)
    }
}

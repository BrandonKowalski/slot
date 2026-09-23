use crate::{Btn, Millis, RawEvent};

/// How long a *held* SELECT may still arm a chord with a second key. Generous on purpose:
/// 120 ms was not enough to land the second key of a chord.
///
/// This no longer withholds anything. It used to be how long SELECT waited before conceding it
/// was a plain press, which meant a held SELECT arrived at the game 600 ms late whether or not
/// a chord ever followed. The press goes straight through now (see `select_down`), so this is the arming window and nothing else.
pub const SELECT_CHORD_MS: Millis = 600;

/// The least time SELECT stays down on the pad, measured from the press. The core reads the
/// mask once a frame, so a press and its release drained in the same batch would be set and
/// cleared before it ever looked; a tap shorter than three frames is held on until the tick
/// can let go of it.
pub const SELECT_TAP_MS: Millis = 50;
pub const MENU_TAP_MS: Millis = 250;
pub const MENU_DOUBLE_TAP_MS: Millis = 350;
pub const MENU_HOLD_MS: Millis = 1000;
pub const FF_DOUBLE_TAP_MS: Millis = 250;
/// How far apart the two volume keys may go down and still read as one gesture. Short,
/// because neither key is deferred waiting for it: the pair is recognised behind the presses
/// it is made of, not in front of them.
pub const MUTE_CHORD_MS: Millis = 200;
/// Well short of the PMIC's own six second cutoff (`pmu_powkey_off_time` in the device
/// tree), so slot always gets to power off gracefully before the hardware cuts the rails.
pub const POWER_HOLD_MS: Millis = 1000;

/// How long a volume key is held before the level starts running, and how fast it runs after
/// that. The press itself is the first step; this is the wait before the second, long enough
/// that a tap is never two steps and short enough that a hold does not feel stuck.
pub const VOLUME_REPEAT_DELAY_MS: Millis = 400;
pub const VOLUME_REPEAT_MS: Millis = 120;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Action {
    GbaDown(Btn),
    GbaUp(Btn),
    ShelfLeft,
    ShelfRight,
    Insert,
    Eject,
    Polaroids,
    SaveState,
    LoadState,
    RewindStart,
    RewindStop,
    FfStart,
    FfStop,
    BrightnessUp,
    BrightnessDown,
    BlueLightUp,
    BlueLightDown,
    VolumeUp,
    VolumeDown,
    /// The quick menu, off a tap of MENU. The button's other two gestures both need a
    /// game under them, so on the shelf a tap of it meant nothing at all.
    QuickMenu,
    /// The in-game menu, off SELECT+MENU. Emitted on every screen, exactly as `QuickMenu`
    /// and `Polaroids` are: this file is blind to which screen is up, and the app is what
    /// decides where a gesture lands.
    ///
    /// Not START, which in a game is the GBA's own and the game needs it; not a bare MENU
    /// tap, which the quick menu already has.
    GameMenu,
    MuteToggle,
    /// TEMPORARY. SELECT+Y, for judging colour correction in a game, where the settings row
    /// cannot reach. Y never reaches the core, so the chord costs the game nothing. Remove with
    /// its `chord` entry and the branch in `App::adjust`.
    ColourCorrectionToggle,
    /// The press itself. Nothing visible hangs off it — it exists so the save state is
    /// flushed before a hold can reach the PMIC's own cutoff, which takes the rails away
    /// whatever the software wanted.
    PowerPress,
    /// A short press, delivered on release. Locking on the release rather than the press is
    /// what lets a press become a hold without dozing on the way through.
    PowerTap,
    /// The hold threshold, while the button is still down. Arms the shutdown and puts it on
    /// screen; it is `PowerOff` that commits.
    PowerHold,
    /// Released after a hold. The graceful shutdown starts here, so the screen `PowerHold`
    /// raised is on the panel for as long as the button is held.
    PowerOff,
    LidClose,
    LidOpen,
}

#[derive(Copy, Clone, Default)]
enum Select {
    #[default]
    Idle,
    /// Physically down, and already handed to the core. `since` is when, which is what the
    /// chord window is measured from; `chorded` says a chord has already fired under this same
    /// hold, which keeps the window open for the rest of it.
    Held { since: Millis, chorded: bool },
    /// Physically up, with the release still owed. Held back only until `due`, so a tap short
    /// enough to be drained whole inside one batch is still on the pad when the core looks.
    ReleaseDue(Millis),
}

#[derive(Default)]
pub struct Gestures {
    select: Select,
    /// Buttons swallowed by a chord, so their release is swallowed too.
    chord_held: u8,
    menu_down_at: Option<Millis>,
    menu_last_tap: Option<Millis>,
    menu_eject_fired: bool,
    power_down_at: Option<Millis>,
    power_hold_fired: bool,
    vol_up_at: Option<Millis>,
    vol_down_at: Option<Millis>,
    /// When the ramp last emitted, which is `None` until it starts. Cleared by both edges, so
    /// every press begins its own ramp rather than inheriting the pace of the last one.
    vol_up_ramp: Option<Millis>,
    vol_down_ramp: Option<Millis>,
    /// The pair has already fired. Cleared only once both keys are up, so a key tapped again
    /// under a held one is not a second chord.
    mute_fired: bool,
    ff_on: bool,
    ff_latched: bool,
    /// The press that established the latch, whose release must not clear it.
    ff_latching_press: bool,
    /// A press `ff_down` turned away because L2 had the time. Its release is not the release of
    /// anything, and must not be remembered as one: see `ff_up`.
    r2_refused: bool,
    r2_last_release: Option<Millis>,
    rewinding: bool,
}

/// Whether a held key owes a step at `now`. The wait before the first is longer than the gap
/// between the rest, so a slow tap never lands as two.
fn ramp_due(down: Option<Millis>, last: Option<Millis>, now: Millis) -> bool {
    let Some(down) = down else {
        return false;
    };
    if now.saturating_sub(down) < VOLUME_REPEAT_DELAY_MS {
        return false;
    }
    last.is_none_or(|l| now.saturating_sub(l) >= VOLUME_REPEAT_MS)
}

impl Gestures {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether fast forward is latched rather than held. Not an action: the latch is
    /// established by a release that emits nothing, and only this machine knows it happened.
    pub fn ff_latched(&self) -> bool {
        self.ff_latched
    }

    /// Lets go of a latched fast forward from outside the machine. The latch is the one thing
    /// this file holds with no finger on it — every other hold ends when its own button does —
    /// so it is the only one that can outlive the thing it was applied to. This file is blind
    /// to screens on purpose, so whoever knows the slot is empty is what calls this.
    ///
    /// A *held* fast forward is deliberately left alone: a thumb still on R2 through an eject
    /// is a thumb still asking for it, exactly as a held direction is, and it ends the moment
    /// the thumb does.
    pub fn drop_ff_latch(&mut self) -> Vec<Action> {
        if !self.ff_latched {
            return Vec::new();
        }
        self.ff_clear()
    }

    pub fn feed(&mut self, ev: RawEvent, now: Millis) -> Vec<Action> {
        match ev {
            RawEvent::Down(b) => self.down(b, now),
            RawEvent::Up(b) => self.up(b, now),
        }
    }

    pub fn tick(&mut self, now: Millis) -> Vec<Action> {
        let mut out = Vec::new();
        // The only thing SELECT still owes a tick is the release of a tap too short to have
        // been polled. The window closing is no longer an event: it withholds nothing, so
        // there is nothing for its expiry to hand over — `chording` reads the clock itself.
        if let Select::ReleaseDue(due) = self.select {
            if now >= due {
                self.select = Select::Idle;
                out.push(Action::GbaUp(Btn::Select));
            }
        }
        if let Some(d) = self.menu_down_at {
            if !self.menu_eject_fired && now.saturating_sub(d) >= MENU_HOLD_MS {
                self.menu_eject_fired = true;
                out.push(Action::Eject);
            }
        }
        if let Some(d) = self.power_down_at {
            if !self.power_hold_fired && now.saturating_sub(d) >= POWER_HOLD_MS {
                self.power_hold_fired = true;
                out.push(Action::PowerHold);
            }
        }
        // Held volume runs the level. Not while the pair has fired: that press was a mute,
        // and ramping under it would move the level the mute just remembered.
        if !self.mute_fired {
            if ramp_due(self.vol_up_at, self.vol_up_ramp, now) {
                self.vol_up_ramp = Some(now);
                out.push(Action::VolumeUp);
            }
            if ramp_due(self.vol_down_at, self.vol_down_ramp, now) {
                self.vol_down_ramp = Some(now);
                out.push(Action::VolumeDown);
            }
        }
        out
    }

    fn down(&mut self, b: Btn, now: Millis) -> Vec<Action> {
        match b {
            Btn::Select => self.select_down(now),
            Btn::Menu => self.menu_down(now),
            // The flush hangs off the press, because a POWER that is being held may be cut
            // by the PMIC before there is any release to see. Everything the user can
            // observe waits for the release.
            Btn::Power => {
                self.power_down_at = Some(now);
                self.power_hold_fired = false;
                vec![Action::PowerPress]
            }
            Btn::Lid => vec![Action::LidClose],
            Btn::VolUp | Btn::VolDown => self.volume_press(b, now),
            Btn::L2 => self.rewind_start(),
            Btn::R2 => self.ff_down(now),
            _ => {
                if let (true, Some((bit, action))) = (self.chording(now), chord(b)) {
                    self.mark_chorded();
                    self.chord_held |= bit;
                    return vec![action];
                }
                vec![Action::GbaDown(b)]
            }
        }
    }

    fn up(&mut self, b: Btn, now: Millis) -> Vec<Action> {
        match b {
            Btn::Select => self.select_up(now),
            Btn::Menu => self.menu_up(now),
            Btn::Power => self.power_up(),
            Btn::Lid => vec![Action::LidOpen],
            Btn::VolUp | Btn::VolDown => self.volume_release(b),
            Btn::L2 => self.rewind_stop(),
            Btn::R2 => self.ff_up(now),
            _ => {
                if let Some((bit, _)) = chord(b) {
                    if self.chord_held & bit != 0 {
                        self.chord_held &= !bit;
                        return Vec::new();
                    }
                }
                vec![Action::GbaUp(b)]
            }
        }
    }

    /// Whether a key landing at `now` is the second half of a chord rather than the game's own
    /// press. True while SELECT is physically down and either the window is still open, or a
    /// chord has already fired under this same hold.
    ///
    /// The second half is what lets a held SELECT ramp the brightness: without it the window
    /// would expire between the second nudge of Up and the third, and the rest of them would
    /// reach the game as directions. Asked of the clock rather than answered by a state the
    /// tick has to move, so a batch drained after a stall reads the window it really landed in.
    fn chording(&self, now: Millis) -> bool {
        match self.select {
            Select::Held { since, chorded } => {
                chorded || now.saturating_sub(since) < SELECT_CHORD_MS
            }
            _ => false,
        }
    }

    /// Remembers that a chord fired under the hold SELECT is in, so the rest of that hold keeps
    /// chording however long it lasts.
    fn mark_chorded(&mut self) {
        if let Select::Held { chorded, .. } = &mut self.select {
            *chorded = true;
        }
    }

    /// The press goes straight to the game, and the chord arms off the same hold behind it.
    ///
    /// SELECT used to be withheld for the whole of `SELECT_CHORD_MS` so that a chord could
    /// swallow it whole, which meant a *held* SELECT reached the game 600 ms late whether or
    /// not a chord ever followed. The latency was never the chord's, it was the waiting to find
    /// out.
    ///
    /// What it costs is that a chord now hands the game a SELECT press it did not mean to send.
    /// There is no way around that while the two share the button — the press is already out by
    /// the time the second key says what it was for, and taking it back would be a release the
    /// player never made, which is the exact shape of failure this area has been bitten by three
    /// times. The second key is still the chord's alone, on both of its edges.
    ///
    /// Plus whatever the press before it still owed: `select_up` can leave a release held back
    /// for `SELECT_TAP_MS`, and a press arriving inside that window overwrote the state owing
    /// it. That is a switch bouncing rather than anything a player does — 50 ms is three frames
    /// — but the release it lost left the core holding SELECT with no up ever coming, so the
    /// interrupting press hands it back itself, ahead of its own.
    fn select_down(&mut self, now: Millis) -> Vec<Action> {
        let mut out = Vec::new();
        if matches!(self.select, Select::ReleaseDue(_)) {
            out.push(Action::GbaUp(Btn::Select));
        }
        // A button the game is already holding must not be pressed at it a second time, or the
        // one release coming ends two presses and the count the pad reads stops balancing.
        let held = matches!(self.select, Select::Held { .. });
        self.select = Select::Held {
            since: now,
            chorded: false,
        };
        if !held {
            out.push(Action::GbaDown(Btn::Select));
        }
        out
    }

    /// The release always reaches the game, because the press always did. The one thing it
    /// waits for is a tap too short to have been polled: down and up drained in the same batch
    /// set and clear the mask before the core ever reads it, so the press would be invisible.
    /// Measured from the press rather than from the release — that is where the game got it —
    /// so a tap the core has certainly already seen ends on its own edge with nothing deferred.
    fn select_up(&mut self, now: Millis) -> Vec<Action> {
        let Select::Held { since, .. } = std::mem::take(&mut self.select) else {
            return Vec::new();
        };
        if now.saturating_sub(since) >= SELECT_TAP_MS {
            return vec![Action::GbaUp(Btn::Select)];
        }
        self.select = Select::ReleaseDue(since + SELECT_TAP_MS);
        Vec::new()
    }

    /// MENU is dispatched by name from `down`, ahead of the `_` arm that reads the `chord`
    /// table, so a row there would never be looked at. Its one chord lives here instead.
    ///
    /// Ahead of the double tap check, and the order is load-bearing: behind it, a SELECT+MENU
    /// that follows a recent tap opens the switcher rather than the menu.
    fn menu_down(&mut self, now: Millis) -> Vec<Action> {
        if self.chording(now) {
            // So the rest of this hold keeps chording rather than handing the game the next
            // key, exactly as the `chord` table's own arm does. SELECT itself is the game's
            // from the press either way, and its release is still owed.
            self.mark_chorded();
            // The chord is the whole gesture, and these two are what make it one. Clearing
            // the hold stops this press also arming an eject — and, because `menu_up` reads
            // that same field to decide the press ever happened, it is what makes the release
            // silent rather than a tap landing on top of the menu it just opened. Clearing
            // the tap stops the press standing as half of a double tap in either direction:
            // this one is not the first half of one, and it must not complete one either.
            self.menu_down_at = None;
            self.menu_last_tap = None;
            return vec![Action::GameMenu];
        }
        if let Some(tap) = self.menu_last_tap {
            if now.saturating_sub(tap) <= MENU_DOUBLE_TAP_MS {
                self.menu_last_tap = None;
                // A double tap acts on the second press, so that press cannot also arm an eject.
                self.menu_down_at = None;
                return vec![Action::Polaroids];
            }
        }
        self.menu_down_at = Some(now);
        self.menu_eject_fired = false;
        Vec::new()
    }

    fn menu_up(&mut self, now: Millis) -> Vec<Action> {
        // No armed press to release, because `menu_down` already spent this one: on a
        // double tap, or on the SELECT+MENU chord. Either way, letting go of it is not a
        // second gesture on top of what it already did.
        let Some(d) = self.menu_down_at.take() else {
            return Vec::new();
        };
        let ejected = self.menu_eject_fired;
        self.menu_eject_fired = false;
        let tapped = !ejected && now.saturating_sub(d) < MENU_TAP_MS;
        self.menu_last_tap = tapped.then_some(now);
        // On the release rather than after the double tap window closes: waiting would put a
        // third of a second between the press and the screen, to serve a gesture that means
        // nothing on the shelf anyway. A second tap inside the window still opens the
        // polaroids, and whoever gets both is on a screen where only one of them lands.
        match tapped {
            true => vec![Action::QuickMenu],
            false => Vec::new(),
        }
    }

    /// Which of the two the release means depends on whether the hold already fired. A
    /// press that never reached the threshold is a lock; one that did is a shutdown the
    /// user has been watching the screen for.
    fn power_up(&mut self) -> Vec<Action> {
        let held = self.power_hold_fired;
        self.power_down_at = None;
        self.power_hold_fired = false;
        vec![if held {
            Action::PowerOff
        } else {
            Action::PowerTap
        }]
    }

    /// The press always lands. Volume is held for repeat, so putting a chord window in front
    /// of every press would make the whole control feel slow to serve a gesture used once a
    /// session; the pair is recognised behind its own presses and whoever acts on it undoes
    /// them.
    fn volume_press(&mut self, b: Btn, now: Millis) -> Vec<Action> {
        let (mine, other, action) = match b {
            Btn::VolUp => (&mut self.vol_up_at, self.vol_down_at, Action::VolumeUp),
            _ => (&mut self.vol_down_at, self.vol_up_at, Action::VolumeDown),
        };
        *mine = Some(now);
        match b {
            Btn::VolUp => self.vol_up_ramp = None,
            _ => self.vol_down_ramp = None,
        }
        let mut out = vec![action];
        let paired = other.is_some_and(|t| now.abs_diff(t) <= MUTE_CHORD_MS);
        if paired && !self.mute_fired {
            self.mute_fired = true;
            out.push(Action::MuteToggle);
        }
        out
    }

    fn volume_release(&mut self, b: Btn) -> Vec<Action> {
        match b {
            Btn::VolUp => (self.vol_up_at, self.vol_up_ramp) = (None, None),
            _ => (self.vol_down_at, self.vol_down_ramp) = (None, None),
        }
        if self.vol_up_at.is_none() && self.vol_down_at.is_none() {
            self.mute_fired = false;
        }
        Vec::new()
    }

    fn rewind_start(&mut self) -> Vec<Action> {
        if self.rewinding {
            return Vec::new();
        }
        let mut out = Vec::new();
        out.extend(self.ff_clear());
        self.rewinding = true;
        out.push(Action::RewindStart);
        out
    }

    fn rewind_stop(&mut self) -> Vec<Action> {
        if !self.rewinding {
            return Vec::new();
        }
        self.rewinding = false;
        vec![Action::RewindStop]
    }

    fn ff_down(&mut self, now: Millis) -> Vec<Action> {
        if self.rewinding {
            // Turned away, and remembered as turned away. A press that produced nothing is not
            // half of anything either — see `ff_up`.
            self.r2_refused = true;
            return Vec::new();
        }
        if self.ff_latched {
            // Any further press is the one whose release clears the latch.
            self.ff_latching_press = false;
        } else {
            let double = self
                .r2_last_release
                .is_some_and(|rel| now.saturating_sub(rel) <= FF_DOUBLE_TAP_MS);
            self.ff_latched = double;
            self.ff_latching_press = double;
        }
        if self.ff_on {
            return Vec::new();
        }
        self.ff_on = true;
        vec![Action::FfStart]
    }

    fn ff_up(&mut self, now: Millis) -> Vec<Action> {
        // Letting go of a press L2 turned away. `ff_down` did nothing with it, so this does
        // nothing with it either — and above all it is not written down. Recorded as an
        // ordinary release, it stood as the first half of a double tap the player never made:
        // hold L2 to rewind, tap R2 and watch it be refused, let go of L2, then press R2 once
        // and fast forward *latches* rather than being held, and stays on after the finger
        // comes off. A refused press cannot be half of a gesture.
        if std::mem::take(&mut self.r2_refused) {
            return Vec::new();
        }
        self.r2_last_release = Some(now);
        if self.ff_latching_press {
            self.ff_latching_press = false;
            return Vec::new();
        }
        self.ff_clear()
    }

    fn ff_clear(&mut self) -> Vec<Action> {
        self.ff_latched = false;
        self.ff_latching_press = false;
        if !self.ff_on {
            return Vec::new();
        }
        self.ff_on = false;
        vec![Action::FfStop]
    }
}

fn chord(b: Btn) -> Option<(u8, Action)> {
    Some(match b {
        Btn::Up => (1, Action::BrightnessUp),
        Btn::Down => (2, Action::BrightnessDown),
        Btn::Left => (4, Action::BlueLightDown),
        Btn::Right => (8, Action::BlueLightUp),
        Btn::L1 => (16, Action::LoadState),
        Btn::R1 => (32, Action::SaveState),
        // TEMPORARY. See `Action::ColourCorrectionToggle`.
        Btn::Y => (64, Action::ColourCorrectionToggle),
        _ => return None,
    })
}

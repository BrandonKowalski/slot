use crate::{Btn, Millis, RawEvent};

/// How long a *held* SELECT waits for a second key before conceding it was a plain press.
/// Generous on purpose: the only reason to ever give up is a game that wants SELECT held,
/// and 120 ms was not enough to land the second key of a chord.
pub const SELECT_CHORD_MS: Millis = 600;

/// How long a delivered tap stays down. Press and release in one batch net out to nothing,
/// because the mask is set and cleared before the core reads it.
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
pub const POWER_HOLD_MS: Millis = 2000;

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
    /// The about screen, off a tap of MENU. The button's other two gestures both need a
    /// game under them, so on the shelf a tap of it meant nothing at all.
    OpenAbout,
    MuteToggle,
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

#[derive(Copy, Clone, Default, PartialEq, Eq)]
enum Select {
    #[default]
    Idle,
    /// Held, chord still undecided.
    Pending(Millis),
    /// A chord fired, so this press never reaches the core.
    Consumed,
    /// Passed to the core and still physically held.
    Delivered,
    /// Passed to the core after a release; the core is owed the up edge, but not in the
    /// same batch as the down.
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

    pub fn feed(&mut self, ev: RawEvent, now: Millis) -> Vec<Action> {
        match ev {
            RawEvent::Down(b) => self.down(b, now),
            RawEvent::Up(b) => self.up(b, now),
        }
    }

    pub fn tick(&mut self, now: Millis) -> Vec<Action> {
        let mut out = Vec::new();
        match self.select {
            Select::Pending(d) if now.saturating_sub(d) >= SELECT_CHORD_MS => {
                self.select = Select::Delivered;
                out.push(Action::GbaDown(Btn::Select));
            }
            Select::ReleaseDue(d) if now.saturating_sub(d) >= SELECT_TAP_MS => {
                self.select = Select::Idle;
                out.push(Action::GbaUp(Btn::Select));
            }
            _ => {}
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
            Btn::Select => {
                self.select = Select::Pending(now);
                Vec::new()
            }
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
                let chording = matches!(self.select, Select::Pending(_) | Select::Consumed);
                if let (true, Some((bit, action))) = (chording, chord(b)) {
                    self.select = Select::Consumed;
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

    fn select_up(&mut self, now: Millis) -> Vec<Action> {
        match std::mem::take(&mut self.select) {
            // The release settles it: no chord can follow, so the game gets the press now
            // rather than waiting out a window that can no longer produce one.
            Select::Pending(_) => {
                self.select = Select::ReleaseDue(now);
                vec![Action::GbaDown(Btn::Select)]
            }
            Select::Delivered => vec![Action::GbaUp(Btn::Select)],
            _ => Vec::new(),
        }
    }

    fn menu_down(&mut self, now: Millis) -> Vec<Action> {
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
            true => vec![Action::OpenAbout],
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
        _ => return None,
    })
}

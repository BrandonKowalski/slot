use std::path::PathBuf;

use slot_input::{Action, Gestures, Millis, RawEvent};
use slot_retro::Rumble;
use slot_ui::FfState;

use crate::app::{App, Phase};
use crate::audio::{open_sink, AudioSink, Ring, Sfx, GBA_HZ};
use crate::core::open_core;
use crate::emu::{CoreState, EmuHandle, Speed};
use crate::frames::FrameRef;
use crate::input::Pad;
use crate::persist;

/// Everything the frontend is that is not a window: the app, the core behind it, and the
/// gesture layer between the two. The binary owns the GL and hands raw events in.
pub struct Session {
    root: PathBuf,
    app: App,
    emu: Option<EmuHandle>,
    /// Opened once and outliving every cart. The cart clicks home while it is still on its
    /// way in, which is exactly when there is no core to own a sink.
    sink: Box<dyn AudioSink>,
    gestures: Gestures,
    pad: Pad,
    rewinding: bool,
    fast: bool,
    /// What the motor was last set to. On the device that setting is a write to hardware and
    /// the core asks for the same value most frames.
    motor: u16,
}

impl Session {
    pub fn boot(root: PathBuf) -> Self {
        let mut sink: Box<dyn AudioSink> = open_sink();
        // A frontend for one console knows the rate before it knows the cart. A device that
        // refuses it still opens, and the worker resamples to whatever it did take.
        if let Err(e) = sink.open(GBA_HZ) {
            eprintln!("slot: audio: {e}");
        }
        Session {
            app: App::boot(&root),
            root,
            emu: None,
            sink,
            gestures: Gestures::new(),
            pad: Pad::default(),
            rewinding: false,
            fast: false,
            motor: 0,
        }
    }

    /// Mixed in over whatever the game is already playing, so it lands with the thing on
    /// screen rather than a buffer behind it.
    pub fn play_sfx(&mut self, sfx: Sfx) {
        let ring = self.sink.ring();
        let rate = ring.sample_rate();
        if rate == 0 {
            return;
        }
        let mut samples = sfx.render(rate);
        // The core's audio is levelled by the worker on its way to the ring. A clip mixed in
        // here never passes that, so the same level has to be applied on this path or the
        // slot stays loud under a game turned all the way down.
        crate::audio::volume::apply(&mut samples, self.app.output_volume());
        ring.mix(&samples);
    }

    pub fn audio_queued(&self) -> usize {
        self.sink.ring().queued_frames()
    }

    /// What the device is about to play, for the tests that need to hear what was queued
    /// rather than only how much of it there is.
    pub fn audio_ring(&self) -> std::sync::Arc<Ring> {
        self.sink.ring()
    }

    /// Straight to the motor, skipping the phase. Only `sync_rumble` and a caller standing
    /// in for a cart that buzzes have any business here.
    pub fn rumble(&mut self, strength: u16) {
        if strength == self.motor {
            return;
        }
        self.motor = strength;
        self.app.set_rumble(strength);
    }

    /// The core's end of the motor, or nothing when the slot is empty.
    pub fn core_rumble(&self) -> Option<&Rumble> {
        self.emu.as_ref().map(EmuHandle::rumble)
    }

    pub fn app(&self) -> &App {
        &self.app
    }

    pub fn app_mut(&mut self) -> &mut App {
        &mut self.app
    }

    pub fn frame(&self) -> Option<FrameRef> {
        self.emu.as_ref().and_then(|e| e.latest_frame())
    }

    /// A cart in the slot is a core running, so this is also "is there a game layer".
    pub fn has_core(&self) -> bool {
        self.emu.is_some()
    }

    /// Whether the game layer may be drawn at all. Gate the draw on this, never on
    /// `has_core`: a handle exists before its worker has produced anything.
    /// Diagnostic: how many frames have been taken out of the handoff buffer. Only the
    /// render path may take one, so this must equal the number the renderer received.
    pub fn frame_ready(&self) -> bool {
        self.emu.as_ref().is_some_and(EmuHandle::frame_ready)
    }

    /// Frames the current core has produced. Zero while it is loaded but paused, which is
    /// what the insert relies on.
    pub fn frames_published(&self) -> u64 {
        self.emu.as_ref().map_or(0, EmuHandle::published_count)
    }

    /// What the worker last reported reading `speed` as, or `None` with no core to ask. Unlike
    /// `sync_speed`'s write, this is a fact about the worker's own last pass through its loop —
    /// see `EmuHandle::observed_speed` for why that is a stronger thing to wait on than a frame
    /// count holding still.
    pub fn observed_speed(&self) -> Option<Speed> {
        self.emu.as_ref().map(EmuHandle::observed_speed)
    }

    pub fn frames_taken(&self) -> u64 {
        self.emu.as_ref().map_or(0, EmuHandle::frames_taken)
    }

    /// Gated on the screen being up as well as on a published frame. A core starts loading
    /// on the way into the slot and publishes long before the cart is home, so without this
    /// the game is playing behind the cart for most of the animation. The app owns the answer
    /// because it owns the draw list the game layer is an item in.
    pub fn game_visible(&self) -> bool {
        self.app.game_visible()
    }

    /// Called every frame whether or not anything was pressed: the gesture windows expire on
    /// the tick, not on an event.
    pub fn feed(&mut self, events: impl IntoIterator<Item = RawEvent>, now: Millis) {
        let mut actions = Vec::new();
        for ev in events {
            actions.extend(self.gestures.feed(ev, now));
        }
        actions.extend(self.gestures.tick(now));
        for action in actions {
            self.act(action);
        }
        if let Some(emu) = &self.emu {
            emu.set_input(self.pad.mask());
        }
    }

    fn act(&mut self, action: Action) {
        if trace() {
            eprintln!("slot: {action:?} in {:?}", self.app.phase());
        }
        match action {
            Action::RewindStart => self.rewinding = true,
            Action::RewindStop => self.rewinding = false,
            Action::FfStart => self.fast = true,
            Action::FfStop => self.fast = false,
            _ => {}
        }
        // A button the switcher used is not the game's, on either edge of it: the press that
        // opens it and the one that dismisses it both belong to the switcher.
        let switcher = self.showing_polaroids();
        self.app.apply(action);
        // After apply: the level the sink wants is the one the action just produced.
        if matches!(
            action,
            Action::VolumeUp | Action::VolumeDown | Action::MuteToggle
        ) {
            if let Some(emu) = &self.emu {
                emu.set_volume(self.app.output_volume());
            }
        }
        if switcher || self.showing_polaroids() {
            self.pad.clear();
        } else {
            self.pad.apply(action);
        }
        // On the action rather than on the next frame: an eject or a doze may be the last
        // thing this process does, and a motor left running outlives it.
        self.sync_rumble();
    }

    pub fn update(&mut self, dt: f32) {
        self.app.update(dt);
        if let Some(sfx) = self.app.take_sfx() {
            self.play_sfx(sfx);
        }
        self.sync_core();
        // After the core sync: a handle spawned or dropped this frame has published nothing
        // the renderer may show.
        self.app
            .set_game_ready(self.emu.as_ref().is_some_and(EmuHandle::has_published));
        self.sync_speed();
        self.sync_rewind_hud();
        self.sync_ff_hud();
        self.sync_rumble();
    }

    /// The core writes its motor from the emulator thread and this is the one place that
    /// reaches the hardware with it. The phase has the last word: a cart on its way out, a
    /// paused switcher and a doze all stop the motor whatever the core last asked for.
    fn sync_rumble(&mut self) {
        let want = match &self.emu {
            Some(emu) if self.playing() => emu.rumble().strength(),
            _ => 0,
        };
        self.rumble(want);
    }

    /// The badge tracks the speed the game is actually running at, so a cart on its way in or
    /// a paused switcher takes it down even with R2 still latched.
    fn sync_ff_hud(&mut self) {
        let ff = match (self.fast && self.playing(), self.gestures.ff_latched()) {
            (false, _) => FfState::Off,
            (true, false) => FfState::Held,
            (true, true) => FfState::Latched,
        };
        self.app.set_ff(ff);
    }

    /// The bar belongs to the hold, so it is pushed every frame it lasts and taken down the
    /// moment L2 stops being a rewind, whether that was the button or the phase.
    fn sync_rewind_hud(&mut self) {
        let fill = (self.rewinding && self.playing())
            .then(|| self.emu.as_ref().map(EmuHandle::rewind_fill))
            .flatten();
        match fill {
            Some(fill) => self.app.show_rewind(fill),
            None => self.app.hide_rewind(),
        }
    }

    fn inserting(&self) -> bool {
        matches!(self.app.phase(), Phase::Inserting { .. })
    }

    fn showing_polaroids(&self) -> bool {
        matches!(self.app.phase(), Phase::Polaroids { .. })
    }

    /// Whether the game is live and in charge of the device. Not the phase alone: the power
    /// menu and the shutdown screen are overlays rather than phases — deliberately, so
    /// cancelling returns to whatever was underneath — and the phase stays `Playing` under
    /// both. Reading only the phase left the core running flat out, and the motor buzzing,
    /// behind a screen that had already replaced the game.
    fn playing(&self) -> bool {
        matches!(self.app.phase(), Phase::Playing { .. }) && !self.held()
    }

    /// The screens that have taken the panel away from a cart still seated. The switcher is
    /// not one of them: it has its own phase and `sync_speed` names it separately.
    fn held(&self) -> bool {
        self.app.power_menu().is_some() || self.app.shutting_down()
    }

    fn dozing(&self) -> bool {
        matches!(self.app.phase(), Phase::Doze { .. })
    }

    fn ejecting(&self) -> bool {
        matches!(self.app.phase(), Phase::Ejecting { .. })
    }

    /// The switcher pauses the game rather than dimming a live one. Paused publishes no
    /// frames, so the compositor keeps showing the last one behind the cards.
    fn sync_speed(&self) {
        if let Some(emu) = &self.emu {
            // Loading a core and running one are separate things. The insert animation
            // hides the load, but a core left running behind the cart burns through the
            // GBA bios intro, so the reveal catches only its tail. Paused until the cart is
            // home, the boot animation starts from its first frame as the screen comes on.
            //
            // An eject stops it for the same reason in reverse: the game is over as soon as
            // the button is held, and a core still running behind a dark screen is a game
            // still being heard after the player ended it.
            //
            // Fast forward belongs to the game, so a cart still sliding in runs at its own
            // pace no matter what R2 is doing.
            emu.set_speed(
                if self.inserting()
                    || self.ejecting()
                    || self.showing_polaroids()
                    || self.dozing()
                    || self.held()
                {
                    Speed::Paused
                } else if self.fast && self.playing() {
                    Speed::Fast
                } else {
                    Speed::Normal
                },
            );
            // Held through an eject or into the switcher, L2 stops rewinding rather than
            // eating the history of a cart that is on its way out.
            emu.set_rewinding(self.rewinding && self.playing());
        }
    }

    /// `SLOT_NO_CORE=1` leaves the slot on screen with the cart in it and never starts a
    /// game, so the insert can be watched at full length. Eject and insert again to replay.
    fn no_core() -> bool {
        std::env::var_os("SLOT_NO_CORE").is_some_and(|v| v != "0")
    }

    /// The core exists exactly while a cart is in the slot. Loading it is what the insert
    /// animation is hiding, so the spawn happens on the way in, not on arrival.
    fn sync_core(&mut self) {
        if Self::no_core() {
            return;
        }
        let stem = match self.app.phase() {
            Phase::Shelf => {
                self.emu = None;
                return;
            }
            Phase::Inserting { cart, .. } => cart.clone(),
            _ => return,
        };
        if self.emu.is_none() {
            self.spawn_core(&stem);
        }
        match self.emu.as_ref().map(EmuHandle::state) {
            Some(CoreState::Loading) => {}
            Some(CoreState::Ready) => self.app.on_core_ready(),
            // A refused cart leaves a dead worker behind. Dropping it here is what frees the
            // core for the next insert, since libretro allows only one.
            Some(CoreState::Failed) | None => {
                self.emu = None;
                self.app.on_core_failed();
            }
        }
    }

    fn spawn_core(&mut self, stem: &str) {
        let Some(rom) = self
            .app
            .carts()
            .iter()
            .find(|c| c.stem == stem)
            .map(|c| c.rom.clone())
        else {
            return;
        };
        // A clean start skips the state, it does not delete it: the file stays on the card
        // for the next tap to resume from.
        let resume = (!self.app.starting_clean())
            .then(|| persist::read_resume(&self.root, stem))
            .flatten();
        let emu = EmuHandle::spawn(
            open_core(&self.root),
            rom,
            self.sink.ring(),
            persist::read_sav(&self.root, stem),
            resume,
        );
        // A cart seated after the level was lowered has to start there, not at full.
        emu.set_volume(self.app.output_volume());
        self.app.set_snapshot(Box::new(emu.snapshot()));
        self.emu = Some(emu);
    }
}

/// `SLOT_TRACE=1` prints every semantic action and the phase it landed in. The one thing
/// the tests cannot cover is whether a key reaches the window at all, so this is how that
/// question gets answered without guessing at the platform.
pub(crate) fn trace() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("SLOT_TRACE").is_some())
}

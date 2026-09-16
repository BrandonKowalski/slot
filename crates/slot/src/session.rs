use std::path::PathBuf;

use slot_input::{Action, Gestures, Millis, RawEvent};
use slot_retro::Rumble;
use slot_store::{Core, Platform};
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
    /// A reload for a link is underway: the core in the slot was spawned for it, and `App` is
    /// waiting to hear whether it loaded. See `reload_for_link`.
    reloading: bool,
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
            reloading: false,
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

    /// The emulator thread's own handle, reachable only from here — `App` never touches the
    /// transport or the core itself (see its `link` field's doc comment). `None` before a
    /// cart has spawned one. Exists for whoever ends up wiring a link indicator, and for
    /// tests proving `act`'s own bridge to `EmuHandle::end_link` actually reaches the
    /// emulator thread rather than only `App`'s bookkeeping.
    pub fn emu(&self) -> Option<&EmuHandle> {
        self.emu.as_ref()
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

    /// `App` only ever holds a session's own bookkeeping (see `App::link`'s doc comment) —
    /// the transport and the core it feeds live on the emulator thread, reachable only
    /// through `EmuHandle`. Watching the edge here, around whatever `f` does to `App`, is
    /// what closes that gap for every action that can end a session, without either side
    /// having to know the other exists.
    ///
    /// The one place this is called from used to be inline inside `act`, wrapping only
    /// `App::apply` — which covered every button but missed a critical battery reading,
    /// which reaches `App` through `update`/`timers` instead, with no `Action` and no
    /// `apply` call anywhere on its path. `App::begin_power_off` now ends a live session
    /// itself (see its own doc comment) the same way `doze` already did, but that ending
    /// still needed a way to reach the emulator thread — the whole reason this moved out of
    /// `act` and became the one thing both of `Session`'s own entry points into `App`
    /// (`act`'s `apply`, `update`'s `update`) route every call through. A session can now
    /// only ever end from inside `App` on a path this already watches; there is no longer a
    /// way to add a sixth route that skips it.
    fn bridge_link(&mut self, f: impl FnOnce(&mut App)) {
        let had_link = self.app.link_active();
        f(&mut self.app);
        if had_link && !self.app.link_active() {
            if let Some(emu) = &self.emu {
                // Tells the core the session is over (`RetroCore::stop_link`, if it offered
                // a `stop` to hear it through) and drops the transport, which is what
                // actually closes the wire — see `Cmd::EndLink` in `emu.rs`.
                emu.end_link();
            }
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
        // A button a menu used is not the game's, on either edge of it: the press that opens
        // one and the press that dismisses it both belong to the menu. Read on both sides of
        // the `apply`, because either of those two presses is the one that moves the answer.
        let menu = self.overlaid();
        self.bridge_link(|app| app.apply(action));
        // After apply: the level the sink wants is the one the action just produced.
        if matches!(
            action,
            Action::VolumeUp | Action::VolumeDown | Action::MuteToggle
        ) {
            if let Some(emu) = &self.emu {
                emu.set_volume(self.app.output_volume());
            }
        }
        if menu || self.overlaid() {
            self.pad.clear();
        } else {
            self.pad.apply(action);
        }
        // On the action rather than on the next frame: an eject or a doze may be the last
        // thing this process does, and a motor left running outlives it.
        self.sync_rumble();
    }

    pub fn update(&mut self, dt: f32) {
        self.bridge_link(|app| app.update(dt));
        // The wire a link that just came up runs over. `App` holds a session's own
        // bookkeeping and never a transport (see `App::link`), so this is the hop that
        // carries one to the emulator thread — the mirror of `bridge_link`'s own hop for the
        // ending. Ahead of `sync_speed` below, so the frame the overlay closes on is already
        // a frame the game is running again.
        if let Some((client_id, transport)) = self.app.take_link_transport() {
            match &self.emu {
                Some(emu) => emu.begin_link(client_id, transport),
                // No core to carry it. Dropping the transport closes the socket, which is
                // the only honest thing to do with a session that has nowhere to run.
                None => eprintln!("slot: link: a transport arrived with no core to run it"),
            }
        }
        // A link picked in a mode the running core was not loaded with. Carried out here for the
        // same reason the wire is: `App` never touches the core.
        if let Some((stem, serial)) = self.app.take_link_reload() {
            self.reload_for_link(&stem, serial);
        }
        // The far end going, which happens two ways, and the order between them is the whole
        // point. A peer that ends a link deliberately sends word and *then* drops its wire, so
        // by the time this runs both flags can be up on the same frame. Asking the deliberate
        // question first is what keeps "they ended it" from being reported as "they vanished".
        //
        // The two also end the session by different routes. `peer_ended` ends it outright, so
        // it goes through `bridge_link` — the one hop that carries an ending to the emulator
        // thread. `peer_lost` only breaks the badge; `App::timers` is what ends that one,
        // `LINK_LOST_MS` later, from inside the `update` above that `bridge_link` already
        // wraps. That timeout stays underneath this for every ending nobody could send word
        // about: a crash, a flat battery, an SP carried out of range.
        if self.app.link_active() {
            if self.emu.as_ref().is_some_and(EmuHandle::peer_ended) {
                self.bridge_link(|app| app.peer_ended());
            } else if self.emu.as_ref().is_some_and(EmuHandle::link_lost) {
                self.app.peer_lost();
            }
        }
        if let Some(sfx) = self.app.take_sfx() {
            self.play_sfx(sfx);
        }
        self.sync_core();
        self.sync_reload();
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
    /// paused switcher and a doze all stop the motor whatever the core last asked for. So does
    /// rumble being off in the quick menu: the game rumbles on as far as the emulator knows, and
    /// the motor is only ever told 0.
    fn sync_rumble(&mut self) {
        let want = match &self.emu {
            Some(emu) if self.playing() && self.app.rumble_enabled() => emu.rumble().strength(),
            _ => 0,
        };
        self.rumble(want);
    }

    /// The badge tracks the speed the game is actually running at, so a cart on its way in, a
    /// paused switcher, or a live link session refusing the hold all take it down even with
    /// R2 still latched — a badge that kept showing Held or Latched over a session withholding
    /// the speed would be telling the player their input landed when it did not, the exact
    /// lie `sync_rewind_hud`'s own `actually_rewinding` guards against for the rewind bar.
    fn sync_ff_hud(&mut self) {
        let ff = match (self.actually_fast_forwarding(), self.gestures.ff_latched()) {
            (false, _) => FfState::Off,
            (true, false) => FfState::Held,
            (true, true) => FfState::Latched,
        };
        self.app.set_ff(ff);
    }

    /// The bar belongs to the hold, so it is pushed every frame it lasts and taken down the
    /// moment L2 stops being a rewind, whether that was the button, the phase, or a live link
    /// session refusing it — a bar held up over a rewind that never actually happens would be
    /// showing the player a lie about their own input.
    fn sync_rewind_hud(&mut self) {
        let fill = self
            .actually_rewinding()
            .then(|| self.emu.as_ref().map(EmuHandle::rewind_fill))
            .flatten();
        match fill {
            Some(fill) => self.app.show_rewind(fill),
            None => self.app.hide_rewind(),
        }
    }

    /// L2 held, over a game actually in charge of the device, with nothing forbidding it.
    /// Shared between `sync_speed` (which acts on it) and `sync_rewind_hud` (which shows it),
    /// so the two can never drift into disagreeing about whether a rewind is really underway.
    fn actually_rewinding(&self) -> bool {
        self.rewinding && self.playing() && self.app.may_rewind()
    }

    /// R2's counterpart to `actually_rewinding`, for the identical reason: shared between
    /// `sync_speed` (which acts on it) and `sync_ff_hud` (which shows it), so the badge and
    /// the speed the core is actually run at can never drift into disagreeing about whether
    /// fast forward is really underway.
    fn actually_fast_forwarding(&self) -> bool {
        self.fast && self.playing() && self.app.may_fast_forward()
    }

    fn inserting(&self) -> bool {
        matches!(self.app.phase(), Phase::Inserting { .. })
    }

    fn showing_polaroids(&self) -> bool {
        matches!(self.app.phase(), Phase::Polaroids { .. })
    }

    /// The screens whose buttons belong to them rather than to the game underneath. Both
    /// pause the core as well (`held`, and `sync_speed`'s own `showing_polaroids`), which is
    /// what keeps a press landing here from being seen — but a pause is not a mask, and a
    /// press taken while paused whose release arrives after it is a button the game finds
    /// already down. This is what stops either edge reaching the pad at all.
    fn overlaid(&self) -> bool {
        self.showing_polaroids() || self.app.game_menu_open()
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
        self.app.power_menu().is_some() || self.app.game_menu_open() || self.app.shutting_down()
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
            // Ahead of the speed, so the first fast present already runs at the chosen one. The
            // quick menu lives on the shelf and these cannot change under a seated cart, but the
            // next cart seated after they did picks them up here.
            // A ceiling, not a multiplier: the worker runs as many core frames as each present
            // can afford, up to this.
            emu.set_fast_steps(u32::from(self.app.ff_speed()));
            emu.set_ff_sound(self.app.ff_sound());
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
            // `held()` stops pausing for as long as a session is live, and only here. A
            // paused GBA cannot hold a link open: the far end keeps running and gpSP drops a
            // peer after 240 frames of silence, so pausing a live session does not protect it
            // — it ends it about four seconds later. It is also what the hardware does, since
            // the other player's machine cannot be paused from this one. `Session::overlaid`
            // is what keeps the menu's buttons out of the game underneath it, which is the
            // part a pause was doing by accident.
            emu.set_speed(
                if self.inserting()
                    || self.ejecting()
                    || self.showing_polaroids()
                    || self.dozing()
                    || (self.held() && !self.app.link_active())
                {
                    Speed::Paused
                } else if self.actually_fast_forwarding() {
                    // A live link session forbids fast forward the same way it forbids
                    // rewind: running this device's machine out ahead of what the peer has
                    // actually been sent is a desync with no way back, and libretro's
                    // netpacket contract names fast forward in the same breath as pausing
                    // and rewinding. `App::apply`'s own `FfStart` arm is what shakes the
                    // screen for the player; this is what actually withholds the speed.
                    Speed::Fast
                } else {
                    Speed::Normal
                },
            );
            // Held through an eject or into the switcher, L2 stops rewinding rather than
            // eating the history of a cart that is on its way out — and refused outright
            // during a live link session, since rewinding one device desynchronises the
            // other with no way back to agreement.
            emu.set_rewinding(self.actually_rewinding());
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
            // In whatever mode the cart is in now: what SELECT last switched it to, or what gpSP
            // picks for it.
            let (_, serial) = self.app.link_mode(&stem);
            self.spawn_core(&stem, serial);
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

    /// `serial` is the `gpsp_serial` the core loads with.
    fn spawn_core(&mut self, stem: &str, serial: &'static str) {
        let Some((rom, platform)) = self
            .app
            .carts()
            .find(|c| c.stem == stem)
            .map(|c| (c.rom.clone(), c.platform))
        else {
            return;
        };
        // Resolved once, and only here: this is which dylib gets opened, which
        // `States/<platform>/<core>/` directory the resume lookup below reads from, and — via
        // `set_core` — every later flush, eject and polaroid read for this cart too. Deriving it
        // twice let a `gpsp` cart run on mGBA with its state filed under the `gpsp` directory —
        // the two calls always agreed in practice, right up until `open_core` did not yet know
        // `Core` existed. `App` stores this rather than re-deriving it later, which is what
        // makes that class of drift structurally unreachable now instead of merely unobserved.
        //
        // The platform outranks the file, and only a GBA cart's line is read at all.
        // `selected_core.ini` is hand-edited on the card, so nothing there stops a line reading
        // `Tetris = gpsp` — and gpSP does not run Game Boy games: it would refuse the ROM or
        // paint garbage, with the cart's states filed under a core that never ran it. mGBA is
        // the only core that runs one, so there is no choice for the file to be expressing and
        // nothing is lost by not reading it. A GBA cart's line still means everything it did.
        // `SLOT_CORE` is untouched by this: it names a dylib rather than a `Core`, and its own
        // doc comment already calls it the trap it is.
        let core = match platform {
            Platform::Gba => slot_store::core_for(&self.root, stem),
            Platform::Gb | Platform::Gbc => Core::Mgba,
        };
        self.app.set_core(core);
        // `platform` comes straight off the `Cart` the shelf scanned, not re-derived from the
        // stem: it is what closes the same class of drift for a `.gb` and a `.gba` cart that
        // happen to share a stem.
        self.app.set_platform(platform);
        // gpSP reads its link mode only while a game loads, so what this hands the core is what
        // the game links over from here on, and what `App` compares a picked link against.
        self.app.set_link_loaded(serial);
        // A clean start skips the state, it does not delete it: the file stays on the card
        // for the next tap to resume from.
        let resume = (!self.app.starting_clean())
            .then(|| persist::read_resume(&self.root, platform, core, stem))
            .flatten();
        let emu = EmuHandle::spawn(
            open_core(&self.root, core, serial),
            rom,
            self.sink.ring(),
            persist::read_sav(&self.root, platform, stem),
            resume,
        );
        // A cart seated after the level was lowered has to start there, not at full.
        emu.set_volume(self.app.output_volume());
        self.app.set_snapshot(Box::new(emu.snapshot()));
        self.emu = Some(emu);
    }

    /// Loads the seated game again with `serial`, carrying on from where it is. Durable first,
    /// through the flush the lid, the power button and the autosave all use, because the new
    /// core resumes from exactly what it writes. A load on its way back from one that failed has
    /// no running core to flush, and the state on the card is already the one it resumes. The
    /// old core is dropped before the new one opens: dropping joins its worker, which is what
    /// lets the core go, and libretro allows only one.
    fn reload_for_link(&mut self, stem: &str, serial: &'static str) {
        eprintln!("slot: link: loading {stem} again with gpsp_serial={serial}");
        if self.emu.is_some() {
            self.app.flush_resume();
        }
        self.emu = None;
        self.spawn_core(stem, serial);
        self.reloading = true;
    }

    /// Follows a reload for a link to its end, which `App` is waiting on. A core that will not
    /// load is dropped, as `sync_core` drops a refused cart's, and `App` decides what follows.
    /// The first time that is the mode the game came from, carried out here straight away so no
    /// frame passes with a seated cart and no core behind it; the second time, the cart comes
    /// back out of the slot.
    fn sync_reload(&mut self) {
        if !self.reloading {
            return;
        }
        match self.emu.as_ref().map(EmuHandle::state) {
            Some(CoreState::Loading) => {}
            Some(CoreState::Ready) => {
                self.reloading = false;
                self.app.link_reload_done();
            }
            Some(CoreState::Failed) | None => {
                self.reloading = false;
                self.emu = None;
                self.app.link_reload_failed();
                if let Some((stem, serial)) = self.app.take_link_reload() {
                    self.reload_for_link(&stem, serial);
                }
            }
        }
    }
}

/// `SLOT_TRACE=1` prints every semantic action and the phase it landed in. The one thing
/// the tests cannot cover is whether a key reaches the window at all, so this is how that
/// question gets answered without guessing at the platform.
pub(crate) fn trace() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("SLOT_TRACE").is_some())
}

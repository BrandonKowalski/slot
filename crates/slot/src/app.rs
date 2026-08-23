use std::path::{Path, PathBuf};

use slot_gfx::{OUT_H, OUT_W};
use slot_input::{Action, Btn, MUTE_CHORD_MS};
use slot_power::{Battery, Charge, LedState, LidPolicy, Power};
use slot_store::{
    format_stamp, read_slot_state, scan, write_slot_state, Cart, Core, SlotState, StateEntry,
    StateRing, Theme, BLUE_LIGHT_MAX, BRIGHTNESS_MAX, RING_MAX, VOLUME_MAX,
};
use slot_ui::{
    draw_backdrop, draw_footer, draw_sticker, ClockPicker, Draw, FfState, Hud, HudKind, Icon,
    Millis, Polaroids, PowerChoice, Refusal, Shelf, SlotChrome, TexId, Toast,
};

use crate::audio::Sfx;
use crate::persist::{self, Snapshot};

/// A floor, not a delay. The animation is where the core load hides, so a slow load
/// extends it and a load that is already done still waits it out.
pub const INSERT_S: f32 = 0.73;
/// The tail of the insert, spent on a cart that has already landed. The game arriving on the
/// frame the cart seats reads as a cut; a beat of nothing first says the cart caused it.
///
/// Long enough to cover the rest of the sound of it landing and leave a little air after it,
/// which `the_game_waits_for_the_cart_to_finish_landing` holds against the clip.
const INSERT_HOLD_S: f32 = 0.28;

/// When the travel ends and the cart is against the contacts, which is what it clicks on.
pub const SEATED_AT: f32 = INSERT_S - INSERT_HOLD_S;

/// The eject is the insert played backwards, so it is the same length. It used to be shorter,
/// on the grounds that pulling something out is a quicker movement than pushing it in, but
/// every part of the screen is driven off one progress and two lengths make every one of them
/// come back faster than it left.
pub const EJECT_S: f32 = SEATED_AT;

/// Between the picture going out and the cart starting to move. Long enough for the game to
/// have actually stopped: the core is paused the moment the button is held, but what it has
/// already handed the device is up to a ring's worth of audio, and the cart must not start
/// coming out over the last of it.
const EJECT_HOLD_S: f32 = 0.35;

/// The panel striking, once the cart is home. Long enough to read as a screen coming up,
/// short enough that it is not something to sit through.
const POWER_ON_S: f32 = 0.22;

/// Going out is quicker than coming up, the way a panel dies faster than it strikes.
const POWER_OFF_S: f32 = 0.16;

/// Volume has ten times the range of the other two, so it moves ten times as far. Twenty
/// presses end to end is close enough to their ten that the three feel like one control.
const VOLUME_STEP: u8 = 5;

/// Crash insurance, and the only durable write that happens with the game still running.
const AUTOSAVE_MS: Millis = 60_000;

/// A dead battery is a far likelier hard cutoff than anyone holding POWER for eight
/// seconds, so the last of the charge goes on the state and then on stopping.
const BATTERY_CRITICAL: u8 = 5;

/// The gauge moves by a percent over minutes and on the device it is a sysfs read, so it
/// is not worth a look every frame.
const BATTERY_POLL_MS: Millis = 10_000;

/// The gauge moves over minutes but the charge state is a step change: it flips the instant
/// a cable goes in. Ten seconds of a stale bolt on screen, and a stale colour on the LED, is
/// worse than the read costs — `status` is a short string, far cheaper than the pair.
const CHARGE_POLL_MS: Millis = 1_000;

/// Below this the LED goes red. Well clear of `BATTERY_CRITICAL`, since it is a warning with
/// time to act on it rather than a cutoff.
const BATTERY_LOW: u8 = 20;

/// Long enough to notice the wrong state loading, short enough that the offer is gone by the
/// time the switcher is opened for any other reason.
pub const UNDO_GRACE_MS: Millis = 30_000;

/// How long A has to be down on the shelf before it means "start this cart clean". Past the
/// point a press could be a tap, and short enough to hold without wondering whether the
/// device is still listening.
const PLAY_HOLD_MS: Millis = 500;

/// How far apart the menu's rows sit, and what marks the one in hand. The pitch clears the
/// 40 px face with a little air; the bar is drawn to the face's own width, padding included,
/// so it wraps the words rather than the panel.
/// How long the shutdown screen is on the panel before the machine is allowed to stop. Only
/// needs to outlast a couple of frames — it exists so the ordinary loop presents the screen,
/// rather than the binary rendering one out of band on a GPU that is about to go away.
const SHUTDOWN_SHOW_MS: Millis = 250;

const POWER_MENU_PITCH: f32 = 44.0;
/// How much shorter the bar is than the row it marks, top and bottom. Enough that the rows
/// stay separate things rather than one continuous block when the selection moves.
const POWER_MENU_BAR_INSET: f32 = 4.0;

/// How far through a refused cart's exit the alert holds at full, and where it has finished
/// going. Fractions of that exit rather than seconds, because a cart refused early has a
/// short way to come back and the symbol has to fit inside it either way. It is gone before
/// the end: an alert still lit on the frame the shelf returns reads as a thing to dismiss.
const ALERT_HOLD: f32 = 0.45;
const ALERT_GONE: f32 = 0.9;

/// Any clock reading earlier than this was never set. An RTC that has lost power reports a
/// fault rather than a time, the kernel then starts at the epoch, and nothing that reaches
/// this frontend can legitimately be older than the frontend itself.
const CLOCK_FLOOR: i64 = 1_577_836_800;

/// The most recent undoable action. There is exactly one slot for it and a new save or load
/// replaces it: a stack of undos would be a knob.
pub enum PendingUndo {
    Save {
        stamp: String,
        /// Read out of the ring before the push that dropped it, which is what makes the
        /// undo a restore rather than a reconstruction.
        evicted: Option<(String, Vec<u8>, Vec<u8>)>,
    },
    Load {
        prior: Vec<u8>,
    },
}

/// One side of a live netpacket session. Nothing about the transport or the packets lives
/// here — only what a session being live at all means for the rest of `App`, and which of
/// libretro's two client ids this device is, for whatever the UI ends up showing while one
/// is open.
struct LinkSession {
    client_id: u16,
}

#[derive(Debug)]
pub enum Phase {
    /// Slot's own first launch, ahead of the shelf and ahead of a seated cart. Three things
    /// run off the wall clock and a cartridge RTC is the one that breaks silently.
    SetClock {
        picker: ClockPicker,
    },
    Shelf,
    Inserting {
        cart: String,
        t: f32,
        core_ready: bool,
        /// A cart already in the slot at boot. It is drawn seated from the first frame and
        /// the shelf is never drawn behind it, because a resume is not a movement the user
        /// made and there is nothing for the cart to have come from.
        resumed: bool,
        /// Start the cart from the beginning, ignoring whatever `resume.state` holds. The
        /// state is skipped rather than deleted, so a later tap still resumes it.
        clean: bool,
    },
    Playing {
        cart: String,
    },
    Ejecting {
        cart: String,
        t: f32,
    },
    Polaroids {
        cart: String,
    },
    /// The label. A screen of its own rather than a panel, because it is one object being
    /// looked at and there is nothing else on it.
    About,
    Doze {
        cart: Option<String>,
    },
}

pub struct App {
    phase: Phase,
    shelf: Shelf,
    /// When A went down on the shelf, and `None` the rest of the time. The hold lives here
    /// rather than in the gesture layer because A is the GBA's A button everywhere else, and
    /// `Gestures` is deliberately blind to which screen is up.
    play_held: Option<Millis>,
    /// The last refused action, and the only thing that tells an eject apart from a cart
    /// that would not seat: both leave down the same path.
    refusal: Option<Refusal>,
    /// The `t` a refused cart's exit started from, which is also how long there is left to
    /// say so. `None` for an eject the user asked for: nothing was refused.
    refused_from: Option<f32>,
    alert_face: Option<TexId>,
    /// What the shutdown says, one per `PowerChoice::ALL` in that order and rastered at the
    /// menu's own size. "Powering down" under a restart was the screen contradicting the row
    /// the user had just chosen.
    shutdown_faces: Vec<(TexId, u32, u32)>,
    /// Open when a held POWER raised the menu, holding the highlighted row. An overlay
    /// rather than a phase, so cancelling returns to whatever was underneath without the
    /// phase having to be remembered anywhere.
    power_menu: Option<usize>,
    /// One per `PowerChoice::ALL`, in that order, with the size each was rastered at.
    power_menu_faces: Vec<(TexId, u32, u32)>,
    /// Which core row is highlighted, and `None` when the picker is closed. The cart it acts
    /// on is whichever the shelf has, read when it opens rather than held here: the shelf
    /// cannot move while it is up, so there is only ever one answer and no way for a
    /// remembered one to go stale against it.
    core_picker: Option<usize>,
    /// One per `Core::ALL`, in that order, with the size each was rastered at. Uploaded at
    /// startup beside the power menu's, for a quieter version of the same reason: the faces
    /// never change, so rastering them the moment a button opens the menu would put a font
    /// pass in front of a press that has nothing to gain by it.
    core_picker_faces: Vec<(TexId, u32, u32)>,
    /// Set when the menu's Restart is chosen. The binary acts on it, like `powering_off`.
    restarting: bool,
    /// When the binary is allowed to act. The screen is drawn from the instant the choice is
    /// made, but the shutdown itself waits a few frames so the ordinary loop has drawn and
    /// presented it. Rendering out of band instead — one extra draw and swap between the
    /// choice and `poweroff` — hung the device: the swap can block on a GPU about to be torn
    /// down, and slot then never reached `poweroff` at all, leaving a machine that needed
    /// the PMIC held to recover.
    act_at: Millis,
    /// `None` outside the binary, where there is no content root and nothing persists.
    root: Option<PathBuf>,
    state: SlotState,
    /// The volume and the silence as they stood before each of the last two volume presses,
    /// oldest first. The mute chord is delivered behind the two presses that make it, so
    /// toggling has to give back what they already moved.
    vol_before: Vec<(u8, bool, Millis)>,
    /// `None` until a cart is in the slot. There is nothing to flush without a core.
    snapshot: Option<Box<dyn Snapshot>>,
    /// The seated cart's `Core`, resolved once by whoever spawned `snapshot` and handed here
    /// through `set_core` rather than re-read. `ring`, `flush_resume` and the eject path all
    /// take this instead of calling `core_for` themselves, which is what makes it structurally
    /// impossible for a later read or write to disagree with the dylib actually running: there
    /// is nowhere left in this file to derive a second opinion from. Stale between carts in
    /// exactly the way `snapshot` is — both are set together and neither is cleared on eject —
    /// which is safe because every reader of either is gated on a cart actually being seated.
    core: Core,
    /// `Some` for as long as a netpacket session is live. `App` never touches the transport
    /// or the core itself — those live on the emulator thread, wherever `EmuHandle::begin_link`
    /// was called from the same gesture this answers — this is only what the interlocks below
    /// need: that one is live at all, and which side of it this device is.
    link: Option<LinkSession>,
    /// What the slot itself is about to sound like, drained by whoever owns the device. One
    /// slot: two of these in a frame is not a movement the cart can make.
    sfx: Option<Sfx>,
    /// `Some` exactly while the switcher is showing. It holds the ring as it was when it
    /// opened, so a save behind it cannot renumber what the user is looking at.
    polaroids: Option<Polaroids>,
    /// The one undoable action and the moment it happened. Belongs to the cart in the slot,
    /// so it leaves with it.
    pending: Option<(PendingUndo, Millis)>,
    /// The key caps on the switcher's bottom plate. The three fixed ones never change what
    /// they say and are uploaded once; the undo says which action it will take back, so it is
    /// rasterised on the way into the switcher. All of them outlive any one opening.
    legend_faces: Vec<TexId>,
    undo_face: Option<TexId>,
    /// The clock screen's line of type and its one instruction. Rasterised by the binary,
    /// and gone for the rest of the session once the clock is confirmed.
    clock_faces: Option<(TexId, TexId)>,
    /// The label, rasterised whole. Re-uploaded when the gauge moves.
    sticker_face: Option<TexId>,
    /// One picture from `Wallpapers`, behind everything the shelf draws. `None` on a card
    /// that carries none, which is the common case.
    wallpaper: Option<TexId>,
    /// What is printed on the case: the battery's percent, and the time as it stands.
    battery_percent: slot_ui::Printed,
    /// The charging glyph, uploaded once at boot with the other icons rather than whenever
    /// the percent changes: unlike the percent, its face never varies.
    bolt: Option<TexId>,
    shelf_clock: slot_ui::Printed,
    hud: Hud,
    /// How far up the game layer's own screen is. Not a phase: it outlives the insert, since
    /// the cart is home and the chrome is still on screen while the picture arrives.
    screen: f32,
    /// Whether the core behind the slot has published anything yet. Pushed in, because only
    /// whoever owns the emulator knows: the compositor still holds the last cart's frame.
    game_ready: bool,
    /// Accumulated from `update`, which is the only clock the app has. Milliseconds, since
    /// that is what the HUD fade is stated in.
    clock: f64,
    /// `None` in unit tests, where there is no panel to darken and no battery to run out.
    power: Option<Power>,
    dozed_at: Millis,
    /// When the state next has to be on the card. Moved by every resume write, not only by
    /// the autosave itself.
    autosave_at: Millis,
    battery_at: Millis,
    charge_at: Millis,
    /// The last full reading, with its charge half kept current by the fast tick. One
    /// snapshot rather than two values, so nothing on screen can show a percent and a bolt
    /// that never coexisted.
    battery: Option<Battery>,
    /// What the platform was last told to show. The fast tick recomputes `led_state()` every
    /// second whether or not anything moved, and `set_led` is a real write on a real device —
    /// `motor_change` two crates over exists for exactly the same reason, translating a strength
    /// asked for every frame into a write only on the edge between still and moving. This is
    /// that same edge kept here rather than behind the platform boundary: unlike the motor, the
    /// LED has no protocol-specific state of its own to translate through (`LedState` is
    /// already the discrete value the tick computes), and `App` is where the state it is
    /// computed from already lives, so every `Platform` gets the deduplication for free instead
    /// of each one having to grow its own copy of it.
    last_led: Option<LedState>,
    powering_off: bool,
}

impl App {
    pub fn new(carts: Vec<Cart>) -> Self {
        App {
            phase: Phase::Shelf,
            shelf: Shelf::new(carts),
            play_held: None,
            refusal: None,
            refused_from: None,
            alert_face: None,
            shutdown_faces: Vec::new(),
            power_menu: None,
            power_menu_faces: Vec::new(),
            core_picker: None,
            core_picker_faces: Vec::new(),
            restarting: false,
            act_at: 0,
            root: None,
            state: SlotState::default(),
            vol_before: Vec::new(),
            snapshot: None,
            core: Core::default(),
            link: None,
            sfx: None,
            polaroids: None,
            pending: None,
            legend_faces: Vec::new(),
            undo_face: None,
            clock_faces: None,
            sticker_face: None,
            wallpaper: None,
            battery_percent: slot_ui::Printed::default(),
            bolt: None,
            shelf_clock: slot_ui::Printed::default(),
            hud: Hud::new(),
            screen: 0.0,
            game_ready: false,
            clock: 0.0,
            power: None,
            dozed_at: 0,
            autosave_at: AUTOSAVE_MS,
            battery_at: BATTERY_POLL_MS,
            charge_at: CHARGE_POLL_MS,
            battery: None,
            last_led: None,
            powering_off: false,
        }
    }

    /// A seated cart goes back in through the insert animation rather than appearing
    /// already playing, so a boot and a resume are the same movement. A card with no
    /// `Games` directory scans empty, which is a shelf, not a boot failure.
    pub fn boot(root: &Path) -> Self {
        crate::root::ensure(root);
        crate::root::migrate(root);
        // Before anything is drawn. The card's palette cannot change while the device is on,
        // so it is read once and never asked for again.
        slot_ui::set_theme(Theme::read(root));
        let mut app = App::new(scan(root).unwrap_or_default());
        app.root = Some(root.to_path_buf());
        app.state = read_slot_state(root);
        if app.state.clock_set {
            app.start();
        } else {
            // Seeded from the system clock and re-seeded by `set_power`, which is the first
            // moment there is a platform whose clock is the device's rather than the host's.
            app.phase = Phase::SetClock {
                picker: ClockPicker::from_secs(system_secs()),
            };
        }
        app
    }

    /// Into the slot or onto the shelf. Reached on boot once the clock is known, and from
    /// the clock screen when it becomes known.
    fn start(&mut self) {
        // One cart is a dedicated device. There is nothing to choose between, so whatever
        // `slot.state` remembers, including a cart that is no longer on the card, names the
        // only thing it could have meant.
        let seated = if self.single_cart() {
            Some(0)
        } else {
            let stem = self.state.cart.clone();
            stem.and_then(|stem| self.shelf.carts.iter().position(|c| c.stem == stem))
        };
        self.phase = Phase::Shelf;
        match seated {
            Some(i) => {
                // The shelf sits on the resumed cart so ejecting it lands where it left.
                self.shelf.index = i;
                self.shelf.scroll = i as f32;
                // Never clean: a resume is the whole point of the cart still being in there.
                self.insert(false);
                if let Phase::Inserting { resumed, t, .. } = &mut self.phase {
                    *resumed = true;
                    // Seated already. The floor still runs, so the core has the same time to
                    // load; the cart simply does not travel to get there.
                    *t = INSERT_S;
                }
            }
            // A cart the library no longer has is an empty slot. Left uncorrected on disk:
            // the next seat rewrites it, and a boot is the worst moment to need a write.
            None => self.state.cart = None,
        }
    }

    /// Confirms whatever is on the clock screen. It is asked once, so this is also the only
    /// way off it: there is no way back and no second chance to get it wrong.
    pub fn confirm_clock(&mut self) {
        let Phase::SetClock { picker } = &self.phase else {
            return;
        };
        // Both read off before the borrow ends. The platform is given utc, because that is
        // what the base system's clock and its ntp both assume the card holds; the offset is
        // kept beside it as the only thing that turns it back into the time on the wall.
        let (secs, offset) = (picker.secs(), picker.offset_min());
        if let Some(power) = &mut self.power {
            power.set_clock(secs);
        }
        self.state.utc_offset_min = offset as i16;
        self.state.clock_set = true;
        self.persist();
        self.start();
    }

    /// Seconds since the epoch, from the platform once there is one. The shelf clock and the
    /// polaroid captions both read it, so neither can disagree with the cartridge RTC.
    /// Where the card is mounted. `None` only in the tests that never touch one.
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Local, not utc. The shelf clock, the polaroid captions and the stamps the states are
    /// named by all come through here, so the offset is applied once rather than at each of
    /// them, and none of them can disagree with the others about what time it is.
    pub fn wall_secs(&self) -> i64 {
        let utc = self.power.as_ref().map_or_else(system_secs, |p| p.now());
        utc + i64::from(self.state.utc_offset_min) * 60
    }

    /// What the clock screen is showing, or `None` off it. The binary rasterises from it and
    /// watches its text to know when to do so again.
    pub fn picker(&self) -> Option<&ClockPicker> {
        match &self.phase {
            Phase::SetClock { picker } => Some(picker),
            _ => None,
        }
    }

    pub fn set_sticker_face(&mut self, face: TexId) {
        self.sticker_face = Some(face);
    }

    pub fn set_clock_faces(&mut self, line: TexId, hint: TexId) {
        self.clock_faces = Some((line, hint));
    }

    pub fn set_cart_shadow(&mut self, face: TexId) {
        self.shelf.set_shadow(face);
    }

    pub fn set_wallpaper(&mut self, face: TexId) {
        self.wallpaper = Some(face);
    }

    pub fn set_bolt_face(&mut self, bolt: TexId) {
        self.bolt = Some(bolt);
    }

    pub fn set_battery_percent_face(&mut self, face: TexId, w: u32) {
        self.battery_percent = slot_ui::Printed::new(face, w);
    }

    pub fn set_shelf_clock_face(&mut self, face: TexId, w: u32) {
        self.shelf_clock = slot_ui::Printed::new(face, w);
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    pub fn carts(&self) -> &[Cart] {
        &self.shelf.carts
    }

    /// Exactly one cart on the card. The shelf is unreachable and eject is refused.
    pub fn single_cart(&self) -> bool {
        self.shelf.carts.len() == 1
    }

    /// Face textures in `carts` order. Only the compositor can mint a `TexId`.
    pub fn set_faces(&mut self, faces: Vec<TexId>) {
        self.shelf.set_faces(faces);
    }

    /// Handed over when the core is spawned, which is on the way into the slot.
    pub fn set_snapshot(&mut self, snapshot: Box<dyn Snapshot>) {
        self.snapshot = Some(snapshot);
    }

    /// The `Core` that `session.rs` resolved for the cart it just spawned. Called in the same
    /// breath as `set_snapshot`, from the one place a cart's core is ever decided, so every
    /// later read or write in this file has a stored answer to take rather than a reason to
    /// ask `core_for` again.
    pub fn set_core(&mut self, core: Core) {
        self.core = core;
    }

    /// Whether a netpacket session is live right now. libretro disables an entire class of
    /// time manipulation for as long as one is — see `may_rewind`/`may_load_state` — because
    /// rewinding or loading a state on one device desynchronises the other with no way back
    /// to agreement.
    pub fn link_active(&self) -> bool {
        self.link.is_some()
    }

    /// Begins a session. `client_id` is libretro's own: 0 the host, 1 the joiner — the only
    /// two this product has. Nothing here touches a transport or a core; that lives on the
    /// emulator thread, wherever `EmuHandle::begin_link` is called from the same gesture this
    /// answers. A session always starts from the cart's battery save, never a state — that
    /// falls out for free here, since nothing on this path touches the state ring at all.
    pub fn begin_link(&mut self, client_id: u16) {
        self.link = Some(LinkSession { client_id });
    }

    /// Which side of the session this device is, for whatever the UI ends up showing while
    /// one is live. `None` when there is nothing to ask about.
    pub fn link_client_id(&self) -> Option<u16> {
        self.link.as_ref().map(|s| s.client_id)
    }

    /// Rewinding one device desynchronises the other with no way back, which is exactly what
    /// libretro's netpacket contract forbids for as long as a session is open.
    pub fn may_rewind(&self) -> bool {
        !self.link_active()
    }

    /// Loading a state is the same hazard rewinding is: it moves this device's machine to a
    /// moment the peer never agreed to and has no way to follow.
    pub fn may_load_state(&self) -> bool {
        !self.link_active()
    }

    /// Fast forward runs this device's machine out ahead of what the peer has actually been
    /// sent — the same desync `may_rewind` refuses for running it backwards instead, and
    /// named in the very libretro.h sentence `may_rewind`'s own contract comes from
    /// ("pausing, slow motion, fast forward, rewinding, save state loading... are disabled").
    pub fn may_fast_forward(&self) -> bool {
        !self.link_active()
    }

    /// Ends the session and leaves the cart playing single player. Never an error: the peer
    /// vanishing and the user ending it deliberately look the same from here.
    ///
    /// This is the app's own bookkeeping only, exactly like `link` itself (see its doc
    /// comment) — it does not touch the core or the transport, both of which live on the
    /// emulator thread. `Session::act` is what actually reaches them: it watches
    /// `link_active()` around every `apply`, and mirrors an ending onto
    /// `EmuHandle::end_link()`, which is what tells the core (`RetroCore::stop_link`, if it
    /// offered one to call) and drops the transport. Anything that ends a session without
    /// going through `apply` would need to repeat that mirroring by hand — there is no such
    /// call site today.
    pub fn end_link(&mut self) {
        self.link = None;
    }

    /// The app has no device, so the sound it wants is left here for whoever does.
    pub fn take_sfx(&mut self) -> Option<Sfx> {
        self.sfx.take()
    }

    /// The panel comes up at the level the card remembers rather than at whatever the
    /// kernel left it at.
    pub fn set_power(&mut self, mut power: Power) {
        power.set_backlight(self.state.brightness);
        // The device's own clock, which the host's stands in for. Boot has nothing better to
        // seed the picker from, so a device with a live RTC only gets its confirmation here.
        // The first moment the device's own clock can be asked, and so the first moment a
        // clock that was never set can be told apart from one that was. Boot has already
        // taken `clock_set` at its word by here, which is exactly the case that leaves a
        // dead RTC with no way back to the one screen that could fix it.
        let secs = power.now();
        match &mut self.phase {
            Phase::SetClock { picker } => *picker = ClockPicker::from_secs(secs),
            _ if secs < CLOCK_FLOOR => {
                self.phase = Phase::SetClock {
                    picker: ClockPicker::from_secs(secs),
                }
            }
            _ => {}
        }
        self.power = Some(power);
        // There is nothing to read before this call — no gauge for `battery_at`, no charge
        // for `charge_at`, and whatever `led_state` computed with no battery is not a real
        // state to have already told a platform that did not exist yet either. All three are
        // placeholders with nothing behind them, not real deadlines or a real prior write, so
        // the first tick after this one has to act as if nothing has been read or written
        // yet — or the case band's left shelf sits blank and the LED sits stale for the first
        // poll of whichever cadence is longer, which is the one moment either is cheapest to
        // have been wrong to skip.
        self.battery_at = self.now();
        self.charge_at = self.now();
        self.last_led = None;
    }

    /// The motor. Never persisted and never a level: it belongs to the cart that asked for
    /// it and stops with it.
    pub fn set_rumble(&mut self, strength: u16) {
        if let Some(power) = &mut self.power {
            power.set_rumble(strength);
        }
    }

    /// Set by the doze timeout and by a graceful power off. The binary is what acts on it:
    /// everything durable has already been written by the time it is true.
    ///
    pub fn powering_off(&self) -> bool {
        self.powering_off
    }

    /// What the binary waits for. The decision is made the instant the choice is, but the
    /// machine is not allowed to stop until the ordinary loop has drawn and presented the
    /// shutdown screen. Rendering out of band instead — one extra draw and swap between the
    /// choice and `poweroff` — hung the device: the swap can block on a GPU about to be torn
    /// down, and slot then never reached `poweroff` at all.
    pub fn ready_to_power_off(&self) -> bool {
        self.powering_off && self.now() >= self.act_at
    }

    pub fn ready_to_restart(&self) -> bool {
        self.restarting && self.now() >= self.act_at
    }

    /// Whether the shutdown screen is what should be on the panel. True from the instant the
    /// choice is made, which is earlier than `powering_off`.
    pub fn shutting_down(&self) -> bool {
        self.powering_off || self.restarting
    }

    /// Set by the menu's Restart. Goes through the same shutdown as a power off — busybox
    /// init runs rcK for a reboot too — so the GPU module is unloaded either way, which is
    /// what stops this hardware hanging with the rails up.
    pub fn restarting(&self) -> bool {
        self.restarting
    }

    pub fn restart(&mut self) {
        if let Some(power) = &mut self.power {
            power.restart();
        }
    }

    pub fn power_menu(&self) -> Option<usize> {
        self.power_menu
    }

    /// Which core row the picker is on, and `None` while it is closed. The chrome draws from
    /// it, exactly as it does from `power_menu`.
    pub fn core_picker(&self) -> Option<usize> {
        self.core_picker
    }

    /// The cart under the highlight, and `None` on an empty shelf.
    pub fn selected_stem(&self) -> Option<&str> {
        self.shelf
            .carts
            .get(self.shelf.index)
            .map(|c| c.stem.as_str())
    }

    /// The cached reading. `None` until the first slow tick, and on any device with no gauge.
    pub fn battery(&self) -> Option<Battery> {
        self.battery
    }

    /// Does not return when there is a platform to power off. A unit test has none, and
    /// there the flag is the whole of it.
    pub fn poweroff(&mut self) {
        if let Some(power) = &mut self.power {
            power.poweroff();
        }
    }

    /// The face buttons belong to whatever is on screen. The shelf and the switcher each
    /// take them; while the game is playing they are the game's and the app sees only the
    /// gestures that are never the game's.
    pub fn apply(&mut self, action: Action) {
        // Ahead of everything, including the device's own keys: the menu is a decision the
        // user is in the middle of making, and a volume press underneath it would be one
        // more thing happening while they read.
        if self.power_menu.is_some() {
            match action {
                Action::LidClose => return self.doze(),
                Action::LidOpen => return self.wake(),
                _ => return self.power_menu_input(action),
            }
        }
        // The lid, the light and the sound belong to the device rather than to whatever is
        // on screen, so they are taken before the phase gets a look at the action.
        match action {
            Action::LidClose => return self.doze(),
            Action::LidOpen => return self.wake(),
            Action::PowerPress => {
                // A live session ends here rather than flushing: this is the one button a
                // trade partner mid-exchange can still reach, and ending the session is a
                // decision, not "nothing to flush" — the two must not be the same press.
                if self.link_active() {
                    self.end_link();
                    return;
                }
                return self.flush_resume();
            }
            Action::PowerTap => return self.power_press(),
            Action::PowerHold => return self.open_power_menu(),
            // The release no longer means anything once the menu is what a hold raises:
            // the choice is the commitment, and it is made with A.
            Action::PowerOff => return,
            _ => {}
        }
        // Ahead of the levels too. A screen with no way back is not one to be adjusting the
        // backlight from, and the clock owns all four directions.
        if let Phase::SetClock { picker } = &mut self.phase {
            match action {
                Action::GbaDown(Btn::Left) | Action::ShelfLeft => picker.left(),
                Action::GbaDown(Btn::Right) | Action::ShelfRight => picker.right(),
                Action::GbaDown(Btn::Up) => picker.up(),
                Action::GbaDown(Btn::Down) => picker.down(),
                Action::GbaDown(Btn::A) | Action::Insert => self.confirm_clock(),
                _ => {}
            }
            return;
        }
        if self.adjust(action) {
            return;
        }
        // The release reaches the shelf whatever is on screen. A direction let go of during
        // an insert would otherwise still be held when the cart comes back out.
        match action {
            Action::GbaUp(Btn::Left) => self.shelf.release_left(),
            Action::GbaUp(Btn::Right) => self.shelf.release_right(),
            _ => {}
        }
        let now = self.now();
        match self.phase {
            Phase::Shelf => match action {
                // START rather than SELECT, and the difference is not cosmetic. SELECT is
                // the chord key: held, it turns Up/Down into brightness and Left/Right into
                // blue light, and `adjust` answers those on every screen including this one.
                // Opening a menu the instant SELECT goes down would eat the first half of
                // every one of those chords; waiting out the 600 ms window instead would put
                // that delay in front of the menu. START is bound to nothing here and reaches
                // no core from the shelf, so it costs neither.
                Action::GbaDown(Btn::Start) if self.core_picker.is_none() => {
                    self.open_core_picker()
                }
                // Ahead of the shelf's own movement, so an open picker takes the arrows
                // before the row of carts underneath it does.
                _ if self.core_picker.is_some() => self.core_picker_input(action),
                Action::ShelfLeft | Action::GbaDown(Btn::Left) => self.shelf.hold_left(now),
                Action::ShelfRight | Action::GbaDown(Btn::Right) => self.shelf.hold_right(now),
                Action::OpenAbout => self.phase = Phase::About,
                // A is two actions and the press cannot tell them apart yet, so the cart
                // goes in on the release. The hold has already taken it if it got there
                // first, and then the release is not a second press.
                Action::GbaDown(Btn::A) => self.play_held = Some(now),
                Action::GbaUp(Btn::A) => {
                    if self.play_held.take().is_some() {
                        self.insert(false);
                    }
                }
                Action::Insert => self.insert(false),
                _ => {}
            },
            // Eject reaches an insert as well, so a cart whose core never arrived can still
            // be got out. Nothing else here applies until there is a game.
            Phase::Inserting { .. } if action == Action::Eject => self.eject(),
            Phase::Playing { .. } => match action {
                Action::Eject => self.eject(),
                Action::Polaroids => self.open_polaroids(),
                Action::SaveState => self.save_state(),
                Action::LoadState => self.load_newest(),
                // Rewinding interrupts communication libretro's contract says must not be
                // interrupted. Declined the same way every other "nothing doing" action in
                // this file is, so the press reads as answered rather than dropped.
                Action::RewindStart if !self.may_rewind() => self.refuse(),
                // Fast forward is the same interruption run forwards. `Session::sync_speed`
                // is what actually withholds `Speed::Fast` for as long as `may_fast_forward`
                // says no — this is only the shake, so the press reads as answered.
                Action::FfStart if !self.may_fast_forward() => self.refuse(),
                _ => {}
            },
            Phase::Polaroids { .. } => match action {
                Action::ShelfLeft | Action::GbaDown(Btn::Left) => self.flick(Polaroids::left),
                Action::ShelfRight | Action::GbaDown(Btn::Right) => self.flick(Polaroids::right),
                Action::GbaDown(Btn::A) => self.load_selected(),
                Action::GbaDown(Btn::B) | Action::Polaroids => self.close_polaroids(),
                // The offer lives on this screen and nowhere else. X and Y are free
                // everywhere: the GBA has neither, so the game can never want them.
                Action::GbaDown(Btn::X) => self.undo(self.now()),
                Action::GbaDown(Btn::Y) => self.delete_selected(),
                _ => {}
            },
            // MENU closes it as well as opening it, so the button that got you here gets you
            // back without having to know that B also works.
            Phase::About if action == Action::GbaDown(Btn::B) || action == Action::OpenAbout => {
                self.phase = Phase::Shelf
            }
            _ => {}
        }
    }

    /// Applied at a stated moment rather than at whatever the accumulated clock has reached.
    /// The clock is set rather than advanced: a caller that says when something happened is
    /// stating the whole timeline, not adding to one.
    pub fn apply_at(&mut self, action: Action, now: Millis) {
        self.clock = now as f64;
        self.apply(action);
    }

    /// `true` if the action was one of the three levels, whether or not it moved. A press
    /// that hits an end still shows the bar, which is how the end announces itself.
    fn adjust(&mut self, action: Action) -> bool {
        if action == Action::MuteToggle {
            self.mute_toggle();
            return true;
        }
        let s = &self.state;
        let (kind, value) = match action {
            Action::BrightnessUp => (HudKind::Brightness, up(s.brightness, 1, BRIGHTNESS_MAX)),
            Action::BrightnessDown => (HudKind::Brightness, s.brightness.saturating_sub(1)),
            Action::BlueLightUp => (HudKind::BlueLight, up(s.blue_light, 1, BLUE_LIGHT_MAX)),
            Action::BlueLightDown => (HudKind::BlueLight, s.blue_light.saturating_sub(1)),
            Action::VolumeUp => (HudKind::Volume, up(s.volume, VOLUME_STEP, VOLUME_MAX)),
            Action::VolumeDown => (HudKind::Volume, s.volume.saturating_sub(VOLUME_STEP)),
            _ => return false,
        };
        if kind == HudKind::Volume {
            self.remember_volume();
        }
        let level = match kind {
            HudKind::Brightness => &mut self.state.brightness,
            HudKind::BlueLight => &mut self.state.blue_light,
            HudKind::Volume => &mut self.state.volume,
            // The bar is shared with rewind, which is not a level and is never an action.
            HudKind::Rewind => return false,
        };
        let moved = *level != value;
        *level = value;
        // Turning it up or down is the plainest way to say you want to hear it again.
        let unmuted = kind == HudKind::Volume && std::mem::take(&mut self.state.muted);
        let (shown, now) = (self.hud_value(kind, value), self.now());
        self.hud.show(kind, shown, self.state.muted, now);
        if let (HudKind::Brightness, Some(power)) = (kind, &mut self.power) {
            power.set_backlight(value);
        }
        // A key held against an end would otherwise rewrite the file at the repeat rate.
        if moved || unmuted {
            self.persist();
        }
        true
    }

    /// Where the volume stood before the press about to happen. Only the last two are kept:
    /// a chord is two presses, and anything older belongs to a gesture that already ended.
    fn remember_volume(&mut self) {
        if self.vol_before.len() == 2 {
            self.vol_before.remove(0);
        }
        self.vol_before
            .push((self.state.volume, self.state.muted, self.now()));
    }

    /// Silence is a state rather than a level, so muting neither moves the number nor is
    /// moved by the two presses that asked for it. Both keys fire their own adjustment on
    /// the way to the chord, and from an end those two do not cancel.
    fn mute_toggle(&mut self) {
        let now = self.now();
        if let Some((volume, muted, _)) = self
            .vol_before
            .iter()
            .find(|(_, _, at)| now.saturating_sub(*at) <= MUTE_CHORD_MS)
            .copied()
        {
            self.state.volume = volume;
            self.state.muted = muted;
        }
        self.vol_before.clear();
        self.state.muted = !self.state.muted;
        self.hud
            .show(HudKind::Volume, self.output_volume(), self.state.muted, now);
        self.persist();
    }

    /// What the bar reads. Muted draws as an empty bar under the muted glyph, which is what
    /// zero already looks like and is what it already means.
    fn hud_value(&self, kind: HudKind, value: u8) -> u8 {
        match kind {
            HudKind::Volume => self.output_volume(),
            _ => value,
        }
    }

    /// Pushed from outside because only the emulator knows how much history is left. Held
    /// open until `hide_rewind`, unlike the levels.
    pub fn show_rewind(&mut self, fill: u8) {
        let now = self.now();
        // Rewind is not a level and cannot be silenced, so it is never the muted glyph.
        self.hud.show(HudKind::Rewind, fill, false, now);
    }

    pub fn hide_rewind(&mut self) {
        self.hud.release_rewind();
    }

    /// Pushed from outside for the same reason the rewind fill is: held and latched are one
    /// action apiece to the app and two different things on screen.
    pub fn set_ff(&mut self, ff: FfState) {
        self.hud.set_ff(ff);
    }

    pub fn ff_badge(&self) -> Option<Icon> {
        self.hud.badge()
    }

    pub fn blue_light(&self) -> u8 {
        self.state.blue_light
    }

    /// The level the user chose, which a mute does not touch.
    pub fn volume(&self) -> u8 {
        self.state.volume
    }

    pub fn muted(&self) -> bool {
        self.state.muted
    }

    /// What the sink is actually to be set to. The only one of the two the audio path may
    /// read: a muted device at level 70 is silent, not 70.
    pub fn output_volume(&self) -> u8 {
        if self.state.muted {
            0
        } else {
            self.state.volume
        }
    }

    pub fn hud_icon(&self) -> Icon {
        self.hud.glyph()
    }

    pub fn now(&self) -> Millis {
        self.clock as Millis
    }

    fn flick(&mut self, step: fn(&mut Polaroids)) {
        if let Some(p) = &mut self.polaroids {
            step(p);
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.clock += dt as f64 * 1000.0;
        self.timers();
        let now = self.now();
        // A direction still held as the shelf leaves the screen is not held when it comes
        // back: the row repeats only while it is the thing being looked at.
        if !self.on_shelf() {
            self.shelf.release_hold();
        }
        let mut touched = false;
        let next = match &mut self.phase {
            Phase::Shelf => {
                self.shelf.tick(now);
                self.shelf.update(dt);
                None
            }
            Phase::Inserting {
                cart,
                t,
                core_ready,
                resumed,
                ..
            } => {
                let was = *t;
                *t += dt;
                // Started early enough that the contacts in the clip land on the frame
                // the cart does. A resumed cart never travelled, so it never touched
                // anything.
                let at = SEATED_AT - Sfx::Insert.lead();
                touched = !*resumed && was < at && *t >= at;
                (*t >= INSERT_S && *core_ready).then(|| Phase::Playing {
                    cart: std::mem::take(cart),
                })
            }
            // Two movements, in the order the insert made them: the panel goes out, and only
            // once there is nothing on it does the cart start to travel. A cart sliding out
            // across a live picture is the insert played back with its halves overlapping.
            // The clock only starts once the picture is out, and it starts below zero: the
            // beat before the cart moves is that stretch. The contacts let go as it starts
            // moving, which is neither when the button was held nor while the screen is
            // still going down.
            Phase::Ejecting { t, .. } => {
                if self.screen <= 0.0 {
                    let was = *t;
                    *t += dt;
                    touched = was < 0.0 && *t >= 0.0;
                }
                (*t >= EJECT_S).then_some(Phase::Shelf)
            }
            _ => None,
        };
        // One clip for the whole movement, and the only thing done to it is when it starts.
        if touched {
            self.sfx = Some(match self.phase {
                Phase::Ejecting { .. } => Sfx::Eject,
                _ => Sfx::Insert,
            });
        }
        if let Some(phase) = next {
            // Ahead of the screen step, so the frame the cart finishes arriving is already
            // the first frame of the power on rather than one more frame of nothing.
            self.phase = phase;
            // The cart is out. Whatever it was carrying goes with it.
            self.refused_from = None;
            // `slot.state` mirrors the slot, so it changes where the phase does: seated on
            // the way in, empty on the way back to the shelf whether that was an eject or
            // a refusal.
            let seated = match &self.phase {
                Phase::Playing { cart } => Some(cart.clone()),
                _ => None,
            };
            self.record_cart(seated);
        }
        self.step_screen(dt);
    }

    /// The game layer's own power, which answers to the phase rather than to an event: an
    /// insert, a resume and a wake all bring the picture up the same way.
    fn step_screen(&mut self, dt: f32) {
        let lit = matches!(self.phase, Phase::Playing { .. } | Phase::Polaroids { .. });
        let step = if lit {
            dt / POWER_ON_S
        } else {
            -dt / POWER_OFF_S
        };
        self.screen = (self.screen + step).clamp(0.0, 1.0);
    }

    /// 0.0 dark, 1.0 fully on. The compositor scales and brightens the game layer by it, and
    /// nothing may draw the game at all while it is zero.
    pub fn screen_power(&self) -> f32 {
        self.screen
    }

    pub fn set_game_ready(&mut self, ready: bool) {
        self.game_ready = ready;
    }

    /// Whether the draw list carries the game layer. A core that has published nothing would
    /// otherwise show the last cart's final frame for the length of the insert.
    pub fn game_visible(&self) -> bool {
        self.game_ready && self.screen > 0.0
    }

    /// Jumps the clock without advancing an animation. The autosave and the doze timeout
    /// are minutes apart, which is further than a test wants to walk a frame at a time.
    pub fn tick_ms(&mut self, now: Millis) {
        self.clock = self.clock.max(now as f64);
        self.timers();
    }

    /// Everything the clock alone drives. The play hold is the one thing here the user did
    /// ask for; it is only the clock that decides which of the two things it was.
    fn timers(&mut self) {
        self.play_hold();
        // The grace period can run out with the switcher open, so the hint answers to the
        // clock rather than to whatever was on offer on the way in.
        let offer = self.undo_label();
        if let Some(p) = &mut self.polaroids {
            p.set_undo(offer);
        }
        if self.doze_expired() {
            self.on_doze_timeout();
        }
        if self.now() >= self.autosave_at {
            self.flush_resume();
        }
        if self.now() >= self.battery_at {
            self.battery_at = self.now() + BATTERY_POLL_MS;
            self.battery = self.power.as_ref().and_then(|p| p.battery());
            if let Some(b) = self.battery {
                self.on_battery(b);
            }
        }
        // Only the charge half. The percent it is written beside is at most one slow tick
        // old, which is the staleness the slow tick was always chosen for.
        if self.now() >= self.charge_at {
            self.charge_at = self.now() + CHARGE_POLL_MS;
            if let (Some(power), Some(b)) = (self.power.as_ref(), self.battery.as_mut()) {
                b.charge = power.charge();
            }
            // On the fast tick rather than the slow one: an amber-on-plug-in that lags ten
            // seconds behind the cable is worse than no LED at all.
            let state = self.led_state();
            self.set_led(state);
        }
    }

    /// Modelled on the OG SP: green running, red low, amber charging, green once it is full.
    /// Charging outranks low, since a flat device on a cable is filling rather than dying.
    pub fn led_state(&self) -> LedState {
        let Some(b) = self.battery else {
            return LedState::Running;
        };
        match b.charge {
            Charge::Charging => LedState::Charging,
            Charge::Full => LedState::Charged,
            _ if b.percent <= BATTERY_LOW => LedState::Low,
            _ => LedState::Running,
        }
    }

    /// The one place that ever reaches the platform's own `set_led`, so the edge kept in
    /// `last_led` cannot be bypassed by a call site that forgot it. Called every second with
    /// whatever `led_state` just computed, so on any device that never asserts a charge state
    /// this is the only branch pair — `Low` and `Running` — a write ever leaves this function
    /// with; a state repeated from the previous second returns before touching the platform.
    fn set_led(&mut self, state: LedState) {
        // A shutdown darkens the case and nothing lights it again. The fast tick recomputes
        // `led_state` from the gauge every second and knows nothing about a shutdown in
        // progress, so a charge tick landing inside the window between the choice and
        // `poweroff` put the light straight back to green for the five seconds rcK takes.
        // Guarded here rather than at that call site for the same reason the edge is: this is
        // the one seam, and a caller cannot forget what it never has to remember.
        if self.shutting_down() && state != LedState::Off {
            return;
        }
        if self.last_led == Some(state) {
            return;
        }
        self.last_led = Some(state);
        if let Some(power) = self.power.as_mut() {
            power.set_led(state);
        }
    }

    fn record_cart(&mut self, cart: Option<String>) {
        if self.state.cart == cart {
            return;
        }
        self.state.cart = cart;
        self.persist();
    }

    fn persist(&self) {
        let Some(root) = &self.root else {
            return;
        };
        if let Err(e) = write_slot_state(root, &self.state) {
            eprintln!("slot: slot.state: {e}");
        }
    }

    pub fn on_core_ready(&mut self) {
        if let Phase::Inserting { core_ready, .. } = &mut self.phase {
            *core_ready = true;
        }
    }

    pub fn on_core_failed(&mut self) {
        let caught = self.seat();
        let Phase::Inserting { cart, .. } = &mut self.phase else {
            return;
        };
        let cart = std::mem::take(cart);
        // Resumed at the depth it caught rather than at zero, so the refusal reads as one
        // movement instead of a jump to seated and back out.
        let t = (1.0 - caught) * EJECT_S;
        self.phase = Phase::Ejecting { cart, t };
        // No shake here. The cart is on screen and carries the alert instead, and a screen
        // that flinched as well would read as two separate failures.
        self.refused_from = Some(t);
    }

    /// Any action the app will not carry out. There are no words for it and no state to
    /// clear: it decays on its own clock, wherever it is being drawn.
    pub fn refuse(&mut self) {
        self.refusal = Some(Refusal::started(self.now()));
    }

    pub fn refusal_active(&self, now: Millis) -> bool {
        self.refusal.is_some_and(|r| r.active(now))
    }

    /// How far the cart is into the slot: 0.0 standing on the shelf, 1.0 swallowed.
    pub fn seat(&self) -> f32 {
        match &self.phase {
            Phase::Shelf => 0.0,
            Phase::Inserting { t, resumed, .. } => {
                if *resumed {
                    1.0
                } else {
                    (t / SEATED_AT).clamp(0.0, 1.0)
                }
            }
            Phase::Ejecting { t, .. } => 1.0 - (t / EJECT_S).clamp(0.0, 1.0),
            _ => 1.0,
        }
    }

    pub fn draw(&self, out: &mut Vec<Draw>) {
        // Ahead of every phase, because a shutdown is not a screen the user navigated to.
        // rcK takes about five seconds on this hardware — it stops the frontend and unloads
        // the GPU module before the kernel is allowed to halt — and five seconds of black
        // panel after holding the button is indistinguishable from a device that has hung.
        if let Some(index) = self.power_menu {
            self.draw_power_menu(index, out);
            return;
        }
        if self.shutting_down() {
            out.push(Draw::Rect {
                x: 0.0,
                y: 0.0,
                w: OUT_W as f32,
                h: OUT_H as f32,
                colour: [0.0, 0.0, 0.0, 1.0],
            });
            // The row the user picked is the row the screen repeats back.
            let which = if self.restarting {
                PowerChoice::Restart
            } else {
                PowerChoice::PowerOff
            };
            if let Some((tex, w, h)) = self.shutdown_faces.get(which.index()).copied() {
                out.push(Draw::Tex {
                    x: ((OUT_W - w) / 2) as f32,
                    y: ((OUT_H - h) / 2) as f32,
                    w: w as f32,
                    h: h as f32,
                    tex,
                    alpha: 1.0,
                });
            }
            return;
        }
        match &self.phase {
            // Nothing else is on screen and nothing goes over it, the HUD included: the
            // levels are unreachable here and there is no game to say anything about.
            Phase::SetClock { picker } => {
                let (line, hint) = match self.clock_faces {
                    Some((line, hint)) => (Some(line), Some(hint)),
                    None => (None, None),
                };
                picker.draw(line, hint, out);
                return;
            }
            Phase::Shelf => {
                draw_backdrop(self.wallpaper, out);
                self.shelf.draw(self.shelf_shake(), out);
                draw_footer(
                    self.battery,
                    self.battery_percent,
                    self.bolt,
                    self.shelf_clock,
                    out,
                );
            }
            Phase::About => {
                // The same ground the shelf stands on, scrim and all. The label is a dark
                // object and the scrim is what a dark object needs to read over a
                // photograph — it is there for the carts for exactly the same reason.
                draw_backdrop(self.wallpaper, out);
                draw_sticker(self.sticker_face, out);
                return;
            }
            // The shelf recedes behind the cart on the way in; on the way out the live
            // game is what darkens, and the compositor has already drawn it.
            Phase::Inserting { cart, resumed, .. } => {
                // Spec section 3: a resumed cart shows no shelf, not even one frame of it.
                if !resumed {
                    draw_backdrop(self.wallpaper, out);
                    self.shelf.draw_row(Some(cart), 0.0, self.seat(), out);
                }
                self.chrome(cart, self.seat(), out);
            }
            // The insert run backwards, all of it: the veil lifts, the row closes back up
            // and the cart comes out, every one of them off the same progress running the
            // other way. Darkening on the way out as well as on the way in was the screen
            // playing the same movement twice rather than reversing it.
            Phase::Ejecting { cart, .. } => {
                draw_backdrop(self.wallpaper, out);
                self.shelf.draw_row(Some(cart), 0.0, self.seat(), out);
                self.chrome(cart, self.seat(), out);
            }
            // The slot stays on screen until the picture behind it has finished arriving,
            // so the game blooms out of a lit lip rather than replacing it.
            Phase::Playing { cart } if self.screen < 1.0 => self.chrome(cart, 0.0, out),
            Phase::Playing { .. } => self.push_game(out),
            // The paused game stays underneath, covered by the screenshot the switcher
            // draws over the whole screen.
            Phase::Polaroids { .. } => {
                self.push_game(out);
                if let Some(p) = &self.polaroids {
                    p.draw(
                        self.battery,
                        self.battery_percent,
                        self.bolt,
                        self.shelf_clock,
                        out,
                    );
                }
            }
            // The device answers a shut lid with the backlight; the host has no panel to
            // darken, so the doze is drawn. Nothing goes over it, the HUD included.
            Phase::Doze { .. } => {
                out.push(Draw::Rect {
                    x: 0.0,
                    y: 0.0,
                    w: OUT_W as f32,
                    h: OUT_H as f32,
                    colour: [0.0, 0.0, 0.0, 1.0],
                });
                return;
            }
        }
        // After the shelf, never before it: drawn first it would be painted over by the very
        // row of carts it is a menu for, and START would look like a button that does
        // nothing. Only the shelf can raise it, so no phase needs excluding here — the
        // phases that own the whole panel have already returned.
        if let Some(index) = self.core_picker {
            self.draw_core_picker(index, out);
        }
        // Over everything, in every phase. The bar is never what the user is looking at.
        self.hud.draw(self.now(), out);
    }

    pub fn screen_shake(&self) -> f32 {
        self.shake_at(self.now())
    }

    /// Offscreen pixels the whole presented image is displaced by. The screen flinches only
    /// while the game is playing, which is the one phase whose content fills the frame.
    pub fn shake_at(&self, now: Millis) -> f32 {
        self.shake_when(matches!(self.phase, Phase::Playing { .. }), now)
    }

    /// Pixels the cart row is displaced by. On the shelf the frame is mostly backdrop, so
    /// shaking the whole image would just slide the letterbox in at the edges.
    pub fn shelf_shake(&self) -> f32 {
        self.shake_when(self.on_shelf(), self.now())
    }

    /// Shake whatever represents the thing that was refused, and only that: two of them at
    /// once reads as two separate failures.
    fn shake_when(&self, mine: bool, now: Millis) -> f32 {
        if !mine {
            return 0.0;
        }
        self.refusal.map_or(0.0, |r| r.offset(now))
    }

    /// The game layer's place in the list. Where there is no slot on screen it is the whole
    /// picture; the chrome puts it in the same list, in front of the cart.
    fn push_game(&self, out: &mut Vec<Draw>) {
        if self.game_visible() {
            out.push(Draw::Game);
        }
    }

    fn chrome(&self, stem: &str, dim: f32, out: &mut Vec<Draw>) {
        let Some((cart, face)) = self.shelf.find(stem) else {
            return;
        };
        let alpha = self.alert_alpha();
        SlotChrome {
            cart,
            face,
            seat: self.seat(),
            alert: self.alert_face.filter(|_| alpha > 0.0).map(|t| (t, alpha)),
            dim,
            screen: self.screen,
            game: self.game_ready,
        }
        .draw(out);
    }

    /// Whether the cart on its way back out is carrying the refusal symbol.
    pub fn alert_visible(&self) -> bool {
        self.alert_alpha() > 0.0
    }

    /// How lit that symbol is. It holds for most of the exit and is gone before the end of
    /// it, so the alert leaves with the cart rather than being cut off by the shelf.
    pub fn alert_alpha(&self) -> f32 {
        let (Some(from), Phase::Ejecting { t, .. }) = (self.refused_from, &self.phase) else {
            return 0.0;
        };
        let span = EJECT_S - from;
        if span <= 0.0 {
            return 0.0;
        }
        let u = ((t - from) / span).clamp(0.0, 1.0);
        ((ALERT_GONE - u) / (ALERT_GONE - ALERT_HOLD)).clamp(0.0, 1.0)
    }

    pub fn set_alert_face(&mut self, face: TexId) {
        self.alert_face = Some(face);
    }

    pub fn set_shutdown_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.shutdown_faces = faces;
    }

    pub fn set_power_menu_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.power_menu_faces = faces;
    }

    pub fn set_core_picker_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.core_picker_faces = faces;
    }

    /// Black, the rows, and a bar behind the one in hand. The highlight is a rect rather than
    /// a second face per row: the labels are rastered once at boot and never again, and a
    /// device about to lose its GPU is not the place to be uploading textures.
    /// Type on the case's own ground, with the row in hand marked by a bar the width of its
    /// own words. No plate behind it: the menu is three words and a choice, and a box around
    /// them was furniture the screen did not need.
    ///
    /// The bar is `edge` — the lightest thing in the theme — because it has to read at a
    /// glance on a device someone is about to switch off. `recess` was tried first and is the
    /// right idea and the wrong value: it and `housing` are adjacent dark greys by design,
    /// which is correct for a slot you look into and far too quiet for a selection.
    ///
    /// A rect rather than a second face per row: the labels are rastered once at boot and
    /// never again, and a device about to lose its GPU is not the place to upload textures.
    fn draw_power_menu(&self, index: usize, out: &mut Vec<Draw>) {
        out.push(Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: OUT_W as f32,
            h: OUT_H as f32,
            colour: slot_ui::opening(),
        });
        let rows = self.power_menu_faces.len();
        if rows == 0 {
            return;
        }
        let pitch = POWER_MENU_PITCH;
        let top = (OUT_H as f32 - pitch * rows as f32) / 2.0;
        for (row, (tex, w, h)) in self.power_menu_faces.iter().copied().enumerate() {
            let y = top + pitch * row as f32;
            let x = ((OUT_W as f32 - w as f32) / 2.0).round();
            if row == index {
                out.push(Draw::Rect {
                    x,
                    y: y + POWER_MENU_BAR_INSET,
                    w: w as f32,
                    h: pitch - 2.0 * POWER_MENU_BAR_INSET,
                    colour: slot_ui::edge(),
                });
            }
            out.push(Draw::Tex {
                x,
                y: y + (pitch - h as f32) / 2.0,
                w: w as f32,
                h: h as f32,
                tex,
                alpha: 1.0,
            });
        }
    }

    /// The power menu's rows, at the power menu's pitch, in the power menu's materials —
    /// they are the same kind of object and there is no reason for a device this small to
    /// have two menu idioms.
    ///
    /// What differs is where it goes in the frame. The power menu is the first thing `draw`
    /// does and it returns straight after: it ends the session, so nothing else on the panel
    /// is still true. This is a property of one cart on a shelf the player is still standing
    /// in front of, so it goes over the shelf rather than in place of it, and the HUD still
    /// lands on top — brightness and blue light are answered while it is up, and their level
    /// bars have to be visible when they are.
    fn draw_core_picker(&self, index: usize, out: &mut Vec<Draw>) {
        let rows = self.core_picker_faces.len();
        if rows == 0 {
            return;
        }
        out.push(Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: OUT_W as f32,
            h: OUT_H as f32,
            colour: slot_ui::opening(),
        });
        let pitch = POWER_MENU_PITCH;
        let top = (OUT_H as f32 - pitch * rows as f32) / 2.0;
        for (row, (tex, w, h)) in self.core_picker_faces.iter().copied().enumerate() {
            let y = top + pitch * row as f32;
            let x = ((OUT_W as f32 - w as f32) / 2.0).round();
            if row == index {
                out.push(Draw::Rect {
                    x,
                    y: y + POWER_MENU_BAR_INSET,
                    w: w as f32,
                    h: pitch - 2.0 * POWER_MENU_BAR_INSET,
                    colour: slot_ui::edge(),
                });
            }
            out.push(Draw::Tex {
                x,
                y: y + (pitch - h as f32) / 2.0,
                w: w as f32,
                h: h as f32,
                tex,
                alpha: 1.0,
            });
        }
    }

    fn on_shelf(&self) -> bool {
        matches!(self.phase, Phase::Shelf)
    }

    fn insert(&mut self, clean: bool) {
        if !self.on_shelf() {
            return;
        }
        let Some(cart) = self
            .shelf
            .carts
            .get(self.shelf.index)
            .map(|c| c.stem.clone())
        else {
            return;
        };
        self.play_held = None;
        self.refusal = None;
        self.refused_from = None;
        self.phase = Phase::Inserting {
            cart,
            t: 0.0,
            core_ready: false,
            resumed: false,
            clean,
        };
    }

    /// Whether the cart going in is starting from the beginning. Read by whoever spawns the
    /// core, which is the one thing that has to know.
    pub fn starting_clean(&self) -> bool {
        matches!(self.phase, Phase::Inserting { clean: true, .. })
    }

    /// The hold fires under the finger rather than on the release, so it has an end the
    /// player can feel. A shelf that left the screen with A still down takes the arming with
    /// it: the press belonged to that screen.
    fn play_hold(&mut self) {
        let Some(at) = self.play_held else {
            return;
        };
        if !self.on_shelf() {
            self.play_held = None;
            return;
        }
        if self.now().saturating_sub(at) >= PLAY_HOLD_MS {
            self.insert(true);
        }
    }

    fn eject(&mut self) {
        // Nowhere to eject to. Refused rather than ignored, so the held MENU says no
        // instead of reading as a device that stopped listening.
        if self.single_cart() {
            return self.refuse();
        }
        // Inserting as well as Playing, so a slot with no core behind it can still be
        // emptied: that is the only way to watch the travel more than once.
        let cart = match &mut self.phase {
            Phase::Playing { cart } | Phase::Inserting { cart, .. } => std::mem::take(cart),
            _ => return,
        };
        // A session does not survive its cart. Without this a phantom session outlives the
        // eject: `link_active()` stays true with no core left to carry it, `may_rewind`/
        // `may_load_state` stay wedged closed for whatever cart goes in next, and
        // `doze_expired`'s own guard refuses to let the device sleep again — forever, since
        // nothing left in the app ever flips it back. `Session::act`'s edge bridge (watching
        // `link_active()` fall across every `apply`) is what carries this to
        // `EmuHandle::end_link` on the emulator thread, the same way it does for a doze or a
        // power press.
        self.end_link();
        self.flush_eject(&cart);
        // The offer names a file in this cart's ring and a state only this cart's core can
        // read. Carried across the slot it would delete or load the wrong one.
        self.pending = None;
        // An eject asked for is not an eject refused, whatever was refused a moment ago.
        self.refusal = None;
        self.refused_from = None;
        self.phase = Phase::Ejecting {
            cart,
            t: -EJECT_HOLD_S,
        };
    }

    /// Everything durable happens here, before the animation rather than after it: the
    /// card can be pulled while the cart is still sliding out. A write that failed leaves
    /// the cart recorded as seated, so the next boot resumes it and the end of the
    /// animation retries the clear.
    fn flush_eject(&mut self, stem: &str) {
        let (Some(root), Some(snapshot)) = (&self.root, &self.snapshot) else {
            return;
        };
        let Some(state) = snapshot.state() else {
            eprintln!("slot: eject: the core gave up no state");
            return;
        };
        let (state, sav) = trusted_write(snapshot.as_ref(), state, "eject");
        match persist::eject(root, self.core, stem, state.as_deref(), sav.as_deref()) {
            Ok(()) => self.state.cart = None,
            Err(e) => eprintln!("slot: eject: {e}"),
        }
    }

    /// Flush, then dark, then idle. The cart stays in the slot and `slot.state` is not
    /// touched: a sleep is not an eject, and the next boot has to resume this session
    /// whether the lid opens again or the battery runs out first.
    ///
    /// The one function every doze actually goes through: both `LidClose` arms (with the
    /// power menu open, and without) and `PowerTap` by way of `power_press` all return
    /// `self.doze()` rather than reimplementing it, so a guard here — and only here — closes
    /// every path in at once. Three copies of the same `if self.link_active()` at each call
    /// site is exactly the kind of duplication that let a mutation slip through unnoticed
    /// last time: `on_doze_timeout` carried a redundant copy of `doze_expired`'s own guard,
    /// and that second copy alone was enough to keep `doze_never_expires_while_a_session_is_live`
    /// passing after the real guard was mutated away.
    ///
    /// A live session ends here rather than surviving the doze — but the doze still happens:
    /// this used to `return` right after `end_link()`, which ended the session and then left
    /// the device sitting in `Phase::Playing`, wide awake, behind a lid the player had just
    /// shut. That is exactly the 400-700 mA outcome the paragraph below argues against,
    /// reached anyway, with the session dead on top of it — proven by `phase` still reading
    /// `Playing` ten seconds after a `LidClose` that hardware delivers exactly once per
    /// physical close, with no second press coming to "retry" into an actual doze.
    ///
    /// `Session::sync_speed` maps `Phase::Doze` to `Speed::Paused`, and pausing is one of the
    /// exact manipulations libretro's netpacket contract names as forbidden while players are
    /// connected — the same desync hazard as dropping the transport outright, not a lesser
    /// one. The alternative — holding the session open through a doze that keeps the core
    /// running *unpaused*, so the panel can go dark for free — is a bigger change than this
    /// fix (`sync_speed` would have to learn about sessions too) and would not even save the
    /// battery it sounds like it would: `doze_expired` already refuses to end a session on
    /// its own idle timer, so a session left open behind a shut lid would sit at 400-700 mA
    /// with the radio up for as long as the lid stayed shut, never once reaching the sub-45
    /// mA a real doze exists to reach. Ending the session costs a trade partner who shut the
    /// lid only to think for a moment — there is no answer here that costs nothing — but it
    /// is the one already chosen for `PowerPress`, and completing the doze underneath it is
    /// the only way to actually reach the low-power state this function exists for.
    fn doze(&mut self) {
        if self.link_active() {
            self.end_link();
        }
        if matches!(self.phase, Phase::Doze { .. }) {
            return;
        }
        self.flush_resume();
        // Only a running cart is worth waking back into. A lid closed over an animation
        // wakes to the shelf, one press from where it was, rather than into a core that
        // may not have finished loading.
        let cart = match &mut self.phase {
            Phase::Playing { cart } | Phase::Polaroids { cart } => Some(std::mem::take(cart)),
            _ => None,
        };
        self.polaroids = None;
        self.phase = Phase::Doze { cart };
        self.dozed_at = self.now();
        if let Some(power) = &mut self.power {
            power.on_close();
        }
    }

    fn wake(&mut self) {
        let Phase::Doze { cart } = &mut self.phase else {
            return;
        };
        self.phase = match cart.take() {
            Some(cart) => Phase::Playing { cart },
            None => Phase::Shelf,
        };
        if let Some(power) = &mut self.power {
            power.on_open();
        }
    }

    /// A dark panel is not a saving: the machine is still running flat out behind it at
    /// 400-700 mA. So the dark is a grace period rather than a state, and when it runs out
    /// the device stops for real.
    ///
    /// It suspends beautifully — under 45 mA — and that is not on offer, because it cannot
    /// wake itself back up: the RTC alarm arms, reads back, and never fires. A sleep nothing
    /// can end is a slow leak with a better name. Powering off costs the user a three second
    /// boot, and `slot.state` still names the cart, so they come back to the same frame.
    pub fn on_doze_timeout(&mut self) {
        if !matches!(self.phase, Phase::Doze { .. }) {
            return;
        }
        self.begin_power_off();
    }

    /// The lid's twin, and the only one of the two the device is certain to see. A tap
    /// dozes and a second one wakes.
    fn power_press(&mut self) {
        match self.phase {
            Phase::Doze { .. } => self.wake(),
            _ => self.doze(),
        }
    }

    /// A held button powers off, through the OS rather than the PMIC. The PMIC's own
    /// six-second hold cuts the rails in hardware with no sync, no unmount and no driver
    /// teardown; the software path unloads the GPU module first, which is the difference
    /// between a machine that stops and one that hangs with the rails up draining the
    /// battery. Six seconds remains the emergency underneath, and needs no help from here.
    ///
    /// Not an eject: the cart stays in the slot so the next boot resumes it. The flush is
    /// a no-op after a doze, which has already written the same file.
    /// The hold threshold raises the menu and nothing else. Every outcome from here is one
    /// the user chose rather than one the button committed them to, which is what makes the
    /// hold safe to discover by accident.
    fn open_power_menu(&mut self) {
        if self.power_menu.is_some() {
            return;
        }
        // Same hazard as the switcher: the menu pauses the core too — `Session::sync_speed`
        // maps `held()`, which the menu is one of, to `Speed::Paused` — one of the exact
        // manipulations libretro's netpacket contract forbids while a session is live. Unlike
        // `PowerPress` this button does not end the session for the player; it just declines,
        // the same shake every other "nothing doing" action in this file answers with.
        if self.link_active() {
            return self.refuse();
        }
        // Durable before the menu is even on screen: from here the user may hold on to the
        // PMIC's own six second cutoff, which takes the rails away whatever we wanted.
        self.flush_resume();
        self.power_menu = Some(0);
    }

    /// Up and down move, A commits, B leaves. Nothing times out: a menu that closed itself
    /// would do it exactly when the user looked away to think.
    fn power_menu_input(&mut self, action: Action) {
        let Some(index) = self.power_menu else {
            return;
        };
        let last = PowerChoice::ALL.len() - 1;
        match action {
            Action::GbaDown(Btn::Up) => self.power_menu = Some(index.saturating_sub(1)),
            Action::GbaDown(Btn::Down) => self.power_menu = Some((index + 1).min(last)),
            Action::GbaDown(Btn::B) => self.power_menu = None,
            Action::GbaDown(Btn::A) => {
                self.power_menu = None;
                match PowerChoice::ALL[index] {
                    PowerChoice::Restart => {
                        self.restarting = true;
                        self.act_at = self.now() + SHUTDOWN_SHOW_MS;
                        self.set_led(LedState::Off);
                    }
                    PowerChoice::PowerOff => self.begin_power_off(),
                }
            }
            _ => {}
        }
    }

    /// SELECT on the shelf offers the highlighted cart's core, opening on the one it already
    /// uses so the menu answers "which is this?" before it asks "which do you want?".
    ///
    /// Both the read that positions the highlight and the write that follows need the card.
    /// Without one there is nothing to configure and nowhere to put an answer, so the button
    /// stays inert rather than raising a menu whose choice would evaporate.
    fn open_core_picker(&mut self) {
        let Some(root) = self.root.clone() else {
            return;
        };
        // Nothing to configure with no cart under the highlight, and a picker that wrote to
        // an empty stem would leave a line for a cart that is not there.
        let Some(cart) = self.shelf.carts.get(self.shelf.index) else {
            return;
        };
        self.core_picker = Some(slot_store::core_for(&root, &cart.stem).index());
    }

    /// The picker owns every button while it is up, including the arrows the shelf uses: a
    /// menu that let the thing behind it move would act on a different cart than the one it
    /// named. Up and down wrap, because with two rows either arrow is the other's undo and
    /// an end that stuck would need the player to know which one they were against.
    fn core_picker_input(&mut self, action: Action) {
        let Some(row) = self.core_picker else {
            return;
        };
        let rows = Core::ALL.len();
        match action {
            Action::GbaDown(Btn::Up) => self.core_picker = Some((row + rows - 1) % rows),
            Action::GbaDown(Btn::Down) => self.core_picker = Some((row + 1) % rows),
            Action::GbaDown(Btn::A) => {
                self.write_core(Core::ALL[row]);
                self.core_picker = None;
            }
            Action::GbaDown(Btn::B) => self.core_picker = None,
            _ => {}
        }
    }

    /// The choice, onto the card. Best effort, like every other card write here: a read only
    /// or absent card is a shelf that still works, not a boot failure. Nothing else in the
    /// app is told — `self.core` is the seated cart's, set when a core is actually spawned,
    /// and the shelf has none seated.
    fn write_core(&self, core: Core) {
        let (Some(root), Some(cart)) = (self.root.clone(), self.shelf.carts.get(self.shelf.index))
        else {
            return;
        };
        if let Err(e) = slot_store::write_selected_core(&root, &cart.stem, core) {
            eprintln!("slot: core: could not write selected_core.ini: {e}");
        }
    }

    /// Every path to shutdown — a held button, an idle doze timing out, and a critical
    /// battery reading that needs no button at all — funnels through here, so none of them
    /// leaves the LED reporting Running or Charging through a shutdown the user is not
    /// watching finish. A real behaviour on a handheld: the case still has a light on it for
    /// as long as `poweroff` takes to actually cut power.
    fn begin_power_off(&mut self) {
        // Idempotent, and that is the whole of why: `doze_expired` is a level rather than an
        // edge and this leaves the phase on `Doze`, so `timers` calls back here every frame
        // for as long as the lid is shut. Re-arming `act_at` each time walked the deadline
        // ahead of the clock forever, and the device sat dark and awake until the lid opened
        // and took the phase out of `Doze` — at which point it powered off in the user's
        // hands, on the frame they came back to the session.
        if self.powering_off {
            return;
        }
        // A power-off pauses the core outright (`shutting_down()`, of which this is the
        // start, is one of the states `Session::sync_speed` maps to `Speed::Paused`) —
        // libretro's netpacket contract forbids that for as long as a session is live, the
        // same hazard `doze`'s own guard exists for. `doze` and the power menu's own open
        // already end a session before either of their own routes reaches here, which is
        // why this was previously always false in practice by the time any caller arrived —
        // right up until a critical battery reading turned out to be a fifth route in, with
        // no button, no menu and no doze anywhere upstream of it to have ended one first.
        // Guarding the chokepoint itself, rather than that one caller, is what keeps a sixth
        // route from reopening the same hole: whatever calls `begin_power_off` next inherits
        // this for free.
        if self.link_active() {
            self.end_link();
        }
        self.powering_off = true;
        self.act_at = self.now() + SHUTDOWN_SHOW_MS;
        self.set_led(LedState::Off);
    }

    /// The gauge, polled from `timers` and injected by the tests. Only a charge state the
    /// device positively asserted suppresses the cutoff: unknown and discharging both power
    /// off at the threshold, which is what the frontend did before it could read one.
    pub fn on_battery(&mut self, b: Battery) {
        if b.percent > BATTERY_CRITICAL || self.powering_off {
            return;
        }
        if matches!(b.charge, Charge::Charging | Charge::Full) {
            return;
        }
        // A real power off, not a sleep. This is the one shutdown the user did not ask for,
        // and suspending a cell this empty only spends what is left of it more slowly.
        self.flush_resume();
        self.begin_power_off();
    }

    fn doze_expired(&self) -> bool {
        // Suspended for as long as a session is live: a trade partner reading a menu on the
        // other device must not have the link dropped out from under them by this one's own
        // idle timer.
        if self.link_active() {
            return false;
        }
        let (Phase::Doze { .. }, Some(power)) = (&self.phase, &self.power) else {
            return false;
        };
        self.now().saturating_sub(self.dozed_at) >= power.timeout().as_millis() as Millis
    }

    /// resume.state and the battery save, with the slot left alone. A cart that is not
    /// playing has no state of its own to write.
    fn flush_resume(&mut self) {
        // The invariant is 60 s since the state was last durable, not 60 s since the last
        // autosave, so an attempt that had nothing to write still moves the deadline.
        self.autosave_at = self.now() + AUTOSAVE_MS;
        let (Some(root), Some(snapshot), Some(cart)) = (&self.root, &self.snapshot, self.seated())
        else {
            return;
        };
        let Some(state) = snapshot.state() else {
            eprintln!("slot: flush: the core gave up no state");
            return;
        };
        let (state, sav) = trusted_write(snapshot.as_ref(), state, "flush");
        if let Err(e) = persist::flush(root, self.core, cart, state.as_deref(), sav.as_deref()) {
            eprintln!("slot: flush: {e}");
        }
    }

    /// The ring for the cart in the slot. `None` outside the binary, where there is no
    /// content root, which reads as a cart that has never been saved.
    fn ring(&self) -> Option<StateRing> {
        let (Some(root), Some(cart)) = (&self.root, self.seated()) else {
            return None;
        };
        Some(StateRing::new(root, self.core, cart))
    }

    fn seated(&self) -> Option<&str> {
        match &self.phase {
            Phase::Playing { cart } | Phase::Polaroids { cart } => Some(cart),
            _ => None,
        }
    }

    fn entries(&self) -> Vec<StateEntry> {
        self.ring().and_then(|r| r.list().ok()).unwrap_or_default()
    }

    /// An empty ring shakes rather than opening an empty screen, per spec section 4.
    fn open_polaroids(&mut self) {
        // Opening the switcher pauses the core — `Session::sync_speed` maps
        // `Phase::Polaroids` straight to `Speed::Paused` — one of the exact manipulations
        // libretro's netpacket contract forbids while a session is live, whether or not the
        // player means to load anything once inside. Checked ahead of even looking for
        // states to show, the same way `load_newest` already checked ahead of looking for
        // one to load.
        if self.link_active() {
            return self.refuse();
        }
        let entries = self.entries();
        if entries.is_empty() {
            return self.refuse();
        }
        let Phase::Playing { cart } = &mut self.phase else {
            return;
        };
        let cart = std::mem::take(cart);
        let mut p = Polaroids::new(entries);
        p.set_undo(self.undo_label());
        self.polaroids = Some(p);
        self.phase = Phase::Polaroids { cart };
        self.push_hint_faces();
    }

    fn close_polaroids(&mut self) {
        let Phase::Polaroids { cart } = &mut self.phase else {
            return;
        };
        let cart = std::mem::take(cart);
        self.polaroids = None;
        self.phase = Phase::Playing { cart };
    }

    fn load_selected(&mut self) {
        let state = self
            .polaroids
            .as_ref()
            .and_then(|p| p.selected())
            .map(|e| e.state.clone());
        // `None` means nothing was selected, not a refusal, and still closes exactly as
        // before. `Some(false)` means `load_file` refused (a live session, most reachably —
        // see its own doc comment) and already shook the screen for it; closing the switcher
        // on top of that shake would read as the pick landing and then being dismissed, when
        // nothing happened at all. Unreachable today, since `open_polaroids` already refuses
        // to open a switcher a session forbids picking from — but wrong the moment that guard
        // moves, and cheap to keep correct regardless of where it lives.
        let refused = state.map(|state| self.load_file(&state)) == Some(false);
        if !refused {
            self.close_polaroids();
        }
    }

    /// Not undoable, and deliberately so. The undo slot holds one save or one load, and a
    /// third kind in it would be an undo whose meaning depended on what you did last. A state
    /// chosen off a screen showing you exactly which one is a decision, not a slip.
    fn delete_selected(&mut self) {
        let stamp = self
            .polaroids
            .as_ref()
            .and_then(|p| p.selected())
            .map(|e| e.stamp.clone());
        let (Some(stamp), Some(ring)) = (stamp, self.ring()) else {
            return;
        };
        if let Err(e) = ring.remove(&stamp) {
            eprintln!("slot: delete: {e}");
            return;
        }
        // An offer left pointing at a file that is gone would remove nothing and then put the
        // evicted entry back, which is not what undoing that save means any more.
        if self.undo_targets(&stamp) {
            self.pending = None;
        }
        let Some(p) = &mut self.polaroids else {
            return;
        };
        p.remove_selected();
        if p.is_empty() {
            self.close_polaroids();
        }
    }

    fn undo_targets(&self, stamp: &str) -> bool {
        match self.pending.as_ref() {
            Some((PendingUndo::Save { stamp: pending, .. }, _)) => pending == stamp,
            // A load's undo holds the prior state in memory, so no file on the card can
            // invalidate it.
            _ => false,
        }
    }

    fn load_newest(&mut self) {
        let Some(newest) = self.entries().first().map(|e| e.state.clone()) else {
            return self.refuse();
        };
        self.load_file(&newest);
    }

    /// The chokepoint every load-from-disk route funnels through — `load_newest` above and
    /// `load_selected` alike — so the session guard lives here once rather than at each
    /// caller. That used to be `load_newest`'s own job, checked ahead of even looking for a
    /// state to load; `load_selected` never got the same check, which is what let the
    /// switcher's own A-button pick bypass it entirely. Guarding here instead closes that
    /// hole for both today's callers and whatever the next one turns out to be.
    /// Reports whether the load actually happened, so a caller that only means to load —
    /// `load_newest` — can ignore it, and one that has something else riding on the answer —
    /// `load_selected`, which must not close the switcher out from under a refusal it just
    /// drew — can ask rather than repeating the guard above for itself.
    fn load_file(&mut self, state: &Path) -> bool {
        // A state load would desynchronise the other device with no way back to agreement.
        if !self.may_load_state() {
            self.refuse();
            return false;
        }
        let Some(snapshot) = &self.snapshot else {
            return false;
        };
        let bytes = match std::fs::read(state) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("slot: load: {e}");
                return false;
            }
        };
        // Taken before the load, which is the last moment there is anything to go back to.
        let prior = snapshot.state();
        snapshot.load(bytes);
        self.hud.toast(Toast::StateLoaded, self.now());
        if let Some(prior) = prior {
            self.pending = Some((PendingUndo::Load { prior }, self.now()));
        }
        true
    }

    /// `SELECT+R1` and nothing else reaches here. A state with no picture is still worth
    /// keeping: the switcher draws a blank card rather than losing the save.
    ///
    /// Declines outright when the live core refused the resume it was opened with — the same
    /// condition `trusted_write` withholds from `flush`/`eject` for. This is the one durable
    /// sink that guard does not reach, because it is not a write-back over an existing file:
    /// `ring.push` is a deliberate ring buffer, and once it holds `RING_MAX` entries, pushing
    /// an eleventh evicts the oldest to make room. A core running on its own default machine
    /// has nothing worth keeping in that slot, so pushing it would not just waste an entry —
    /// it would delete a real one to make room for a placeholder. Refused the same way every
    /// other "nothing to do here" action in this file is, via `refuse()`: the player gets the
    /// same shake `load_newest`/`open_polaroids` already answer with, rather than a save that
    /// silently did not happen.
    fn save_state(&mut self) {
        let (Some(ring), Some(snapshot)) = (self.ring(), &self.snapshot) else {
            return;
        };
        if !snapshot.resume_trusted() {
            eprintln!("slot: save: the core refused the resume it was given, not pushing a state");
            return self.refuse();
        }
        let Some(state) = snapshot.state() else {
            eprintln!("slot: save: the core gave up no state");
            return;
        };
        let thumb = snapshot.thumb().unwrap_or_default();
        let stamp = free_stamp(&ring, self.wall_secs());
        let evicted = doomed(&ring);
        if let Err(e) = ring.push(&state, &thumb, &stamp) {
            eprintln!("slot: save: {e}");
            return;
        }
        self.hud.toast(Toast::StateSaved, self.now());
        self.pending = Some((PendingUndo::Save { stamp, evicted }, self.now()));
    }

    pub fn undo_available(&self, now: Millis) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|(_, at)| now.saturating_sub(*at) <= UNDO_GRACE_MS)
    }

    /// What the offer says, or `None` when there is nothing on offer. The binary rasterises
    /// it; the grace period is read off the app's own clock so the two cannot disagree.
    pub fn undo_label(&self) -> Option<&'static str> {
        if !self.undo_available(self.now()) {
            return None;
        }
        match self.pending.as_ref()?.0 {
            PendingUndo::Save { .. } => Some("undo save"),
            PendingUndo::Load { .. } => Some("undo load"),
        }
    }

    /// In `LEGEND` order, and uploaded once: none of the three ever changes what it says.
    pub fn set_legend_faces(&mut self, faces: Vec<TexId>) {
        self.legend_faces = faces;
        self.push_hint_faces();
    }

    pub fn set_undo_face(&mut self, face: Option<TexId>) {
        self.undo_face = face;
        self.push_hint_faces();
    }

    /// The undo goes last because `hints` puts it last, which is what keeps faces and hints
    /// on the same index.
    fn push_hint_faces(&mut self) {
        let mut faces = self.legend_faces.clone();
        faces.extend(self.undo_face);
        if let Some(p) = &mut self.polaroids {
            p.set_hint_faces(faces);
        }
    }

    /// The HUD glyphs, in `Icon::ALL` order. Uploaded once: they never change.
    pub fn set_icon_faces(&mut self, faces: Vec<TexId>) {
        self.hud.set_icons(faces);
    }

    /// The two lines the HUD can say, in `Toast::ALL` order.
    pub fn set_toast_faces(&mut self, faces: Vec<TexId>) {
        self.hud.set_toasts(faces);
    }

    /// What the HUD is saying, or `None` once it has faded. Only ever set by an action that
    /// happened: a refusal shakes instead.
    pub fn toast(&self) -> Option<Toast> {
        self.hud.said(self.now())
    }

    /// One shot, and it hands the game back the way loading does. Undoing an undo would be a
    /// redo, and the switcher is not a place to sit and shuffle.
    pub fn undo(&mut self, now: Millis) {
        if !self.undo_available(now) {
            self.pending = None;
            return;
        }
        // Undoing a load moves the core to a moment the peer never agreed to — the exact
        // hazard `load_file` guards against, and the one route into it that never passes
        // through `load_file` at all: the bytes are already in hand from when the load
        // happened, not read fresh off disk. Refused without consuming the offer, the same
        // way a refused rewind or state load leaves the player able to try again once the
        // session that refused it is gone — an undo's own save-file cleanup, `undo_save`
        // below, touches no core state at all, so only this arm needs the check.
        if matches!(&self.pending, Some((PendingUndo::Load { .. }, _))) && !self.may_load_state() {
            return self.refuse();
        }
        let Some((what, _)) = self.pending.take() else {
            return;
        };
        match what {
            PendingUndo::Save { stamp, evicted } => self.undo_save(&stamp, evicted),
            PendingUndo::Load { prior } => {
                if let Some(snapshot) = &self.snapshot {
                    snapshot.load(prior);
                }
            }
        }
        self.close_polaroids();
    }

    fn undo_save(&self, stamp: &str, evicted: Option<(String, Vec<u8>, Vec<u8>)>) {
        let Some(ring) = self.ring() else {
            return;
        };
        if let Err(e) = ring.remove(stamp) {
            eprintln!("slot: undo: {e}");
            return;
        }
        let Some((stamp, state, thumb)) = evicted else {
            return;
        };
        if let Err(e) = ring.push(&state, &thumb, &stamp) {
            eprintln!("slot: undo: {e}");
        }
    }

    /// Entries in the switcher's order, newest first. The binary reads these to build the
    /// faces, since only the compositor can mint a `TexId`.
    pub fn polaroid_entries(&self) -> &[StateEntry] {
        match &self.polaroids {
            Some(p) => &p.entries,
            None => &[],
        }
    }

    pub fn set_polaroid_faces(&mut self, faces: Vec<TexId>) {
        if let Some(p) = &mut self.polaroids {
            p.set_faces(faces);
        }
    }

    /// Which entry is under the eye. The binary watches this to know when the title has to be
    /// rasterised again. The stamp rather than the index, because a delete leaves the index
    /// where it was and moves a different entry under it.
    pub fn polaroid_stamp(&self) -> Option<&str> {
        self.polaroids
            .as_ref()
            .and_then(|p| p.selected())
            .map(|e| e.stamp.as_str())
    }

    /// What the top plate says. `now` is a stamp rather than the app's clock: the entries
    /// are named by their filenames and the title is relative to the wall clock.
    pub fn polaroid_title(&self, now: &str) -> String {
        self.polaroids
            .as_ref()
            .map_or_else(String::new, |p| p.title(now))
    }

    pub fn set_polaroid_title_face(&mut self, face: TexId) {
        if let Some(p) = &mut self.polaroids {
            p.set_title_face(Some(face));
        }
    }
}

/// The write-back half of the guard `EmuSnapshot` records. `state` is the bytes the live core
/// actually holds; `snapshot.resume_trusted()`/`save_ram_trusted()` say whether the core that
/// produced them actually accepted the resume/save-ram it was opened with. A region it
/// refused is withheld here — turned into `None` rather than passed on to `persist::flush`/
/// `eject` — because a core running with its own default state has nothing worth writing back
/// over the file that refusal left alone. `verb` names the caller only for the log line
/// ("flush" or "eject"), so a withheld region reads the same as everything else either one
/// already prints.
fn trusted_write(
    snapshot: &dyn Snapshot,
    state: Vec<u8>,
    verb: &str,
) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let state = if snapshot.resume_trusted() {
        Some(state)
    } else {
        eprintln!(
            "slot: {verb}: the core refused the resume it was given, not overwriting the saved one"
        );
        None
    };
    let sav = snapshot.save_ram();
    let sav = if snapshot.save_ram_trusted() {
        sav
    } else {
        if sav.is_some() {
            eprintln!(
                "slot: {verb}: the core refused the save ram it was given, not overwriting the saved one"
            );
        }
        None
    };
    (state, sav)
}

fn up(level: u8, step: u8, max: u8) -> u8 {
    level.saturating_add(step).min(max)
}

/// The host's own clock, which is all there is before `set_power` hands over the device's.
fn system_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The entry the next push will evict, read out while it is still there. `None` until the
/// ring is full, which is where most of a cart's life is spent.
fn doomed(ring: &StateRing) -> Option<(String, Vec<u8>, Vec<u8>)> {
    let entries = ring.list().ok()?;
    let oldest = entries.get(RING_MAX - 1)?;
    let (state, thumb) = ring.read(&oldest.stamp).ok()?;
    Some((oldest.stamp.clone(), state, thumb))
}

/// The stamp is the filename, so two saves inside one second would be one save. The second
/// one moves on by a second, which keeps the ring in order without a finer format that the
/// polaroid captions would then have to read.
fn free_stamp(ring: &StateRing, now: i64) -> String {
    let taken: Vec<String> = ring
        .list()
        .map(|l| l.into_iter().map(|e| e.stamp).collect())
        .unwrap_or_default();
    // Local, from the same wall clock the captions are read against. A stamp in utc would
    // name every state an hour or several from the time the polaroid says it was taken.
    let mut secs = now;
    let mut stamp = format_stamp(secs);
    while taken.contains(&stamp) {
        secs += 1;
        stamp = format_stamp(secs);
    }
    stamp
}

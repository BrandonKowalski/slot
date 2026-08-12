mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use slot::app::Phase;
use slot::session::Session;
use slot_input::{Btn, Millis, RawEvent};
use slot_store::{read_slot_state, StateRing};

/// One simulated present. The gesture clock and the animation clock are the same clock in
/// the binary, so they advance together here too.
const FRAME_MS: Millis = 16;
const DT: f32 = 1.0 / 60.0;

struct Pass {
    session: Session,
    root: PathBuf,
    now: Millis,
}

impl Pass {
    fn boot(root: &Path) -> Self {
        Pass {
            session: Session::boot(root.to_path_buf()),
            root: root.to_path_buf(),
            now: 0,
        }
    }

    fn step(&mut self) {
        self.now += FRAME_MS;
        self.session.feed([], self.now);
        self.session.update(DT);
    }

    fn event(&mut self, ev: RawEvent) {
        self.now += FRAME_MS;
        self.session.feed([ev], self.now);
        self.session.update(DT);
    }

    fn tap(&mut self, b: Btn) {
        self.event(RawEvent::Down(b));
        self.event(RawEvent::Up(b));
    }

    /// SELECT plus a face button inside the 120 ms chord window.
    fn chord(&mut self, b: Btn) {
        self.event(RawEvent::Down(Btn::Select));
        self.event(RawEvent::Down(b));
        self.event(RawEvent::Up(b));
        self.event(RawEvent::Up(Btn::Select));
    }

    /// Steps until the condition holds, sleeping so the emulator thread makes progress.
    fn until(&mut self, what: &str, cond: impl Fn(&Session) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !cond(&self.session) {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            self.step();
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn playing(&self) -> Option<&str> {
        match self.session.app().phase() {
            Phase::Playing { cart } => Some(cart),
            _ => None,
        }
    }

    fn frame(&self) -> Option<Vec<u8>> {
        self.session.frame().map(|f| f.to_vec())
    }

    /// The single most direct evidence that a game is running: the picture keeps changing.
    fn expect_running(&mut self) {
        self.until("a first frame", |s| s.frame().is_some());
        let held = self.frame();
        self.until("the picture to move", move |s| {
            s.frame().map(|f| f.to_vec()) != held
        });
    }

    fn ring(&self, stem: &str) -> StateRing {
        StateRing::new(&self.root, stem)
    }
}

/// The pass the plan asks for, in order, against whichever core is vendored: boot to the
/// shelf, flick, insert, play, adjust the three levels, save a state, fast forward, rewind,
/// open the switcher, load, eject, insert again, sleep, and reboot onto the seated cart.
///
/// It is one test rather than a dozen because a libretro core lives in dylib globals, so two
/// sessions must never be alive at the same moment.
#[test]
fn the_whole_pass_from_boot_to_resume() {
    // The search that finds the vendored core runs from the workspace root, and a test does
    // not. Naming it is what keeps this pass against mGBA rather than against the mock.
    if let Some(dylib) = common::vendored_core() {
        std::env::set_var("SLOT_CORE", dylib);
    }
    let d = common::tmp_root_with_real_carts(&["Advance Wars", "Emerald"]);
    let root = d.path();
    let mut p = Pass::boot(root);

    // A card that has never held slot. is asked for the clock, once, before anything else.
    assert!(matches!(p.session.app().phase(), Phase::SetClock { .. }));
    p.tap(Btn::A);

    // Boot with an empty slot is the shelf, and nothing is running behind it.
    assert!(matches!(p.session.app().phase(), Phase::Shelf));
    assert!(!p.session.has_core());

    // Flick to the second cart and put it in. Which cart arrives is how the flick is read.
    p.tap(Btn::Right);
    p.tap(Btn::A);
    assert!(matches!(p.session.app().phase(), Phase::Inserting { .. }));
    p.until("the cart to seat", |s| {
        matches!(s.app().phase(), Phase::Playing { .. })
    });
    assert_eq!(p.playing(), Some("Emerald"));
    assert_eq!(read_slot_state(root).cart.as_deref(), Some("Emerald"));
    p.expect_running();

    // The three levels, none of which stops the game, all of which reach the card.
    p.chord(Btn::Up); // brightness
    p.chord(Btn::Right); // blue light
    p.tap(Btn::VolUp);
    let levels = read_slot_state(root);
    assert_eq!(levels.brightness, 6);
    assert_eq!(levels.blue_light, 1);
    assert_eq!(levels.volume, 65);
    assert_eq!(p.session.app().blue_light(), 1);
    assert_eq!(
        p.playing(),
        Some("Emerald"),
        "a level adjustment paused the game"
    );

    // SELECT+R1, the only thing that writes to the ring.
    p.chord(Btn::R1);
    let saved = p.ring("Emerald").list().expect("list the ring");
    assert_eq!(saved.len(), 1);
    assert!(
        !std::fs::read(&saved[0].thumb).expect("thumb").is_empty(),
        "the polaroid has no picture"
    );

    // Fast forward and rewind, held and released. Neither may leave the game wedged.
    p.event(RawEvent::Down(Btn::R2));
    p.expect_running();
    p.event(RawEvent::Up(Btn::R2));
    p.event(RawEvent::Down(Btn::L2));
    for _ in 0..10 {
        p.step();
        std::thread::sleep(Duration::from_millis(2));
    }
    p.event(RawEvent::Up(Btn::L2));
    p.expect_running();

    // MENU twice opens the switcher, which pauses the game; A loads and closes it.
    p.tap(Btn::Menu);
    p.event(RawEvent::Down(Btn::Menu));
    assert!(matches!(p.session.app().phase(), Phase::Polaroids { .. }));
    assert_eq!(p.session.app().polaroid_entries().len(), 1);
    p.event(RawEvent::Up(Btn::Menu));
    p.tap(Btn::A);
    assert_eq!(p.playing(), Some("Emerald"));
    p.expect_running(); // the switcher paused it, so this is also the unpause

    // MENU held for two seconds. The state is on the card before the cart is out.
    p.event(RawEvent::Down(Btn::Menu));
    p.until("the eject", |s| {
        !matches!(s.app().phase(), Phase::Playing { .. })
    });
    p.event(RawEvent::Up(Btn::Menu));
    assert!(p
        .ring("Emerald")
        .read_resume()
        .expect("read resume")
        .is_some());
    assert_eq!(read_slot_state(root).cart, None);
    p.until("the shelf", |s| matches!(s.app().phase(), Phase::Shelf));
    assert!(!p.session.has_core(), "the core outlived the cart");

    // Straight back in, which is also the second core this process has opened.
    p.tap(Btn::A);
    p.until("the cart to seat again", |s| {
        matches!(s.app().phase(), Phase::Playing { .. })
    });
    assert_eq!(p.playing(), Some("Emerald"));

    // POWER dozes and flushes; a second press wakes back into the game.
    p.tap(Btn::Power);
    assert!(matches!(p.session.app().phase(), Phase::Doze { .. }));
    assert!(!p.session.app().powering_off());
    p.tap(Btn::Power);
    assert_eq!(p.playing(), Some("Emerald"));

    // Reboot. The cart never left the slot, so the shelf is not what comes back.
    p.tap(Btn::Power);
    drop(p);
    let mut p = Pass::boot(root);
    assert!(
        matches!(p.session.app().phase(), Phase::Inserting { .. }),
        "boot showed the shelf with a cart still in the slot"
    );
    p.until("the resumed cart", |s| {
        matches!(s.app().phase(), Phase::Playing { .. })
    });
    assert_eq!(p.playing(), Some("Emerald"));
    p.expect_running();
}

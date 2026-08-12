mod common;

use std::time::{Duration, Instant};

use slot::app::Phase;
use slot::session::Session;
use slot_input::{Btn, RawEvent};

const FRAME_MS: u64 = 16;

struct Pass {
    session: Session,
    now: u64,
}

impl Pass {
    fn step(&mut self) {
        self.now += FRAME_MS;
        self.session.feed([], self.now);
        self.session.update(0.016);
    }

    fn tap(&mut self, b: Btn) {
        for ev in [RawEvent::Down(b), RawEvent::Up(b)] {
            self.now += FRAME_MS;
            self.session.feed([ev], self.now);
            self.session.update(0.016);
        }
    }

    fn until(&mut self, what: &str, cond: impl Fn(&Session) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !cond(&self.session) {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            self.step();
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

/// The compositor keeps whatever was last uploaded to the game texture. Gating the game
/// layer on "a core exists" draws the previous cart's final frame for the length of the
/// insert, because the handle is built well before its worker publishes anything.
#[test]
fn the_game_layer_stays_hidden_until_the_new_core_has_published() {
    let d = common::tmp_root_with_real_carts(&["Emerald", "Fusion"]);
    common::clocked(d.path());
    let mut p = Pass {
        session: Session::boot(d.path().to_path_buf()),
        now: 0,
    };
    assert!(!p.session.game_visible(), "the slot is empty at boot");

    p.tap(Btn::A);
    assert!(
        !p.session.game_visible(),
        "the game layer must not draw on the frame the core was spawned"
    );
    p.until("the first frame", |s| s.game_visible());
    // A core publishes its first frame partway through the insert, and MENU held before the
    // cart has seated is not an eject. Waiting for the phase keeps what follows about the
    // game layer rather than about how fast the core happened to start.
    p.until("the cart to seat", |s| {
        matches!(s.app().phase(), Phase::Playing { .. })
    });

    // Eject, then seat the other cart. This is the swap that showed the stale frame.
    p.session
        .feed([RawEvent::Down(Btn::Menu)], p.now + FRAME_MS);
    p.until("the eject", |s| !s.has_core());
    p.session.feed([RawEvent::Up(Btn::Menu)], p.now + FRAME_MS);
    assert!(
        !p.session.game_visible(),
        "an empty slot must not keep drawing the cart that just left"
    );

    p.until("the shelf", |s| !s.has_core());
    p.tap(Btn::Right);
    p.tap(Btn::A);
    assert!(
        !p.session.game_visible(),
        "the second cart must not show the first cart's last frame"
    );
    p.until("the second core", |s| s.game_visible());
}

/// `Frames::latest` consumes. Anything that calls it to ask a question, rather than to
/// draw, throws away a frame the renderer would have shown, and always the newest one. The
/// result is a picture that lags while audio stays perfect, because audio is a separate
/// path.
///
/// Paused deliberately: a running core republishes within a frame or two and hides the
/// theft, which is exactly why the first version of this test passed against the bug.
#[test]
fn nothing_but_the_renderer_takes_a_frame() {
    let d = common::tmp_root_with_real_carts(&["Emerald"]);
    common::clocked(d.path());
    let mut p = Pass {
        session: Session::boot(d.path().to_path_buf()),
        now: 0,
    };
    p.tap(Btn::A);
    p.until("the first frame", |s| s.game_visible());

    // Closing the lid dozes, which pauses the core. Nothing is published from here on, so a
    // frame that goes missing cannot be quietly replaced.
    // Down only. A tap would close the lid and reopen it in the same breath.
    p.now += FRAME_MS;
    p.session.feed([RawEvent::Down(Btn::Lid)], p.now);
    p.session.update(0.016);
    p.until("a frame waiting on a paused core", |s| s.frame_ready());
    let before = p.session.frames_taken();

    for _ in 0..30 {
        p.now += FRAME_MS;
        p.session.feed([], p.now);
        p.session.update(0.016);
    }

    assert_eq!(
        p.session.frames_taken(),
        before,
        "update took {} frame(s) the renderer never saw",
        p.session.frames_taken() - before
    );
    assert!(
        p.session.frame_ready(),
        "the waiting frame was consumed by something other than the renderer"
    );
}

/// Loading a core and running one are different things. The insert animation is there to
/// hide the load, but a core running behind the cart plays the GBA bios intro where nobody
/// can see it, so the reveal catches only its tail.
#[test]
fn the_core_does_not_run_while_the_cart_is_going_in() {
    let d = common::tmp_root_with_real_carts(&["Emerald"]);
    let mut p = Pass {
        session: Session::boot(d.path().to_path_buf()),
        now: 0,
    };
    p.tap(Btn::A);
    p.until("the core to load", |s| s.has_core());

    // Give it well over a frame's worth of wall clock while the cart is still travelling.
    let mut ran = 0;
    for _ in 0..40 {
        if !matches!(p.session.app().phase(), Phase::Inserting { .. }) {
            break;
        }
        let before = p.session.frames_published();
        p.step();
        std::thread::sleep(Duration::from_millis(4));
        // The step itself can end the insert, and a core running after that is running
        // exactly when it should. Only frames produced while the cart is still travelling
        // at both ends of the step count against it.
        let still_inserting = matches!(p.session.app().phase(), Phase::Inserting { .. });
        if still_inserting && p.session.frames_published() > before {
            ran += 1;
        }
    }
    assert_eq!(
        ran, 0,
        "the core produced {ran} frames while the cart was still going in"
    );
}

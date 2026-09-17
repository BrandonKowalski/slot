//! POWER on a dozing device, through the real frontend: the frame composited on the GPU and
//! read back, with the backlight the device would actually write recorded beside it.
//!
//! Both halves are needed and neither is enough on its own. The panel is off during a doze
//! because `doze` writes `set_backlight(0)`, and no pixel can say that; the screen behind it is
//! the doze's own black, and no backlight reading can say that. The bug this pins was a
//! perfectly correct draw list rendered onto a panel nobody could see, so a draw-list assertion
//! would have passed straight through it.
//!
//! `SCRATCH_PNG_DIR=/tmp cargo test -p slot --test render_power_wake -- --nocapture`

#![cfg(target_os = "macos")]

mod common;

use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{clocked, tmp_root_with_carts};
use slot::frontend::Frontend;
use slot_gfx::{Compositor, HeadlessSurface, OUT_H, OUT_W};
use slot_input::{Btn, InputSource, Millis, RawEvent, POWER_HOLD_MS};
use slot_power::{Battery, Charge, LedState, Platform, SimPlatform};

/// One batch of events per poll, and nothing once they run out.
struct Script(VecDeque<Vec<RawEvent>>);

impl InputSource for Script {
    fn poll(&mut self, _now: Millis) -> Vec<RawEvent> {
        self.0.pop_front().unwrap_or_default()
    }
}

/// `SimPlatform` with the one write this file is about kept rather than dropped. A Mac has no
/// panel the frontend may drive, so the host platform throws `set_backlight` away — and that is
/// precisely the value that decides whether the frame the compositor just drew is one anybody
/// could have seen.
struct RecordingPanel {
    inner: SimPlatform,
    backlight: Arc<AtomicU8>,
}

impl Platform for RecordingPanel {
    fn set_backlight(&mut self, step: u8) {
        self.backlight.store(step, Ordering::Relaxed);
    }

    fn battery(&self) -> Option<Battery> {
        self.inner.battery()
    }

    fn charge(&self) -> Charge {
        self.inner.charge()
    }

    fn set_led(&mut self, state: LedState) {
        self.inner.set_led(state);
    }

    fn poweroff(&mut self) -> ! {
        self.inner.poweroff()
    }

    fn restart(&mut self) -> ! {
        self.inner.restart()
    }

    fn root(&self) -> &Path {
        self.inner.root()
    }

    fn now(&self) -> i64 {
        self.inner.now()
    }

    fn set_clock(&mut self, secs: i64) {
        self.inner.set_clock(secs);
    }

    fn set_rumble(&mut self, strength: u16) {
        self.inner.set_rumble(strength);
    }
}

fn at(px: &[u8], x: usize, y: usize) -> [u8; 3] {
    let o = (y * OUT_W as usize + x) * 4;
    [px[o], px[o + 1], px[o + 2]]
}

/// Whether the whole panel is the flat black `Phase::Doze` paints. Sampled on a lattice rather
/// than every pixel, but across the whole frame: what separates a doze from every other screen
/// in the tree is that nothing at all is drawn over it.
fn all_black(px: &[u8]) -> bool {
    (0..OUT_H as usize).step_by(7).all(|y| {
        (0..OUT_W as usize)
            .step_by(7)
            .all(|x| at(px, x, y) == [0; 3])
    })
}

/// Whether anything on the panel is bright enough to read. The menu's own ground is nearly as
/// dark as the doze it replaced, so "not black" is a weaker claim than it sounds: what says a
/// screen is there to be looked at is its type.
fn any_ink(px: &[u8]) -> bool {
    (0..OUT_H as usize).step_by(3).any(|y| {
        (0..OUT_W as usize)
            .step_by(3)
            .any(|x| at(px, x, y)[0] > 0x80)
    })
}

fn composed(f: &mut Frontend, c: &mut Compositor, name: &str) -> Vec<u8> {
    f.compose(c);
    let px = c.read_frame();
    if let Ok(dir) = std::env::var("SCRATCH_PNG_DIR") {
        let path = format!("{dir}/power-wake-{name}.png");
        let file = std::fs::File::create(&path).expect("create png");
        let mut e = png::Encoder::new(std::io::BufWriter::new(file), OUT_W, OUT_H);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header()
            .expect("png header")
            .write_image_data(&px)
            .expect("png data");
        println!("wrote {path}");
    }
    px
}

/// A tap: down on one frame, up on the next.
fn tap(f: &mut Frontend, input: &mut Script, btn: Btn) {
    input.0.push_back(vec![RawEvent::Down(btn)]);
    f.advance(input);
    input.0.push_back(vec![RawEvent::Up(btn)]);
    f.advance(input);
}

/// The reported bug, on the panel. Hold POWER while the device is dozing and the power menu was
/// raised onto a screen the backlight had been taken away from: no feedback of any kind, so the
/// user keeps holding and at six seconds the PMIC cuts the rails — the ungraceful stop
/// `POWER_HOLD_MS` exists to get in front of.
///
/// `Frontend::advance` runs off the wall clock, so the hold is a real second. That is the price
/// of driving the same loop the device runs rather than a stand-in for it.
#[test]
fn power_on_a_dozing_device_brings_the_screen_back_before_the_menu() {
    let Ok(surface) = HeadlessSurface::new() else {
        return;
    };
    let Ok(mut c) = Compositor::new(&surface) else {
        return;
    };
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    clocked(d.path());
    let backlight = Arc::new(AtomicU8::new(0));
    let mut f = Frontend::boot(Box::new(RecordingPanel {
        inner: SimPlatform::at(d.path().to_path_buf()),
        backlight: backlight.clone(),
    }));
    f.upload_faces(&mut c);
    let mut input = Script(VecDeque::new());
    f.advance(&mut input);

    let shelf = composed(&mut f, &mut c, "shelf");
    let lit = backlight.load(Ordering::Relaxed);
    assert!(lit > 0, "the panel never came on");
    assert!(
        any_ink(&shelf),
        "the shelf drew nothing, so going dark proves nothing"
    );

    // A tap of POWER is one of the two things that puts it out.
    tap(&mut f, &mut input, Btn::Power);
    let dozing = composed(&mut f, &mut c, "doze");
    assert_eq!(
        backlight.load(Ordering::Relaxed),
        0,
        "the doze left the panel lit"
    );
    assert!(all_black(&dozing), "the doze left something on the screen");

    // And POWER pressed again. The press, held: the screen has to come back before the menu
    // does, not a second after it.
    input.0.push_back(vec![RawEvent::Down(Btn::Power)]);
    f.advance(&mut input);
    let woken = composed(&mut f, &mut c, "woken");
    assert_eq!(
        backlight.load(Ordering::Relaxed),
        lit,
        "the panel is still dark under the thumb trying to wake it"
    );
    assert!(
        !all_black(&woken) && any_ink(&woken),
        "the screen is still the doze's own black"
    );

    // The same press, held on past the threshold, with the thumb never leaving the button.
    let until = Instant::now() + Duration::from_millis(POWER_HOLD_MS + 200);
    while Instant::now() < until {
        f.advance(&mut input);
        std::thread::sleep(Duration::from_millis(8));
    }
    let menu = composed(&mut f, &mut c, "menu");
    assert_eq!(
        backlight.load(Ordering::Relaxed),
        lit,
        "the menu is up on a panel nobody can see"
    );
    assert!(
        any_ink(&menu),
        "the menu's own rows are not on the panel to be read"
    );
    assert_ne!(
        menu, woken,
        "the hold never raised the menu over what was underneath it"
    );
}

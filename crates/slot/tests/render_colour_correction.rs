//! The quick menu's Colour Correction against the real core: the same cart, the same number of
//! emulated frames, once with the row off and once with it on, and the two pictures compared.
//!
//! The one question a row like this has to answer is whether anyone can see it. A draw list
//! cannot answer that and neither can an option read back out of the frontend's own map — the
//! core would accept `Autp` just as quietly as `Auto` and simply never tint anything. So this
//! runs the machine and looks at the pixels it drew.
//!
//! Deliberately deterministic: both runs load the same ROM, press nothing, and run exactly the
//! same number of frames, so every pixel that differs between them differs because of the
//! option and not because one run got further into an animation than the other. That is what
//! lets the difference be measured rather than merely eyeballed.
//!
//! `SCRATCH_PNG_DIR=/tmp cargo test -p slot --test render_colour_correction -- --nocapture`
//!
//! Skipped on a machine with no mGBA dylib and no card to take a cart off, the same way every
//! other test here that needs a real core is.

mod common;

use std::path::{Path, PathBuf};

use common::{core_lock, repo_root, vendored_core};
use slot_retro::{ButtonMask, GBA_H, GBA_W};
use slot_store::Core;

/// How many frames each cart is run before its picture is read. Per cart rather than shared,
/// because how long a cart spends on black before it draws anything is the cart's own business
/// — and a picture read too early is black, which no correction can tint. Both runs of a cart
/// use its own number, which is what keeps the pair the same instant of the same game.
fn frames_for(name: &str) -> usize {
    match name {
        // Metroid Fusion is on its intro's starfield well before this.
        "gba" => 240,
        // Pokémon Crystal holds black through its boot and the Game Freak logo's lead-in; at
        // 240 frames the picture is still empty, which is how this number was arrived at.
        _ => 900,
    }
}

/// The core's framebuffer is little endian XRGB8888, which on the wire is B, G, R, unused.
fn to_rgba(xrgb: &[u8]) -> Vec<u8> {
    xrgb.chunks_exact(4)
        .flat_map(|p| [p[2], p[1], p[0], 0xff])
        .collect()
}

fn write_png(name: &str, w: u32, h: u32, rgba: &[u8]) {
    let Ok(dir) = std::env::var("SCRATCH_PNG_DIR") else {
        return;
    };
    let path = format!("{dir}/{name}.png");
    let file = std::fs::File::create(&path).expect("create png");
    let mut e = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header()
        .expect("png header")
        .write_image_data(rgba)
        .expect("png data");
    println!("wrote {path}");
}

/// The two pictures blown up side by side with a rule between them, which is the only way to
/// judge a tint: a colour cast is invisible on its own and obvious beside the same frame
/// without it.
fn side_by_side(left: &[u8], right: &[u8], n: usize) -> (u32, u32, Vec<u8>) {
    let (w, h) = (GBA_W as usize, GBA_H as usize);
    let gap = 8;
    let out_w = w * n * 2 + gap;
    let out_h = h * n;
    let mut out = vec![0u8; out_w * out_h * 4];
    for y in 0..out_h {
        for x in 0..out_w {
            let (src, sx) = match x < w * n {
                true => (left, x / n),
                false if x < w * n + gap => {
                    let o = (y * out_w + x) * 4;
                    out[o..o + 4].copy_from_slice(&[0x60, 0x60, 0x60, 0xff]);
                    continue;
                }
                false => (right, (x - w * n - gap) / n),
            };
            let from = ((y / n) * w + sx) * 4;
            let o = (y * out_w + x) * 4;
            out[o..o + 4].copy_from_slice(&src[from..from + 4]);
        }
    }
    (out_w as u32, out_h as u32, out)
}

/// The mean of each channel across the whole picture, which is what a tint moves.
fn mean_rgb(rgba: &[u8]) -> [f64; 3] {
    let n = (rgba.len() / 4) as f64;
    let mut sum = [0f64; 3];
    for p in rgba.chunks_exact(4) {
        for (c, s) in sum.iter_mut().enumerate() {
            *s += f64::from(p[c]);
        }
    }
    sum.map(|s| s / n)
}

/// How much colour the picture has: the mean, over every pixel, of how far its channels spread
/// apart. Grey is zero and a saturated hue is high.
fn mean_saturation(rgba: &[u8]) -> f64 {
    let n = (rgba.len() / 4) as f64;
    let sum: f64 = rgba
        .chunks_exact(4)
        .map(|p| {
            let (hi, lo) = (p[..3].iter().max(), p[..3].iter().min());
            f64::from(hi.copied().unwrap_or(0) - lo.copied().unwrap_or(0))
        })
        .sum();
    sum / n
}

/// How far apart two pictures are, as a share of the pixels that are not identical. A tint
/// touches almost everything that is not already black; an animation one run got further into
/// touches only what moved.
fn share_changed(a: &[u8], b: &[u8]) -> f64 {
    let n = a.len() / 4;
    let diff = a
        .chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(p, q)| p[..3] != q[..3])
        .count();
    diff as f64 / n as f64
}

/// One run of the machine: a core opened through slot's own `open_core_for`, so the option is
/// applied by the same function production uses and not by the test reaching past it.
fn picture(root: &Path, dylib: &Path, rom: &Path, colour: bool, frames: usize) -> Vec<u8> {
    let mut core = slot::core::open_core_for(
        root,
        Core::Mgba,
        "auto",
        colour,
        std::slice::from_ref(&dylib.to_path_buf()),
    );
    core.load(rom).expect("the core would not take the rom");
    for _ in 0..frames {
        core.run_frame(ButtonMask(0));
    }
    to_rgba(core.video_xrgb8888())
}

/// A cart of the user's own, copied out of the ignored `/sdcard` into the root the test runs
/// from. Read only: nothing here writes to the card. `None` on a clone that has no card, which
/// is a skip rather than a failure — a stand-in rom paints nothing worth tinting.
fn card_cart(root: &Path, from: &str, to: &str) -> Option<PathBuf> {
    let rom = std::fs::read(repo_root().join(from)).ok()?;
    let at = root.join(to);
    std::fs::write(&at, rom).expect("copy the card's cart");
    Some(at)
}

/// The card's own setting, carried all the way to the picture by the real `Session`.
///
/// The test below opens the core through `open_core_for` directly, which proves the option
/// works but says nothing about whether anything ever hands it the user's answer:
/// `session.rs`'s `open_core(&self.root, core, serial, self.app.colour_correction())` is the
/// one production line that does, and pinning that literal to `false` would leave every test
/// above passing and the row doing nothing. This is what fails in that case.
///
/// Read out of the session's own published frames rather than off a composited panel, and
/// compared on saturation rather than pixel for pixel: two sessions run on wall clock do not
/// land on the same emulated frame, and the frames either side of one differ by far less than
/// a correction does — the measured drop here is about a third of the picture's colour, while
/// a frame of the same intro drifting is a fraction of a percent.
#[test]
fn the_cards_setting_reaches_the_core_through_the_session() {
    use slot::app::Phase;
    use slot::session::Session;
    use slot_input::{Btn, RawEvent};
    use slot_store::{write_slot_state, SlotState};
    use std::time::{Duration, Instant};

    let Some(dylib) = vendored_core() else {
        eprintln!("no mgba dylib, skipping");
        return;
    };
    let _g = core_lock();

    let mut sat = Vec::new();
    for colour in [false, true] {
        let d = common::tmp_root_with_carts(&[]);
        let Some(_) = card_cart(
            d.path(),
            "sdcard/Games/GBA/Metroid Fusion.gba",
            "Games/GBA/Metroid Fusion.gba",
        ) else {
            eprintln!("no GBA cart on this machine's card, skipping");
            return;
        };
        // The content root's own `System/` is the first place `candidates` looks, which is how
        // a test plants a core somewhere the real search will find it.
        std::fs::copy(&dylib, d.path().join("System/mgba_libretro.dylib")).expect("plant a core");
        let state = SlotState {
            clock_set: true,
            colour_correction: colour,
            ..SlotState::default()
        };
        write_slot_state(d.path(), &state).expect("write slot.state");

        let mut s = Session::boot(d.path().to_path_buf());
        s.feed([RawEvent::Down(Btn::A)], 16);
        s.feed([RawEvent::Up(Btn::A)], 32);
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut now = 32;
        // The same published frame count for both runs, which is as close to the same instant
        // as a session driven on real time gets.
        while !matches!(s.app().phase(), Phase::Playing { .. }) || s.frames_published() < 240 {
            assert!(
                Instant::now() < deadline,
                "the cart never got playing: {:?}",
                s.app().phase()
            );
            now += 16;
            s.feed([], now);
            s.update(1.0 / 60.0);
            std::thread::sleep(Duration::from_millis(1));
        }
        // A published frame is only handed out once, so this loops until one is waiting rather
        // than assuming the last `update` left one there.
        let picture = loop {
            assert!(Instant::now() < deadline, "no frame was ever published");
            if let Some(f) = s.frame() {
                break to_rgba(&f);
            }
            now += 16;
            s.feed([], now);
            s.update(1.0 / 60.0);
            std::thread::sleep(Duration::from_millis(1));
        };
        write_png(
            if colour { "session-on" } else { "session-off" },
            GBA_W,
            GBA_H,
            &picture,
        );
        println!(
            "session colour={colour}: mean {:.1?} sat {:.1}",
            mean_rgb(&picture),
            mean_saturation(&picture)
        );
        sat.push(mean_saturation(&picture));
        // Dropped before the next boot: libretro keeps one machine per process, and the handle's
        // own `Drop` is what joins the worker that owns it.
        drop(s);
    }
    let [off, on] = sat[..] else {
        unreachable!("two runs");
    };
    assert!(
        on < off * 0.9,
        "the card's colour correction never reached the core: saturation {off:.1} with it off \
         against {on:.1} with it on"
    );
}

/// Both consoles, because `Auto` is the whole reason the row says On rather than naming a
/// correction: mGBA is asked to pick the right tint per cart, and a value that only moved the
/// GBA picture would leave every Game Boy Color cart looking exactly as it did.
#[test]
fn colour_correction_changes_the_picture_on_both_consoles() {
    let Some(dylib) = vendored_core() else {
        eprintln!("no mgba dylib, skipping");
        return;
    };
    let _g = core_lock();
    let d = common::tmp_root_with_carts(&[]);

    for (name, from, to) in [
        (
            "gba",
            "sdcard/Games/GBA/Metroid Fusion.gba",
            "Games/GBA/Metroid Fusion.gba",
        ),
        (
            "gbc",
            "sdcard/Games/GBC/Pokemon - Crystal Version (USA).gbc",
            "Games/GBC/Pokemon - Crystal Version (USA).gbc",
        ),
    ] {
        let Some(rom) = card_cart(d.path(), from, to) else {
            eprintln!("no {name} cart on this machine's card, skipping it");
            continue;
        };
        // One core at a time: libretro keeps its machine in dylib globals, so the first has to
        // be dropped before the second opens.
        let frames = frames_for(name);
        let off = picture(d.path(), &dylib, &rom, false, frames);
        let on = picture(d.path(), &dylib, &rom, true, frames);
        // A picture that is still black has nothing to tint, so a test reading one would prove
        // the option does nothing rather than that it does. Said out loud because that is a
        // failure of the fixture, not of the row.
        assert!(
            mean_rgb(&off).iter().sum::<f64>() > 12.0,
            "{name}: still black after {frames} frames, so there is no picture to correct"
        );

        write_png(&format!("colour-{name}-off"), GBA_W, GBA_H, &off);
        write_png(&format!("colour-{name}-on"), GBA_W, GBA_H, &on);
        let (w, h, both) = side_by_side(&off, &on, 3);
        write_png(&format!("colour-{name}-side-by-side"), w, h, &both);

        let (a, b) = (mean_rgb(&off), mean_rgb(&on));
        let (sat_off, sat_on) = (mean_saturation(&off), mean_saturation(&on));
        println!(
            "{name}: off mean {a:.1?} sat {sat_off:.1}  on mean {b:.1?} sat {sat_on:.1}  \
             changed {:.1}%",
            share_changed(&off, &on) * 100.0
        );

        // The same cart run the same number of frames with nothing pressed, so the two are the
        // same instant of the same game and the only thing between them is the option.
        assert_ne!(
            off, on,
            "{name}: the picture is byte for byte identical with correction on and off, so \
             either the option never reached the core or it does nothing worth a row"
        );
        // A tint is a change to almost everything, not to a corner of the screen. Half the
        // picture is a low bar for a colour cast and far above anything a stray pixel could
        // reach, which is what keeps this from passing on a difference nobody could see.
        let changed = share_changed(&off, &on);
        assert!(
            changed > 0.5,
            "{name}: only {:.1}% of the picture changed, which is not a tint",
            changed * 100.0
        );
        // And it took colour out, which is the one thing the two corrections have in common.
        // Brightness is not: the GBA's correction darkens the picture (mean 148 to 76 on this
        // frame) while the GBC's lifts it (133 to 173), so a test that asserted "darker" would
        // be right about one console and wrong about the other. What both do is wash the
        // picture out, which is the whole of what the row promises.
        assert!(
            sat_on < sat_off,
            "{name}: correction did not wash the picture out: saturation {sat_off:.1} to \
             {sat_on:.1}"
        );
    }
}

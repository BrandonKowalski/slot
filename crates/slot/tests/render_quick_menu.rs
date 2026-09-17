//! The quick menu through the real frontend: faces uploaded at boot, MENU pressed through the
//! gesture layer, and the frame composited on the GPU and read back.
//!
//! `SCRATCH_PNG_DIR=/tmp cargo test -p slot --test render_quick_menu -- --nocapture`

#![cfg(target_os = "macos")]

mod common;

use std::collections::VecDeque;

use common::{clocked, tmp_root_with_carts};
use slot::frontend::Frontend;
use slot_gfx::{Compositor, HeadlessSurface, OUT_H, OUT_W};
use slot_input::{Btn, InputSource, Millis, RawEvent};
use slot_power::SimPlatform;
use slot_ui::{QuickRow, QUICK_PITCH, QUICK_TOP};

/// One batch of events per poll, and nothing once they run out.
struct Script(VecDeque<Vec<RawEvent>>);

impl InputSource for Script {
    fn poll(&mut self, _now: Millis) -> Vec<RawEvent> {
        self.0.pop_front().unwrap_or_default()
    }
}

/// A tap: down on one frame, up on the next.
fn tap(f: &mut Frontend, input: &mut Script, btn: Btn) {
    input.0.push_back(vec![RawEvent::Down(btn)]);
    f.advance(input);
    input.0.push_back(vec![RawEvent::Up(btn)]);
    f.advance(input);
}

fn at(px: &[u8], x: usize, y: usize) -> [u8; 3] {
    let o = (y * OUT_W as usize + x) * 4;
    [px[o], px[o + 1], px[o + 2]]
}

/// The columns in `xs` with type in them anywhere across a row's bar: brighter than both the
/// ground and the bar, which grey values are as well.
fn inked(px: &[u8], xs: std::ops::Range<usize>, top: usize) -> Vec<usize> {
    xs.filter(|&x| (top + 8..top + 44).any(|y| at(px, x, y)[0] > 0x80))
        .collect()
}

fn composed(f: &mut Frontend, c: &mut Compositor, name: &str) -> Vec<u8> {
    f.compose(c);
    let px = c.read_frame();
    if let Ok(dir) = std::env::var("SCRATCH_PNG_DIR") {
        let path = format!("{dir}/quick-menu-{name}.png");
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

#[test]
fn the_quick_menu_renders_full_screen() {
    let Ok(surface) = HeadlessSurface::new() else {
        return;
    };
    let Ok(mut c) = Compositor::new(&surface) else {
        return;
    };
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    clocked(d.path());
    let mut f = Frontend::boot(Box::new(SimPlatform::at(d.path().to_path_buf())));
    f.upload_faces(&mut c);
    let mut input = Script(VecDeque::new());
    tap(&mut f, &mut input, Btn::Menu);

    let bar = [0x4d, 0x4d, 0x57];
    let ground = [0x05, 0x05, 0x08];
    // Walked down from wherever the bar was left, counted off the rows' own indices rather than
    // written out: a row added to the menu then moves the bar the right distance on its own
    // instead of leaving this quietly one press short of where it says it is.
    let mut bar_on = QuickRow::ALL[0];
    for (name, selected) in [
        ("fast-forward", QuickRow::FastForward),
        ("colour-correction", QuickRow::ColourCorrection),
        ("date-time", QuickRow::DateTime),
        ("about", QuickRow::About),
    ] {
        for _ in bar_on.index()..selected.index() {
            tap(&mut f, &mut input, Btn::Down);
        }
        bar_on = selected;
        let px = composed(&mut f, &mut c, name);

        let top = (QUICK_TOP + QUICK_PITCH * selected.index() as f32) as usize;
        for x in [1, OUT_W as usize - 2] {
            assert_eq!(at(&px, x, top + 26), bar, "{name}: no bar at x {x}");
        }
        // Edge to edge means unbroken all the way across, which is a stronger claim than the two
        // ends and the panel's centre — and a truer one, now that Colour Correction's label is
        // long enough to have type sitting on that centre. Ink over the bar is not a gap in it,
        // and every ink here is lighter than the bar, so only the ground would be a real break.
        assert!(
            (0..OUT_W as usize).all(|x| at(&px, x, top + 26) != ground),
            "{name}: the bar breaks somewhere across the row"
        );
        assert_eq!(
            at(&px, 360, top + 1),
            ground,
            "{name}: the bar is not inset"
        );
        assert_eq!(
            at(&px, 2, OUT_H as usize - 2),
            ground,
            "{name}: not on the ground"
        );

        // Labels start 32 px in and values end 32 px from the right, on every row, measured the
        // way the mockup's type is placed: by where its line starts and ends, not by its ink.
        // The type's own side bearings are the only slack. A capital's stem stands a pixel or
        // three inside its line, and a tabular 1, which the clock can end on, stands about seven
        // inside its own advance.
        for row in QuickRow::ALL {
            let top = (QUICK_TOP + QUICK_PITCH * row.index() as f32) as usize;
            let label = inked(&px, 0..360, top);
            let first = *label.first().expect("a row with no label");
            assert!(
                (32..=36).contains(&first),
                "{name}: {row:?}'s label starts at x {first}"
            );
            if row == QuickRow::About {
                continue;
            }
            let value = inked(&px, 360..OUT_W as usize, top);
            let last = *value.last().expect("a row with no value");
            assert!(
                (679..=688).contains(&last),
                "{name}: {row:?}'s value ends at x {last}"
            );
        }
    }

    // Ruling S6: Date & Time opens the clock with B BACK beside its own key. Centred as a pair,
    // B's cap runs from about x 225 to 244. The first boot's lone key is centred on its own and
    // its cap starts near x 279, so only ink left of x 270 on this row can be B BACK.
    tap(&mut f, &mut input, Btn::Up);
    tap(&mut f, &mut input, Btn::A);
    let px = composed(&mut f, &mut c, "clock");
    assert!(
        (200..270).any(|x| at(&px, x, 298) == [0xf6, 0xf4, 0xef]),
        "the clock from the menu does not offer B BACK"
    );
}

/// Every value the Fast Forward row can show, each one rendered and read off the panel rather
/// than off the draw list. What matters is that the set type lands on the same right edge as
/// every other value, arrows and all, and still leaves the label its side of the row — and only
/// the rendered pixels can answer that. Walked end to end because a row whose values are all one
/// shape is exactly where a draw-list assertion would pass on a screen that was wrong.
#[test]
fn every_fast_forward_speed_sits_on_the_rows_right_edge_and_clears_the_label() {
    let Ok(surface) = HeadlessSurface::new() else {
        return;
    };
    let Ok(mut c) = Compositor::new(&surface) else {
        return;
    };
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    clocked(d.path());
    let mut f = Frontend::boot(Box::new(SimPlatform::at(d.path().to_path_buf())));
    f.upload_faces(&mut c);
    let mut input = Script(VecDeque::new());
    tap(&mut f, &mut input, Btn::Menu);
    // The menu opens on Fast Forward, showing the default 4×. Two presses left is the bottom of
    // the row, and from there each value is one press right of the last.
    tap(&mut f, &mut input, Btn::Left);
    tap(&mut f, &mut input, Btn::Left);

    let top = QUICK_TOP as usize;
    for name in ["2x", "3x", "4x", "6x", "8x"] {
        let px = composed(&mut f, &mut c, name);
        let value = inked(&px, 360..OUT_W as usize, top);
        let last = *value
            .last()
            .unwrap_or_else(|| panic!("{name}: the Fast Forward row has no value"));
        assert!(
            (679..=688).contains(&last),
            "{name} ends at x {last}, off the edge every other value keeps"
        );
        // The row in hand carries an arrow either side of its value, so nothing on it may be
        // inked across the middle of the row, where the label is heading.
        assert!(
            inked(&px, 350..370, top).is_empty(),
            "{name} and its arrows reach the middle of the row"
        );
        let label = inked(&px, 0..350, top);
        let first = *label
            .first()
            .unwrap_or_else(|| panic!("{name}: the Fast Forward row has no label"));
        assert!(
            (32..=36).contains(&first),
            "the label moved to x {first} to make room for {name}"
        );
        tap(&mut f, &mut input, Btn::Right);
    }
}

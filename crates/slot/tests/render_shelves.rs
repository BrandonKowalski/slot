//! The two shelves through the real frontend: the card scanned into shelves, faces uploaded at
//! boot, a shoulder pressed through the gesture layer, and the frame composited on the GPU and
//! read back. A draw list can say the right things about a screen that is empty; only the
//! rendered pixels can say the Game Boy shelf is a row of carts with a name over it.
//!
//! `SCRATCH_PNG_DIR=/tmp cargo test -p slot --test render_shelves -- --nocapture`

#![cfg(target_os = "macos")]

mod common;

use std::collections::VecDeque;
use std::path::Path;

use common::{clocked, repo_root, tmp_root_with_carts};
use slot::app::{App, SEATED_AT};
use slot::frontend::Frontend;
use slot_gfx::{Compositor, HeadlessSurface, OUT_H, OUT_W};
use slot_input::{Action, Btn, InputSource, Millis, RawEvent};
use slot_power::SimPlatform;
use slot_ui::{clean_label, label_colour, PLATE_H};

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

/// The average colour of a patch, which is what a cart's label reads as: the type printed across
/// it makes any single pixel a coin toss between the paper and a letter.
fn patch(px: &[u8], x: usize, y: usize) -> [u32; 3] {
    let mut sum = [0u32; 3];
    let mut n = 0;
    for py in y - 6..y + 6 {
        for qx in x - 20..x + 20 {
            let c = at(px, qx, py);
            for (k, v) in c.iter().enumerate() {
                sum[k] += *v as u32;
            }
            n += 1;
        }
    }
    [sum[0] / n, sum[1] / n, sum[2] / n]
}

/// How far apart two readings are, summed over the channels.
fn apart(a: [u32; 3], b: [u32; 3]) -> u32 {
    (0..3).map(|k| a[k].abs_diff(b[k])).sum()
}

/// Lit pixels across the middle of the top plate: the banner's own type, which is near white
/// where the plate behind it is dark and the backdrop darker still.
fn banner_ink(px: &[u8]) -> usize {
    (0..PLATE_H as usize)
        .flat_map(|y| (200..520).map(move |x| (x, y)))
        .filter(|(x, y)| at(px, *x, *y).iter().all(|c| *c > 0x80))
        .count()
}

fn composed(f: &mut Frontend, c: &mut Compositor, name: &str) -> Vec<u8> {
    f.compose(c);
    let px = c.read_frame();
    if let Ok(dir) = std::env::var("SCRATCH_PNG_DIR") {
        let path = format!("{dir}/shelves-{name}.png");
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

/// The card's own Game Boy carts, copied into the root the test boots from — the point of
/// rendering at all is to look at the real library. Read only: nothing here writes to the card.
/// A fresh clone has no `sdcard/`, so a cart of the right shape stands in and the test still runs.
fn put_game_boy_carts(root: &Path) {
    for (dir, stem, ext, cgb) in [
        ("GB", "Tetris Rosy Retrospection", "gb", 0x00u8),
        ("GBC", "Tetris Chromatic", "gbc", 0xc0),
    ] {
        let from = repo_root().join(format!("sdcard/Games/{dir}/{stem}.{ext}"));
        let to = root.join(format!("Games/{dir}/{stem}.{ext}"));
        match std::fs::read(&from) {
            Ok(rom) => std::fs::write(&to, rom).expect("copy the card's cart"),
            Err(_) => std::fs::write(&to, gb_rom(cgb)).expect("write a stand-in cart"),
        }
    }
}

/// 32 KiB with a Game Boy header in it. The CGB flag at 0x143 is the only byte the shelf reads
/// for itself; the name on the label comes from the filename, as it does for every cart.
fn gb_rom(cgb: u8) -> Vec<u8> {
    let mut rom = vec![0u8; 0x8000];
    rom[0x143] = cgb;
    rom
}

/// Where a cart lands, in screen pixels. A lone cart stands dead centre; a shelf of two is
/// centred as a pair, which puts the selection at x 141 to 338 and its neighbour out at 402 to
/// 557, lower and shorter for standing shrunk. Each of these three reads one layout and lands on
/// bare ground in the other, which is what makes them able to tell the two apart.
///
/// `ALONE` is only ever read on a Game Boy shelf, and it is the pak's bare plastic rather than
/// its label: the pak is 253 px tall against a GBA cart's 135, so it stands from y 55 to 308
/// with its label well up at y 87 to 230, and this sits below that on the moulded face.
const ALONE: (usize, usize) = (360, 250);
/// The middle of that same pak's label well. Read beside `ALONE` because neither reading can
/// tell a Game Boy shelf from a Colour one on its own any more:
///
/// - The plastic is 55 apart. A pak and a Colour pak are the same silhouette, and the two
///   shells are deliberately close in value — see `GB_CLEAR_SHELL`, which is cooler than the
///   grey pak beside it precisely because only the lit rim separates them otherwise.
/// - The paper is 59 apart. `label_colour` turns a title into a hue at one fixed saturation and
///   value, and this card's two Tetris titles happen to hash into the same sector of the wheel,
///   which leaves them differing in one channel alone.
///
/// Both sit inside the tolerance; together they are twice outside it. That is also the stronger
/// claim: what is worth refusing is a shelf showing the same plastic *and* the same label, not
/// one that merely came up in a similar colour.
const ALONE_LABEL: (usize, usize) = (360, 158);
const PAIR_LEFT: (usize, usize) = (180, 250);
const PAIR_RIGHT: (usize, usize) = (540, 260);
/// Out at the edges of the row, where neither layout puts anything. A cart here is a row that
/// was laid out from its selection rather than centred on what it holds.
const EDGE_LEFT: (usize, usize) = (40, 250);
const EDGE_RIGHT: (usize, usize) = (680, 250);
/// The ground the carts stand on, which is what an empty place on the row leaves behind.
const GROUND: [u32; 3] = [0x05, 0x05, 0x08];

/// The shoulders ring over one shelf per platform, and each one says which system it is. Read
/// off the panel rather than the draw list: what is being checked is that the row looks like a
/// row of carts with a name over it, which a list of rectangles cannot answer.
///
/// The Game Boy Advance shelf holds two carts here, so it also stands for every two-cart shelf:
/// the pair is centred together, with neither of them out at an edge and no hole beside them.
#[test]
fn the_shoulders_ring_over_a_shelf_for_each_system() {
    let Ok(surface) = HeadlessSurface::new() else {
        return;
    };
    let Ok(mut c) = Compositor::new(&surface) else {
        return;
    };
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    put_game_boy_carts(d.path());
    clocked(d.path());
    let mut f = Frontend::boot(Box::new(SimPlatform::at(d.path().to_path_buf())));
    f.upload_faces(&mut c);
    let mut input = Script(VecDeque::new());
    f.advance(&mut input);

    let gba = composed(&mut f, &mut c, "gba");
    assert_eq!(
        banner_ink(&gba),
        0,
        "the carousel named a system nobody had switched to"
    );
    let left = patch(&gba, PAIR_LEFT.0, PAIR_LEFT.1);
    let right = patch(&gba, PAIR_RIGHT.0, PAIR_RIGHT.1);
    assert!(
        apart(left, GROUND) > 60 && apart(right, GROUND) > 60,
        "the two Game Boy Advance carts are not standing as a centred pair: {left:?} and {right:?}"
    );
    for (name, (x, y)) in [("left", EDGE_LEFT), ("right", EDGE_RIGHT)] {
        let edge = patch(&gba, x, y);
        assert!(
            apart(edge, GROUND) < 30,
            "the pair is not centred: something is out at the {name} edge: {edge:?}"
        );
    }

    // One shelf per platform, so the Colour cart is not on the Game Boy shelf: each stands
    // alone in the middle of its own.
    let mut seen = Vec::new();
    for (name, banner) in [
        ("game-boy", "Game Boy"),
        ("game-boy-color", "Game Boy Color"),
    ] {
        tap(&mut f, &mut input, Btn::R1);
        let px = composed(&mut f, &mut c, name);
        let cart = patch(&px, ALONE.0, ALONE.1);
        let label = patch(&px, ALONE_LABEL.0, ALONE_LABEL.1);
        assert!(
            apart(cart, GROUND) > 60,
            "no cart in the middle of the {banner} shelf: {cart:?}"
        );
        for (side, (x, y)) in [("left", PAIR_LEFT), ("right", PAIR_RIGHT)] {
            let beside = patch(&px, x, y);
            assert!(
                apart(beside, GROUND) < 30,
                "the {banner} shelf holds one cart but drew something on its {side}: {beside:?}"
            );
        }
        let ink = banner_ink(&px);
        assert!(
            ink > 100,
            "the {banner} shelf came up without its name: {ink} lit pixels"
        );
        assert!(
            seen.iter()
                .all(|(c, l)| apart(cart, *c) + apart(label, *l) > 60),
            "{banner} is showing a cart another shelf already showed: {cart:?} in {label:?}"
        );
        seen.push((cart, label));
    }

    // Round the ring and back to where it started, on the cart the shelf was left on.
    tap(&mut f, &mut input, Btn::R1);
    let back = composed(&mut f, &mut c, "gba-again");
    assert!(
        apart(patch(&back, PAIR_LEFT.0, PAIR_LEFT.1), left) < 30,
        "the ring did not come back to the cart the first shelf was left on"
    );
    assert!(banner_ink(&back) > 100, "the way back said nothing");
}

/// The cart going into the slot from a shelf of two, which is a shelf that was not standing it
/// in the middle of the screen. It has to leave from where it stood and arrive over the mouth:
/// starting it at the slot is a cart that jumps on the frame the button is pressed.
///
/// Composed from the app's own draw list rather than through the frontend, because the travel
/// is a fifth of a second long and the app's clock can be stepped to the middle of it exactly.
#[test]
fn a_cart_going_in_from_a_pair_leaves_from_where_it_stood() {
    let Ok(surface) = HeadlessSurface::new() else {
        return;
    };
    let Ok(mut c) = Compositor::new(&surface) else {
        return;
    };
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    clocked(d.path());
    // No faces are uploaded, so each cart draws as a rect in the colour its label would have
    // been — which is all this needs, since the question is where the cart is.
    let mut app = App::boot(d.path());
    let ink = label_colour(&clean_label("Emerald"));

    let standing = shot(&app, &mut c, "insert-0-standing");
    let (from, _) = span(&standing, ink);
    app.apply(Action::Insert);
    app.update(SEATED_AT / 2.0);
    let halfway = shot(&app, &mut c, "insert-1-halfway");
    let (mid, y) = span(&halfway, ink);
    for _ in 0..120 {
        app.update(1.0 / 60.0);
    }
    let seated = shot(&app, &mut c, "insert-2-seated");
    let (home, home_y) = span(&seated, ink);

    assert!(
        (from - 240.0).abs() < 8.0,
        "the pair did not stand its selection at 240: {from}"
    );
    assert!(
        (home - 360.0).abs() < 8.0,
        "the cart did not seat in the middle of the slot: {home}"
    );
    assert!(
        mid > from + 8.0 && mid < home - 8.0,
        "the cart jumped rather than sliding: {from} then {mid} then {home}"
    );
    assert!(
        home_y > y,
        "the cart did not go down the slot: {y} then {home_y}"
    );
}

/// The horizontal centre of everything drawn in `ink`, and the lowest row it reaches. The cart
/// is the only thing on screen wearing its own label colour.
fn span(px: &[u8], ink: [u8; 3]) -> (f32, usize) {
    let close = |c: [u8; 3]| (0..3).all(|k| c[k].abs_diff(ink[k]) <= 24);
    let mut cols: Vec<usize> = Vec::new();
    let mut bottom = 0;
    for y in 0..OUT_H as usize {
        for x in 0..OUT_W as usize {
            if close(at(px, x, y)) {
                cols.push(x);
                bottom = y;
            }
        }
    }
    let first = *cols
        .first()
        .expect("nothing on screen in the cart's colour");
    let last = *cols.iter().max().expect("nothing in the cart's colour");
    ((first + last) as f32 / 2.0, bottom)
}

/// One frame of the app's own draw list, composited and written out to look at.
fn shot(app: &App, c: &mut Compositor, name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    app.draw(&mut out);
    c.set_screen_power(1.0);
    c.begin_frame();
    c.draw_list(&out);
    let px = c.read_frame();
    if let Ok(dir) = std::env::var("SCRATCH_PNG_DIR") {
        let path = format!("{dir}/shelves-{name}.png");
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

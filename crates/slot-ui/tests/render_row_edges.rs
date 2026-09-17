//! The carousel's edges and its handover to the slot, composited on the GPU and read back.
//!
//! A draw list can agree that a row was drawn while the screen shows a black hole where a
//! cartridge should be leaving the frame: `cart_at_offset` returning `None` for a slot is a
//! quad that never reaches the list at all, and no assertion about the quads that *are* in it
//! can see the one that is missing. So what is measured here is the panel — how much bare
//! backdrop the row leaves at each edge, frame by frame, through a scroll — and the yardstick is
//! another row of the same carousel rather than a number chosen by hand.
//!
//! `SCRATCH_PNG_DIR=/tmp cargo test -p slot-ui --test render_row_edges -- --nocapture`

#![cfg(target_os = "macos")]

use std::sync::{Mutex, MutexGuard, PoisonError};

use slot_gfx::{Compositor, HeadlessSurface};
use slot_store::{Cart, Platform};
use slot_ui::{cart_face, cart_shadow, Draw, Shelf, TexId, OUT_H, OUT_W};

/// `gl::load_with` writes global function pointers, so two GL tests must not overlap.
static GL: Mutex<()> = Mutex::new(());

fn compositor() -> Option<(MutexGuard<'static, ()>, HeadlessSurface, Compositor)> {
    let guard = GL.lock().unwrap_or_else(PoisonError::into_inner);
    let surface = HeadlessSurface::new().ok()?;
    let compositor = Compositor::new(&surface).ok()?;
    Some((guard, surface, compositor))
}

fn shelf_with(n: usize) -> Shelf {
    Shelf::new(
        (0..n)
            .map(|i| Cart {
                platform: Platform::Gba,
                stem: format!("Game {i}"),
                rom: format!("Games/GBA/Game {i}.gba").into(),
                label: None,
                code: String::new(),
                title: format!("GAME {i}"),
            })
            .collect(),
    )
}

/// The row with its faces on the GPU, as the frontend uploads them.
fn uploaded(n: usize, c: &mut Compositor) -> (Shelf, Vec<TexId>) {
    let mut s = shelf_with(n);
    let faces: Vec<TexId> = s
        .carts
        .iter()
        .map(|cart| {
            let f = cart_face(cart);
            c.create_texture(f.w, f.h, &f.rgba)
        })
        .collect();
    s.set_faces(faces.clone());
    let shadow = cart_shadow();
    let tex = c.create_texture(shadow.w, shadow.h, &shadow.rgba);
    s.set_shadow(tex);
    (s, faces)
}

fn composed(c: &mut Compositor, list: &[Draw]) -> Vec<u8> {
    c.begin_frame();
    c.draw_list(list);
    c.read_frame()
}

fn write_png(px: &[u8], path: &str) {
    let file = std::fs::File::create(path).expect("create png");
    let mut e = png::Encoder::new(std::io::BufWriter::new(file), OUT_W, OUT_H);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header()
        .expect("png header")
        .write_image_data(px)
        .expect("png data");
    println!("wrote {path}");
}

fn shot(px: &[u8], name: &str) {
    if let Ok(dir) = std::env::var("SCRATCH_PNG_DIR") {
        write_png(px, &format!("{dir}/{name}.png"));
    }
}

/// The band the carts stand in, which is the only part of the frame this reads: the slot's own
/// housing runs across the bottom and would make every column look occupied. Taken as a fraction
/// of the panel rather than as two screen rows, because the carousel has already moved 59 px
/// under constants typed when it stood a cartridge somewhere else.
const BAND: std::ops::Range<usize> = (OUT_H as usize / 4)..(OUT_H as usize / 2);

/// Whether any pixel of a column, in the band the carts stand in, is something other than the
/// backdrop. A cart is a solid object over black, so a lit column is a column a cartridge is in.
fn occupied(px: &[u8], x: usize) -> bool {
    BAND.map(|y| (y * OUT_W as usize + x) * 4)
        .any(|o| px[o] > 0x18 || px[o + 1] > 0x18 || px[o + 2] > 0x18)
}

/// How many columns of bare backdrop the row leaves at each edge of the panel.
fn bare_edges(px: &[u8]) -> (usize, usize) {
    let w = OUT_W as usize;
    let left = (0..w).take_while(|x| !occupied(px, *x)).count();
    let right = (0..w).take_while(|x| !occupied(px, w - 1 - *x)).count();
    (left, right)
}

/// A direction held down from the first frame, as `App::update` runs the row: `tick` then
/// `update`, once a frame. Every frame comes back composited.
fn held_scroll(c: &mut Compositor, n: usize, frames: usize) -> Vec<Vec<u8>> {
    let (mut s, _) = uploaded(n, c);
    // Start on the cart a right press wraps off the end of, so the wrap is the first thing on
    // screen rather than something the sequence has to be long enough to reach.
    s.select(n - 1);
    s.hold_right(0);
    (0..frames)
        .map(|f| {
            s.tick(f as u64 * 1000 / 60);
            s.update(1.0 / 60.0);
            let mut list = Vec::new();
            s.draw(0.0, &mut list);
            composed(c, &list)
        })
        .collect()
}

/// A short row must leave no more of the panel bare than a long one does.
///
/// The row is a ring and the screen is wider than three carts, so at every moment there is a
/// cartridge on its way off one edge and another arriving at the other. That was true of a long
/// row and not of a short one: `cart_at_offset` handed each cart a single image, the slot nearest
/// the *selection*, and a press moves the selection a whole slot before the spring has moved the
/// row at all — so for the length of the travel the image the rule withheld was the one leaving
/// the frame. At three carts and at four, the cart standing in the left slot was struck out of
/// the row while it was still fully on screen: it vanished where it stood, and the left third of
/// the panel stayed black until the row settled.
///
/// Ten carts is the yardstick because ten is the user's own GBA shelf and was never wrong. The
/// claim is a comparison and not a constant, so it cannot go stale the way a hand-typed column
/// number does when the pitch or the cart's width changes.
#[test]
fn a_short_row_leaves_no_more_of_the_panel_bare_than_a_long_one() {
    let Some((_g, _s, mut c)) = compositor() else {
        return;
    };
    // Two seconds: the 400 ms repeat delay and then fifteen repeats, which laps a short row
    // several times over.
    const FRAMES: usize = 120;
    let worst = |frames: &[Vec<u8>]| -> (usize, usize) {
        frames
            .iter()
            .map(|px| bare_edges(px))
            .fold((0, 0), |a, b| (a.0.max(b.0), a.1.max(b.1)))
    };
    let long = held_scroll(&mut c, 10, FRAMES);
    let (lref, rref) = worst(&long);
    shot(&long[1], "row-10-early");
    for n in [2usize, 3, 4, 5, 8] {
        let frames = held_scroll(&mut c, n, FRAMES);
        shot(&frames[1], &format!("row-{n}-early"));
        let (left, right) = worst(&frames);
        assert!(
            left <= lref + 2 && right <= rref + 2,
            "{n} carts left {left} px bare at the left and {right} at the right against a ten \
             cart row's {lref} and {rref}: the row has a hole in it where a cartridge should be \
             leaving the frame"
        );
    }
}

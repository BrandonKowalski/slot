//! L and R on a Game Boy cart, through the whole frontend: the real core, the real cart, the
//! real compositor, composited on the GPU and read back as pixels.
//!
//! The draw list is not the screen and neither is a uniform. This crate has had list
//! assertions pass while the panel was visibly wrong, so the claim "L fills the panel and R
//! gives back the centred picture" is settled here, against the composited frame — and the
//! PNGs are written so the one thing only eyes can judge, what the grille's change of
//! relationship looks like, can actually be looked at.
//!
//! `SCRATCH_PNG_DIR=/tmp cargo test -p slot --test render_picture_modes -- --nocapture`
//!
//! Skipped on a machine with no mGBA dylib and no card to take a cart off, the same way every
//! other test here that needs a real core is.

#![cfg(target_os = "macos")]

mod common;

use std::collections::VecDeque;
use std::path::Path;
use std::time::{Duration, Instant};

use common::{clocked, core_lock, repo_root, tmp_root_with_gb_carts};
use slot::frontend::Frontend;
use slot_gfx::{Compositor, HeadlessSurface, OUT_H, OUT_W};
use slot_input::{Btn, InputSource, Millis, RawEvent};
use slot_power::SimPlatform;
use slot_store::{Core, Platform};

/// One batch of events per poll, and nothing once they run out.
struct Script(VecDeque<Vec<RawEvent>>);

impl InputSource for Script {
    fn poll(&mut self, _now: Millis) -> Vec<RawEvent> {
        self.0.pop_front().unwrap_or_default()
    }
}

fn at(px: &[u8], x: usize, y: usize) -> [u8; 3] {
    let o = (y * OUT_W as usize + x) * 4;
    [px[o], px[o + 1], px[o + 2]]
}

/// Where a Game Boy picture lands at actual size: 480x432 in the middle of the panel, with
/// 120 px of nothing either side and 24 top and bottom.
const BAR_W: usize = 120;
const BAR_H: usize = 24;

/// How much of a band has anything on it at all. Black is the backdrop the margin shows, and
/// the mask darkens but never blanks a lit pixel, so this separates picture from margin
/// without having to know what the game drew.
fn lit(px: &[u8], xs: std::ops::Range<usize>, ys: std::ops::Range<usize>) -> usize {
    ys.flat_map(|y| xs.clone().map(move |x| (x, y)))
        .filter(|(x, y)| at(px, *x, *y).iter().any(|c| *c > 8))
        .count()
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

/// Two panels beside each other with a grey rule between them, which is the only way to judge
/// what the stretch did to the grille's relationship with the picture's own pixels.
fn side_by_side(left: &[u8], right: &[u8], gap: usize) -> (u32, u32, Vec<u8>) {
    let w = OUT_W as usize * 2 + gap;
    let mut out = vec![0u8; w * OUT_H as usize * 4];
    for y in 0..OUT_H as usize {
        let row = y * w * 4;
        let src = y * OUT_W as usize * 4;
        let line = OUT_W as usize * 4;
        out[row..row + line].copy_from_slice(&left[src..src + line]);
        let over = row + line;
        for p in out[over..over + gap * 4].chunks_exact_mut(4) {
            p.copy_from_slice(&[0x60, 0x60, 0x60, 0xff]);
        }
        let from = over + gap * 4;
        out[from..from + line].copy_from_slice(&right[src..src + line]);
    }
    (w as u32, OUT_H, out)
}

/// A patch of each panel blown up, nearest, so single mask cells and single source pixels are
/// both big enough to see. At actual size a 3x3 cell sits on exactly one source pixel; stretched
/// it does not, and that is the whole of what this picture is for.
fn zoom(
    left: &[u8],
    right: &[u8],
    x0: usize,
    y0: usize,
    w: usize,
    h: usize,
    n: usize,
) -> (u32, u32, Vec<u8>) {
    let gap = 12;
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
            let c = at(src, x0 + sx, y0 + y / n);
            let o = (y * out_w + x) * 4;
            out[o..o + 4].copy_from_slice(&[c[0], c[1], c[2], 0xff]);
        }
    }
    (out_w as u32, out_h as u32, out)
}

/// The card's own Game Boy cart, copied into the root the test boots from. Read only: nothing
/// here writes to the card. `None` on a fresh clone with no `sdcard/`, which is a skip rather
/// than a failure — a stand-in rom paints nothing worth looking at.
fn put_card_cart(root: &Path) -> Option<()> {
    let from = repo_root().join("sdcard/Games/GB/Tetris Rosy Retrospection.gb");
    let rom = std::fs::read(&from).ok()?;
    std::fs::write(root.join("Games/GB/Tetris Rosy Retrospection.gb"), rom)
        .expect("copy the card's cart");
    Some(())
}

/// Advances the whole frontend for a stretch of wall clock. The animations and the emulator
/// thread both run on real time, so this is what a cart going in and a game painting actually
/// costs; a tight loop with no clock behind it would compose the same first frame forever.
fn run_for(f: &mut Frontend, input: &mut Script, secs: f32) {
    let until = Instant::now() + Duration::from_secs_f32(secs);
    while Instant::now() < until {
        f.advance(input);
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn l_fills_the_panel_and_r_gives_back_the_centred_picture() {
    let _core = core_lock();
    // The core this cart will actually be opened on, not a named one. Planting mgba here while
    // the cart resolved to another core left nothing for the search to find, so `open_core` fell
    // back to `MockCore` and this test measured its test pattern: a full-bleed gradient, which
    // lights every margin the assertions below require to be dark, for a reason that has nothing
    // to do with picture modes.
    let core = Core::default_for(Platform::Gb);
    let file = format!(
        "{}_libretro.{}",
        core.as_str(),
        std::env::consts::DLL_EXTENSION
    );
    let dylib = repo_root().join("vendor").join(&file);
    if !dylib.exists() {
        eprintln!("no {} dylib, skipping", core.as_str());
        return;
    }
    let Ok(surface) = HeadlessSurface::new() else {
        return;
    };
    let Ok(mut c) = Compositor::new(&surface) else {
        return;
    };

    // Two carts so the shelf is a shelf, and the real one first alphabetically so A plays it.
    let d = tmp_root_with_gb_carts(&["Zzz"]);
    let Some(()) = put_card_cart(d.path()) else {
        eprintln!("no Game Boy cart on this machine's card, skipping");
        return;
    };
    // `candidates` looks in the content root's own `System/` first, which is how a test plants
    // a core somewhere the search will actually find it.
    std::fs::copy(&dylib, d.path().join("System").join(&file)).expect("plant the core");
    clocked(d.path());

    let mut f = Frontend::boot(Box::new(SimPlatform::at(d.path().to_path_buf())));
    f.upload_faces(&mut c);
    let mut input = Script(VecDeque::new());
    input.0.push_back(vec![RawEvent::Down(Btn::A)]);
    f.advance(&mut input);
    input.0.push_back(vec![RawEvent::Up(Btn::A)]);
    f.advance(&mut input);
    // Long enough for the cart to go in, the panel to strike, and Tetris to get past its
    // copyright screen onto something with structure in it.
    run_for(&mut f, &mut input, 5.0);

    f.compose(&mut c);
    let actual = c.read_frame();
    write_png("picture-actual", OUT_W, OUT_H, &actual);

    let mid_y = OUT_H as usize / 2 - 40..OUT_H as usize / 2 + 40;
    assert!(
        lit(&actual, BAR_W..OUT_W as usize - BAR_W, mid_y.clone()) > 1000,
        "nothing is playing: the panel is dark where the picture should be"
    );
    assert_eq!(
        lit(&actual, 0..BAR_W, 0..OUT_H as usize),
        0,
        "actual size has something in the left margin"
    );
    assert_eq!(
        lit(
            &actual,
            OUT_W as usize - BAR_W..OUT_W as usize,
            0..OUT_H as usize
        ),
        0,
        "actual size has something in the right margin"
    );
    assert_eq!(
        lit(&actual, 0..OUT_W as usize, 0..BAR_H),
        0,
        "actual size has something above the picture"
    );

    input.0.push_back(vec![RawEvent::Down(Btn::L1)]);
    f.advance(&mut input);
    input.0.push_back(vec![RawEvent::Up(Btn::L1)]);
    run_for(&mut f, &mut input, 1.0);
    f.compose(&mut c);
    let stretched = c.read_frame();
    write_png("picture-stretch", OUT_W, OUT_H, &stretched);

    // The margins are gone, which is the whole claim. Measured as "most of the band has
    // something on it", because a Game Boy picture has black in it too.
    let left = lit(&stretched, 0..BAR_W, 0..OUT_H as usize);
    let top = lit(&stretched, 0..OUT_W as usize, 0..BAR_H);
    assert!(
        left > BAR_W * OUT_H as usize / 4,
        "the left margin is still mostly empty in fullscreen: {left} lit"
    );
    assert!(
        top > OUT_W as usize * BAR_H / 4,
        "the top margin is still mostly empty in fullscreen: {top} lit"
    );

    let (w, h, both) = side_by_side(&actual, &stretched, 8);
    write_png("picture-side-by-side", w, h, &both);
    // Around the middle of the playfield, where there is an edge to watch the grille cross.
    let (w, h, close) = zoom(&actual, &stretched, 300, 200, 90, 60, 6);
    write_png("picture-zoom", w, h, &close);

    input.0.push_back(vec![RawEvent::Down(Btn::R1)]);
    f.advance(&mut input);
    input.0.push_back(vec![RawEvent::Up(Btn::R1)]);
    run_for(&mut f, &mut input, 1.0);
    f.compose(&mut c);
    let back = c.read_frame();
    write_png("picture-back", OUT_W, OUT_H, &back);
    assert_eq!(
        lit(&back, 0..BAR_W, 0..OUT_H as usize),
        0,
        "R did not give the margins back"
    );
    assert!(
        lit(&back, BAR_W..OUT_W as usize - BAR_W, mid_y) > 1000,
        "R left the panel dark"
    );
}

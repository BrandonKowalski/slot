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
// Aliased for the same reason `tests/common` aliases it: `slot_power::Platform` is the device
// this runs on and is already spoken for, and this one is the console a cart is for.
use slot_store::{Cart, Platform as CartPlatform};
use slot_ui::{
    cart_box, cart_face, clean_label, edge, housing, label_colour, mark_box, opening, recess,
    rest_y, Draw, SlotChrome, CART_W, GB_CART_H, GB_LABEL_H, GB_LABEL_Y, HINT_H, LABEL_H, LABEL_Y,
    MOUTH_H, PLATE_H,
};

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

/// Lit pixels across the middle of the top plate, where the shelf's name used to be banner'd
/// over the carts. Nothing is drawn there now — the case band's mark says which shelf this is —
/// so this exists to catch the banner coming back, not to find it.
fn banner_ink(px: &[u8]) -> usize {
    (0..PLATE_H as usize)
        .flat_map(|y| (200..520).map(move |x| (x, y)))
        .filter(|(x, y)| at(px, *x, *y).iter().all(|c| *c > 0x80))
        .count()
}

/// The mark's own box on the case band: `FOOTER_MARGIN` in from the left, `mark_box` across,
/// centred in the `HINT_H` row that starts `FOOTER_Y` down. Named from the layout rather than
/// typed as four numbers, so moving the band moves the reading with it.
fn mark_window() -> (usize, usize, usize, usize) {
    let (w, h) = mark_box();
    let row_y = OUT_H as f32 - MOUTH_H + (MOUTH_H - HINT_H as f32) / 2.0;
    let y = row_y + (HINT_H as f32 - h as f32) / 2.0;
    (24, y as usize, w as usize, h as usize)
}

/// Every pixel of the mark, as it reached the panel. Two shelves' marks compare equal only if
/// they are the same drawing — which is what "the mark changed when the shoulder was pressed"
/// actually means, and what a count of lit pixels could agree on while showing one picture
/// three times.
fn mark_pixels(px: &[u8]) -> Vec<[u8; 3]> {
    let (x0, y0, w, h) = mark_window();
    (y0..y0 + h)
        .flat_map(|y| (x0..x0 + w).map(move |x| (x, y)))
        .map(|(x, y)| at(px, x, y))
        .collect()
}

/// How much of the mark's box is lit above the band it is printed on. The band is a flat
/// housing colour, so anything appreciably lighter is the mark's own ink.
fn mark_ink(px: &[u8]) -> usize {
    let ground = (housing()[0] * 255.0) as u8;
    mark_pixels(px)
        .iter()
        .filter(|c| c[0] > ground + 0x20)
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

/// A point `down` pixels down the face of the lone pak on a Game Boy shelf, which is the only
/// shelf either reading below is ever taken on. Both go through here rather than naming a screen
/// row, because a screen row is only right for as long as nobody moves the cartridge: they were
/// typed when a pak stood on a floor it shared with a GBA cart, from y 55 to 308, and the
/// carousel centring dropped it to 113 to 366. That is far enough that the constant meant to
/// land on plastic would have landed on paper and the one meant for paper on plastic — with both
/// readings still passing and neither meaning what its name says.
fn on_the_lone_pak(down: u32) -> (usize, usize) {
    (
        (OUT_W / 2) as usize,
        (rest_y(GB_CART_H as f32) + down as f32) as usize,
    )
}

/// The pak's bare plastic: half way down the shoulder above its label, which is the moulded
/// lettering plate.
fn alone() -> (usize, usize) {
    on_the_lone_pak(GB_LABEL_Y / 2)
}

/// The middle of that same pak's label well. Read beside `alone` because neither reading can
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
fn alone_label() -> (usize, usize) {
    on_the_lone_pak(GB_LABEL_Y + GB_LABEL_H / 2)
}

/// Where a shelf of two lands its carts, in screen pixels: it is centred as a pair, which puts
/// the selection at x 141 to 338 and its neighbour out at 402 to 557, lower and shorter for
/// standing shrunk. Read on a Game Boy shelf too, where they are out beyond the lone pak's own
/// 240 px width and so must come back as bare ground.
const PAIR_LEFT: (usize, usize) = (180, 250);
const PAIR_RIGHT: (usize, usize) = (540, 260);
/// Out at the edges of the row, where neither layout puts anything. A cart here is a row that
/// was laid out from its selection rather than centred on what it holds.
const EDGE_LEFT: (usize, usize) = (40, 250);
const EDGE_RIGHT: (usize, usize) = (680, 250);
/// The ground the carts stand on, which is what an empty place on the row leaves behind.
const GROUND: [u32; 3] = [0x05, 0x05, 0x08];

/// The shoulders ring over one shelf per platform, and each one says which system it is — on the
/// case band, as the machine that shelf's cartridges were made for, rather than as a name
/// banner'd over the carts for a second and a half. Read off the panel rather than the draw
/// list: what is being checked is that the band is showing a different drawing on each shelf,
/// which a list of rectangles cannot answer — a draw list would agree three times over that a
/// texture landed at x 24 while the same picture came up every time.
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
    assert!(
        mark_ink(&gba) > 20,
        "the case band came up with no mark on it at all: {} lit pixels",
        mark_ink(&gba)
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
    let mut marks = vec![mark_pixels(&gba)];
    for (name, banner) in [
        ("game-boy", "Game Boy"),
        ("game-boy-color", "Game Boy Color"),
    ] {
        tap(&mut f, &mut input, Btn::R1);
        let px = composed(&mut f, &mut c, name);
        let (ax, ay) = alone();
        let cart = patch(&px, ax, ay);
        let (lx, ly) = alone_label();
        let label = patch(&px, lx, ly);
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
        assert_eq!(
            banner_ink(&px),
            0,
            "the {banner} shelf banner'd its name over the carts"
        );
        let ink = mark_ink(&px);
        assert!(
            ink > 20,
            "the {banner} shelf came up with no mark on the case: {ink} lit pixels"
        );
        let mark = mark_pixels(&px);
        assert!(
            !marks.contains(&mark),
            "the {banner} shelf is showing a mark another shelf already showed"
        );
        marks.push(mark);
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
    assert_eq!(banner_ink(&back), 0, "the way back put a banner up");
    assert_eq!(
        mark_pixels(&back),
        marks[0],
        "the ring came back to the Game Boy Advance shelf under another system's mark"
    );
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
    frame(c, &out, name)
}

/// The same for a draw list somebody built by hand, which is how the insertion below is driven.
/// The travel is under half a second and the cartridge that goes down it is whichever one the
/// shelf was on, so every frame of it has to be reachable by seat and by platform, not by
/// stepping a clock and hoping to land somewhere useful.
fn frame(c: &mut Compositor, out: &[Draw], name: &str) -> Vec<u8> {
    c.set_screen_power(1.0);
    c.begin_frame();
    c.draw_list(out);
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

/// A cartridge for each shape, with a title that hashes to a label colour of its own so the two
/// can be told apart on the screen as well as in the list.
fn cartridges() -> [(&'static str, Cart); 2] {
    [
        (
            "gba",
            Cart {
                platform: CartPlatform::Gba,
                stem: "Emerald".into(),
                rom: "Games/GBA/Emerald.gba".into(),
                label: None,
                code: String::new(),
                title: "POKEMON EMER".into(),
            },
        ),
        (
            "pak",
            Cart {
                platform: CartPlatform::Gb,
                stem: "Tetris".into(),
                rom: "Games/GB/Tetris.gb".into(),
                label: None,
                code: String::new(),
                title: "TETRIS".into(),
            },
        ),
    ]
}

/// Where this cartridge's paper starts down its face. The trap this avoids is a fixed sample
/// coordinate: a pak is 253 px tall against a GBA cart's 135, so a row that lands on paper for
/// one lands on plastic for the other and the test passes for the wrong reason.
fn label_top(p: CartPlatform) -> usize {
    match p {
        CartPlatform::Gba => LABEL_Y as usize,
        CartPlatform::Gb | CartPlatform::Gbc => GB_LABEL_Y as usize,
    }
}

/// The first and last screen rows showing the cartridge's own paper. Only ever asked of a
/// cartridge standing clear of the machine: a seated one may legitimately show none, which is
/// what a Game Boy pak does and why this is no longer how the cartridge itself is found.
fn paper_rows(px: &[u8], ink: [u8; 3]) -> Option<(usize, usize)> {
    let close = |c: [u8; 3]| (0..3).all(|k| c[k].abs_diff(ink[k]) <= 24);
    let mut rows =
        (0..OUT_H as usize).filter(|y| (0..OUT_W as usize).any(|x| close(at(px, x, *y))));
    let first = rows.next()?;
    Some((first, rows.next_back().unwrap_or(first)))
}

/// Everything this frame is made of that is *not* the cartridge: the black the compositor clears
/// to, and the four flat theme colours the slot's own bands are painted in. The list under test
/// holds those and one cart, so whatever is none of them is the cart.
fn backdrop() -> [[f32; 4]; 5] {
    [[0.0, 0.0, 0.0, 1.0], housing(), opening(), edge(), recess()]
}

/// The first and last screen rows the cartridge covers, found by its shell.
///
/// This used to scan for the label's paper colour, which worked only while every cartridge's
/// label stayed outside the machine. A seated Game Boy pak's does not — its well is 27.7% down
/// a 253 px body, so the whole of it is swallowed — and the finder then reported an empty
/// screen for a frame with a cartridge plainly in it. What is true of every cartridge in every
/// frame is that it is the one object on screen that is neither the backdrop nor the machine,
/// so that is what is looked for. Nothing here names a coordinate: the answer is wherever the
/// cart turns out to be.
///
/// The tolerance is 8 a channel against a 17 gap: the nearest a cartridge's plastic comes to a
/// theme colour is the GBA cart's 0x35 shell against the 0x24 housing.
fn shell_rows(px: &[u8]) -> Option<(usize, usize)> {
    let flat = backdrop();
    let cart = |c: [u8; 3]| {
        !flat.iter().any(|f| {
            (0..3).all(|k| {
                let want = (f[k] * 255.0).round() as u8;
                c[k].abs_diff(want) <= 8
            })
        })
    };
    let mut rows = (0..OUT_H as usize).filter(|y| (0..OUT_W as usize).any(|x| cart(at(px, x, *y))));
    let first = rows.next()?;
    Some((first, rows.next_back().unwrap_or(first)))
}

/// The insertion, rendered. A Game Boy pak has to go into the slot as the object it is: standing
/// centred on the carousel where a GBA cart stands centred, travelling at its own size rather
/// than squashed into one, catching on the lip where a cart's foot meets it, and coming to rest
/// with exactly as much cartridge left out of the machine as a GBA cart leaves. None of that is
/// a claim a draw list can settle, which is why this one goes through the compositor and writes
/// the frames out to be looked at.
#[test]
fn both_cartridges_go_into_the_slot_at_their_own_size() {
    let Ok(surface) = HeadlessSurface::new() else {
        return;
    };
    let Ok(mut c) = Compositor::new(&surface) else {
        return;
    };
    // Named for what the frame is of, so a sequence read back in order is the animation.
    let beats = [
        ("0-standing", 0.0),
        ("1-falling", 0.25),
        ("2-at-the-catch", 0.42),
        ("3-caught", 0.55),
        ("4-pushed-through", 0.80),
        ("5-seated", 1.0),
    ];
    let mut seated = Vec::new();
    for (name, cart) in cartridges() {
        let face = cart_face(&cart);
        let (w, h) = cart_box(cart.platform);
        assert_eq!(
            (face.w, face.h),
            (w, h),
            "{name}: the face is not the size the layout thinks it is"
        );
        let tex = c.create_texture(face.w, face.h, &face.rgba);
        let ink = label_colour(&clean_label(&cart.stem));
        let rest = (OUT_W - w) as f32 / 2.0;

        for (beat, seat) in beats {
            let mut out = Vec::new();
            SlotChrome {
                cart: &cart,
                face: Some(tex),
                rest,
                seat,
                alert: None,
                dim: 0.0,
                screen: 0.0,
                game: false,
            }
            .draw(&mut out);
            let px = frame(&mut c, &out, &format!("insert-{name}-{beat}"));

            let Some((top, bottom)) = shell_rows(&px) else {
                panic!("{name} at {beat}: no cartridge on the screen at all");
            };
            if seat == 0.0 {
                // Standing, centred on the screen: the carousel shares a centre across
                // platforms, not a floor, so a 253 px pak and a 135 px cart are in the same
                // place in the frame with the pak simply reaching further both ways.
                assert!(
                    (top as f32 - rest_y(h as f32)).abs() < 1.5,
                    "{name} stands with its top edge at {top}, not at {} where the carousel \
                     centres a {h} px cartridge",
                    rest_y(h as f32)
                );
                let middle = (top + bottom) as f32 / 2.0;
                assert!(
                    (middle - OUT_H as f32 / 2.0).abs() < 1.5,
                    "{name} stands {top}..{bottom}, centred on {middle} rather than on the \
                     screen's own {}",
                    OUT_H as f32 / 2.0
                );
                // The paper is the full height the cartridge's own label well is, and starts
                // its own inset down the shell: a squashed cart shows a squashed label, and one
                // drawn from the wrong platform's numbers shows it in the wrong place.
                let (paper_top, paper_bottom) =
                    paper_rows(&px, ink).expect("a standing cartridge shows its label");
                let inset = paper_top - top;
                assert!(
                    inset.abs_diff(label_top(cart.platform)) <= 2,
                    "{name}'s paper starts {inset} px down its face, not the {} its platform \
                     puts it at",
                    label_top(cart.platform)
                );
                let paper = paper_bottom - paper_top + 1;
                let want = match cart.platform {
                    CartPlatform::Gba => LABEL_H as usize,
                    _ => GB_LABEL_H as usize,
                };
                assert!(
                    paper.abs_diff(want) <= 2,
                    "{name}'s {want} px label came out {paper} px tall: it is being scaled"
                );
            }
            if seat == 1.0 {
                seated.push((name, top as f32, bottom as f32));
            }
        }
    }

    // Seated, the two are the same picture: the same top edge, and the same run of cartridge
    // left out of the machine. How much shows is the recess's business and not the cartridge's,
    // so a taller one may not be swallowed further than a short one.
    let (first, rest) = seated.split_first().expect("both cartridges seated");
    for (name, top, bottom) in rest {
        assert!(
            (top - first.1).abs() < 1.5,
            "{name} seats with its top edge at {top} and {} at {}: one is in deeper than the \
             other",
            first.0,
            first.1
        );
        assert!(
            ((bottom - top) - (first.2 - first.1)).abs() < 1.5,
            "{name} leaves {} px of itself out of the machine and {} leaves {}: the slot is \
             showing one cartridge more of itself than the other",
            bottom - top,
            first.0,
            first.2 - first.1
        );
    }
}

/// How far the cartridge moves on each frame of the travel, at the rate the device runs. Printed
/// rather than asserted on a number pulled out of the air: what it is for is judging whether the
/// push through the lip reads as a shove or as a teleport, and that is an eye's call. The one
/// thing held here is that no frame of it is a jump of more than half the cartridge, which is
/// where a moving object stops overlapping itself and starts reading as two objects.
#[test]
fn no_frame_of_the_travel_jumps_further_than_the_cartridge_is_tall() {
    for (name, cart) in cartridges() {
        let (_, h) = cart_box(cart.platform);
        let ys: Vec<f32> = (0..=27)
            .map(|f| {
                let mut out = Vec::new();
                SlotChrome {
                    cart: &cart,
                    face: None,
                    rest: (OUT_W - CART_W) as f32 / 2.0,
                    seat: (f as f32 / 27.0).min(1.0),
                    alert: None,
                    dim: 0.0,
                    screen: 0.0,
                    game: false,
                }
                .draw(&mut out);
                out.iter()
                    .find_map(|d| match d {
                        Draw::Rect { y, h: qh, .. } if (*qh - h as f32).abs() < 0.01 => Some(*y),
                        _ => None,
                    })
                    .expect("no cartridge in the list")
            })
            .collect();
        let steps: Vec<f32> = ys.windows(2).map(|w| (w[1] - w[0]).round()).collect();
        println!("{name}: {steps:?}");
        let worst = steps.iter().cloned().fold(0.0f32, f32::max);
        assert!(
            worst < h as f32 / 2.0,
            "{name} moves {worst} px in one frame, over half of its own {h} px: that is a \
             cut, not a movement"
        );
    }
}

//! The Game Boy palette against the real core: a monochrome cart run through slot's own
//! `open_core_for`, and the pixels it drew compared against the same cart with the palette
//! options left off.
//!
//! `apply_core_options` sets `mgba_gb_colors_preset` and `mgba_gb_colors` to reproduce what a
//! Game Boy Advance SP put on screen when an original Game Boy cartridge was pushed into it: the
//! Game Boy Color boot ROM the AGB inherits colourises a monochrome cart from a table keyed on
//! its header, and a cart with no entry in that table — which is every third-party and homebrew
//! cart, both of the monochrome ones on this machine's card among them — is given one specific
//! palette rather than a grey ramp. That comment says why those values; this says whether they
//! reach the screen.
//!
//! Nothing a draw list or an option read back out of the frontend's own map can answer: the core
//! would take `GBC Dark Green -A` as quietly as `GBC Dark Green →A` and simply never colour
//! anything, and mGBA's own default is grayscale, so a typo looks exactly like the option not
//! being set at all. So this runs the machine and looks at what came out.
//!
//! `SCRATCH_PNG_DIR=/tmp cargo test -p slot --test render_gb_palette -- --nocapture`
//!
//! Skipped on a machine with no mGBA dylib and no card to take a cart off, the same way every
//! other test here that needs a real core is.

mod common;

use std::path::{Path, PathBuf};

use common::{core_lock, repo_root, vendored_core};
use slot_retro::{ButtonMask, LibretroCore, RetroCore, GBA_H, GBA_W};
use slot_store::Core;

/// The three colours the Game Boy Color boot ROM's default background palette is made of, as
/// they land in slot's framebuffer — white, green, blue, and a black that proves nothing because
/// the margin around a 160x144 picture is black too.
///
/// These are the boot ROM's own palette 29 (`$7FFF`, `$1BEF`, `$6180`, `$0000` in its table)
/// carried through mGBA's 5-bit-to-8-bit expansion and slot's RGB565 unpack, which is why they
/// are not round numbers. Compared with a tolerance rather than exactly, because how many bits
/// wide the core hands its picture over is the core's business and not this test's: a rebuild
/// that switched mGBA to XRGB8888 would move every one of these by two or three and mean
/// nothing at all had changed about the palette.
const DEFAULT_BG: [[u8; 3]; 3] = [[255, 251, 255], [123, 251, 49], [0, 97, 198]];

/// How many frames each cart is run before its picture is read: far enough in that the cart has
/// drawn something, and the same for both runs of a cart, so the two pictures are the same
/// instant of the same game and every pixel that differs differs because of the option.
const FRAMES: usize = 600;

fn to_rgba(xrgb: &[u8]) -> Vec<u8> {
    xrgb.chunks_exact(4)
        .flat_map(|p| [p[2], p[1], p[0], 0xff])
        .collect()
}

fn write_png(name: &str, rgba: &[u8]) {
    let Ok(dir) = std::env::var("SCRATCH_PNG_DIR") else {
        return;
    };
    let path = format!("{dir}/{name}.png");
    let file = std::fs::File::create(&path).expect("create png");
    let mut e = png::Encoder::new(std::io::BufWriter::new(file), GBA_W, GBA_H);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header()
        .expect("png header")
        .write_image_data(rgba)
        .expect("png data");
    println!("wrote {path}");
}

/// How much colour a picture has: the mean, over every pixel, of how far its channels spread
/// apart. A grey ramp measures near zero however light or dark it is — near rather than exactly,
/// because a grey that has been through a 5-bit channel and back is a couple of counts off grey
/// — and a colourised cart measures several times that.
fn mean_saturation(rgba: &[u8]) -> f64 {
    let sum: f64 = rgba
        .chunks_exact(4)
        .map(|p| {
            let (hi, lo) = (p[..3].iter().max(), p[..3].iter().min());
            f64::from(hi.copied().unwrap_or(0) - lo.copied().unwrap_or(0))
        })
        .sum();
    sum / (rgba.len() / 4) as f64
}

fn holds_colour(rgba: &[u8], want: [u8; 3]) -> bool {
    rgba.chunks_exact(4)
        .any(|p| (0..3).all(|c| p[c].abs_diff(want[c]) <= 6))
}

/// How many distinct colours a picture is made of. A Game Boy picture has at most twelve and a
/// blank one has a single black; a Game Boy Color cart has far more. Used only to catch a cart
/// that has not drawn anything yet, which would otherwise pass a comparison by being identically
/// empty in both arms.
fn distinct_colours(rgba: &[u8]) -> usize {
    let mut seen: Vec<[u8; 3]> = Vec::new();
    for p in rgba.chunks_exact(4) {
        let c = [p[0], p[1], p[2]];
        if !seen.contains(&c) {
            seen.push(c);
        }
    }
    seen.len()
}

/// One run of the machine through slot's own `open_core_for`, so the options come from the same
/// `apply_core_options` production calls and not from the test reaching past it.
fn shipped(root: &Path, dylib: &Path, rom: &Path) -> Vec<u8> {
    let mut core = slot::core::open_core_for(
        root,
        Core::Mgba,
        "auto",
        false,
        std::slice::from_ref(&dylib.to_path_buf()),
    );
    core.load(rom).expect("the core would not take the rom");
    for _ in 0..FRAMES {
        core.run_frame(ButtonMask(0));
    }
    to_rgba(core.video_xrgb8888())
}

/// The control: the same core and the same cart with everything `apply_core_options` sets
/// *except* the two palette options, which is mGBA left to its own grayscale default. Opened
/// past `open_core_for` on purpose — it is the one arm that must not be affected by the change
/// this file is about, so it cannot be built by the function under test.
fn without_palette(root: &Path, dylib: &Path, rom: &Path) -> Vec<u8> {
    let mut core = LibretroCore::open_with(
        dylib,
        &slot::root::bios_dir(root),
        &slot::root::saves_dir(root),
    )
    .expect("open the core");
    core.set_option("mgba_frameskip", "auto");
    core.set_option("mgba_sgb_borders", "OFF");
    core.set_option("mgba_color_correction", "OFF");
    core.load(rom).expect("the core would not take the rom");
    for _ in 0..FRAMES {
        core.run_frame(ButtonMask(0));
    }
    to_rgba(core.video_xrgb8888())
}

/// A cart of the user's own, read straight out of the ignored `/sdcard`. Read only: nothing here
/// writes to the card. `None` on a clone that has no card, which is a skip rather than a failure
/// — a stand-in rom paints nothing worth colouring.
fn card_cart(name: &str) -> Option<PathBuf> {
    let p = repo_root().join(name);
    p.exists().then_some(p)
}

/// A monochrome cart comes up in the palette the SP gave a cart its boot ROM had no entry for,
/// and not in mGBA's grey ramp.
///
/// Two carts rather than one, and these two rather than any two, because both are third-party
/// — Asmik's and Atlus's — and the boot ROM only looks up a palette for a cart whose licensee
/// code is Nintendo's. So they take the miss by the first of the two routes to it, which is the
/// route that is safe to assert on: the card's third Game Boy cart is a homebrew that declares
/// Nintendo's licensee code and the title `TETRIS`, so real hardware hands it Nintendo's Tetris
/// palette while mGBA, which keys the same table on a CRC32 of the whole header instead, does
/// not. Asserting the default palette for that cart would be writing down a known disagreement
/// with the hardware as though it were the hardware.
#[test]
fn a_monochrome_cart_comes_up_in_the_colours_the_sp_gave_it() {
    let Some(dylib) = vendored_core() else {
        eprintln!("no mgba dylib, skipping");
        return;
    };
    let _g = core_lock();
    let d = common::tmp_root_with_carts(&[]);

    for cart in ["Catrap (USA).gb", "A-mazing Tater (USA).gb"] {
        let Some(rom) = card_cart(&format!("sdcard/Games/GB/{cart}")) else {
            eprintln!("no {cart} on this machine's card, skipping");
            continue;
        };
        let stem = cart.split_whitespace().next().unwrap_or(cart);
        let off = without_palette(d.path(), &dylib, &rom);
        let on = shipped(d.path(), &dylib, &rom);
        write_png(&format!("gb-palette-{stem}-off"), &off);
        write_png(&format!("gb-palette-{stem}-on"), &on);

        // The control really is a grey ramp, which is what makes the comparison mean anything:
        // if mGBA's default ever stopped being grayscale this number would move and the test
        // would say so rather than quietly comparing two colour pictures. Not zero, because a
        // grey that has been through a 5-bit channel and back is a couple of counts off grey.
        //
        // The threshold below it is deliberately far from both measurements rather than tight
        // around the one taken today: how colourful a cart's title screen is depends on how much
        // of it the cart paints white, which is the cart's business — Catrap's is mostly white
        // and measures about 11, A-mazing Tater's about 15. What has to hold is that a
        // colourised picture is nowhere near a grey one, and the palette assertions below are
        // what pin down that the colour is the right colour.
        let (sat_off, sat_on) = (mean_saturation(&off), mean_saturation(&on));
        println!("{cart}: saturation {sat_off:.2} off, {sat_on:.2} on");
        assert!(
            sat_off < 4.0,
            "{cart}: the control should be grey, saturation was {sat_off:.1}"
        );
        assert!(
            sat_on > 6.0,
            "{cart}: the palette should have coloured this, saturation was {sat_on:.1}"
        );
        // Saturation alone only says *some* colour arrived. These say it is the boot ROM's
        // default background palette and not some other palette off the list of forty-eight.
        for want in DEFAULT_BG {
            assert!(
                holds_colour(&on, want),
                "{cart}: {want:?} is in the boot ROM's default background palette and is not on \
                 screen; some other palette was applied"
            );
        }
    }
}

/// A Game Boy Color cart is left exactly as it was. The two options are declared for monochrome
/// carts only, and a Colour cart carries its own palettes, so this has to be pixel for pixel
/// rather than merely close: anything at all moving here would mean the option had reached a
/// cart it has no business touching.
#[test]
fn a_colour_cart_is_untouched() {
    let Some(dylib) = vendored_core() else {
        eprintln!("no mgba dylib, skipping");
        return;
    };
    let _g = core_lock();
    let d = common::tmp_root_with_carts(&[]);

    let Some(rom) = card_cart("sdcard/Games/GBC/Tetris Chromatic.gbc") else {
        eprintln!("no Game Boy Color cart on this machine's card, skipping");
        return;
    };
    let off = without_palette(d.path(), &dylib, &rom);
    let on = shipped(d.path(), &dylib, &rom);
    write_png("gb-palette-colour-off", &off);
    write_png("gb-palette-colour-on", &on);

    // Guards the comparison itself: a Colour cart that had not drawn anything yet would be two
    // identical black frames and would pass this test without proving a thing. Counted in
    // colours rather than measured in saturation because this cart's title screen is a starfield
    // on black, which is barely saturated on average and unmistakably a Colour picture — no
    // monochrome cart can put more than twelve colours on screen at once.
    let colours = distinct_colours(&on);
    println!("the Colour cart drew {colours} distinct colours");
    assert!(
        colours > 12,
        "the Colour cart has not drawn a colour picture yet ({colours} colours), so comparing \
         the two proves nothing"
    );
    assert_eq!(
        off, on,
        "the Game Boy palette options changed what a Game Boy Color cart draws"
    );
}

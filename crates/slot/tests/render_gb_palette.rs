mod common;

use std::path::{Path, PathBuf};

use common::{core_lock, repo_root, vendored_core};
use slot_retro::{ButtonMask, LibretroCore, RetroCore, GBA_H, GBA_W};
use slot_store::{Core, GbPalette};

const DEFAULT_BG: [[u8; 3]; 3] = [[255, 251, 255], [123, 251, 49], [0, 97, 198]];

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

fn shipped(root: &Path, dylib: &Path, rom: &Path, palette: Option<GbPalette>) -> Vec<u8> {
    let mut core = slot::core::open_core_for_palette(
        root,
        Core::Mgba,
        "auto",
        false,
        palette,
        std::slice::from_ref(&dylib.to_path_buf()),
    );
    core.load(rom).expect("the core would not take the rom");
    for _ in 0..FRAMES {
        core.run_frame(ButtonMask(0));
    }
    to_rgba(core.video_xrgb8888())
}

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

fn card_cart(name: &str) -> Option<PathBuf> {
    let p = repo_root().join(name);
    p.exists().then_some(p)
}

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
        let on = shipped(d.path(), &dylib, &rom, None);
        write_png(&format!("gb-palette-{stem}-off"), &off);
        write_png(&format!("gb-palette-{stem}-on"), &on);

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
        for want in DEFAULT_BG {
            assert!(
                holds_colour(&on, want),
                "{cart}: {want:?} is in the boot ROM's default background palette and is not on \
                 screen; some other palette was applied"
            );
        }
    }
}

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
    let on = shipped(d.path(), &dylib, &rom, None);
    write_png("gb-palette-colour-off", &off);
    write_png("gb-palette-colour-on", &on);

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

fn as_sgb(rom: &Path, out: &Path) -> PathBuf {
    let mut bytes = std::fs::read(rom).expect("read rom");
    bytes[0x146] = 0x03;
    bytes[0x14B] = 0x33;
    let sum = bytes[0x134..=0x14C]
        .iter()
        .fold(0u8, |acc, b| acc.wrapping_sub(*b).wrapping_sub(1));
    bytes[0x14D] = sum;
    let p = out.join("sgb.gb");
    std::fs::write(&p, bytes).expect("write rom");
    p
}

#[test]
fn a_named_palette_colours_plain_and_sgb_carts_alike() {
    let Some(dylib) = vendored_core() else {
        eprintln!("no mgba dylib, skipping");
        return;
    };
    let _g = core_lock();
    let d = common::tmp_root_with_carts(&[]);
    let Some(plain) = card_cart("sdcard/Games/GB/Catrap (USA).gb") else {
        eprintln!("no Catrap on this machine's card, skipping");
        return;
    };
    let sgb = as_sgb(&plain, d.path());
    let grey = GbPalette::parse("Grayscale").unwrap();
    let green = GbPalette::parse("DMG Green").unwrap();

    let auto_sgb = shipped(d.path(), &dylib, &sgb, None);
    let grey_plain = shipped(d.path(), &dylib, &plain, Some(grey));
    let grey_sgb = shipped(d.path(), &dylib, &sgb, Some(grey));
    let green_sgb = shipped(d.path(), &dylib, &sgb, Some(green));
    write_png("gb-palette-sgb-auto", &auto_sgb);
    write_png("gb-palette-plain-grey", &grey_plain);
    write_png("gb-palette-sgb-grey", &grey_sgb);
    write_png("gb-palette-sgb-green", &green_sgb);

    assert!(
        mean_saturation(&grey_plain) < 4.0,
        "Grayscale left colour on a plain cart"
    );
    assert!(
        mean_saturation(&grey_sgb) < 4.0,
        "Grayscale left colour on an SGB cart, so it still ran as a Super Game Boy"
    );
    assert_ne!(grey_sgb, green_sgb, "two palettes drew the same picture");
    assert_ne!(
        grey_sgb, auto_sgb,
        "the palette changed nothing on an SGB cart"
    );
}

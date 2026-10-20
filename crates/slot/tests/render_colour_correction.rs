mod common;

use std::path::{Path, PathBuf};

use common::{core_lock, repo_root, vendored_core};
use slot_retro::{ButtonMask, GBA_H, GBA_W};
use slot_store::{Core, GbPalette, Platform};

fn frames_for(name: &str) -> usize {
    match name {
        "gba" => 240,
        _ => 900,
    }
}

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

fn share_changed(a: &[u8], b: &[u8]) -> f64 {
    let n = a.len() / 4;
    let diff = a
        .chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(p, q)| p[..3] != q[..3])
        .count();
    diff as f64 / n as f64
}

fn picture(
    root: &Path,
    dylib: &Path,
    rom: &Path,
    platform: Platform,
    palette: Option<GbPalette>,
    colour: bool,
    frames: usize,
) -> Vec<u8> {
    let mut core = slot::core::open_core_for_palette(
        root,
        Core::Mgba,
        platform,
        "auto",
        colour,
        palette.map(slot_store::GbColors::Palette),
        std::slice::from_ref(&dylib.to_path_buf()),
    );
    core.load(rom).expect("the core would not take the rom");
    for _ in 0..frames {
        core.run_frame(ButtonMask(0));
    }
    to_rgba(core.video_xrgb8888())
}

fn card_cart(root: &Path, from: &str, to: &str) -> Option<PathBuf> {
    let rom = std::fs::read(repo_root().join(from)).ok()?;
    let at = root.join(to);
    std::fs::write(&at, rom).expect("copy the card's cart");
    Some(at)
}

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
        std::fs::copy(&dylib, d.path().join("System/mgba_libretro.dylib")).expect("plant a core");
        let state = SlotState {
            clock_set: true,
            colour_correction_gba: colour,
            ..SlotState::default()
        };
        write_slot_state(d.path(), &state).expect("write slot.state");

        let mut s = Session::boot(d.path().to_path_buf());
        s.feed([RawEvent::Down(Btn::A)], 16);
        s.feed([RawEvent::Up(Btn::A)], 32);
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut now = 32;
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

#[test]
fn colour_correction_changes_the_picture_on_every_platform() {
    let Some(dylib) = vendored_core() else {
        eprintln!("no mgba dylib, skipping");
        return;
    };
    let _g = core_lock();
    let d = common::tmp_root_with_carts(&[]);

    for (name, from, to, platform, palette) in [
        (
            "gba",
            "sdcard/Games/GBA/Metroid Fusion.gba",
            "Games/GBA/Metroid Fusion.gba",
            Platform::Gba,
            None,
        ),
        (
            "gbc",
            "sdcard/Games/GBC/Pokemon - Crystal Version (USA).gbc",
            "Games/GBC/Pokemon - Crystal Version (USA).gbc",
            Platform::Gbc,
            None,
        ),
        (
            "gb",
            "sdcard/Games/GB/Catrap (USA).gb",
            "Games/GB/Catrap (USA).gb",
            Platform::Gb,
            None,
        ),
    ] {
        let Some(rom) = card_cart(d.path(), from, to) else {
            eprintln!("no {name} cart on this machine's card, skipping it");
            continue;
        };
        let frames = frames_for(name);
        let off = picture(d.path(), &dylib, &rom, platform, palette, false, frames);
        let on = picture(d.path(), &dylib, &rom, platform, palette, true, frames);
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

        assert_ne!(
            off, on,
            "{name}: the picture is byte for byte identical with correction on and off, so \
             either the option never reached the core or it does nothing worth a row"
        );
        let changed = share_changed(&off, &on);
        assert!(
            changed > 0.5,
            "{name}: only {:.1}% of the picture changed, which is not a tint",
            changed * 100.0
        );
        assert!(
            sat_on < sat_off,
            "{name}: correction did not wash the picture out: saturation {sat_off:.1} to \
             {sat_on:.1}"
        );
    }
}

#[test]
fn gbc_colour_correction_still_tints_a_gb_palette() {
    let Some(dylib) = vendored_core() else {
        eprintln!("no mgba dylib, skipping");
        return;
    };
    let _g = core_lock();
    let d = common::tmp_root_with_carts(&[]);
    let Some(rom) = card_cart(
        d.path(),
        "sdcard/Games/GB/Catrap (USA).gb",
        "Games/GB/Catrap (USA).gb",
    ) else {
        eprintln!("no GB cart on this machine's card, skipping");
        return;
    };
    let frames = frames_for("gb");
    let palette = Some(GbPalette::DEFAULT);
    let off = picture(d.path(), &dylib, &rom, Platform::Gb, palette, false, frames);
    let on = picture(d.path(), &dylib, &rom, Platform::Gb, palette, true, frames);
    assert_ne!(
        on, off,
        "GBC correction left a Game Boy picture in a GB palette untouched"
    );
}

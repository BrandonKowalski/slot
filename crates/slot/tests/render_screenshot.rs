#![cfg(target_os = "macos")]

mod common;

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use common::{clocked, tmp_root_with_carts};
use slot::frontend::Frontend;
use slot_gfx::{Compositor, HeadlessSurface};
use slot_input::{Btn, InputSource, Millis, RawEvent};
use slot_power::SimPlatform;
use slot_store::{Platform, ShotDir};
use slot_ui::{QuickRow, Toast, PLATE_H};

struct Script(VecDeque<Vec<RawEvent>>);

impl InputSource for Script {
    fn poll(&mut self, _now: Millis) -> Vec<RawEvent> {
        self.0.pop_front().unwrap_or_default()
    }
}

fn press(f: &mut Frontend, input: &mut Script, events: &[RawEvent]) {
    for ev in events {
        input.0.push_back(vec![*ev]);
        f.advance(input);
    }
}

fn tap(f: &mut Frontend, input: &mut Script, btn: Btn) {
    press(f, input, &[RawEvent::Down(btn), RawEvent::Up(btn)]);
}

fn select_with(f: &mut Frontend, input: &mut Script, btn: Btn) {
    press(
        f,
        input,
        &[
            RawEvent::Down(Btn::Select),
            RawEvent::Down(btn),
            RawEvent::Up(btn),
            RawEvent::Up(Btn::Select),
        ],
    );
}

fn run_for(f: &mut Frontend, c: &mut Compositor, input: &mut Script, window: (u32, u32), secs: f32) {
    let until = Instant::now() + Duration::from_secs_f32(secs);
    while Instant::now() < until {
        f.advance(input);
        f.render(c, window);
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn scratch(name: &str, w: u32, h: u32, rgba: &[u8]) {
    let Ok(dir) = std::env::var("SCRATCH_PNG_DIR") else {
        return;
    };
    let file = std::fs::File::create(format!("{dir}/{name}.png")).expect("create png");
    let mut e = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header()
        .expect("png header")
        .write_image_data(rgba)
        .expect("png data");
}

fn newest_shot(root: &std::path::Path) -> (u32, u32, Vec<u8>) {
    let shots = ShotDir::new(root, Platform::Gba, "Emerald")
        .list()
        .expect("list shots");
    let newest = shots.first().expect("no shot was written");
    let decoder = png::Decoder::new(std::io::BufReader::new(
        std::fs::File::open(&newest.path).expect("open shot"),
    ));
    let mut reader = decoder.read_info().expect("png header");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("png data");
    assert_eq!(info.color_type, png::ColorType::Rgb);
    buf.truncate(info.buffer_size());
    (info.width, info.height, buf)
}

fn worst_gap(rgb: &[u8], rgba: &[u8], rows: std::ops::Range<usize>, w: usize) -> u8 {
    rows.flat_map(|y| (0..w).map(move |x| y * w + x))
        .flat_map(|i| (0..3).map(move |c| rgb[i * 3 + c].abs_diff(rgba[i * 4 + c])))
        .max()
        .unwrap_or(0)
}

fn a_shot_is_the_screen_without_the_toast_or_the_tint(window: (u32, u32), name: &str) {
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
    f.upload_bezel(&mut c, window);
    let mut input = Script(VecDeque::new());
    run_for(&mut f, &mut c, &mut input, window, 0.2);
    tap(&mut f, &mut input, Btn::A);
    run_for(&mut f, &mut c, &mut input, window, 4.0);
    for _ in 0..20 {
        select_with(&mut f, &mut input, Btn::Right);
    }
    run_for(&mut f, &mut c, &mut input, window, 3.0);
    assert!(f.app().blue_light() > 0, "blue light did not go up");

    select_with(&mut f, &mut input, Btn::R1);
    assert_eq!(f.app().toast(), Some(Toast::StateSaved));
    run_for(&mut f, &mut c, &mut input, window, 0.3);
    select_with(&mut f, &mut input, Btn::X);
    assert!(f.app().shot_requested(), "SELECT+X did not ask for a shot");
    f.render(&mut c, window);
    let screen = c.read_frame();
    let panel = c.read_window(window);
    f.app().flush_shots();
    assert!(!f.app().shot_requested(), "the frame did not take the shot");
    assert_eq!(f.app().toast(), Some(Toast::ShotCaptured));
    let (w, h, shot) = newest_shot(d.path());
    assert_eq!((w, h), window, "the shot is not the size of the panel");
    let shot_rgba: Vec<u8> = shot.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
    scratch(&format!("shot-{name}-shot"), w, h, &shot_rgba);
    scratch(&format!("shot-{name}-screen"), w, h, &screen);

    let (w, h) = (w as usize, h as usize);
    let band = PLATE_H as usize;
    let below = worst_gap(&shot, &screen, band..h, w);
    assert!(below <= 2, "below the toast the shot differs from the screen by {below}");
    let toast = worst_gap(&shot, &screen, 0..band, w);
    assert!(toast > 60, "the screen shows no toast the shot left out: {toast}");
    let tint = worst_gap(&shot, &panel, band..h, w);
    assert!(tint > 10, "the blue light did not tint the panel, so this proves nothing: {tint}");

    run_for(&mut f, &mut c, &mut input, window, 0.5);
    select_with(&mut f, &mut input, Btn::X);
    f.render(&mut c, window);
    f.app().flush_shots();
    let (_, _, newer) = newest_shot(d.path());
    let apart = worst_gap(&shot, &newer.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect::<Vec<_>>(), band..h, w);
    assert!(apart > 10, "the two shots are the same, so the late decode proves nothing");
    let newer_rgba: Vec<u8> = newer.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect();

    tap(&mut f, &mut input, Btn::Menu);
    run_for(&mut f, &mut c, &mut input, window, 0.5);
    for _ in 0..6 {
        if f.app().in_game_menu() == Some(QuickRow::Screenshots) {
            break;
        }
        tap(&mut f, &mut input, Btn::Down);
    }
    assert_eq!(f.app().in_game_menu(), Some(QuickRow::Screenshots));
    tap(&mut f, &mut input, Btn::A);
    run_for(&mut f, &mut c, &mut input, window, 0.2);
    assert!(f.app().shot_viewer().is_some(), "the viewer did not open");
    run_until(&mut f, &mut c, &mut input, window, |f| {
        f.app().shot_viewer().and_then(|v| v.thumb(0)).is_some()
    });
    let grid = c.read_frame();
    scratch(&format!("shot-{name}-grid"), w as u32, h as u32, &grid);
    let canvas = slot_ui::Canvas { w: w as f32, h: h as f32 };
    let (cw, ch) = slot_ui::grid_cell(canvas);
    let (thumb, tw, th) = slot_ui::thumbnail(&newer_rgba, w as u32, h as u32, cw as u32, ch as u32);
    let (cx, cy) = (16.0 + cw / 2.0, 56.0 + ch / 2.0);
    let screen = ((cy as usize) * w + cx as usize) * 4;
    let t = (((th / 2) * tw + tw / 2) * 4) as usize;
    let gap = (0..3).map(|i| grid[screen + i].abs_diff(thumb[t + i])).max().unwrap();
    assert!(gap <= 24, "the first cell does not show its shot: off by {gap}");
    let edge = ((cy as usize) * w + 12) * 4;
    assert!(grid[edge..edge + 3].iter().all(|&v| v >= 240), "no white box round the newest shot");

    tap(&mut f, &mut input, Btn::A);
    f.render(&mut c, window);
    tap(&mut f, &mut input, Btn::Right);
    run_until(&mut f, &mut c, &mut input, window, |f| {
        f.app().shot_viewer().is_some_and(|v| v.has_face())
    });
    let viewed = c.read_frame();
    scratch(&format!("shot-{name}-full"), w as u32, h as u32, &viewed);
    let middle = worst_gap(&shot, &viewed, band..h - band, w);
    assert!(middle <= 2, "the viewer does not show the shot as taken: off by {middle}");

    tap(&mut f, &mut input, Btn::Left);
    tap(&mut f, &mut input, Btn::Right);
    run_until(&mut f, &mut c, &mut input, window, |f| {
        f.app().shot_viewer().is_some_and(|v| v.has_face())
    });

    tap(&mut f, &mut input, Btn::A);
    run_for(&mut f, &mut c, &mut input, window, 0.2);
    let zoomed = c.read_frame();
    scratch(&format!("shot-{name}-zoom"), w as u32, h as u32, &zoomed);
    let worst = (band..h - band)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .flat_map(|(x, y)| {
            let s = ((h / 4 + y / 2) * w + w / 4 + x / 2) * 3;
            let z = (y * w + x) * 4;
            (0..3).map(move |i| (s + i, z + i))
        })
        .map(|(s, z)| shot[s].abs_diff(zoomed[z]))
        .max()
        .unwrap_or(0);
    assert!(worst <= 2, "the 2x zoom is not the shot's middle, nearest: off by {worst}");
}

fn run_until(
    f: &mut Frontend,
    c: &mut Compositor,
    input: &mut Script,
    window: (u32, u32),
    done: impl Fn(&Frontend) -> bool,
) {
    let until = Instant::now() + Duration::from_secs(5);
    while !done(f) {
        assert!(Instant::now() < until, "timed out waiting for the gallery");
        f.advance(input);
        f.render(c, window);
        std::thread::sleep(Duration::from_millis(2));
    }
    f.render(c, window);
}

#[test]
fn a_shot_on_the_rg_sp_is_the_screen_without_the_toast_or_the_tint() {
    a_shot_is_the_screen_without_the_toast_or_the_tint((720, 480), "rgsp");
}

#[test]
fn a_shot_on_the_rg35xxsp_is_the_screen_without_the_toast_or_the_tint() {
    a_shot_is_the_screen_without_the_toast_or_the_tint((640, 480), "rg35xxsp");
}

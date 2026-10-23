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
use slot_ui::{QuickRow, ShotMode};

const SHOTS: usize = 1000;
const WINDOW: (u32, u32) = (720, 480);
const SLOW_FRAME: Duration = Duration::from_millis(60);
const MAX_TEXTURES: usize = 18;

struct Script(VecDeque<Vec<RawEvent>>);

impl InputSource for Script {
    fn poll(&mut self, _now: Millis) -> Vec<RawEvent> {
        self.0.pop_front().unwrap_or_default()
    }
}

struct Rig {
    f: Frontend,
    c: Compositor,
    input: Script,
    slowest: Duration,
    _root: tempfile::TempDir,
    _surface: HeadlessSurface,
}

impl Rig {
    fn frame(&mut self) {
        let at = Instant::now();
        self.f.advance(&mut self.input);
        self.f.render(&mut self.c, WINDOW);
        self.slowest = self.slowest.max(at.elapsed());
        assert!(
            self.f.gallery_textures() <= MAX_TEXTURES,
            "{} thumbnail textures alive",
            self.f.gallery_textures()
        );
    }

    fn send(&mut self, ev: RawEvent) {
        self.input.0.push_back(vec![ev]);
        self.frame();
    }

    fn tap(&mut self, btn: Btn) {
        self.send(RawEvent::Down(btn));
        self.send(RawEvent::Up(btn));
    }

    fn run_for(&mut self, secs: f32) {
        let until = Instant::now() + Duration::from_secs_f32(secs);
        while Instant::now() < until {
            self.frame();
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn run_until(&mut self, what: &str, done: impl Fn(&Frontend) -> bool) {
        let until = Instant::now() + Duration::from_secs(5);
        while !done(&self.f) {
            assert!(Instant::now() < until, "timed out waiting for {what}");
            self.frame();
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn hold(&mut self, btn: Btn, secs: f32) {
        self.send(RawEvent::Down(btn));
        self.run_for(secs);
        self.send(RawEvent::Up(btn));
    }

    fn reset_slowest(&mut self) -> Duration {
        std::mem::take(&mut self.slowest)
    }
}

fn scratch(name: &str, rgba: &[u8]) {
    let Ok(dir) = std::env::var("SCRATCH_PNG_DIR") else {
        return;
    };
    let file = std::fs::File::create(format!("{dir}/{name}.png")).expect("create png");
    let mut e = png::Encoder::new(std::io::BufWriter::new(file), WINDOW.0, WINDOW.1);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(rgba).unwrap();
}

fn shots_on_card(root: &std::path::Path) {
    let dir = ShotDir::new(root, Platform::Gba, "Emerald");
    let pngs: Vec<Vec<u8>> = (0..8u8)
        .map(|k| {
            let rgb: Vec<u8> = (0..WINDOW.0 * WINDOW.1)
                .flat_map(|i| [k * 30, (i % 251) as u8, 255 - k * 30])
                .collect();
            let mut out = Vec::new();
            let mut e = png::Encoder::new(&mut out, WINDOW.0, WINDOW.1);
            e.set_color(png::ColorType::Rgb);
            e.set_depth(png::BitDepth::Eight);
            e.write_header().unwrap().write_image_data(&rgb).unwrap();
            out
        })
        .collect();
    let first = dir.path("2026-01-01_00-00-00-000");
    std::fs::create_dir_all(first.parent().unwrap()).unwrap();
    for i in 0..SHOTS {
        let stamp = format!(
            "2026-01-01_{:02}-{:02}-{:02}-{:03}",
            i / 3600,
            (i / 60) % 60,
            i % 60,
            i % 1000
        );
        std::fs::write(dir.path(&stamp), &pngs[i % pngs.len()]).unwrap();
    }
    assert_eq!(dir.list().unwrap().len(), SHOTS);
}

fn rig() -> Option<Rig> {
    let Some(surface) = HeadlessSurface::new().ok() else {
        eprintln!("render_gallery_stress: no GPU surface, skipped");
        return None;
    };
    let Some(c) = Compositor::new(&surface).ok() else {
        eprintln!("render_gallery_stress: no compositor, skipped");
        return None;
    };
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    clocked(d.path());
    shots_on_card(d.path());
    let mut f = Frontend::boot(Box::new(SimPlatform::at(d.path().to_path_buf())));
    let mut c = c;
    f.upload_faces(&mut c);
    f.upload_bezel(&mut c, WINDOW);
    let mut r = Rig {
        f,
        c,
        input: Script(VecDeque::new()),
        slowest: Duration::ZERO,
        _root: d,
        _surface: surface,
    };
    r.run_for(0.2);
    r.tap(Btn::A);
    r.run_for(4.0);
    Some(r)
}

fn open_gallery(r: &mut Rig) {
    r.tap(Btn::Menu);
    r.run_for(0.3);
    for _ in 0..6 {
        if r.f.app().in_game_menu() == Some(QuickRow::Screenshots) {
            break;
        }
        r.tap(Btn::Down);
    }
    assert_eq!(r.f.app().in_game_menu(), Some(QuickRow::Screenshots));
    r.tap(Btn::A);
    assert!(r.f.app().shot_viewer().is_some(), "the gallery did not open");
    r.run_until("the list", |f| f.app().shot_viewer().is_some_and(|v| !v.is_empty()));
}

fn shown_thumbs_loaded(f: &Frontend) -> bool {
    let Some(v) = f.app().shot_viewer() else {
        return false;
    };
    let first = v.top_row() * slot_ui::GRID_COLS;
    (first..(first + 9).min(v.entries.len())).all(|i| v.thumb(i).is_some())
}

fn ink(frame: &[u8], (x0, y0, w, h): (usize, usize, usize, usize)) -> (usize, usize) {
    let px = (y0..y0 + h).flat_map(|y| (x0..x0 + w).map(move |x| (y * WINDOW.0 as usize + x) * 4));
    px.fold((0, 0), |(light, dark), i| {
        let p = &frame[i..i + 3];
        match (p.iter().all(|&v| v > 200), p.iter().all(|&v| v < 80)) {
            (true, _) => (light + 1, dark),
            (_, true) => (light, dark + 1),
            _ => (light, dark),
        }
    })
}

fn mode(r: &Rig) -> Option<ShotMode> {
    r.f.app().shot_viewer().map(|v| v.mode())
}

fn a_thousand_shots_never_stall_the_gallery() {
    let Some(mut r) = rig() else {
        return;
    };
    r.reset_slowest();
    open_gallery(&mut r);
    r.run_until("the first thumbnails", shown_thumbs_loaded);
    let open = r.reset_slowest();
    assert!(open < Duration::from_millis(150), "opening took a {open:?} frame");
    r.run_for(0.5);
    let decoded = r.f.gallery_decodes();
    assert!(decoded <= 15, "opening decoded {decoded} shots, not just the window");

    r.hold(Btn::Down, 3.0);
    let scroll = r.reset_slowest();
    let index = r.f.app().shot_viewer().unwrap().index;
    assert!(index > 30, "holding Down only reached shot {index}");
    r.run_for(0.5);
    let decoded = r.f.gallery_decodes();
    let rows = index / slot_ui::GRID_COLS;
    assert!(
        decoded <= 15 + 3 * rows + 3,
        "{decoded} decodes for {rows} rows scrolled"
    );
    assert!(scroll < SLOW_FRAME, "a {scroll:?} frame while scrolling");
    r.run_until("thumbnails after scrolling", shown_thumbs_loaded);

    r.hold(Btn::Up, 1.0);
    r.hold(Btn::Right, 1.0);
    r.run_until("thumbnails after scrolling back", shown_thumbs_loaded);
    assert!(r.reset_slowest() < SLOW_FRAME);

    r.tap(Btn::A);
    assert_eq!(mode(&r), Some(ShotMode::Full));
    r.hold(Btn::Right, 2.0);
    r.run_until("the full shot", |f| f.app().shot_viewer().is_some_and(|v| v.has_face()));
    r.tap(Btn::A);
    r.tap(Btn::A);
    r.tap(Btn::A);
    assert_eq!(r.f.app().shot_viewer().map(|v| v.zoom()), Some(4));
    r.frame();
    let zoomed = r.c.read_frame();
    scratch("gallery-zoomed", &zoomed);
    let (light, dark) = ink(&zoomed, (16, 450, 20, 20));
    assert!(light > 40 && dark > 10, "the Pan cap has no arrows: {light} light, {dark} dark");
    r.send(RawEvent::Down(Btn::Up));
    r.send(RawEvent::Down(Btn::Left));
    r.run_for(1.0);
    r.send(RawEvent::Up(Btn::Up));
    r.send(RawEvent::Up(Btn::Left));
    let viewing = r.reset_slowest();
    assert!(viewing < SLOW_FRAME, "a {viewing:?} frame stepping and panning");
    for _ in 0..4 {
        r.tap(Btn::B);
    }
    assert_eq!(mode(&r), Some(ShotMode::Grid));
}

fn opening_and_closing_fast_leaves_the_gallery_whole() {
    let Some(mut r) = rig() else {
        return;
    };
    open_gallery(&mut r);
    r.reset_slowest();
    for _ in 0..40 {
        r.tap(Btn::B);
        r.tap(Btn::A);
        r.tap(Btn::Down);
    }
    assert!(r.f.app().shot_viewer().is_some());
    r.run_until("the list after the churn", |f| {
        f.app().shot_viewer().is_some_and(|v| !v.is_empty())
    });
    assert_eq!(mode(&r), Some(ShotMode::Grid));
    r.run_until("thumbnails after the churn", shown_thumbs_loaded);
    let churn = r.reset_slowest();
    assert!(churn < SLOW_FRAME, "a {churn:?} frame opening and closing");
}

fn deleting_hundreds_at_once_does_not_freeze() {
    let Some(mut r) = rig() else {
        return;
    };
    open_gallery(&mut r);
    r.tap(Btn::Y);
    r.frame();
    let one = r.c.read_frame();
    r.tap(Btn::B);
    assert!(!r.f.app().delete_prompt());
    r.tap(Btn::Select);
    assert!(r.f.app().shot_viewer().is_some_and(|v| v.selecting()));
    for _ in 0..300 {
        r.tap(Btn::A);
        r.tap(Btn::Right);
    }
    assert_eq!(r.f.app().shot_viewer().map(|v| v.ticked_count()), Some(300));
    r.run_until("the ticked rows' thumbnails", shown_thumbs_loaded);
    let ticked = r.c.read_frame();
    scratch("gallery-ticked", &ticked);
    let (cw, _) = slot_ui::grid_cell(slot_ui::Canvas { w: WINDOW.0 as f32, h: WINDOW.1 as f32 });
    let badge = ((16.0 + cw - 26.0) as usize, 62, 20, 20);
    let (light, dark) = ink(&ticked, badge);
    assert!(light > 40 && dark > 10, "no tick badge: {light} light, {dark} dark");
    r.tap(Btn::Y);
    assert!(r.f.app().delete_prompt());
    r.frame();
    let many = r.c.read_frame();
    scratch("gallery-delete-prompt", &many);
    let title = |f: &[u8]| ink(f, (0, 180, WINDOW.0 as usize, 60)).0;
    assert!(title(&many) > title(&one), "the prompt does not say how many");
    r.reset_slowest();
    r.tap(Btn::X);
    let delete = r.reset_slowest();
    assert!(delete < SLOW_FRAME, "a {delete:?} frame deleting 300");
    assert_eq!(r.f.app().shot_viewer().map(|v| v.entries.len()), Some(SHOTS - 300));
    r.run_until("thumbnails after the delete", shown_thumbs_loaded);
    r.reset_slowest();
    r.tap(Btn::B);
    r.tap(Btn::B);
    open_gallery(&mut r);
    r.run_until("the list after the delete", |f| {
        f.app().shot_viewer().is_some_and(|v| !v.is_empty())
    });
    assert_eq!(r.f.app().shot_viewer().map(|v| v.entries.len()), Some(SHOTS - 300));
    let after = r.reset_slowest();
    assert!(after < SLOW_FRAME, "a {after:?} frame reopening after the delete");
    r.f.app().flush_shots();
    let left = ShotDir::new(r._root.path(), Platform::Gba, "Emerald").list().unwrap().len();
    assert_eq!(left, SHOTS - 300);
}

#[test]
fn the_gallery_holds_up_under_a_thousand_shots() {
    a_thousand_shots_never_stall_the_gallery();
    opening_and_closing_fast_leaves_the_gallery_whole();
    deleting_hundreds_at_once_does_not_freeze();
}


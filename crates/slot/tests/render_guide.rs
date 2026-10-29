#![cfg(target_os = "macos")]

mod common;

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use common::{clocked, tmp_root_with_carts};
use slot::frontend::Frontend;
use slot_gfx::{Compositor, HeadlessSurface};
use slot_input::{Btn, InputSource, Millis, RawEvent};
use slot_power::SimPlatform;
use slot_ui::{page_box, Canvas, QuickRow};

struct Script(VecDeque<Vec<RawEvent>>);

impl InputSource for Script {
    fn poll(&mut self, _now: Millis) -> Vec<RawEvent> {
        self.0.pop_front().unwrap_or_default()
    }
}

fn tap(f: &mut Frontend, input: &mut Script, btn: Btn) {
    input.0.push_back(vec![RawEvent::Down(btn)]);
    f.advance(input);
    input.0.push_back(vec![RawEvent::Up(btn)]);
    f.advance(input);
}

fn run_for(f: &mut Frontend, input: &mut Script, secs: f32) {
    let until = Instant::now() + Duration::from_secs_f32(secs);
    while Instant::now() < until {
        f.advance(input);
        std::thread::sleep(Duration::from_millis(2));
    }
}

const GUIDE: &str = "\
==============================================================================
 EMERALD WALKTHROUGH                                          Version 1.02
==============================================================================

 Table of Contents
 -----------------
   1. Controls ................................................. [CTRL]
   2. Walkthrough .............................................. [WALK]
   3. Item List ................................................ [ITEM]

+-----------+--------------------------------+-------------------------------+
| Button    | Field                          | Battle                        |
+-----------+--------------------------------+-------------------------------+
| A         | Talk, check, confirm           | Choose a move                 |
| B         | Cancel, hold to run            | Back out of a menu            |
| Start     | Open the menu                  | Nothing                       |
+-----------+--------------------------------+-------------------------------+

 2. Walkthrough                                                        [WALK]
 ----------------------------------------------------------------------------
 Leave the house and head north. The path splits at the old sign; take the
 left fork and keep to the tall grass edge until the bridge comes into view.
\u{250c}\u{2500}\u{2500}\u{2500}\u{2500}\u{2510}  Box art survives: caf\u{e9}, na\u{ef}ve.
\u{2514}\u{2500}\u{2500}\u{2500}\u{2500}\u{2518}
";

fn guide_frame(panel: (u32, u32), name: &str) -> (Vec<u8>, Canvas) {
    let surface = HeadlessSurface::new().expect("surface");
    let mut c = Compositor::new(&surface).expect("compositor");
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    clocked(d.path());
    let dir = d.path().join("Guides/GBA");
    std::fs::create_dir_all(&dir).unwrap();
    let long: String = (0..400).map(|i| format!(" filler line {i}\n")).collect();
    std::fs::write(dir.join("Emerald.txt"), format!("{GUIDE}{long}")).unwrap();
    let mut f = Frontend::boot(Box::new(SimPlatform::at(d.path().to_path_buf())));
    f.upload_faces(&mut c);
    f.upload_bezel(&mut c, panel);
    c.fit(panel);
    let mut input = Script(VecDeque::new());
    tap(&mut f, &mut input, Btn::A);
    run_for(&mut f, &mut input, 4.0);
    tap(&mut f, &mut input, Btn::Menu);
    run_for(&mut f, &mut input, 0.4);
    for _ in 0..10 {
        if f.app().in_game_menu() == Some(QuickRow::Guide) {
            break;
        }
        tap(&mut f, &mut input, Btn::Down);
    }
    assert_eq!(f.app().in_game_menu(), Some(QuickRow::Guide));
    tap(&mut f, &mut input, Btn::A);
    run_for(&mut f, &mut input, 0.3);
    assert!(f.app().guide().is_some(), "the guide did not open");
    f.compose(&mut c);
    let (w, h) = c.frame_size();
    let px = c.read_frame();
    if let Ok(dir) = std::env::var("SCRATCH_PNG_DIR") {
        let file = std::fs::File::create(format!("{dir}/guide-{name}.png")).expect("png");
        let mut e = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header()
            .expect("png header")
            .write_image_data(&px)
            .expect("png data");
    }
    tap(&mut f, &mut input, Btn::Menu);
    run_for(&mut f, &mut input, 0.5);
    assert!(
        f.app().guide().is_some_and(|g| g.help()),
        "help did not open"
    );
    f.compose(&mut c);
    let help = c.read_frame();
    let through = (50..430)
        .flat_map(|y| (16..120).map(move |x| (x, y)))
        .filter(|&(x, y)| help[(y * w as usize + x) * 4] > 0x30)
        .count();
    assert_eq!(through, 0, "{name}: the guide shows through the help");
    let lit = |xs: std::ops::Range<usize>, ys: std::ops::Range<usize>| {
        ys.flat_map(|y| xs.clone().map(move |x| (x, y)))
            .filter(|&(x, y)| help[(y * w as usize + x) * 4] > 0x60)
            .count()
    };
    let (wu, hu) = (w as usize, h as usize);
    assert_eq!(
        lit(0..140, 0..40),
        0,
        "{name}: the battery shows on the help"
    );
    assert_eq!(
        lit(wu - 140..wu, 0..40),
        0,
        "{name}: the clock shows on the help"
    );
    assert_eq!(
        lit(wu - 200..wu, hu - 40..hu),
        0,
        "{name}: MENU HELP shows on the help"
    );
    assert!(
        lit(0..140, hu - 40..hu) > 0,
        "{name}: B BACK is missing from the help"
    );
    if let Ok(dir) = std::env::var("SCRATCH_PNG_DIR") {
        let file = std::fs::File::create(format!("{dir}/guide-{name}-help.png")).expect("png");
        let mut e = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header()
            .expect("png header")
            .write_image_data(&help)
            .expect("png data");
    }
    (px, f.app().canvas())
}

fn f_px(canvas: Canvas) -> f32 {
    slot_ui::GUIDE_ZOOMS[slot_ui::fitting_zoom(canvas) as usize]
}

#[test]
fn the_guide_fills_the_page_with_its_text_on_both_panels() {
    for (panel, name) in [((720, 480), "3x2"), ((640, 480), "4x3")] {
        let (px, canvas) = guide_frame(panel, name);
        let w = canvas.w as usize;
        let (x, y, pw, _) = page_box(canvas);
        let adv = slot_ui::cell(f_px(canvas)).0;
        let first = y as usize..y as usize + 12;
        let lit: Vec<usize> = (0..x as usize + pw as usize)
            .filter(|&cx| {
                first.clone().any(|cy| {
                    let o = (cy * w + cx) * 4;
                    px[o..o + 3].iter().all(|&c| c > 0x60)
                })
            })
            .collect();
        let (left, right) = (lit[0] as f32, *lit.last().unwrap() as f32);
        assert!((left - x).abs() < adv, "{name}: the rule starts at {left}");
        let end = x + 78.0 * adv;
        assert!(
            (right - end).abs() < adv,
            "{name}: the 78 column rule ends at {right}, want about {end}"
        );
        let gutter = (0..x as usize - 2)
            .filter(|&gx| (y as usize..y as usize + 200).any(|gy| px[(gy * w + gx) * 4] > 0x40))
            .count();
        assert_eq!(gutter, 0, "{name}: text spilled into the left gutter");
    }
}

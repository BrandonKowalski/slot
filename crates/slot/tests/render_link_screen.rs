//! The link screen's art, through `draw_link_art`, composited in software.
//!
//! `SCRATCH_PNG_DIR=/tmp cargo test -p slot --test render_link_screen -- --nocapture`

mod common;

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};

use slot::app::{GameMenu, LinkLegend, LinkRow};
use slot::link_kind::LinkKind;
use slot::link_net::Cancel;
use slot::link_radio::{LinkRole, RadioJob, RadioJobs};
use slot::link_screen::{draw_link_art, LinkSprites, Sprite};
use slot::link_start::{LinkFail, LinkStarter, LinkStep};
use slot_input::Action;
use slot_store::Core;
use slot_ui::{
    arrows_hint_face, hint_face, link_art, menu_face, toast_face, CartFace, Draw, TexId, Toast,
    UndoFace, OUT_H, OUT_W,
};

/// One rastered face, whatever rasterised it. `CartFace` and `UndoFace` are the same three
/// fields under two names — the art builders hand back one and the key caps the other — and the
/// compositor below wants only the pixels and their size, so both arrive here.
struct Face {
    rgba: Vec<u8>,
    w: u32,
    h: u32,
}

impl From<CartFace> for Face {
    fn from(f: CartFace) -> Self {
        Face {
            rgba: f.rgba,
            w: f.w,
            h: f.h,
        }
    }
}

impl From<UndoFace> for Face {
    fn from(f: UndoFace) -> Self {
        Face {
            rgba: f.rgba,
            w: f.w,
            h: f.h,
        }
    }
}

fn sprites_and_faces() -> (LinkSprites, Vec<(TexId, Face)>) {
    let art = link_art();
    let mut faces = Vec::new();
    let mut n = 0;
    let mut put = |f: CartFace| {
        n += 1;
        let tex = TexId::from_raw(n);
        let sprite = Sprite {
            tex,
            w: f.w,
            h: f.h,
        };
        faces.push((tex, f.into()));
        sprite
    };
    let [ar0, ar1, ar2] = art.arcs_right;
    let [al0, al1, al2] = art.arcs_left;
    let sprites = LinkSprites {
        port: put(art.port),
        plug_host: put(art.plug_host),
        plug_join: put(art.plug_join),
        adapter: put(art.adapter),
        arcs_right: [put(ar0), put(ar1), put(ar2)],
        arcs_left: [put(al0), put(al1), put(al2)],
        clicks: put(art.clicks),
        arrow_left: put(art.arrow_left),
        arrow_right: put(art.arrow_right),
    };
    (sprites, faces)
}

/// Straight-alpha over-compositing, nearest neighbour, with `Turned` rotated about its centre.
fn composite(out: &[Draw], faces: &[(TexId, Face)]) -> Vec<u8> {
    let (w, h) = (OUT_W as usize, OUT_H as usize);
    let mut px = vec![0u8; w * h * 4];
    for i in 0..w * h {
        px[i * 4..i * 4 + 4].copy_from_slice(&[0x05, 0x05, 0x08, 255]);
    }
    let blend = |x: i32, y: i32, c: [u8; 4], alpha: f32, px: &mut Vec<u8>| {
        if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
            return;
        }
        let a = (c[3] as f32 / 255.0) * alpha;
        let d = (y as usize * w + x as usize) * 4;
        for (k, cc) in c.iter().take(3).enumerate() {
            px[d + k] = (*cc as f32 * a + px[d + k] as f32 * (1.0 - a)).round() as u8;
        }
    };
    for d in out {
        let (x, y, dw, dh, tex, alpha, turn) = match *d {
            Draw::Tex {
                x,
                y,
                w,
                h,
                tex,
                alpha,
            } => (x, y, w, h, tex, alpha, 0.0),
            Draw::Turned {
                x,
                y,
                w,
                h,
                tex,
                alpha,
                turn,
            } => (x, y, w, h, tex, alpha, turn),
            _ => continue,
        };
        let face = &faces
            .iter()
            .find(|(t, _)| *t == tex)
            .expect("unknown tex")
            .1;
        let (cx, cy) = (x + dw / 2.0, y + dh / 2.0);
        let (sin, cos) = turn.sin_cos();
        let reach = (dw.max(dh) * 0.75) as i32;
        for py in (cy as i32 - reach)..(cy as i32 + reach) {
            for qx in (cx as i32 - reach)..(cx as i32 + reach) {
                let (rx, ry) = (qx as f32 + 0.5 - cx, py as f32 + 0.5 - cy);
                let (ux, uy) = (
                    rx * cos + ry * sin + dw / 2.0,
                    -rx * sin + ry * cos + dh / 2.0,
                );
                if ux < 0.0 || uy < 0.0 || ux >= face.w as f32 || uy >= face.h as f32 {
                    continue;
                }
                let s = (uy as usize * face.w as usize + ux as usize) * 4;
                let c = [
                    face.rgba[s],
                    face.rgba[s + 1],
                    face.rgba[s + 2],
                    face.rgba[s + 3],
                ];
                blend(qx, py, c, alpha, &mut px);
            }
        }
    }
    px
}

fn at(px: &[u8], x: usize, y: usize) -> [u8; 3] {
    let o = (y * OUT_W as usize + x) * 4;
    [px[o], px[o + 1], px[o + 2]]
}

fn render(menu: GameMenu, kind: LinkKind, now: u64, name: &str) -> Vec<u8> {
    let (sprites, faces) = sprites_and_faces();
    let mut out = Vec::new();
    draw_link_art(menu, kind, now, &sprites, &mut out);
    let px = composite(&out, &faces);
    dump(&px, name);
    px
}

/// The composited frame, written out to look at when `SCRATCH_PNG_DIR` names somewhere to put it.
fn dump(px: &[u8], name: &str) {
    let Ok(dir) = std::env::var("SCRATCH_PNG_DIR") else {
        return;
    };
    let path = format!("{dir}/link-{name}.png");
    let file = std::fs::File::create(&path).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(file), OUT_W, OUT_H);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(px).unwrap();
    println!("wrote {path}");
}

#[test]
fn the_host_picks_a_purple_plug_over_the_port() {
    let px = render(
        GameMenu::Pick(LinkRow::Host),
        LinkKind::Cable,
        0,
        "cable-host-pick",
    );
    let housing = at(&px, 356, 330 - 60);
    assert!(
        housing[2] > housing[1] + 30,
        "no purple housing above the port: {housing:?}"
    );
    // R11: (360, 396) lands on the port's middle gold pin (pins sit at y 7-12, x 359-364 on
    // the port face); sample the dark slot below the pins instead.
    let slot = at(&px, 360, 406);
    assert!(
        slot.iter().all(|c| *c < 0x14),
        "no port under the plug: {slot:?}"
    );
}

#[test]
fn a_joiner_picks_a_gray_plug() {
    let px = render(
        GameMenu::Pick(LinkRow::Join),
        LinkKind::Cable,
        0,
        "cable-join-pick",
    );
    let housing = at(&px, 356, 330 - 60);
    assert!(
        (housing[0] as i32 - housing[2] as i32).abs() < 14 && housing[0] > 0x70,
        "not gray: {housing:?}"
    );
}

#[test]
fn a_wireless_link_seats_the_adapter_with_its_label_plate() {
    let menu = GameMenu::Linked {
        role: LinkRow::Host,
        worked: 0,
        since: 600,
        opened: false,
    };
    let px = render(menu, LinkKind::Wireless, 2000, "wireless-linked");
    let plate = at(&px, 360, 388 - 12);
    assert!(
        plate[0] > 0x28 && plate[0] < 0x70,
        "no plate where the seated adapter's label goes: {plate:?}"
    );
}

#[test]
fn a_failed_plug_leaves_where_it_waited() {
    let working = GameMenu::Working {
        role: LinkRow::Host,
        step: LinkStep::Waiting,
        since: 0,
    };
    let waiting = render(working, LinkKind::Cable, 1200, "cable-working");
    let failed = GameMenu::Failed {
        role: LinkRow::Host,
        fail: LinkFail::NobodyCame,
        worked: 0,
        since: 1200,
    };
    let gone = render(failed, LinkKind::Cable, 1600, "cable-failed");
    let spot = (356, 350 - 60);
    assert_ne!(
        at(&waiting, spot.0, spot.1),
        at(&gone, spot.0, spot.1),
        "the plug never lifted away"
    );
}

/// The ground the compositor lays down before anything is drawn over it. A sample still equal
/// to this is a sample nothing reached.
const GROUND: [u8; 3] = [0x05, 0x05, 0x08];

/// The plug actually leaves the port, in pixels. `plug_tip` returning a moving number is not
/// the same claim: a sprite drawn at zero alpha, at zero size, or left behind while only the
/// number moved all satisfy the motion and none of them reach the glass.
///
/// The sample sits inside the plug's own housing, 60 px above the tip, and above `PORT_Y` so
/// the port face is never what is being read. Seated it is plug; withdrawn it is bare ground.
#[test]
fn ending_a_link_pulls_the_plug_back_out_of_the_port() {
    let menu = GameMenu::Unplug {
        role: LinkRow::Host,
        since: 0,
    };
    let seated = render(menu, LinkKind::Cable, 0, "cable-unplug-start");
    let gone = render(menu, LinkKind::Cable, 400, "cable-unplug-end");
    let (x, y) = (356, 359);
    assert_ne!(
        at(&seated, x, y),
        GROUND,
        "no plug in the port on the frame the unplug begins"
    );
    assert_eq!(
        at(&gone, x, y),
        GROUND,
        "the plug never came back out of the port"
    );
}

/// The same for the adapter, which lifts off the port rather than sliding out of it. The
/// sample is the label plate `a_wireless_link_seats_the_adapter_with_its_label_plate` reads,
/// taken where a seated adapter puts it and vacated once it has lifted.
#[test]
fn ending_a_wireless_link_lifts_the_adapter_off_the_port() {
    let menu = GameMenu::Unplug {
        role: LinkRow::Join,
        since: 0,
    };
    let seated = render(menu, LinkKind::Wireless, 0, "wireless-unplug-start");
    let gone = render(menu, LinkKind::Wireless, 400, "wireless-unplug-end");
    let (x, y) = (360, 380);
    assert_ne!(
        at(&seated, x, y),
        GROUND,
        "no adapter on the port on the frame the unplug begins"
    );
    assert_eq!(
        at(&gone, x, y),
        GROUND,
        "the adapter never lifted off the port"
    );
}

// --- the key legend -----------------------------------------------------------------------
//
// Everything above composites `draw_link_art`, which is a pure function. The legend is not: it
// is laid out in `App::draw_game_menu`, from the seated cart, so it takes a whole `App` to get
// at. A draw list is not evidence that it reached the screen — a cap placed off the panel, at
// zero alpha or at zero size is in the list and not on the glass — so it is rendered here.

/// Whether a lit pixel is there at all: anything the compositor left other than the ground.
fn lit(px: &[u8], o: usize) -> bool {
    px[o] != 0x05 || px[o + 1] != 0x05 || px[o + 2] != 0x08
}

/// How much type the frame actually carries.
fn ink(px: &[u8]) -> usize {
    (0..px.len() / 4).filter(|i| lit(px, i * 4)).count()
}

/// The rows the frame has anything on, top and bottom.
fn inked_rows(px: &[u8]) -> (usize, usize) {
    let rows: Vec<usize> = (0..OUT_H as usize)
        .filter(|y| (0..OUT_W as usize).any(|x| lit(px, (y * OUT_W as usize + x) * 4)))
        .collect();
    (
        *rows.first().expect("nothing on the frame"),
        *rows.last().expect("nothing on the frame"),
    )
}

/// The real key caps, rastered the way the device rasters them, in `LinkLegend::ALL` order.
fn legend_faces() -> Vec<(TexId, Face)> {
    LinkLegend::ALL
        .iter()
        .map(|k| {
            let f: Face = match k {
                LinkLegend::Cancel => hint_face("B", "Cancel"),
                LinkLegend::Mode => hint_face("SELECT", "Mode"),
                LinkLegend::Swap => arrows_hint_face("Swap"),
                LinkLegend::Link => hint_face("A", "Link"),
                LinkLegend::Ok => hint_face("A", "OK"),
                LinkLegend::Back => hint_face("B", "Back"),
                LinkLegend::EndLink => hint_face("A", "End Link"),
            }
            .into();
            (TexId::from_raw(900 + k.index()), f)
        })
        .collect()
}

/// The link screen's legend for a cart with the given header, composited into a frame.
///
/// Only the legend's textures are looked up. The rest of a playing app's draw list — the game
/// picture, the HUD, the cart — has no face in this table, and the row of key caps is the thing
/// under test, so the filter decides which textures get a face and never which pixels count.
fn legend_pixels(title: &str, code: &str, name: &str) -> Vec<u8> {
    let d = common::tmp_root_with_carts(&["Zzz"]);
    // "Cart" sorts before "Zzz", so `Action::Insert` seats it.
    common::write_retail_header(&d, "Cart", title, code);
    let mut app = common::boot(d.path());
    app.apply(Action::Insert);
    app.set_core(Core::Gpsp);
    app.on_core_ready();
    for _ in 0..120 {
        app.update(1.0 / 60.0);
    }
    let faces = legend_faces();
    app.set_link_legend_faces(faces.iter().map(|(t, f)| (*t, f.w)).collect());
    app.apply(Action::GameMenu);
    assert!(app.game_menu_open(), "{code} never opened its link screen");
    let mut out = Vec::new();
    app.draw(&mut out);
    let legend: Vec<Draw> = out
        .into_iter()
        .filter(|d| matches!(*d, Draw::Tex { tex, .. } if faces.iter().any(|(t, _)| *t == tex)))
        .collect();
    let px = composite(&legend, &faces);
    dump(&px, name);
    px
}

/// The legend of the screen a player opens over a live session, composited the same way.
fn connected_legend_pixels(name: &str) -> Vec<u8> {
    let d = common::tmp_root_with_carts(&["Zzz"]);
    common::write_retail_header(&d, "Cart", "POKEMON RUBY", "AXVE");
    let mut app = common::boot(d.path());
    app.apply(Action::Insert);
    app.set_core(Core::Gpsp);
    app.on_core_ready();
    for _ in 0..120 {
        app.update(1.0 / 60.0);
    }
    let faces = legend_faces();
    app.set_link_legend_faces(faces.iter().map(|(t, f)| (*t, f.w)).collect());
    // A session, then the shortcut: which is the screen this is about.
    app.begin_link(0);
    app.apply(Action::GameMenu);
    assert!(
        matches!(app.game_menu(), Some(GameMenu::Linked { opened: true, .. })),
        "the shortcut did not open the connected screen over a live session"
    );
    let mut out = Vec::new();
    app.draw(&mut out);
    let legend: Vec<Draw> = out
        .into_iter()
        .filter(|d| matches!(*d, Draw::Tex { tex, .. } if faces.iter().any(|(t, _)| *t == tex)))
        .collect();
    let px = composite(&legend, &faces);
    dump(&px, name);
    px
}

/// One cap's ink, composited alone, for holding a row against the caps it should be made of.
fn cap_ink(key: &'static str, label: &'static str) -> usize {
    let f: Face = hint_face(key, label).into();
    let (w, h) = (f.w as f32, f.h as f32);
    let tex = TexId::from_raw(1);
    let draw = Draw::Tex {
        x: 100.0,
        y: 422.0,
        w,
        h,
        tex,
        alpha: 1.0,
    };
    ink(&composite(&[draw], &[(tex, f)]))
}

/// The screen opened over a live session offers two keys and no others: B to leave the session
/// running, A to end it. On the glass rather than in the draw list — a cap that is missing,
/// blank, clipped or zero-sized all reach the same draw list and none of them reach the same
/// pixels.
#[test]
fn the_connected_screen_shows_back_and_end_link() {
    let px = connected_legend_pixels("legend-connected");
    assert!(ink(&px) > 0, "the connected screen drew no legend at all");
    assert_eq!(
        ink(&px),
        cap_ink("B", "Back") + cap_ink("A", "End Link"),
        "the row is not exactly a Back cap and an End Link cap"
    );
}

// --- the banner a refused press gets -------------------------------------------------------
//
// Two refusals can apply to one press: the cart nothing can link, and the core that cannot link
// anything. Which sentence the player reads is the whole of the fix, and `app.toast()` is the
// app's own bookkeeping rather than the glass — a face uploaded in the wrong order, or a banner
// drawn at zero alpha, agrees with it and says nothing. So the sentence is read off the pixels.

/// The real banner faces, in `Toast::ALL` order, which is how `App` finds each one.
fn toast_faces() -> Vec<(TexId, Face)> {
    Toast::ALL
        .iter()
        .map(|t| (TexId::from_raw(800 + t.index()), toast_face(*t).into()))
        .collect()
}

/// The banner on the glass after SELECT+MENU over a cart this core is running, composited from
/// the real faces. Only the banner's own textures are looked up; the game picture, the HUD plate
/// and the cart have no face in this table, so the filter decides which textures get a face and
/// never which pixels count.
fn banner_pixels(core: Core, title: &str, code: &str, name: &str) -> Vec<u8> {
    let d = common::tmp_root_with_carts(&["Zzz"]);
    // "Cart" sorts before "Zzz", so `Action::Insert` seats it.
    common::write_retail_header(&d, "Cart", title, code);
    let mut app = common::boot(d.path());
    app.apply(Action::Insert);
    app.set_core(core);
    app.on_core_ready();
    for _ in 0..120 {
        app.update(1.0 / 60.0);
    }
    let faces = toast_faces();
    app.set_toast_faces(faces.iter().map(|(t, _)| *t).collect());
    app.apply(Action::GameMenu);
    assert!(
        !app.game_menu_open(),
        "{code} on {core:?} opened a link screen instead of refusing"
    );
    let mut out = Vec::new();
    app.draw(&mut out);
    // Drawn on the frame the press landed on, which is where the fade is still at full.
    let banner: Vec<Draw> = out
        .into_iter()
        .filter(|d| matches!(*d, Draw::Tex { tex, .. } if faces.iter().any(|(t, _)| *t == tex)))
        .collect();
    let px = composite(&banner, &faces);
    dump(&px, name);
    px
}

/// One banner's ink, composited alone, for holding a frame against the sentence it should be.
fn banner_ink(t: Toast) -> usize {
    let f: Face = toast_face(t).into();
    let (w, h) = (f.w as f32, f.h as f32);
    let tex = TexId::from_raw(1);
    let draw = Draw::Tex {
        x: 100.0,
        y: 20.0,
        w,
        h,
        tex,
        alpha: 1.0,
    };
    ink(&composite(&[draw], &[(tex, f)]))
}

/// Apotris on mGBA, which is the press the user made: two refusals apply and only one of them is
/// true advice. NO LINK SUPPORT has to be the sentence that reaches the glass — PLEASE SWITCH TO
/// GPSP would send them to a core that cannot carry this cart either.
#[test]
fn a_cart_nothing_can_link_reads_no_link_support_on_the_glass() {
    let px = banner_pixels(Core::Mgba, "APOTRIS", "2ATE", "banner-apotris-mgba");
    assert!(
        ink(&px) > 0,
        "the refusal put no banner on the frame at all"
    );
    assert_eq!(
        ink(&px),
        banner_ink(Toast::NoLink),
        "the banner is not the NO LINK SUPPORT line"
    );
    assert_ne!(
        banner_ink(Toast::NoLink),
        banner_ink(Toast::NeedsGpsp),
        "the two sentences carry the same ink, so this frame proves nothing"
    );
    // In the plate band at the top, whole: a banner placed off the panel is in the draw list and
    // on none of these rows.
    let (first, last) = inked_rows(&px);
    assert!(
        last < OUT_H as usize / 4,
        "the banner is not up in the plate band: rows {first}..{last}"
    );
}

/// The other half of the order on the glass: a cart gpSP really can carry, sitting on mGBA, reads
/// the sentence that names the core which would link it.
#[test]
fn a_cart_gpsp_can_link_reads_please_switch_to_gpsp() {
    let px = banner_pixels(Core::Mgba, "POKEMON RUBY", "AXVE", "banner-ruby-mgba");
    assert!(
        ink(&px) > 0,
        "the refusal put no banner on the frame at all"
    );
    assert_eq!(
        ink(&px),
        banner_ink(Toast::NeedsGpsp),
        "the banner is not the PLEASE SWITCH TO GPSP line"
    );
}

/// The SELECT Mode cap alone, composited the same way, which is the exact amount of type the
/// switchable screen should carry over the other one.
fn mode_cap_ink() -> usize {
    let f: Face = hint_face("SELECT", "Mode").into();
    let (w, h) = (f.w as f32, f.h as f32);
    let tex = TexId::from_raw(1);
    let draw = Draw::Tex {
        x: 100.0,
        y: 422.0,
        w,
        h,
        tex,
        alpha: 1.0,
    };
    ink(&composite(&[draw], &[(tex, f)]))
}

/// The legend names SELECT only where SELECT does something, on the glass and not merely in the
/// draw list. Ruby loads as `mul_poke` by cable and `rfu` by adapter, two modes gpSP really does
/// run it differently in, so the switch is a choice. Mario Golf has no cable protocol of its own
/// and gpSP links it over the adapter whichever hardware is picked, so the press only shakes.
///
/// What the first frame carries over the second is exactly one SELECT Mode cap's worth of type,
/// which is the assertion a missing, blank, clipped or zero-sized cap all fail.
#[test]
fn the_legend_shows_select_mode_only_where_the_hardware_can_be_switched() {
    let switchable = legend_pixels("POKEMON RUBY", "AXVE", "legend-switchable");
    let fixed = legend_pixels("MARIO GOLF", "BMGE", "legend-fixed");

    assert!(
        ink(&fixed) > 0,
        "the legend put no type on the frame at all"
    );
    assert!(
        ink(&switchable) > ink(&fixed),
        "both screens carry the same type: {} and {}",
        ink(&switchable),
        ink(&fixed)
    );
    assert_eq!(
        ink(&switchable) - ink(&fixed),
        mode_cap_ink(),
        "the difference between the two frames is not one SELECT Mode cap"
    );

    // On the strip, and the whole of it on the panel: a cap placed off the bottom would be in
    // the draw list and missing from every one of these rows.
    for (px, what) in [(&switchable, "switchable"), (&fixed, "fixed")] {
        let (first, last) = inked_rows(px);
        assert!(
            first >= OUT_H as usize * 3 / 4,
            "the {what} legend is not down on the strip: rows {first}..{last}"
        );
        assert!(
            last < OUT_H as usize,
            "the {what} legend runs off the bottom of the panel"
        );
    }
}

// --- the sentence the first step shows ------------------------------------------------------
//
// Opening the link screen asks for a warm, which takes about 1.1 s out of the step that runs
// `ags-net link host|join`. Once the driver is loaded there is no radio left to bring up, and the
// step is looking for the other player from its first frame — so that is what it says. Read off
// the glass, because the two sentences are two faces uploaded in one list and an index into the
// wrong one draws a perfectly good line that says the wrong thing.

/// A radio whose warm finishes when the test says so, keeping what it was asked for separate from
/// what it finished. The caption has to follow the second, and this is what lets a test tell them
/// apart: it can be handed a `Warm` and still report a cold driver, which is the 1.1 s a quick
/// player presses A inside.
#[derive(Clone, Default)]
struct FakeRadio {
    warm: Arc<AtomicBool>,
    asked: Arc<Mutex<Vec<RadioJob>>>,
}

impl RadioJobs for FakeRadio {
    fn ask(&mut self, job: RadioJob) {
        self.asked.lock().expect("radio log").push(job);
    }

    fn warmed(&self) -> bool {
        self.warm.load(Ordering::SeqCst)
    }
}

/// The real step sentences, in `LinkStep::ALL` order, through the rasteriser the device uses.
fn step_faces() -> Vec<(TexId, Face)> {
    LinkStep::ALL
        .iter()
        .map(|s| (TexId::from_raw(950 + s.index()), menu_face(s.line()).into()))
        .collect()
}

/// The line on the glass while the worker sits on its first step, with the driver warm or cold,
/// and what the radio was asked for on the way there.
fn first_step_pixels(warm: bool, name: &str) -> (Vec<u8>, Vec<RadioJob>) {
    let d = common::tmp_root_with_carts(&["Zzz"]);
    // "Cart" sorts before "Zzz", so `Action::Insert` seats it. Ruby's identity, because the link
    // screen only opens for a cart gpSP can carry.
    common::write_retail_header(&d, "Cart", "POKEMON RUBY", "AXVE");
    let mut app = common::boot(d.path());
    app.apply(Action::Insert);
    app.set_core(Core::Gpsp);
    app.on_core_ready();
    for _ in 0..120 {
        app.update(1.0 / 60.0);
    }
    let radio = FakeRadio::default();
    radio.warm.store(warm, Ordering::SeqCst);
    app.set_radio_jobs(Box::new(radio.clone()));
    let faces = step_faces();
    app.set_link_step_faces(faces.iter().map(|(t, f)| (*t, f.w, f.h)).collect());
    app.apply(Action::GameMenu);
    assert!(app.game_menu_open(), "the link screen never opened");
    // A worker held on its first step: this stands in for an `ags-net link` that has not answered
    // yet, which is the whole window the caption is about.
    let (release, held) = channel::<()>();
    app.start_link(
        LinkStarter::spawn_with(
            Box::new(move |_, _| {
                let _ = held.recv();
                Ok(())
            }),
            Box::new(|| {}),
            LinkRole::Host,
            0,
            Box::new(|_, _: &Cancel| Err(io::Error::new(io::ErrorKind::TimedOut, "from a test"))),
        ),
        0,
    );
    assert!(
        matches!(
            app.game_menu(),
            Some(GameMenu::Working {
                step: LinkStep::Radio,
                ..
            })
        ),
        "the screen is not on the step this is about: {:?}",
        app.game_menu()
    );
    let mut out = Vec::new();
    app.draw(&mut out);
    let line: Vec<Draw> = out
        .into_iter()
        .filter(|d| matches!(*d, Draw::Tex { tex, .. } if faces.iter().any(|(t, _)| *t == tex)))
        .collect();
    let px = composite(&line, &faces);
    dump(&px, name);
    drop(release);
    let asked = radio.asked.lock().expect("radio log").clone();
    (px, asked)
}

/// One sentence's ink, composited alone, for holding a frame against the line it should carry.
fn line_ink(step: LinkStep) -> usize {
    let f: Face = menu_face(step.line()).into();
    let (w, h) = (f.w as f32, f.h as f32);
    let tex = TexId::from_raw(1);
    let draw = Draw::Tex {
        x: 100.0,
        y: 20.0,
        w,
        h,
        tex,
        alpha: 1.0,
    };
    ink(&composite(&[draw], &[(tex, f)]))
}

/// Cold, the load is still ahead of the player, and the screen still says so. The warm was asked
/// for on the way in — that is the point: the caption follows the driver finishing, and asking is
/// not finishing.
#[test]
fn a_cold_radio_says_it_is_bringing_the_radio_up() {
    let (px, asked) = first_step_pixels(false, "step-radio-cold");
    assert!(ink(&px) > 0, "the step put no sentence on the frame at all");
    assert!(
        asked.contains(&RadioJob::Warm),
        "the screen never asked for a warm, so this frame says nothing about finishing one"
    );
    assert_ne!(
        line_ink(LinkStep::Radio),
        line_ink(LinkStep::Waiting),
        "the two sentences carry the same ink, so neither frame below proves anything"
    );
    assert_eq!(
        ink(&px),
        line_ink(LinkStep::Radio),
        "a screen whose warm has not finished is not saying the radio is coming up"
    );
}

/// Warm, and the sentence about the wait is gone: the step goes straight to what it is doing.
#[test]
fn a_warm_radio_goes_straight_to_looking_for_the_other_player() {
    let (px, _) = first_step_pixels(true, "step-radio-warm");
    assert!(ink(&px) > 0, "the step put no sentence on the frame at all");
    assert_eq!(
        ink(&px),
        line_ink(LinkStep::Waiting),
        "a warm radio is still being announced as coming up"
    );
    // Up where the link screen's one line sits, and whole: a line placed off the panel is in the
    // draw list and on none of these rows.
    let (first, last) = inked_rows(&px);
    assert!(
        last < OUT_H as usize / 2,
        "the line is not up where the link screen's sentence goes: rows {first}..{last}"
    );
}

//! The link screen's art, through `draw_link_art`, composited in software.
//!
//! `SCRATCH_PNG_DIR=/tmp cargo test -p slot --test render_link_screen -- --nocapture`

use slot::app::{GameMenu, LinkRow};
use slot::link_kind::LinkKind;
use slot::link_screen::{draw_link_art, LinkSprites, Sprite};
use slot::link_start::{LinkFail, LinkStep};
use slot_ui::{link_art, CartFace, Draw, TexId, OUT_H, OUT_W};

fn sprites_and_faces() -> (LinkSprites, Vec<(TexId, CartFace)>) {
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
        faces.push((tex, f));
        sprite
    };
    let [ar0, ar1, ar2] = art.arcs_right;
    let [al0, al1, al2] = art.arcs_left;
    let sprites = LinkSprites {
        port: put(art.port),
        plug_host: put(art.plug_host),
        plug_join: put(art.plug_join),
        adapter: put(art.adapter),
        glow_host: put(art.glow_host),
        glow_neutral: put(art.glow_neutral),
        arcs_right: [put(ar0), put(ar1), put(ar2)],
        arcs_left: [put(al0), put(al1), put(al2)],
        clicks: put(art.clicks),
        arrow_left: put(art.arrow_left),
        arrow_right: put(art.arrow_right),
    };
    (sprites, faces)
}

/// Straight-alpha over-compositing, nearest neighbour, with `Turned` rotated about its centre.
fn composite(out: &[Draw], faces: &[(TexId, CartFace)]) -> Vec<u8> {
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
    if let Ok(dir) = std::env::var("SCRATCH_PNG_DIR") {
        let path = format!("{dir}/link-{name}.png");
        let file = std::fs::File::create(&path).unwrap();
        let mut e = png::Encoder::new(std::io::BufWriter::new(file), OUT_W, OUT_H);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&px).unwrap();
        println!("wrote {path}");
    }
    px
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

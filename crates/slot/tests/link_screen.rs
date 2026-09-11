use slot::app::{GameMenu, LinkRow};
use slot::link_kind::LinkKind;
use slot::link_screen::*;
use slot::link_start::{LinkFail, LinkStep};
use slot_ui::*;

fn working(role: LinkRow, since: u64) -> GameMenu {
    GameMenu::Working {
        role,
        step: LinkStep::Waiting,
        since,
    }
}

fn failed(since: u64) -> GameMenu {
    GameMenu::Failed {
        role: LinkRow::Host,
        fail: LinkFail::NobodyCame,
        worked: 0,
        since,
    }
}

#[test]
fn pick_hangs_the_plug_above_the_port() {
    assert_eq!(plug_tip(GameMenu::Pick(LinkRow::Host), 0), 330.0);
}

#[test]
fn working_drops_the_plug_in_200ms_then_bobs_between_352_and_364() {
    let m = working(LinkRow::Host, 1000);
    assert_eq!(plug_tip(m, 1000), 330.0);
    assert!((plug_tip(m, 1200) - 352.0).abs() < 0.01);
    let bob: Vec<f32> = (0..1600)
        .step_by(50)
        .map(|t| plug_tip(m, 1200 + t))
        .collect();
    assert!(bob.iter().all(|y| (351.9..=364.1).contains(y)), "{bob:?}");
    assert!(
        bob.iter().any(|y| *y > 363.0),
        "the bob never reached the bottom"
    );
}

#[test]
fn linked_seats_the_plug_from_where_it_was_and_clicks_as_it_lands() {
    let (worked, since) = (1000, 2700);
    let before = plug_tip(working(LinkRow::Host, worked), since);
    let m = GameMenu::Linked {
        role: LinkRow::Host,
        worked,
        since,
    };
    assert!(
        (plug_tip(m, since) - before).abs() < 0.01,
        "the plug jumped"
    );
    assert!((plug_tip(m, since + 160) - 419.0).abs() < 0.01);
    assert_eq!(clicks_alpha(m, since + 100), 0.0);
    assert_eq!(clicks_alpha(m, since + 160), 1.0);
}

#[test]
fn failed_lifts_tilts_and_dims_over_250ms() {
    let m = failed(5000);
    assert_eq!(plug_turn(m, 5000), 0.0);
    assert!((plug_tip(m, 5250) - 276.0).abs() < 0.01);
    assert!((plug_turn(m, 5250) - 14f32.to_radians()).abs() < 1e-4);
    assert!((art_alpha(m, 5250) - 0.45).abs() < 1e-4);
}

#[test]
fn the_adapter_seats_while_working_and_stays_seated() {
    assert_eq!(adapter_base(GameMenu::Pick(LinkRow::Join), 0), 336.0);
    assert!((adapter_base(working(LinkRow::Join, 0), 200) - PORT_Y).abs() < 0.01);
    assert!((adapter_base(failed(3000), 3250) - PORT_Y).abs() < 0.01);
}

#[test]
fn the_arcs_call_in_turn_hold_when_linked_and_die_on_failure() {
    let a = arc_alphas(working(LinkRow::Host, 0), 200 + 600);
    assert!(
        (a[0] - 1.0).abs() < 0.01,
        "ring 0 is not at its peak: {a:?}"
    );
    assert!(a[1] < 1.0);
    assert_eq!(arc_alphas(GameMenu::Pick(LinkRow::Host), 0), [0.0; 3]);
    let linked = GameMenu::Linked {
        role: LinkRow::Host,
        worked: 0,
        since: 4000,
    };
    assert_eq!(arc_alphas(linked, 4160), [1.0; 3]);
    assert_eq!(arc_alphas(failed(4000), 4250), [0.0; 3]);
}

fn sprites() -> LinkSprites {
    let s = |n: usize, w: u32, h: u32| Sprite {
        tex: TexId::from_raw(n),
        w,
        h,
    };
    let arc = |n: usize, i: usize| s(n, ARCS[i].2, ARCS[i].3);
    LinkSprites {
        port: s(1, PORT_W, PORT_H),
        plug_host: s(2, PLUG_W, PLUG_H),
        plug_join: s(3, PLUG_W, PLUG_H),
        adapter: s(4, ADAPTER_W, ADAPTER_H),
        glow_host: s(5, GLOW_HOST_R * 2, GLOW_HOST_R * 2),
        glow_neutral: s(6, GLOW_NEUTRAL_R * 2, GLOW_NEUTRAL_R * 2),
        arcs_right: [arc(7, 0), arc(8, 1), arc(9, 2)],
        arcs_left: [arc(10, 0), arc(11, 1), arc(12, 2)],
        clicks: s(13, CLICKS_W, CLICKS_H),
        arrow_left: s(14, ARROW_W, ARROW_H),
        arrow_right: s(15, ARROW_W, ARROW_H),
    }
}

fn texes(out: &[Draw]) -> Vec<TexId> {
    out.iter()
        .filter_map(|d| match d {
            Draw::Tex { tex, .. } | Draw::Turned { tex, .. } => Some(*tex),
            _ => None,
        })
        .collect()
}

#[test]
fn cable_carts_draw_the_plug_under_the_port_and_wireless_carts_the_adapter() {
    let s = sprites();
    let mut out = Vec::new();
    draw_link_art(
        GameMenu::Pick(LinkRow::Host),
        LinkKind::Cable,
        0,
        &s,
        &mut out,
    );
    let t = texes(&out);
    let plug = t
        .iter()
        .position(|x| *x == s.plug_host.tex)
        .expect("no plug");
    let port = t.iter().position(|x| *x == s.port.tex).expect("no port");
    assert!(
        port > plug,
        "the port must be drawn over a plug that goes into it"
    );
    assert!(!t.contains(&s.adapter.tex));

    out.clear();
    draw_link_art(
        GameMenu::Pick(LinkRow::Host),
        LinkKind::Wireless,
        0,
        &s,
        &mut out,
    );
    let t = texes(&out);
    assert!(t.contains(&s.adapter.tex) && !t.contains(&s.plug_host.tex));
}

#[test]
fn a_joiner_holds_the_gray_plug() {
    let s = sprites();
    let mut out = Vec::new();
    draw_link_art(
        GameMenu::Pick(LinkRow::Join),
        LinkKind::Cable,
        0,
        &s,
        &mut out,
    );
    let t = texes(&out);
    assert!(t.contains(&s.plug_join.tex) && !t.contains(&s.plug_host.tex));
}

#[test]
fn the_swap_arrows_show_on_pick_only() {
    let s = sprites();
    let mut out = Vec::new();
    draw_link_art(
        GameMenu::Pick(LinkRow::Host),
        LinkKind::Cable,
        0,
        &s,
        &mut out,
    );
    assert!(texes(&out).contains(&s.arrow_left.tex));
    out.clear();
    draw_link_art(working(LinkRow::Host, 0), LinkKind::Cable, 50, &s, &mut out);
    assert!(!texes(&out).contains(&s.arrow_left.tex));
}

#[test]
fn a_failed_plug_is_drawn_turned() {
    let s = sprites();
    let mut out = Vec::new();
    draw_link_art(failed(0), LinkKind::Cable, 250, &s, &mut out);
    assert!(out.iter().any(
        |d| matches!(d, Draw::Turned { tex, turn, .. } if *tex == s.plug_host.tex && *turn > 0.2)
    ));
}

#[test]
fn the_glow_eases_between_states_too() {
    let m = working(LinkRow::Host, 1000);
    assert_eq!(glow_alpha(m, 1000), 1.0);
    assert!(
        (glow_alpha(m, 1199) - glow_alpha(m, 1200)).abs() < 0.02,
        "the glow stepped at the end of the drop"
    );
    let (worked, since) = (1000, 2700);
    let waiting = glow_alpha(working(LinkRow::Host, worked), since);
    let linked = GameMenu::Linked {
        role: LinkRow::Host,
        worked,
        since,
    };
    assert!(
        (glow_alpha(linked, since) - waiting).abs() < 1e-4,
        "the glow jumped when the link came up"
    );
    assert!((glow_alpha(linked, since + 160) - 1.0).abs() < 1e-4);
    let failed = GameMenu::Failed {
        role: LinkRow::Host,
        fail: LinkFail::NobodyCame,
        worked,
        since,
    };
    assert!(
        (glow_alpha(failed, since) - waiting).abs() < 1e-4,
        "the glow jumped when the link failed"
    );
    assert!((glow_alpha(failed, since + 250) - 0.45).abs() < 1e-4);
}

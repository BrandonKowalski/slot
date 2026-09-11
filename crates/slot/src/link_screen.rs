//! The link screen: its sprites, motion and drawing.

use std::f32::consts::TAU;

use slot_ui::{
    ease, Draw, Millis, TexId, ADAPTER_BASE_X, ADAPTER_BASE_Y, ARCS, ARROW_LEFT_X, ARROW_RIGHT_X,
    ARROW_Y, CLICKS_X, CLICKS_Y, OUT_W, PLUG_H, PLUG_TIP_X, PORT_Y,
};

use crate::app::{GameMenu, LinkRow};
use crate::link_kind::LinkKind;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Sprite {
    pub tex: TexId,
    pub w: u32,
    pub h: u32,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct LinkSprites {
    pub port: Sprite,
    pub plug_host: Sprite,
    pub plug_join: Sprite,
    pub adapter: Sprite,
    pub glow_host: Sprite,
    pub glow_neutral: Sprite,
    pub arcs_right: [Sprite; 3],
    pub arcs_left: [Sprite; 3],
    pub clicks: Sprite,
    pub arrow_left: Sprite,
    pub arrow_right: Sprite,
}

const PICK_TIP: f32 = 330.0;
const WORK_TIP: f32 = 352.0;
/// The bob swings this far either side of its middle.
const BOB: f32 = 6.0;
const BOB_MS: f32 = 1600.0;
const LINKED_TIP: f32 = 419.0;
const FAILED_TIP: f32 = 276.0;
const FAILED_TURN_DEG: f32 = 14.0;
const FAILED_ALPHA: f32 = 0.45;
const DROP_MS: Millis = 200;
const SEAT_MS: Millis = 160;
const LIFT_MS: Millis = 250;
const PICK_BASE: f32 = 336.0;
const ARC_MS: f32 = 1200.0;
const ARC_STAGGER_MS: f32 = 300.0;
const ARROW_ALPHA: f32 = 0.7;
/// The glow's pulse while waiting: dimmest at the top of the bob, brightest nearest the port.
const GLOW_MID: f32 = 0.725;
const GLOW_SWING: f32 = 0.275;

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    if t >= 1.0 {
        b
    } else {
        a + (b - a) * t
    }
}

/// Eased progress through a move of `ms` that began at `since`.
fn eased(now: Millis, since: Millis, ms: Millis) -> f32 {
    ease((now.saturating_sub(since) as f32 / ms as f32).clamp(0.0, 1.0))
}

fn working_tip(since: Millis, now: Millis) -> f32 {
    let t = now.saturating_sub(since);
    if t < DROP_MS {
        lerp(PICK_TIP, WORK_TIP, eased(now, since, DROP_MS))
    } else {
        WORK_TIP + BOB - BOB * (TAU * (t - DROP_MS) as f32 / BOB_MS).cos()
    }
}

fn working_base(since: Millis, now: Millis) -> f32 {
    lerp(PICK_BASE, PORT_Y, eased(now, since, DROP_MS))
}

fn working_arcs(since: Millis, now: Millis) -> [f32; 3] {
    let t = now.saturating_sub(since);
    let mut out = [0.0; 3];
    if t < DROP_MS {
        return out;
    }
    for (i, a) in out.iter_mut().enumerate() {
        let u = (t - DROP_MS) as f32 - ARC_STAGGER_MS * i as f32;
        if u >= 0.0 {
            let phase = (u / ARC_MS).fract();
            *a = 1.0 - (2.0 * phase - 1.0).abs();
        }
    }
    out
}

fn working_glow(since: Millis, now: Millis) -> f32 {
    let t = now.saturating_sub(since);
    if t < DROP_MS {
        lerp(1.0, GLOW_MID - GLOW_SWING, eased(now, since, DROP_MS))
    } else {
        GLOW_MID - GLOW_SWING * (TAU * (t - DROP_MS) as f32 / BOB_MS).cos()
    }
}

pub fn plug_tip(menu: GameMenu, now: Millis) -> f32 {
    match menu {
        GameMenu::Pick(_) => PICK_TIP,
        GameMenu::Working { since, .. } => working_tip(since, now),
        GameMenu::Linked { worked, since, .. } => lerp(
            working_tip(worked, since),
            LINKED_TIP,
            eased(now, since, SEAT_MS),
        ),
        GameMenu::Failed { worked, since, .. } => lerp(
            working_tip(worked, since),
            FAILED_TIP,
            eased(now, since, LIFT_MS),
        ),
    }
}

pub fn plug_turn(menu: GameMenu, now: Millis) -> f32 {
    match menu {
        GameMenu::Failed { since, .. } => FAILED_TURN_DEG.to_radians() * eased(now, since, LIFT_MS),
        _ => 0.0,
    }
}

pub fn art_alpha(menu: GameMenu, now: Millis) -> f32 {
    match menu {
        GameMenu::Failed { since, .. } => lerp(1.0, FAILED_ALPHA, eased(now, since, LIFT_MS)),
        _ => 1.0,
    }
}

pub fn adapter_base(menu: GameMenu, now: Millis) -> f32 {
    match menu {
        GameMenu::Pick(_) => PICK_BASE,
        GameMenu::Working { since, .. } => working_base(since, now),
        GameMenu::Linked { worked, since, .. } => lerp(
            working_base(worked, since),
            PORT_Y,
            eased(now, since, SEAT_MS),
        ),
        GameMenu::Failed { worked, since, .. } => working_base(worked, since),
    }
}

pub fn arc_alphas(menu: GameMenu, now: Millis) -> [f32; 3] {
    match menu {
        GameMenu::Pick(_) => [0.0; 3],
        GameMenu::Working { since, .. } => working_arcs(since, now),
        GameMenu::Linked { worked, since, .. } => {
            let from = working_arcs(worked, since);
            let t = eased(now, since, SEAT_MS);
            [
                lerp(from[0], 1.0, t),
                lerp(from[1], 1.0, t),
                lerp(from[2], 1.0, t),
            ]
        }
        GameMenu::Failed { worked, since, .. } => {
            let from = working_arcs(worked, since);
            let fade = 1.0 - eased(now, since, LIFT_MS);
            [from[0] * fade, from[1] * fade, from[2] * fade]
        }
    }
}

pub fn clicks_alpha(menu: GameMenu, now: Millis) -> f32 {
    match menu {
        GameMenu::Linked { since, .. } if now.saturating_sub(since) >= SEAT_MS => 1.0,
        _ => 0.0,
    }
}

pub fn glow_alpha(menu: GameMenu, now: Millis) -> f32 {
    match menu {
        GameMenu::Pick(_) => 1.0,
        GameMenu::Working { since, .. } => working_glow(since, now),
        GameMenu::Linked { worked, since, .. } => {
            lerp(working_glow(worked, since), 1.0, eased(now, since, SEAT_MS))
        }
        GameMenu::Failed { worked, since, .. } => lerp(
            working_glow(worked, since),
            FAILED_ALPHA,
            eased(now, since, LIFT_MS),
        ),
    }
}

fn role_of(menu: GameMenu) -> LinkRow {
    match menu {
        GameMenu::Pick(role)
        | GameMenu::Working { role, .. }
        | GameMenu::Linked { role, .. }
        | GameMenu::Failed { role, .. } => role,
    }
}

fn tex(out: &mut Vec<Draw>, s: Sprite, x: f32, y: f32, alpha: f32) {
    out.push(Draw::Tex {
        x: x.round(),
        y: y.round(),
        w: s.w as f32,
        h: s.h as f32,
        tex: s.tex,
        alpha,
    });
}

/// The art between the scrim and the text: glow, plug or adapter, the port over it, then the
/// arcs, click marks and swap arrows.
pub fn draw_link_art(
    menu: GameMenu,
    kind: LinkKind,
    now: Millis,
    s: &LinkSprites,
    out: &mut Vec<Draw>,
) {
    let host = role_of(menu) == LinkRow::Host;
    let alpha = art_alpha(menu, now);
    let centre = OUT_W as f32 / 2.0;
    match kind {
        LinkKind::Cable => {
            let tip = plug_tip(menu, now);
            let glow = if host { s.glow_host } else { s.glow_neutral };
            tex(
                out,
                glow,
                centre - glow.w as f32 / 2.0,
                tip - 110.0 - glow.h as f32 / 2.0,
                glow_alpha(menu, now),
            );
            let plug = if host { s.plug_host } else { s.plug_join };
            let (x, y) = ((centre - PLUG_TIP_X).round(), (tip - PLUG_H as f32).round());
            let turn = plug_turn(menu, now);
            if turn == 0.0 {
                tex(out, plug, x, y, alpha);
            } else {
                out.push(Draw::Turned {
                    x,
                    y,
                    w: plug.w as f32,
                    h: plug.h as f32,
                    tex: plug.tex,
                    alpha,
                    turn,
                });
            }
            tex(out, s.port, 0.0, PORT_Y, 1.0);
            let clicks = clicks_alpha(menu, now);
            if clicks > 0.0 {
                tex(out, s.clicks, CLICKS_X, CLICKS_Y, clicks);
            }
        }
        LinkKind::Wireless => {
            let base = adapter_base(menu, now);
            let glow = s.glow_neutral;
            tex(
                out,
                glow,
                centre - glow.w as f32 / 2.0,
                base - 70.0 - glow.h as f32 / 2.0,
                glow_alpha(menu, now),
            );
            tex(
                out,
                s.adapter,
                centre - ADAPTER_BASE_X,
                base - ADAPTER_BASE_Y,
                alpha,
            );
            tex(out, s.port, 0.0, PORT_Y, 1.0);
            let dy = base - PORT_Y;
            for (i, a) in arc_alphas(menu, now).into_iter().enumerate() {
                if a <= 0.0 {
                    continue;
                }
                let (x, y, w, _) = ARCS[i];
                tex(out, s.arcs_right[i], x, y + dy, a);
                tex(out, s.arcs_left[i], OUT_W as f32 - x - w as f32, y + dy, a);
            }
        }
    }
    if matches!(menu, GameMenu::Pick(_)) {
        tex(out, s.arrow_left, ARROW_LEFT_X, ARROW_Y, ARROW_ALPHA);
        tex(out, s.arrow_right, ARROW_RIGHT_X, ARROW_Y, ARROW_ALPHA);
    }
}

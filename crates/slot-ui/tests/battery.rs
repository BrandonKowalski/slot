use slot_power::{Battery, Charge};
use slot_ui::{draw_gauge, Draw, Printed, TexId, GAUGE_W, WALL};

fn percent_face() -> Printed {
    Printed { face: None, w: 30 }
}

fn quads(out: &[Draw]) -> Vec<(f32, f32, f32, f32)> {
    out.iter()
        .filter_map(|d| match *d {
            Draw::Rect { x, y, w, h, .. } | Draw::Tex { x, y, w, h, .. } => Some((x, y, w, h)),
            _ => None,
        })
        .collect()
}

fn at(percent: u8, charge: Charge) -> Option<Battery> {
    Some(Battery { percent, charge })
}

/// The reason the bolt went inside the capsule instead of beside it: a leading bolt on the
/// left-aligned case band would either break the margin or hold a permanent gap for the
/// times it is absent. Nothing may move when a cable goes in.
#[test]
fn nothing_moves_when_the_charge_state_changes() {
    let mut idle = Vec::new();
    let mut charging = Vec::new();
    draw_gauge(
        24.0,
        400.0,
        at(68, Charge::Discharging),
        percent_face(),
        None,
        &mut idle,
    );
    draw_gauge(
        24.0,
        400.0,
        at(68, Charge::Charging),
        percent_face(),
        Some(TexId::from_raw(7)),
        &mut charging,
    );
    let idle = quads(&idle);
    for q in idle.iter() {
        assert!(
            quads(&charging).contains(q),
            "{q:?} moved or vanished when charging started"
        );
    }
}

/// The defect this whole file is guarding against was the bolt drawn *inside* the capsule,
/// over the fill, knocking a hole in whatever charge was showing. Nothing else stops that
/// from happening again except this: the bolt's own quad must end at or before the
/// capsule's leftmost wall begins.
#[test]
fn the_bolt_never_reaches_the_capsule() {
    let mut out = Vec::new();
    draw_gauge(
        24.0,
        400.0,
        at(68, Charge::Charging),
        percent_face(),
        Some(TexId::from_raw(7)),
        &mut out,
    );
    let bolt_right = out
        .iter()
        .find_map(|d| match *d {
            Draw::Tex { x, w, tex, .. } if tex == TexId::from_raw(7) => Some(x + w),
            _ => None,
        })
        .expect("the bolt did not draw while charging");
    // Every capsule stroke and the fill are drawn as `Draw::Rect`; the bolt is the only
    // `Draw::Tex`, and the percent's placeholder (also a `Rect`) sits well to the right, so
    // the leftmost rect is always the capsule's own left wall.
    let capsule_left = out
        .iter()
        .filter_map(|d| match *d {
            Draw::Rect { x, .. } => Some(x),
            _ => None,
        })
        .fold(f32::MAX, f32::min);
    assert!(
        bolt_right <= capsule_left,
        "the bolt's right edge ({bolt_right}) reaches past the capsule's left wall ({capsule_left})"
    );
}

#[test]
fn the_bolt_is_only_drawn_while_charging() {
    let bolt = |charge| {
        let mut out = Vec::new();
        draw_gauge(
            24.0,
            400.0,
            at(68, charge),
            percent_face(),
            Some(TexId::from_raw(7)),
            &mut out,
        );
        out.iter()
            .any(|d| matches!(d, Draw::Tex { tex: t, .. } if *t == TexId::from_raw(7)))
    };
    assert!(bolt(Charge::Charging));
    assert!(!bolt(Charge::Discharging));
    assert!(!bolt(Charge::Full));
    // The degraded case on hardware where `status` reads empty: a plain capsule, exactly
    // what the screen would show if none of this had been added.
    assert!(!bolt(Charge::Unknown));
}

/// The fill is the one quad in the cluster whose size is meant to move with the percent, and
/// the name's two clauses are two separate properties: the fill has to grow strictly as the
/// percent does (or a constant full bar would pass), and it may never cross the capsule's own
/// inner wall (or a fill formula that outruns 100% above the midpoint would pass). Bounding
/// against the whole cluster's width — including the gap and the printed number, which sit
/// well clear of the capsule regardless of the fill — proved neither.
#[test]
fn the_fill_tracks_the_percent_and_never_leaves_the_capsule() {
    let mut widths = Vec::new();
    for percent in [0u8, 1, 50, 99, 100] {
        let mut out = Vec::new();
        draw_gauge(
            24.0,
            400.0,
            at(percent, Charge::Discharging),
            percent_face(),
            None,
            &mut out,
        );
        // The capsule's own left wall is the leftmost `Rect` in the list: nothing else in
        // `draw_gauge` ever draws further left of `cx` than the wall itself does.
        let capsule_left = out
            .iter()
            .filter_map(|d| match *d {
                Draw::Rect { x, .. } => Some(x),
                _ => None,
            })
            .fold(f32::MAX, f32::min);
        let inner_right = capsule_left + GAUGE_W - 2.0 * WALL;
        // The fill is the only `Rect` that starts two wall-widths in from the capsule's own
        // left edge; every other stroke starts either at the wall itself or past the nub.
        let fill = out.iter().find_map(|d| match *d {
            Draw::Rect { x, w, .. } if (x - (capsule_left + 2.0 * WALL)).abs() < 0.01 => Some(w),
            _ => None,
        });
        if let Some(w) = fill {
            assert!(
                capsule_left + 2.0 * WALL + w <= inner_right + 0.01,
                "a {percent}% fill burst through the capsule's own wall"
            );
        }
        // A 0% battery draws no fill rect at all, which is the correct degenerate case of
        // "the fill tracks the percent": there is nothing to track down to.
        widths.push(fill.unwrap_or(0.0));
    }
    for pair in widths.windows(2) {
        assert!(
            pair[1] > pair[0],
            "the fill must grow strictly with the percent, got {widths:?}"
        );
    }
}

/// No gauge is no capsule. A device slot has not been ported to yet still has to come up.
#[test]
fn no_reading_draws_nothing() {
    let mut out = Vec::new();
    draw_gauge(24.0, 400.0, None, percent_face(), None, &mut out);
    assert!(out.is_empty());
}

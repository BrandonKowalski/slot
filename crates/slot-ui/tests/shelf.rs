use slot_power::{Battery, Charge};
use slot_store::Cart;
use slot_ui::{draw_footer, label_colour, Draw, Printed, Shelf, TexId, CART_W, OUT_W};

fn shelf_with(n: usize) -> Shelf {
    Shelf::new(
        (0..n)
            .map(|i| Cart {
                stem: format!("Game {i}"),
                rom: format!("Games/Game {i}.gba").into(),
                label: None,
                code: String::new(),
                title: format!("GAME {i}"),
            })
            .collect(),
    )
}

fn placed(s: &Shelf) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    s.draw_row(None, 0.0, 0.0, &mut out);
    out.iter()
        .map(|d| match *d {
            Draw::Rect { x, w, .. } => (x, w),
            Draw::Tex { x, w, .. } => (x, w),
            Draw::Game | Draw::Shot { .. } => (0.0, OUT_W as f32),
        })
        .collect()
}

fn xw(d: &Draw) -> (f32, f32) {
    match *d {
        Draw::Rect { x, w, .. } | Draw::Tex { x, w, .. } => (x, w),
        Draw::Game | Draw::Shot { .. } => (0.0, OUT_W as f32),
    }
}

fn settle(s: &mut Shelf) {
    for _ in 0..600 {
        s.update(1.0 / 60.0);
    }
}

/// Which cart each quad in the row belongs to. No faces are uploaded, so every cart draws
/// as a rect in its own label colour, and that colour is the only identity on offer. The
/// colour comes from the cleaned stem, not the header title.
fn drawn_cart_indices(out: &[Draw]) -> Vec<usize> {
    let keys: Vec<[u8; 3]> = (0..16)
        .map(|i| label_colour(&format!("Game {i}")))
        .collect();
    out.iter()
        .map(|d| {
            let Draw::Rect { colour, .. } = d else {
                panic!("a cart with no face should draw as a rect");
            };
            let rgb = [0, 1, 2].map(|c| (colour[c] * 255.0).round() as u8);
            keys.iter()
                .position(|k| *k == rgb)
                .unwrap_or_else(|| panic!("quad {rgb:?} belongs to no cart"))
        })
        .collect()
}

#[test]
fn three_carts_fit_across_the_shelf() {
    let row = CART_W * 3;
    assert!(
        row <= OUT_W,
        "three carts are {row} px across a {OUT_W} px row, so it cannot show one either \
         side of the selection"
    );
}

#[test]
fn the_shelf_wraps_at_both_ends() {
    let mut s = shelf_with(4);
    s.left();
    assert_eq!(
        s.index, 3,
        "going left from the first cart should reach the last"
    );
    s.right();
    assert_eq!(
        s.index, 0,
        "going right from the last cart should reach the first"
    );
}

/// The spring chases `scroll`. If wrapping is a bare index change it unwinds the whole row.
#[test]
fn wrapping_animates_one_step_not_the_long_way_back() {
    let mut s = shelf_with(8);
    for _ in 0..7 {
        s.right();
    }
    settle(&mut s);
    let before = s.scroll;
    s.right();
    let travel = (s.scroll_target() - before).abs();
    assert!(
        travel < 1.5,
        "the spring is travelling {travel} slots to move one"
    );
}

#[test]
fn a_settled_wrap_still_lands_on_the_selected_cart() {
    let mut s = shelf_with(5);
    s.left();
    settle(&mut s);
    assert_eq!(s.index, 4);
    assert!(
        (s.scroll.rem_euclid(5.0) - 4.0).abs() < 0.01,
        "scroll {} did not settle",
        s.scroll
    );
}

#[test]
fn the_neighbour_of_the_last_cart_is_the_first() {
    let s = shelf_with(4);
    assert_eq!(
        s.cart_at_offset(-1),
        Some(3),
        "left of the first is the last"
    );
    assert_eq!(s.cart_at_offset(1), Some(1));
}

/// Two carts would otherwise appear on both sides of the selection at once.
#[test]
fn no_cart_is_drawn_twice_in_one_row() {
    for n in [2usize, 3, 4] {
        let s = shelf_with(n);
        let mut out = Vec::new();
        s.draw_row(None, 0.0, 0.0, &mut out);
        let drawn = drawn_cart_indices(&out);
        let mut uniq = drawn.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(drawn.len(), uniq.len(), "{n} carts: one is on screen twice");
    }
}

#[test]
fn shelf_scroll_settles_on_the_selected_index() {
    let mut s = shelf_with(5);
    s.right();
    s.right();
    for _ in 0..600 {
        s.update(1.0 / 60.0);
    }
    assert!(
        (s.scroll - 2.0).abs() < 0.01,
        "scroll {} did not settle",
        s.scroll
    );
}

#[test]
fn scroll_never_overshoots_the_cart_it_lands_on() {
    let mut s = shelf_with(5);
    s.right();
    for _ in 0..600 {
        s.update(1.0 / 60.0);
        assert!(s.scroll <= 1.0 + 1e-4, "overshot to {}", s.scroll);
    }
}

#[test]
fn an_empty_shelf_is_inert() {
    let mut s = shelf_with(0);
    s.right();
    s.left();
    assert_eq!(s.index, 0);
    s.update(1.0 / 60.0);
    assert!(placed(&s).is_empty());
}

#[test]
fn the_selected_cart_is_centred_and_full_size() {
    let mut s = shelf_with(5);
    s.right();
    s.right();
    settle(&mut s);
    let (x, w) = placed(&s)
        .into_iter()
        .fold((0.0, 0.0), |a, b| if b.1 > a.1 { b } else { a });
    assert!((w - CART_W as f32).abs() < 0.5, "selected cart is {w} wide");
    let centre = x + w / 2.0;
    assert!(
        (centre - 360.0).abs() < 0.5,
        "selected cart centre is {centre}"
    );
}

#[test]
fn holding_a_direction_repeats_after_a_delay() {
    let mut s = shelf_with(6);
    s.hold_right(0);
    assert_eq!(s.index, 1, "the first press did not move");
    s.tick(399);
    assert_eq!(s.index, 1, "it repeated before the delay");
    s.tick(400);
    assert_eq!(s.index, 2, "it never repeated");
    s.tick(510);
    assert_eq!(s.index, 3);
    s.release_right();
    s.tick(2_000);
    assert_eq!(s.index, 3, "it kept repeating after release");
}

/// Letting go of one direction while the other is held is a change of direction, not a stop.
#[test]
fn the_other_direction_letting_go_does_not_stop_the_repeat() {
    let mut s = shelf_with(6);
    s.hold_right(0);
    s.release_left();
    s.tick(400);
    assert_eq!(s.index, 2, "releasing left stopped a held right");
}

/// The gauge takes the shelf the wordmark had, at the same margin, so what is printed on the
/// case still lines up with the row above it. Charging, with a bolt supplied: the bolt's own
/// slot is reserved ahead of the capsule, so it is only while charging that anything actually
/// reaches all the way to the margin — discharging leaves that slot empty and the capsule
/// inset from it, which is the whole point of reserving it unconditionally.
#[test]
fn the_gauge_sits_where_the_wordmark_did() {
    let mut out = Vec::new();
    draw_footer(
        Some(Battery {
            percent: 68,
            charge: Charge::Charging,
        }),
        Printed { face: None, w: 30 },
        Some(TexId::from_raw(1)),
        Printed { face: None, w: 40 },
        &mut out,
    );
    let leftmost = out
        .iter()
        .map(|d| match *d {
            Draw::Rect { x, .. } | Draw::Tex { x, .. } => x,
            _ => f32::MAX,
        })
        .fold(f32::MAX, f32::min);
    assert_eq!(leftmost, 24.0, "the case margin is the case margin");
}

/// `draw_gauge`'s own suite proves the capsule holds still in isolation; `the_gauge_sits_where_
/// the_wordmark_did` above only ever calls `draw_footer` while charging, so nothing here was
/// exercising the discharging path through the call the app actually makes. This is that path,
/// at both charge states, at the same percent: everything but the bolt itself has to come back
/// identical.
#[test]
fn the_footer_does_not_move_the_gauge_when_the_charge_state_changes() {
    let mut idle = Vec::new();
    draw_footer(
        Some(Battery {
            percent: 68,
            charge: Charge::Discharging,
        }),
        Printed { face: None, w: 30 },
        None,
        Printed { face: None, w: 40 },
        &mut idle,
    );
    let mut charging = Vec::new();
    draw_footer(
        Some(Battery {
            percent: 68,
            charge: Charge::Charging,
        }),
        Printed { face: None, w: 30 },
        Some(TexId::from_raw(2)),
        Printed { face: None, w: 40 },
        &mut charging,
    );
    for d in &idle {
        assert!(
            charging.contains(d),
            "{d:?} moved or vanished when charging started"
        );
    }
}

/// The clock is the one thing on this band that did not change.
#[test]
fn the_clock_stays_at_the_right_margin() {
    let mut out = Vec::new();
    draw_footer(
        None,
        Printed::default(),
        None,
        Printed { face: None, w: 40 },
        &mut out,
    );
    let rightmost = out
        .iter()
        .map(|d| match *d {
            Draw::Rect { x, w, .. } | Draw::Tex { x, w, .. } => x + w,
            _ => 0.0,
        })
        .fold(0.0, f32::max);
    assert_eq!(rightmost, OUT_W as f32 - 24.0);
}

/// A device with no gauge shows a band with a clock on it, not a band with a hole in it.
#[test]
fn a_band_with_no_gauge_still_draws_its_clock() {
    let mut out = Vec::new();
    draw_footer(
        None,
        Printed::default(),
        None,
        Printed { face: None, w: 40 },
        &mut out,
    );
    assert_eq!(out.len(), 1);
}

/// The carts are what was refused. Nothing else on the screen was: the slot is part of the
/// device and the legend is printed on it, and a screen that shook wholesale would read as a
/// rendering fault rather than as a cart being rejected.
#[test]
fn a_refusal_moves_the_carts_and_leaves_the_device_where_it_is() {
    let s = shelf_with(3);
    let (mut still, mut shaken) = (Vec::new(), Vec::new());
    s.draw(0.0, &mut still);
    s.draw(9.0, &mut shaken);
    assert_eq!(still.len(), shaken.len(), "the shake changed the row");
    // The row draws first, so its quads are the leading ones. Sizes cannot tell the two
    // apart: the carts either side of the selection are drawn scaled down.
    let mut row = Vec::new();
    s.draw_row(None, 0.0, 0.0, &mut row);
    let carts = row.len();
    assert!(
        carts > 0 && carts < still.len(),
        "{carts} of {}",
        still.len()
    );
    for (i, (a, b)) in still.iter().zip(&shaken).enumerate() {
        let ((ax, _), (bx, _)) = (xw(a), xw(b));
        if i < carts {
            assert!((bx - ax - 9.0).abs() < 0.01, "a cart stood still");
        } else {
            assert_eq!(ax, bx, "the device moved with the carts");
        }
    }
}

#[test]
fn carts_past_the_edges_of_the_row_are_not_drawn() {
    let mut s = shelf_with(30);
    for _ in 0..8 {
        s.right();
    }
    settle(&mut s);
    let n = placed(&s).len();
    assert!(n > 1, "only {n} carts drawn, the neighbours should peek in");
    assert!(n <= 5, "{n} carts drawn into a 720 px row");
}

/// All three carts have to be wholly on screen. At the old pitch the outer two were clipped
/// 24px off each edge, so the row read as two and a bit rather than three.
#[test]
fn all_three_carts_fit_on_screen() {
    let s = shelf_with(5);
    let mut out = Vec::new();
    s.draw(0.0, &mut out);
    let spans = cart_spans(&out);
    assert_eq!(
        spans.len(),
        3,
        "expected three carts on screen, got {}",
        spans.len()
    );
    for (x0, x1) in &spans {
        assert!(*x0 >= 0.0, "a cart starts at {x0}, off the left edge");
        assert!(
            *x1 <= OUT_W as f32,
            "a cart ends at {x1}, off the right edge"
        );
    }
}

/// The edge margin and the gap beside the centre cart should match, or the row looks
/// crowded on one axis and loose on the other.
#[test]
fn the_row_is_evenly_spaced() {
    let s = shelf_with(5);
    let mut out = Vec::new();
    s.draw(0.0, &mut out);
    let mut spans = cart_spans(&out);
    spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let margin = spans[0].0;
    let gap = spans[1].0 - spans[0].1;
    assert!(
        (margin - gap).abs() < 4.0,
        "edge margin {margin:.1} but gap {gap:.1}: the row is lopsided"
    );
}

/// Carts only. The legend shares the draw list and its plates are short, so height is what
/// separates them.
fn cart_spans(out: &[Draw]) -> Vec<(f32, f32)> {
    out.iter()
        .filter_map(|d| match *d {
            Draw::Rect { x, w, h, .. } | Draw::Tex { x, w, h, .. } => {
                (h > 60.0).then_some((x, x + w))
            }
            Draw::Game | Draw::Shot { .. } => None,
        })
        .filter(|(x0, x1)| *x1 > 0.0 && *x0 < OUT_W as f32)
        .collect()
}

/// The row makes way for the cart going into the slot: the others part outwards and are gone
/// by the time it is seated. Fading them where they stand reads as the screen dimming rather
/// than as the shelf clearing.
#[test]
fn the_row_parts_for_the_cart_going_in() {
    let s = shelf_with(5);
    let at = |recede: f32| {
        let mut out = Vec::new();
        s.draw_row(Some("Game 0"), 0.0, recede, &mut out);
        out
    };
    let start = at(0.0);
    let part = at(0.5);
    assert_eq!(start.len(), part.len(), "a cart left the row early");

    let centre = OUT_W as f32 / 2.0;
    for (a, b) in start.iter().zip(&part) {
        let (ax, aw) = xw(a);
        let (bx, _) = xw(b);
        let side = (ax + aw / 2.0) - centre;
        assert!(
            (bx - ax).signum() == side.signum(),
            "a cart at {ax} moved to {bx}, which is towards the slot, not away from it"
        );
        assert!((bx - ax).abs() > 1.0, "the cart at {ax} did not move");
    }
    assert!(
        at(1.0).is_empty(),
        "the row is still on screen with the cart seated"
    );
}

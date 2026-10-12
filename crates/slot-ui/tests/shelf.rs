use slot_power::{Battery, Charge};
use slot_store::{Cart, Platform};
use slot_ui::{
    draw_footer, label_colour, rest_y, Draw, GbShell, Printed, Shelf, TexId, CART_W, GB_CART_H,
    GB_CART_W, OUT_W,
};

fn shelf_with(n: usize) -> Shelf {
    Shelf::new(
        (0..n)
            .map(|i| Cart {
                platform: Platform::Gba,
                stem: format!("Game {i}"),
                rom: format!("Games/GBA/Game {i}.gba").into(),
                label: None,
                code: String::new(),
                shell: None,
                title: format!("GAME {i}"),
            })
            .collect(),
    )
}

fn placed(s: &Shelf) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    s.draw_row(None, 0.0, 0.0, 1.0, &mut out);
    out.iter()
        .map(|d| match *d {
            Draw::Rect { x, w, .. } => (x, w),
            Draw::Tex { x, w, .. } => (x, w),
            Draw::Turned { x, w, .. } => (x, w),
            Draw::Game | Draw::Shot { .. } => (0.0, OUT_W as f32),
        })
        .collect()
}

fn xw(d: &Draw) -> (f32, f32) {
    match *d {
        Draw::Rect { x, w, .. } | Draw::Tex { x, w, .. } | Draw::Turned { x, w, .. } => (x, w),
        Draw::Game | Draw::Shot { .. } => (0.0, OUT_W as f32),
    }
}

fn settle(s: &mut Shelf) {
    for _ in 0..600 {
        s.update(1.0 / 60.0);
    }
}

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

#[test]
fn no_cart_is_drawn_twice_in_a_settled_row_of_three_or_more() {
    for n in [3usize, 4, 7] {
        let s = shelf_with(n);
        let mut out = Vec::new();
        s.draw_row(None, 0.0, 0.0, 1.0, &mut out);
        let drawn = drawn_cart_indices(&out);
        let mut uniq = drawn.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(drawn.len(), uniq.len(), "{n} carts: one is on screen twice");
    }
}

#[test]
fn one_cart_stands_alone_in_the_middle() {
    let s = shelf_with(1);
    let row = placed(&s);
    assert_eq!(row.len(), 1, "a lone cart is not alone on the row");
    let (x, w) = row[0];
    assert!((w - CART_W as f32).abs() < 0.5, "the lone cart is {w} wide");
    let centre = x + w / 2.0;
    assert!(
        (centre - 360.0).abs() < 0.5,
        "the lone cart sits at {centre}"
    );
}

#[test]
fn two_carts_repeat_around_the_ring() {
    let mut s = shelf_with(2);
    settle(&mut s);
    let mut out = Vec::new();
    s.draw_row(None, 0.0, 0.0, 1.0, &mut out);
    assert_eq!(
        drawn_cart_indices(&out),
        vec![1, 0, 1],
        "a row of two is not the other cart, the selection, the other cart again"
    );
    let row = placed(&s);
    let centres: Vec<f32> = row.iter().map(|(x, w)| x + w / 2.0).collect();
    assert!(
        (centres[1] - 360.0).abs() < 0.5,
        "the selected cart sits at {}, not the middle of the screen",
        centres[1]
    );
    for (a, b) in [(centres[0], centres[1]), (centres[1], centres[2])] {
        assert!(
            (b - a - CART_W as f32).abs() < 0.5,
            "the row is {} apart rather than one pitch",
            b - a
        );
    }
    assert!(
        (row[0].1 - row[2].1).abs() < 0.01,
        "the two images of one cart came out at different sizes: {} and {}",
        row[0].1,
        row[2].1
    );
}

#[test]
fn a_press_slides_the_row_rather_than_redrawing_it() {
    let occupied = |s: &Shelf| {
        let mut out = Vec::new();
        s.draw_row(None, 0.0, 0.0, 1.0, &mut out);
        let which = drawn_cart_indices(&out);
        let mut row: Vec<(i64, usize)> = out
            .iter()
            .zip(which)
            .map(|(d, i)| {
                let (x, w) = xw(d);
                (((x + w / 2.0 - 360.0) / 240.0 * 1000.0).round() as i64, i)
            })
            .collect();
        row.sort();
        row
    };
    for n in [2usize, 3, 4, 5, 10] {
        for (name, press) in [
            ("right", Shelf::right as fn(&mut Shelf)),
            ("left", Shelf::left as fn(&mut Shelf)),
        ] {
            let mut s = shelf_with(n);
            settle(&mut s);
            let before = occupied(&s);
            press(&mut s);
            assert_eq!(
                occupied(&s),
                before,
                "{n} carts: the {name} press redrew the row instead of moving it"
            );
        }
    }
}

#[test]
fn a_row_travels_the_way_it_was_pressed() {
    for n in [2usize, 3, 4, 5, 10] {
        for (name, press, way) in [
            ("right", Shelf::right as fn(&mut Shelf), 1.0f32),
            ("left", Shelf::left as fn(&mut Shelf), -1.0),
        ] {
            let mut s = shelf_with(n);
            s.select(if way > 0.0 { n - 1 } else { 0 });
            settle(&mut s);
            let mut aim = s.scroll_target();
            for tap in 0..2 * n {
                press(&mut s);
                let sent = s.scroll_target();
                assert!(
                    (sent - aim - way).abs() < 0.01,
                    "{n} carts, tap {tap} {name}: the row was sent {} from {aim}, not one slot \
                     {name}",
                    sent - aim
                );
                assert_eq!(
                    (sent - s.scroll).signum(),
                    way,
                    "{n} carts, tap {tap} {name}: the row is travelling the other way"
                );
                aim = sent;
                for _ in 0..4 {
                    s.update(1.0 / 60.0);
                }
                assert_eq!(
                    s.scroll_target(),
                    sent,
                    "{n} carts, tap {tap} {name}: the row changed its mind in mid flight"
                );
            }
        }
    }
}

#[test]
fn a_held_scroll_never_travels_against_the_button() {
    for n in [2usize, 3, 4, 5, 10] {
        for (name, hold, way) in [
            ("right", Shelf::hold_right as fn(&mut Shelf, u64), 1.0f32),
            ("left", Shelf::hold_left as fn(&mut Shelf, u64), -1.0),
        ] {
            let mut s = shelf_with(n);
            s.select(if way > 0.0 { n - 1 } else { 0 });
            hold(&mut s, 0);
            let mut was = s.scroll;
            for f in 1..120u64 {
                s.tick(f * 1000 / 60);
                s.update(1.0 / 60.0);
                assert!(
                    (s.scroll - was) * way >= -1e-4,
                    "{n} carts, held {name}: the row travelled {} at frame {f}",
                    s.scroll - was
                );
                was = s.scroll;
            }
            let gone = (s.scroll - (if way > 0.0 { n - 1 } else { 0 }) as f32) * way;
            assert!(
                gone > 14.0,
                "{n} carts, held {name}: two seconds of holding moved the row {gone} pitches"
            );
        }
    }
}

#[test]
fn the_shelf_says_where_its_selected_cart_stands() {
    for n in [1usize, 2, 3, 5] {
        for frames in [0usize, 1, 2, 3, 5, 8, 13, 400] {
            let mut s = shelf_with(n);
            settle(&mut s);
            s.right();
            for _ in 0..frames {
                s.update(1.0 / 60.0);
            }
            let stem = s.carts[s.index].stem.clone();
            let mut whole = Vec::new();
            s.draw_row(None, 0.0, 0.0, 1.0, &mut whole);
            let mut without = Vec::new();
            s.draw_row(Some(&stem), 0.0, 0.0, 1.0, &mut without);
            let dropped: Vec<(f32, f32)> = whole
                .iter()
                .map(xw)
                .filter(|q| !without.iter().map(xw).any(|k| k == *q))
                .collect();
            if n != 2 || frames == 400 {
                assert_eq!(
                    dropped.len(),
                    1,
                    "{n} carts, {frames} frames in: the row drew {} images of its selection",
                    dropped.len()
                );
            }
            let (said_x, scale) = s.selected_at();
            let said = (said_x, CART_W as f32 * scale);
            assert!(
                dropped
                    .iter()
                    .any(|(x, w)| (x - said.0).abs() < 0.01 && (w - said.1).abs() < 0.01),
                "{n} carts, {frames} frames in: the shelf says its selection is at {said:?} and \
                 the row drew it at {dropped:?}"
            );
        }
    }
}

#[test]
fn a_row_of_three_or_more_is_centred_on_its_selection() {
    for n in [3usize, 4, 7] {
        let mut s = shelf_with(n);
        s.right();
        settle(&mut s);
        let (x, w) = placed(&s)
            .into_iter()
            .fold((0.0, 0.0), |a, b| if b.1 > a.1 { b } else { a });
        let centre = x + w / 2.0;
        assert!(
            (centre - 360.0).abs() < 0.5,
            "{n} carts: the selected cart sits at {centre}"
        );
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

#[test]
fn a_held_direction_winds_down_to_a_floor() {
    let mut s = shelf_with(40);
    s.hold_right(0);
    assert_eq!(s.index, 1, "the first press did not move");

    let mut at = 400;
    for (rate, want) in [(110, 2), (85, 3), (65, 4), (50, 5)] {
        s.tick(at - 1);
        assert_eq!(s.index, want - 1, "it repeated early on its way to {want}");
        s.tick(at);
        assert_eq!(s.index, want, "it never repeated into {want}");
        at += rate;
    }

    for want in 6..=9 {
        s.tick(at - 1);
        assert_eq!(s.index, want - 1, "the floor gave way before {want}");
        s.tick(at);
        assert_eq!(s.index, want, "the floor stopped repeating at {want}");
        at += 50;
    }
}

#[test]
fn a_new_press_starts_the_repeat_over_at_the_slow_rate() {
    let mut s = shelf_with(40);
    s.hold_right(0);
    let mut at = 400;
    for rate in [110, 85, 65] {
        s.tick(at);
        at += rate;
    }
    assert_eq!(s.index, 4, "the hold did not wind down as expected");

    s.release_right();
    s.hold_right(at);
    assert_eq!(s.index, 5, "the fresh press did not move");
    s.tick(at + 400 - 1);
    assert_eq!(s.index, 5, "the fresh press repeated before the full delay");
    s.tick(at + 400);
    assert_eq!(s.index, 6, "the fresh press never repeated");
    s.tick(at + 400 + 110 - 1);
    assert_eq!(s.index, 6, "the fresh press kept the old hold's fast rate");
}

#[test]
fn the_other_direction_letting_go_does_not_stop_the_repeat() {
    let mut s = shelf_with(6);
    s.hold_right(0);
    s.release_left();
    s.tick(400);
    assert_eq!(s.index, 2, "releasing left stopped a held right");
}

#[test]
fn the_gauge_starts_at_the_case_margin() {
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

#[test]
fn the_band_prints_the_charge_at_one_margin_and_the_clock_at_the_other() {
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
    let xs: Vec<(f32, f32)> = out
        .iter()
        .filter_map(|d| match *d {
            Draw::Rect { x, w, .. } | Draw::Tex { x, w, .. } => Some((x, x + w)),
            _ => None,
        })
        .collect();
    let leftmost = xs.iter().map(|(a, _)| *a).fold(f32::MAX, f32::min);
    let last = xs
        .iter()
        .cloned()
        .fold((0.0, 0.0), |m, r| if r.1 > m.1 { r } else { m });
    assert_eq!(
        leftmost, 24.0,
        "the charge does not start at the case margin"
    );
    assert_eq!(
        last.1,
        OUT_W as f32 - 24.0,
        "the clock does not end at the case margin"
    );
    assert_eq!(
        last.1 - last.0,
        40.0,
        "the rightmost thing on the band is not the clock"
    );
    for (a, b) in &xs {
        assert!(
            *b <= 224.0 || *a >= 496.0,
            "something is printed from {a} to {b}, across the cart bay at 224..496"
        );
    }
}

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

#[test]
fn a_refusal_moves_the_carts_and_leaves_the_device_where_it_is() {
    let s = shelf_with(3);
    let (mut still, mut shaken) = (Vec::new(), Vec::new());
    s.draw(0.0, &mut still);
    s.draw(9.0, &mut shaken);
    assert_eq!(still.len(), shaken.len(), "the shake changed the row");
    let mut row = Vec::new();
    s.draw_row(None, 0.0, 0.0, 1.0, &mut row);
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

#[test]
fn the_neighbours_peek_in_from_both_edges() {
    let s = shelf_with(5);
    let mut out = Vec::new();
    s.draw(0.0, &mut out);
    let mut spans = cart_spans(&out);
    spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    assert_eq!(spans.len(), 3, "expected three carts, got {}", spans.len());
    let (left, mid, right) = (spans[0], spans[1], spans[2]);
    assert!(
        mid.0 >= 0.0 && mid.1 <= OUT_W as f32,
        "the selected cart runs off screen at {mid:?}"
    );
    assert!(
        left.0 < 0.0 && left.1 > 20.0,
        "the left neighbour is not peeking in: {left:?}"
    );
    assert!(
        right.1 > OUT_W as f32 && right.0 < OUT_W as f32 - 20.0,
        "the right neighbour is not peeking in: {right:?}"
    );
}

fn cart_spans(out: &[Draw]) -> Vec<(f32, f32)> {
    out.iter()
        .filter_map(|d| match *d {
            Draw::Rect { x, w, h, .. }
            | Draw::Tex { x, w, h, .. }
            | Draw::Turned { x, w, h, .. } => (h > 60.0).then_some((x, x + w)),
            Draw::Game | Draw::Shot { .. } => None,
        })
        .filter(|(x0, x1)| *x1 > 0.0 && *x0 < OUT_W as f32)
        .collect()
}

#[test]
fn the_row_parts_for_the_cart_going_in() {
    let s = shelf_with(5);
    let at = |recede: f32| {
        let mut out = Vec::new();
        s.draw_row(Some("Game 0"), 0.0, recede, 1.0, &mut out);
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

#[test]
fn dim_darkens_a_side_carts_face_and_not_the_black_under_it() {
    let mut s = shelf_with(3);
    let shadow = TexId::from_raw(99);
    s.set_shadow(shadow);
    let side = TexId::from_raw(11);
    s.set_faces(vec![TexId::from_raw(10), side, TexId::from_raw(12)]);
    let drawn = |dim: f32| {
        let mut out = Vec::new();
        s.draw_row(Some("Game 0"), 0.0, 0.3, dim, &mut out);
        let (x, face) = out
            .iter()
            .find_map(|d| match *d {
                Draw::Tex { x, tex, alpha, .. } if tex == side => Some((x, alpha)),
                _ => None,
            })
            .expect("the side cart is not drawn");
        let under = out
            .iter()
            .find_map(|d| match *d {
                Draw::Tex {
                    x: at, tex, alpha, ..
                } if tex == shadow && at == x => Some(alpha),
                _ => None,
            })
            .expect("nothing is drawn under the side cart");
        (face, under)
    };
    let (face, under) = drawn(1.0);
    let (dimmed, dimmed_under) = drawn(0.5);
    assert!(
        (dimmed - face * 0.5).abs() < 1e-6,
        "the face went from {face} to {dimmed} at half dim"
    );
    assert_eq!(
        dimmed_under, under,
        "the black under the side cart changed with the dim"
    );
}

fn gb_shelf_with(n: usize) -> Shelf {
    Shelf::new(
        (0..n)
            .map(|i| Cart {
                platform: Platform::Gb,
                stem: format!("Pak {i}"),
                rom: format!("Games/GB/Pak {i}.gb").into(),
                label: None,
                code: String::new(),
                shell: None,
                title: format!("PAK {i}"),
            })
            .collect(),
    )
}

#[test]
fn the_row_draws_a_game_boy_pak_at_its_own_height() {
    let mut s = gb_shelf_with(3);
    settle(&mut s);
    s.set_faces((0..3).map(|i| TexId::from_raw(20 + i)).collect());
    let mut out = Vec::new();
    s.draw_row(None, 0.0, 0.0, 1.0, &mut out);
    let (h, y) = out
        .iter()
        .find_map(|d| match *d {
            Draw::Tex { y, w, h, .. } if (w - GB_CART_W as f32).abs() < 0.01 => Some((h, y)),
            _ => None,
        })
        .expect("no cart is drawn at full size");
    assert_eq!(h, GB_CART_H as f32, "the pak was drawn at the GBA height");
    assert_eq!(
        y,
        rest_y(GB_CART_H as f32),
        "the pak is not centred on the carousel"
    );
    assert_eq!(y, y.round(), "the pak is off the pixel grid: {y}");
}

#[test]
fn a_game_boy_row_backs_its_carts_with_the_game_boy_shadow() {
    let mut s = gb_shelf_with(3);
    settle(&mut s);
    s.set_faces((0..3).map(|i| TexId::from_raw(20 + i)).collect());
    let gba = TexId::from_raw(98);
    s.set_shadow(gba);
    let drawn = |s: &Shelf| {
        let mut out = Vec::new();
        s.draw_row(None, 0.0, 0.0, 1.0, &mut out);
        out
    };
    assert!(
        !drawn(&s)
            .iter()
            .any(|d| matches!(*d, Draw::Tex { tex, .. } if tex == gba)),
        "the GBA silhouette was stretched under a Game Boy pak"
    );
    let gb = TexId::from_raw(97);
    s.set_gb_shadow(GbShell::Notched, gb);
    assert!(
        drawn(&s)
            .iter()
            .any(|d| matches!(*d, Draw::Tex { tex, .. } if tex == gb)),
        "nothing backs the dimmed paks once their own shadow is uploaded"
    );
}

#[test]
fn a_colour_pak_and_a_grey_one_are_backed_by_their_own_shells() {
    let d = tempfile::tempdir().expect("tempdir");
    let games = d.path().join("Games/GB");
    std::fs::create_dir_all(&games).expect("create games dir");
    let carts: Vec<Cart> = [("Grey", 0x00u8), ("Clear", 0xc0)]
        .iter()
        .map(|(stem, cgb)| {
            let mut rom = vec![0u8; 0x150];
            rom[0x143] = *cgb;
            let path = games.join(format!("{stem}.gb"));
            std::fs::write(&path, rom).expect("write rom");
            Cart {
                platform: Platform::Gb,
                stem: (*stem).into(),
                rom: path,
                label: None,
                code: String::new(),
                shell: None,
                title: (*stem).to_uppercase(),
            }
        })
        .collect();

    let notched = TexId::from_raw(90);
    let rounded = TexId::from_raw(91);
    for (selected, neighbour_shell) in [(0usize, rounded), (1, notched)] {
        let mut s = Shelf::new(carts.clone());
        s.select(selected);
        settle(&mut s);
        s.set_faces(vec![TexId::from_raw(20), TexId::from_raw(21)]);
        s.set_gb_shadow(GbShell::Notched, notched);
        s.set_gb_shadow(GbShell::Rounded, rounded);
        let mut out = Vec::new();
        s.draw_row(None, 0.0, 0.0, 1.0, &mut out);
        let backings: Vec<TexId> = out
            .iter()
            .filter_map(|d| match *d {
                Draw::Tex { tex, .. } if tex == notched || tex == rounded => Some(tex),
                _ => None,
            })
            .collect();
        assert_eq!(
            backings,
            vec![neighbour_shell; 2],
            "with the {} pak selected, its neighbour was backed by the wrong shell",
            carts[selected].stem
        );
    }
}

#[test]
fn a_cart_pushed_onto_the_row_draws_rather_than_stopping_the_device() {
    let mut s = gb_shelf_with(2);
    let gb = TexId::from_raw(97);
    let gba = TexId::from_raw(98);
    s.set_shadow(gba);
    s.set_gb_shadow(GbShell::Notched, gb);
    settle(&mut s);
    let backed = |s: &Shelf| {
        let mut out = Vec::new();
        s.draw_row(None, 0.0, 0.0, 1.0, &mut out);
        out.iter()
            .filter(|d| matches!(**d, Draw::Tex { tex, .. } if tex == gb || tex == gba))
            .count()
    };
    assert_eq!(backed(&s), 2, "the neighbour was not backed to begin with");

    s.carts.push(Cart {
        platform: Platform::Gb,
        stem: "Pushed".into(),
        rom: "Games/GB/Pushed.gb".into(),
        label: None,
        code: String::new(),
        shell: None,
        title: "PUSHED".into(),
    });
    settle(&mut s);
    assert_eq!(
        backed(&s),
        2,
        "the row holding a cart the shells never heard of did not draw both its neighbours"
    );
}

#[test]
fn a_clear_cart_is_drawn_over_its_board_and_a_solid_one_is_not() {
    let cart = |stem: &str, code: &str| Cart {
        platform: Platform::Gba,
        stem: stem.into(),
        rom: format!("Games/GBA/{stem}.gba").into(),
        label: None,
        code: code.into(),
        shell: None,
        title: stem.to_uppercase(),
    };
    let board = TexId::from_raw(80);
    for (code, clear) in [("BPEE", true), ("BPRE", false)] {
        let mut s = Shelf::new(vec![cart("Game", code)]);
        settle(&mut s);
        let face = TexId::from_raw(20);
        s.set_faces(vec![face]);
        s.set_backing(board);
        let mut out = Vec::new();
        s.draw_row(None, 0.0, 0.0, 1.0, &mut out);
        let at = |t: TexId| {
            out.iter()
                .position(|d| matches!(*d, Draw::Tex { tex, .. } if tex == t))
        };
        assert_eq!(at(board).is_some(), clear, "{code}: board layer");
        if clear {
            assert!(at(board) < at(face), "the board was drawn over the plastic");
        }
    }
}

#[test]
fn a_clear_cart_draws_its_interior_then_the_board_then_the_plastic() {
    let mut s = Shelf::new(vec![Cart {
        platform: Platform::Gba,
        stem: "Game".into(),
        rom: "Games/GBA/Game.gba".into(),
        label: None,
        code: "BPEE".into(),
        shell: None,
        title: "GAME".into(),
    }]);
    settle(&mut s);
    let (interior, board, face) = (
        TexId::from_raw(81),
        TexId::from_raw(80),
        TexId::from_raw(20),
    );
    s.set_faces(vec![face]);
    s.set_backing(board);
    s.set_interior(0, interior);
    let mut out = Vec::new();
    s.draw_row(None, 0.0, 0.0, 1.0, &mut out);
    let at = |t: TexId| {
        out.iter()
            .position(|d| matches!(*d, Draw::Tex { tex, .. } if tex == t))
            .expect("layer drawn")
    };
    assert!(at(interior) < at(board) && at(board) < at(face));
    let size = |t: TexId| {
        out.iter().find_map(|d| match *d {
            Draw::Tex { w, h, tex, .. } if tex == t => Some((w, h)),
            _ => None,
        })
    };
    assert_eq!(
        size(interior),
        size(face),
        "the back shell is not the cart's size"
    );
    assert_eq!(size(board), size(face), "the board is not the cart's size");
    assert_eq!(s.take_interior(0), Some(interior));
}

#[test]
fn a_clear_pak_draws_its_interior_then_the_board_then_the_plastic() {
    use slot_store::{Outline, ShellChoice, ShellFinish};
    let mut s = Shelf::new(vec![Cart {
        platform: Platform::Gb,
        stem: "Pak".into(),
        rom: "Games/GB/Pak.gb".into(),
        label: None,
        code: String::new(),
        shell: Some(ShellChoice {
            outline: Outline::Notched,
            colour: [0xff; 3],
            finish: ShellFinish::Clear,
        }),
        title: "PAK".into(),
    }]);
    settle(&mut s);
    let (interior, board, face) = (
        TexId::from_raw(81),
        TexId::from_raw(80),
        TexId::from_raw(20),
    );
    s.set_faces(vec![face]);
    s.set_gb_backing(GbShell::Notched, board);
    s.set_interior(0, interior);
    let mut out = Vec::new();
    s.draw_row(None, 0.0, 0.0, 1.0, &mut out);
    let at = |t: TexId| {
        out.iter()
            .position(|d| matches!(*d, Draw::Tex { tex, .. } if tex == t))
            .expect("layer drawn")
    };
    assert!(at(interior) < at(board) && at(board) < at(face));
}

use slot_power::{Battery, Charge};
use slot_store::{Cart, Platform};
use slot_ui::{
    draw_footer, label_colour, mark_at, mark_box, rest_y, Draw, GbShell, Printed, Shelf, TexId,
    CART_W, GB_CART_H, OUT_W, PLATE_H,
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

/// Three or more carts have one image each on the row. Only a ring of two repeats, and it does
/// so because there is no third cart to put in the third slot: a longer row has one and must
/// use it, or the shelf is showing a cart twice while another is not on screen at all.
#[test]
fn no_cart_is_drawn_twice_in_a_row_of_three_or_more() {
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

/// A shelf with one cart on it stands that cart dead centre and draws nothing else — it does
/// *not* repeat the way a shelf of two does. Repeating would put three identical faces across a
/// row that can never move, because there is no second cart for a press to select, and three
/// copies of one cart holding still read as a drawing fault rather than as a ring. A slot left
/// empty beside it would be no better, so the row is the one cart and nothing else.
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

/// Two carts fill all three slots, which means the cart that is not selected stands on both
/// sides of the one that is. The user asked for this on the hardware, against both of the
/// layouts that came before it — a hole beside the pair, then the pair centred together — so it
/// is the shape of the row, not an accident of the wrap.
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
            (b - a - 240.0).abs() < 0.5,
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

/// The whole reason the repeat was refused when it was first proposed: a ring of two wraps on
/// every press, and the same cart is both the selection and a neighbour. What the eye has to
/// read is a row sliding one pitch, so no cart may change which offset it stands at between the
/// frame before a press and the frame after it — a cart that blinks out at one edge and back in
/// at the other is the row teleporting rather than turning.
#[test]
fn a_press_on_a_row_of_two_slides_the_row_rather_than_swapping_its_carts() {
    // Each drawn cart as (which cart, where it is in pitches from the middle), rounded, so the
    // two sides of a press can be compared as sets of positions.
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
    for (name, press) in [
        ("right", Shelf::right as fn(&mut Shelf)),
        ("left", Shelf::left as fn(&mut Shelf)),
    ] {
        let mut s = shelf_with(2);
        settle(&mut s);
        let before = occupied(&s);
        press(&mut s);
        assert_eq!(
            occupied(&s),
            before,
            "the {name} press redrew the row instead of moving it"
        );
    }
}

/// Which way the row goes is the only thing on screen that says which button was pressed, since
/// both neighbours are the same cart. A ring of two wraps on every press, so the nearest image
/// of the selection is one slot either side and the spring cannot choose between them on
/// distance. What settles it is the press, including a press that lands while the row is still
/// moving from the one before it — which is where measuring from where the row happens to be
/// picked the image behind it and turned the row round.
#[test]
fn a_row_of_two_travels_the_way_it_was_pressed() {
    for (name, press, way) in [
        ("right", Shelf::right as fn(&mut Shelf), 1.0f32),
        ("left", Shelf::left as fn(&mut Shelf), -1.0),
    ] {
        let mut s = shelf_with(2);
        settle(&mut s);
        let mut aim = s.scroll_target();
        for tap in 0..4 {
            press(&mut s);
            let sent = s.scroll_target();
            assert!(
                (sent - aim - way).abs() < 0.01,
                "tap {tap} {name}: the row was sent {} from {aim}, not one slot {name}",
                sent - aim
            );
            assert_eq!(
                (sent - s.scroll).signum(),
                way,
                "tap {tap} {name}: the row is travelling the other way"
            );
            aim = sent;
            // Part way there, so the next press lands with the spring still moving, which is
            // where measuring from the row's own place picked the image behind it.
            for _ in 0..4 {
                s.update(1.0 / 60.0);
            }
            assert_eq!(
                s.scroll_target(),
                sent,
                "tap {tap} {name}: the row changed its mind in mid flight"
            );
        }
    }
}

/// Where the selected cart stands, which is what a cart going into the slot and a cart the
/// picker opens both start from. Every length of row centres its selection, so the answer is
/// the middle of the screen at each of them — asked here rather than assumed, because the
/// handover is a jump the moment the two disagree.
#[test]
fn the_shelf_says_where_its_selected_cart_stands() {
    for n in [1usize, 2, 3, 5] {
        let mut s = shelf_with(n);
        settle(&mut s);
        let widest = placed(&s)
            .into_iter()
            .fold((0.0, 0.0), |a, b| if b.1 > a.1 { b } else { a });
        assert!(
            (widest.0 - s.rest_x()).abs() < 0.5,
            "{n} carts: the selection stands at {} and the shelf says {}",
            widest.0,
            s.rest_x()
        );
    }
}

/// Three or more is the row as it has always been: the selection dead centre with a neighbour
/// peeking in either side.
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

/// Letting go of one direction while the other is held is a change of direction, not a stop.
#[test]
fn the_other_direction_letting_go_does_not_stop_the_repeat() {
    let mut s = shelf_with(6);
    s.hold_right(0);
    s.release_left();
    s.tick(400);
    assert_eq!(s.index, 2, "releasing left stopped a held right");
}

/// The gauge starts at the case margin, where the wordmark used to be. It briefly did not: the
/// shelf's mark stood there and held the gauge one mark and one gap further in. The mark has gone
/// to the top plate's corner and the band is the readout again, so nothing is reserved at this
/// end for anything.
///
/// Charging, with a bolt supplied, because the gauge's own leftmost piece is the bolt's reserved
/// slot and only a charging device fills it — which is what would otherwise make this reading
/// about the capsule rather than about the margin.
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
    assert!(
        out.contains(&Draw::Tex {
            x: 24.0,
            y: 444.0,
            w: 14.0,
            h: 14.0,
            tex: TexId::from_raw(1),
            alpha: 1.0,
        }),
        "the thing at the margin is not the bolt's own slot: {out:?}"
    );
}

/// And the mark it lost is somewhere else entirely: the top right corner, held off both edges by
/// the case's own margin — the one the gauge and the clock are printed at, and the one this file
/// asserts above. Read from `mark_box` and `mark_at` together rather than from four numbers, so
/// moving either moves this with it.
///
/// The right edge is what is really held here. A mark and the clock are at the same end of the
/// screen with nothing between them, and the mark spent a release inset half as far as the clock
/// was, which is what "tucked into the corner" turned out to mean. The halo is the one pixel of
/// slack: a mark's box carries a transparent ring that the clock's type does not.
///
/// The plate is deliberately not part of this any more. A mark is drawn on the carousel, where
/// the HUD's plate is usually not on screen at all, and it is now taller than the plate is deep
/// — so a rule that fitted it inside 40 px would be a rule that capped the drawing at a size the
/// device has twice said is too small. What is held instead is that it clears the plate's depth,
/// which is the geometry the layout now depends on: if a mark ever fits inside the plate again,
/// the reasoning in `mark_at` and in `badge_at` about why the two came apart is stale.
#[test]
fn a_mark_is_held_off_the_screen_edges_by_the_case_margin() {
    let (w, h) = mark_box();
    let (x, y) = mark_at(w as f32);
    assert_eq!(
        x + w as f32,
        OUT_W as f32 - 24.0,
        "the mark's right edge is not on the case margin the clock is printed at"
    );
    assert_eq!(y, 16.0, "the mark is not held off the top of the screen");
    assert!(
        y + h as f32 > PLATE_H,
        "a {w}x{h} mark at {y} fits inside the {PLATE_H} px plate again, which is not what \
         `mark_at` and `badge_at` say about each other"
    );
}

/// `draw_gauge`'s own suite proves the capsule holds still in isolation; `the_gauge_starts_at_
/// the_case_margin` above only ever calls `draw_footer` while charging, so nothing here was
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

/// The clock is the one thing on this band that never changed: a mark arrived at the other end
/// of it and left again, and this end is where it was throughout.
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
            Draw::Rect { x, w, h, .. }
            | Draw::Tex { x, w, h, .. }
            | Draw::Turned { x, w, h, .. } => (h > 60.0).then_some((x, x + w)),
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

/// Dimming darkens a side cart's face and leaves the black under it as the recede has it, so a
/// dimmed cart reads as a cart in shadow rather than a ghost over the wallpaper.
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
                title: format!("PAK {i}"),
            })
            .collect(),
    )
}

/// A Game Boy Game Pak is the same width as a GBA cart and 1.87x as tall, so a row that drew
/// every cart at one size would squash it. It is centred on the carousel exactly as a GBA cart
/// is — the two share a centre, not a floor — so its own floor is 59 px lower than the GBA
/// shelf's and its top edge is 59 px lower too.
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
            Draw::Tex { y, w, h, .. } if (w - CART_W as f32).abs() < 0.01 => Some((h, y)),
            _ => None,
        })
        .expect("no cart is drawn at full size");
    assert_eq!(h, GB_CART_H as f32, "the pak was drawn at the GBA height");
    assert_eq!(
        y,
        rest_y(GB_CART_H as f32),
        "the pak is not centred on the carousel"
    );
}

/// The black backing under a dimmed cart is the cart's own outline. A Game Boy shelf that has
/// only the GBA silhouette uploaded draws no backing rather than a tapered one stretched to a
/// straight sided pak.
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

/// There are three moulds, not two, and the backing is one per mould. A class C pak's top
/// corners are rounded where a class A/B pak's are stepped, so backing one with the other's
/// outline either paints black beside the cart or leaves a corner of the dimmed face with
/// nothing behind it — and over a light wallpaper that corner reads as a bite out of the cart.
///
/// The shell is read off the rom's CGB flag, so these are real files on a real temporary card:
/// what is being checked is that the row picks between the two backings by what the cartridge
/// actually is, and that it does it without opening anything while drawing.
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
                title: (*stem).to_uppercase(),
            }
        })
        .collect();

    let notched = TexId::from_raw(90);
    let rounded = TexId::from_raw(91);
    // Only a dimmed cart is backed, and the selection is not dimmed — so whichever backing the
    // row draws belongs to the *neighbour*, which is what makes the answer unambiguous. A row
    // of two repeats, so that neighbour stands on both sides of the selection and its backing
    // comes back twice: the same shell, drawn under each of its two images.
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

/// `carts` is public and the mould each one came out of is not: `shells` is built once, in
/// `Shelf::new`, with one entry per cart. Nothing in the workspace mutates `carts` today, so the
/// two cannot currently disagree — but they are two vectors kept in step by nobody, and the
/// place that reads them together is the draw loop. A cart pushed straight onto the public field
/// used to index one entry past the end of `shells` and panic there, which on the device is a
/// black screen and a handset that has stopped, with the message nowhere anybody can read it.
///
/// So the row draws whatever it has been given. A cart whose mould was never recorded is backed
/// with the straight sided silhouette, exactly as a cart whose face was never uploaded still
/// holds its place in the row rather than leaving a hole in it.
///
/// The pushed cart is a Game Boy pak on a Game Boy row, which is the shape that makes the
/// difference visible at all: if this ever goes back to indexing, it is a panic and not a wrong
/// backing, and the row is one cart longer than the shells are either way.
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
    assert_eq!(backed(&s), 1, "the neighbour was not backed to begin with");

    // Straight onto the public field, which is the only way the two can come apart. The
    // selection stays where it is, so the new cart stands as a *neighbour* — dimmed, and
    // therefore backed, which is the one read that ever looks a cart's mould up.
    s.carts.push(Cart {
        platform: Platform::Gb,
        stem: "Pushed".into(),
        rom: "Games/GB/Pushed.gb".into(),
        label: None,
        code: String::new(),
        title: "PUSHED".into(),
    });
    settle(&mut s);
    // Drawn at all is the whole claim. Which silhouette backs it is the graceful part; that the
    // device is still running to draw anything is the part this exists for.
    assert_eq!(
        backed(&s),
        2,
        "the row holding a cart the shells never heard of did not draw both its neighbours"
    );
}

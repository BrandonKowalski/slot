use slot_power::{Battery, Charge};
use slot_store::{Cart, Platform};
use slot_ui::{
    draw_footer, label_colour, rest_y, Draw, GbShell, Printed, Shelf, TexId, CART_W, GB_CART_H,
    OUT_W,
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

/// A row of three or more, *standing still*, has one image of each cart on screen. Only a ring
/// of two repeats when it is settled, and it does so because there is no third cart to put in the
/// third slot: a longer row has one and must use it, or the shelf is showing a cart twice while
/// another is not on screen at all.
///
/// Standing still is the whole of the claim and the reason the row below is never pressed. The
/// ring fills every slot at every length, and while a row is moving the screen is wider than
/// three pitches — so a ring of three or of four does put one cart at both edges at once, each
/// half out of frame, which is what a ring shorter than the window looks like drawn honestly. The
/// alternative was withholding that image, and withholding it left the cart leaving the frame
/// undrawn: see `render_row_edges.rs`. From five carts up the ring is wider than the screen and
/// nothing is ever repeated, moving or not.
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
///
/// Every length, not only two. A press moves the selection before the spring has moved the row,
/// so the frame after it has to draw the same carts in the same places the frame before it did —
/// which is a claim about the *row*, and the row of two was merely where it was noticed. It was
/// false at three carts and at four: the cart in the left slot, fully on screen, was not in the
/// list at all on the frame after the press. Held at ten too, which was always right, so this
/// cannot start passing because every length got equally wrong.
#[test]
fn a_press_slides_the_row_rather_than_redrawing_it() {
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

/// Which way the row goes is what says which button was pressed — on a ring of two it is the
/// only thing on screen that does, since both neighbours are the same cart. The row is a ring, so
/// the selection has an image every `n` slots, and the spring used to head for whichever image
/// stood nearest where the row already was. The row is behind its own target whenever it is
/// moving, though, and a press asks for an image one slot further on again, so past a lag of
/// `n / 2 - 1` pitches the image *behind* the row was the nearer one and the row set off the
/// wrong way. That is no pitches at all on a ring of two and half a pitch on a ring of three,
/// both of which a second press inside five frames clears.
///
/// Every length is checked from the cart a press wraps off the end of, because a wrap is where
/// the two answers differ: anywhere else in the row there is only one image to choose.
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
                // Part way there, so the next press lands with the spring still moving, which is
                // where measuring from the row's own place picked the image behind it.
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

/// A direction held down, which is the shape the fault was actually reported in: the repeat lands
/// a press every 110 ms, so the row is still travelling when the next one arrives, every time.
/// On three carts that was enough for the old target to flip to the image a lap behind, and the
/// row then ran backwards for eight frames out of every twenty eight — a stutter, under a button
/// held steadily one way, at the ends of the shelf where every press is a wrap.
///
/// Driven the way `App::update` drives it, `tick` then `update` once a frame, for two seconds:
/// the repeat delay and then fifteen repeats, which is five laps of a three cart row.
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
            // And it got somewhere: fifteen repeats plus the press itself, one pitch each.
            let gone = (s.scroll - (if way > 0.0 { n - 1 } else { 0 }) as f32) * way;
            assert!(
                gone > 14.0,
                "{n} carts, held {name}: two seconds of holding moved the row {gone} pitches"
            );
        }
    }
}

/// Where the selected cart stands, which is what a cart going into the slot and a cart the
/// picker opens both start from. Every length of row centres its selection, so a settled row
/// answers with the middle of the screen at full size — asked here rather than assumed, because
/// the handover is a jump the moment the two disagree.
///
/// And asked on the frames that are not settled, which is where it was wrong. `rest_x`, which
/// this replaces, answered "dead centre, 240 wide" whatever the spring was doing, and the app
/// reads it on the frame A or START goes down — which nothing makes the player delay until the
/// row has stopped. Two frames after a shoulder press the selection is most of a pitch off
/// centre and shrunk to near `SIDE_SCALE`, so the cart handed to the slot jumped 266 px sideways
/// and grew a third of its own width on that one frame, with the cart it was passing still
/// sliding behind it.
///
/// The reading is taken as the difference between the row drawn whole and the row drawn with
/// the selection held back — which is exactly the swap the handover performs — so what is
/// compared is the quad the chrome has to replace and not a quad picked out by being the widest.
/// Mid-travel the selection is not the widest: at half a pitch out it is the same size as the
/// neighbour it is passing.
#[test]
fn the_shelf_says_where_its_selected_cart_stands() {
    for n in [1usize, 2, 3, 5] {
        // Settled, then every frame of a press's travel, so the claim covers the frames the row
        // is moving rather than only the one it has stopped on.
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
            // One image, except on a ring of two while it is travelling: there the selection
            // really is at both edges at once, and the chrome replacing both with one is the
            // row of two's own business rather than this claim's.
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

/// A held direction speeds up the longer it is held, which is what makes a long library
/// crossable without making a short hop overshoot. The rates are 110, 85, 65 then 50 ms, and the
/// last one is a floor rather than a step on the way to zero.
#[test]
fn a_held_direction_winds_down_to_a_floor() {
    let mut s = shelf_with(40);
    s.hold_right(0);
    assert_eq!(s.index, 1, "the first press did not move");

    // The delay, then each rate in turn. Every tick is one millisecond before the repeat is due
    // and then exactly on it, so a rate that had quietly changed would show up as a missed step.
    let mut at = 400;
    for (rate, want) in [(110, 2), (85, 3), (65, 4), (50, 5)] {
        s.tick(at - 1);
        assert_eq!(s.index, want - 1, "it repeated early on its way to {want}");
        s.tick(at);
        assert_eq!(s.index, want, "it never repeated into {want}");
        at += rate;
    }

    // Held on, the floor holds: four more repeats at 50 ms and not one sooner.
    for want in 6..=9 {
        s.tick(at - 1);
        assert_eq!(s.index, want - 1, "the floor gave way before {want}");
        s.tick(at);
        assert_eq!(s.index, want, "the floor stopped repeating at {want}");
        at += 50;
    }
}

/// Acceleration belongs to one hold, not to the shelf. A row of separate presses is someone
/// choosing rather than travelling, and it is paced exactly as it was before any of this.
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

    // Let go and press again: the delay is the long one again and so is the first repeat.
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
/// slot and only a charging device fills it, which is what would otherwise make this reading
/// about the capsule rather than about the margin.
///
/// The gauge was at the left margin and is at the right one now, with the shelf's name having
/// taken the left. What is held is the same thing either way: it ends on the case margin, not a
/// pixel inside it, and the piece that lands there is the percent rather than the capsule.
#[test]
fn the_gauge_ends_at_the_case_margin() {
    let mut out = Vec::new();
    draw_footer(
        Printed::default(),
        Some(Battery {
            percent: 68,
            charge: Charge::Charging,
        }),
        Printed { face: None, w: 30 },
        Some(TexId::from_raw(1)),
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
    assert_eq!(
        rightmost,
        OUT_W as f32 - 24.0,
        "the case margin is the case margin"
    );
    // What lands on the margin is the percent, the gauge's own last piece, rather than the
    // capsule or the clock. `draw_gauge`'s suite holds the order inside the gauge; this holds
    // only that the whole of it finishes on the case margin.
    let ends_on_margin = out.iter().any(|d| match *d {
        Draw::Rect { x, w, .. } | Draw::Tex { x, w, .. } => {
            (x + w - (OUT_W as f32 - 24.0)).abs() < 0.01
        }
        _ => false,
    });
    assert!(
        ends_on_margin,
        "nothing finishes on the case margin: {out:?}"
    );
}

/// The band's two ends, which is the whole of the space it has: the cart slot's bay, opening and
/// thumb scoop run through its middle, so nothing may be printed between them.
///
/// The shelf's name takes the left margin, and the charge takes the right one with the clock
/// inboard of it. Read out of `draw_footer`'s own output rather than typed as coordinates, so
/// moving either end moves this with it.
#[test]
fn the_band_prints_its_name_at_one_margin_and_the_charge_at_the_other() {
    let mut out = Vec::new();
    draw_footer(
        Printed { face: None, w: 160 },
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
    let rightmost = xs.iter().map(|(_, b)| *b).fold(0.0, f32::max);
    assert_eq!(
        leftmost, 24.0,
        "the shelf's name does not start at the case margin"
    );
    assert_eq!(
        rightmost,
        OUT_W as f32 - 24.0,
        "the charge does not end at the case margin"
    );
    // And nothing at all is printed across the bay.
    for (a, b) in &xs {
        assert!(
            *b <= 224.0 || *a >= 496.0,
            "something is printed from {a} to {b}, across the cart bay at 224..496"
        );
    }
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
        Printed::default(),
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
        Printed::default(),
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
        Printed::default(),
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
        Printed::default(),
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
    // Twice, not once: a row of two repeats, so its one neighbour stands on both sides of the
    // selection and is backed under each of its two images. This read 1 until the repeat landed
    // and was left behind by it — before that a row of two had a hole where the second image now
    // stands, and only one cart on the row was dimmed.
    assert_eq!(backed(&s), 2, "the neighbour was not backed to begin with");

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

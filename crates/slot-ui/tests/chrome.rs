use slot_store::{Cart, Platform};
use slot_ui::{
    cart_box, draw_empty_slot, edge, housing, icon_box, opening, recess, Draw, Shelf, SlotChrome,
    ALERT_PX, CART_H, CART_W, GB_LABEL_H, GB_LABEL_Y, LABEL_H, LABEL_Y, MOUTH_H, OUT_H, OUT_W,
};

/// Where a shelf of three or more stands its selected cart, which is what the chrome is handed
/// everywhere but on a shelf holding exactly two.
const CENTRED: f32 = (OUT_W - CART_W) as f32 / 2.0;

fn cart() -> Cart {
    Cart {
        platform: Platform::Gba,
        stem: "Emerald".into(),
        rom: "Games/GBA/Emerald.gba".into(),
        label: None,
        code: String::new(),
        title: "POKEMON EMER".into(),
    }
}

/// A Game Boy Game Pak: the same width and 1.87x the height. Everything about the travel that
/// is a property of the slot rather than of the cartridge has to come out the same for this one
/// as for the cart above, which is why the travel tests below run over both.
fn pak() -> Cart {
    Cart {
        platform: Platform::Gb,
        stem: "Tetris".into(),
        rom: "Games/GB/Tetris.gb".into(),
        label: None,
        code: String::new(),
        title: "TETRIS".into(),
    }
}

/// Both cartridges, each named for the failure message.
fn both() -> [(&'static str, Cart); 2] {
    [("the GBA cart", cart()), ("the Game Boy pak", pak())]
}

/// Where the label well of this cartridge starts and how tall it is. A pak's paper sits high on
/// a tall body and a GBA cart's sits low on a short one, so a test about how much label shows
/// has to ask the cartridge rather than reach for one platform's constants.
fn label_band(c: &Cart) -> (f32, f32) {
    match c.platform {
        Platform::Gba => (LABEL_Y as f32, LABEL_H as f32),
        Platform::Gb | Platform::Gbc => (GB_LABEL_Y as f32, GB_LABEL_H as f32),
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
struct Quad {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

fn quad(d: &Draw) -> Quad {
    match *d {
        Draw::Rect { x, y, w, h, .. }
        | Draw::Tex { x, y, w, h, .. }
        | Draw::Turned { x, y, w, h, .. } => Quad { x, y, w, h },
        // The pass owns its own rect. Fully on is what a list carrying one is asking for.
        Draw::Game | Draw::Shot { .. } => Quad {
            x: 0.0,
            y: 0.0,
            w: OUT_W as f32,
            h: OUT_H as f32,
        },
    }
}

/// A quad the width of a cartridge. Asked of both shapes rather than of `CART_W`, so a pak
/// drawn at a width of its own is still found instead of the detector silently matching
/// nothing — the failure that once had one test fail and another pass for the wrong reason.
fn is_cart(d: &Draw) -> bool {
    [Platform::Gba, Platform::Gb]
        .iter()
        .any(|p| (quad(d).w - cart_box(*p).0 as f32).abs() < 0.01)
}

fn is_game_layer(d: &Draw) -> bool {
    matches!(d, Draw::Game)
}

fn is_lip(d: &Draw) -> bool {
    tinted(d, edge())
}

/// By colour, not by size. The bands were identified by their dimensions, which meant that
/// reshaping the slot made every detector silently match nothing: one test then failed and
/// another passed for the wrong reason.
fn tinted(d: &Draw, c: [f32; 4]) -> bool {
    match *d {
        Draw::Rect { colour, .. } => (0..3).all(|i| (colour[i] - c[i]).abs() < 0.001),
        _ => false,
    }
}

fn is_mouth(d: &Draw) -> bool {
    tinted(d, opening())
}

fn is_housing(d: &Draw) -> bool {
    tinted(d, housing())
}

fn as_mouth(d: &Draw) -> Option<Quad> {
    is_mouth(d).then(|| quad(d))
}

fn as_lip(d: &Draw) -> Option<Quad> {
    is_lip(d).then(|| quad(d))
}

fn alpha(d: &Draw) -> f32 {
    match *d {
        Draw::Rect { colour, .. } => colour[3],
        Draw::Tex { alpha, .. } | Draw::Turned { alpha, .. } => alpha,
        Draw::Game | Draw::Shot { .. } => 1.0,
    }
}

fn opaque(d: &Draw) -> bool {
    alpha(d) >= 1.0
}

fn cart_at(out: &[Draw]) -> usize {
    out.iter()
        .position(is_cart)
        .expect("no cart sized quad in the list")
}

/// How much of the cart the slot leaves showing: the part of its quad that nothing opaque is
/// painted over afterwards. Only that counts, because the cart quad is drawn whole whatever
/// the depth. A cart merely carried off the bottom of the screen is the failure this
/// measures, not a smaller number.
fn cart_visible_height(out: &[Draw]) -> f32 {
    let i = cart_at(out);
    let cart = quad(&out[i]);
    let lid = out[i + 1..]
        .iter()
        .filter(|d| opaque(d))
        .map(|d| quad(d).y)
        .fold(f32::INFINITY, f32::min);
    (cart.y + cart.h).min(lid).max(cart.y) - cart.y
}

fn chrome_into(c: &Cart, seat: f32, out: &mut Vec<Draw>) {
    SlotChrome {
        cart: c,
        face: None,
        rest: CENTRED,
        seat,
        alert: None,
        dim: 0.5,
        screen: 0.0,
        game: true,
    }
    .draw(out);
}

/// A shelf of two carts is centred as a pair, so the cart that goes in was not standing in the
/// middle of the screen. It must leave from where it stood and arrive over the mouth: starting
/// it at the slot instead is a cart that teleports on the frame the button is pressed, and
/// leaving it off to the side is a cart that goes into the case beside the opening.
#[test]
fn a_cart_standing_off_centre_slides_across_as_it_goes_in() {
    let c = cart();
    let off = CENTRED - 120.0;
    let cart_x = |seat: f32| {
        let mut out = Vec::new();
        SlotChrome {
            cart: &c,
            face: None,
            rest: off,
            seat,
            alert: None,
            dim: 0.0,
            screen: 0.0,
            game: false,
        }
        .draw(&mut out);
        quad(&out[cart_at(&out)]).x
    };
    assert!(
        (cart_x(0.0) - off).abs() < 0.01,
        "the cart starts at {} rather than where the shelf left it",
        cart_x(0.0)
    );
    assert!(
        (cart_x(1.0) - CENTRED).abs() < 0.01,
        "the cart seats at {} rather than in the mouth",
        cart_x(1.0)
    );
    let mut last = off;
    for step in 1..=20 {
        let x = cart_x(step as f32 / 20.0);
        assert!(x >= last - 0.01, "the cart went back to {x} from {last}");
        last = x;
    }
}

/// The slot as it is drawn `t` of the way through the power on: cart seated, picture coming
/// up behind the housing.
fn draw_powering_on(t: f32, out: &mut Vec<Draw>) {
    let c = cart();
    SlotChrome {
        cart: &c,
        face: None,
        rest: CENTRED,
        seat: 1.0,
        alert: None,
        dim: 0.0,
        screen: t,
        game: true,
    }
    .draw(out);
}

fn bands(t: f32) -> Vec<Draw> {
    let mut out = Vec::new();
    draw_powering_on(t, &mut out);
    out.into_iter()
        .filter(|d| is_lip(d) || is_mouth(d) || is_housing(d))
        .collect()
}

fn chrome(c: &Cart, seat: f32) -> Vec<Draw> {
    let mut out = Vec::new();
    chrome_into(c, seat, &mut out);
    out
}

fn draw_inserting(c: &Cart, t: f32, out: &mut Vec<Draw>) {
    chrome_into(c, t, out);
}

fn draw_ejecting(c: &Cart, t: f32, out: &mut Vec<Draw>) {
    chrome_into(c, 1.0 - t, out);
}

/// Just short of seated. A cart that has arrived is not in the list at all.
fn cart_y(c: &Cart, t: f32) -> f32 {
    let out = chrome(c, t.min(0.999));
    quad(&out[cart_at(&out)]).y
}

fn visible_cart_height(c: &Cart, t: f32) -> f32 {
    let mut out = Vec::new();
    draw_inserting(c, t, &mut out);
    cart_visible_height(&out)
}

#[test]
fn the_slot_is_at_the_bottom_of_the_screen() {
    let mut out = Vec::new();
    draw_inserting(&cart(), 0.5, &mut out);
    let mouth = out.iter().find_map(as_mouth).expect("no mouth drawn");
    assert!(
        mouth.y >= OUT_H as f32 - MOUTH_H - 1.0,
        "the mouth is not on the bottom edge"
    );
}

#[test]
fn the_cart_travels_downward() {
    for (name, c) in both() {
        let early = cart_y(&c, 0.1);
        let late = cart_y(&c, 0.9);
        assert!(late > early, "{name} is going up, not down into the slot");
    }
}

#[test]
fn the_cart_is_progressively_occluded_by_the_lip() {
    for (name, c) in both() {
        let h = |t: f32| visible_cart_height(&c, t);
        assert!(h(0.5) < h(0.1), "{name} is not sinking behind the lip");
        assert!(h(0.95) < h(0.5) * 0.5, "{name} is barely in by the end");
    }
}

#[test]
fn the_lip_is_the_frontmost_band() {
    let mut out = Vec::new();
    draw_inserting(&cart(), 0.5, &mut out);
    let lip = out.iter().rposition(is_lip).unwrap();
    let cart = out.iter().rposition(is_cart).unwrap();
    let housing = out.iter().rposition(is_housing).unwrap();
    assert!(lip > cart && lip > housing, "the lip is not in front");
}

/// The cart meets the lip and needs a push. Travel per unit time must dip there and then
/// recover, or it reads as a card sliding down a slot rather than seating in one.
#[test]
fn the_cart_catches_on_the_lip_before_going_in() {
    for (name, c) in both() {
        let step = |a: f32, b: f32| cart_y(&c, b) - cart_y(&c, a);
        let approach = step(0.15, 0.30);
        let catching = step(0.45, 0.60);
        let through = step(0.70, 0.85);
        assert!(
            catching < approach * 0.5,
            "{name}: no hesitation at the lip"
        );
        assert!(through > catching * 1.5, "{name}: it never pushes through");
    }
}

/// The handoff out of the shelf. The chrome takes over drawing the cart on the frame the button
/// is pressed, so whatever the shelf was standing there has to be exactly what the chrome starts
/// with — same box, same place. A pak driven off the GBA cart's height jumped from 253 px tall
/// to 135 on that frame, which is the smoosh.
#[test]
fn an_unseated_cart_stands_where_the_shelf_left_it() {
    for (name, c) in both() {
        let mut shelf = Vec::new();
        Shelf::new(vec![c.clone()]).draw_row(None, 0.0, 0.0, 1.0, &mut shelf);
        let on_shelf = quad(&shelf[cart_at(&shelf)]);

        let out = chrome(&c, 0.0);
        let in_slot = quad(&out[cart_at(&out)]);
        assert_eq!(
            on_shelf, in_slot,
            "{name} jumps from {on_shelf:?} to {in_slot:?} on insert"
        );
    }
}

/// The cartridge is an object, not a picture that can be stretched to fit the travel. Its box
/// is the one `cart_box` gives for its platform, on every frame of the way in — which is the
/// other half of the smoosh: the pak's 253 px face was being drawn into a 135 px quad.
#[test]
fn a_cart_keeps_its_own_size_the_whole_way_in() {
    for (name, c) in both() {
        let (w, h) = cart_box(c.platform);
        for step in 0..=20 {
            let out = chrome(&c, step as f32 / 20.0);
            let q = quad(&out[cart_at(&out)]);
            assert!(
                (q.w - w as f32).abs() < 0.01 && (q.h - h as f32).abs() < 0.01,
                "{name} is drawn {}x{} at seat {} rather than at its own {w}x{h}",
                q.w,
                q.h,
                step as f32 / 20.0
            );
        }
    }
}

/// How much cartridge is left out of the machine once it has landed. It is a property of the
/// slot — the recess is a fixed depth — so it cannot come out different for a taller cartridge:
/// a pak that goes further in than a GBA cart is the "inserted deep" half of the complaint.
#[test]
fn every_cartridge_seats_to_the_same_depth() {
    let tops: Vec<(&str, f32)> = both()
        .iter()
        .map(|(name, c)| {
            let out = chrome(c, 1.0);
            (*name, quad(&out[cart_at(&out)]).y)
        })
        .collect();
    let (first, rest) = tops.split_first().expect("two cartridges");
    for (name, top) in rest {
        assert!(
            (top - first.1).abs() < 0.01,
            "{name} seats with its top edge at {top} against {} for {}: one of them is in \
             deeper than the other",
            first.1,
            first.0
        );
    }
}

/// Both cartridges stand with their foot on the row floor, so both have the same distance to
/// fall before they meet the lip — and they must take the same time over it. The catch is the
/// moment the cart lands on the slot, and it cannot happen sooner for one cartridge than for
/// another that was dropped from the same line.
#[test]
fn every_cartridge_reaches_the_lip_at_the_same_moment() {
    let lip = OUT_H as f32 - MOUTH_H;
    let feet: Vec<(&str, Vec<f32>)> = both()
        .iter()
        .map(|(name, c)| {
            let (_, h) = cart_box(c.platform);
            let foot = |t: f32| cart_y(c, t) + h as f32;
            (*name, (0..=20).map(|s| foot(s as f32 / 20.0)).collect())
        })
        .collect();
    let (first, rest) = feet.split_first().expect("two cartridges");
    for (name, drop) in rest {
        for (step, (a, b)) in first.1.iter().zip(drop).enumerate() {
            // Only the fall and the catch: past the lip the two are pushed different
            // distances into the machine, deliberately, and stop being comparable.
            if *a > lip {
                break;
            }
            assert!(
                (a - b).abs() < 0.5,
                "at seat {}, {} has its foot at {a} and {name} at {b}: they are not falling \
                 together",
                step as f32 / 20.0,
                first.0
            );
        }
    }
}

/// In means *in*. The cart comes to rest filling the opening, so the base of the slot ends up
/// covered by the cart rather than going dark again. It used to travel until it had gone
/// entirely, which reads as a cart falling past a window rather than seating in a slot.
#[test]
fn a_seated_cart_stops_in_the_opening_and_covers_its_base() {
    for (name, c) in both() {
        let out = chrome(&c, 1.0);
        let cart = quad(&out[cart_at(&out)]);
        let recess = out
            .iter()
            .find_map(|d| tinted(d, recess()).then(|| quad(d)))
            .expect("no recess drawn");
        assert!(
            cart.y > recess.y && cart.y < recess.y + recess.h,
            "{name} stops at {} against a recess at {}..{}: it is not in the slot",
            cart.y,
            recess.y,
            recess.y + recess.h
        );
        assert!(
            cart.y + cart.h > recess.y + recess.h,
            "{name} does not reach the bottom of the recess"
        );
        assert!(
            cart.y >= OUT_H as f32 - MOUTH_H,
            "{name} is left standing above the case"
        );
    }
}

/// A seated cart shows its moulded grip and the top edge of its label through the thumb
/// scoop, and no more. The recess depth against the cart's own label inset is what sets it:
/// too shallow and the slot looks empty, too deep and the title reads out of the machine.
#[test]
fn a_seated_cart_shows_a_sliver_of_label_and_nothing_readable() {
    for (name, c) in both() {
        let out = chrome(&c, 1.0);
        let cart = quad(&out[cart_at(&out)]);
        let deepest = out
            .iter()
            .filter(|d| is_mouth(d) || tinted(d, recess()))
            .map(|d| {
                let q = quad(d);
                q.y + q.h
            })
            .fold(0.0, f32::max);
        let (label_y, label_h) = label_band(&c);
        let peek = deepest - (cart.y + label_y);
        assert!(
            peek > 0.0,
            "{name} shows no label at all: the slot reads as empty"
        );
        assert!(
            peek < label_h / 4.0,
            "{peek}px of {name}'s {label_h}px label is out of the machine"
        );
    }
}

/// Superseded by `the_slot_is_drawn_in_front_of_and_behind_the_cart`. This used to require
/// the whole opening to paint over the cart, which kept the cart from ever being the
/// frontmost thing but also stopped it entering the slot at all. The worry it encoded, that
/// a cart must not slide over the case, is now the "something is drawn after the cart" half
/// of that test.
#[test]
fn the_cart_is_never_the_frontmost_thing() {
    let out = chrome(&cart(), 0.6);
    let cart = cart_at(&out);
    assert!(
        cart + 1 < out.len(),
        "the cart is the last thing drawn and would sit on top of the case"
    );
}

#[test]
fn ejecting_reverses_the_travel() {
    for (name, c) in both() {
        let mut a = Vec::new();
        draw_ejecting(&c, 0.1, &mut a);
        let mut b = Vec::new();
        draw_ejecting(&c, 0.9, &mut b);
        assert!(
            cart_visible_height(&b) > cart_visible_height(&a),
            "{name} is not coming out"
        );
    }
}

/// The picture comes up out of the middle of the screen, so for most of the power on it does
/// not reach the slot. The case is fading over that same stretch, which is why there must be
/// no cart behind it: this is the frame the user saw a green wash in the opening.
#[test]
fn the_seated_cart_leaves_with_the_case_not_through_it() {
    for t in [0.2, 0.4, 0.6, 0.8] {
        let mut out = Vec::new();
        draw_powering_on(t, &mut out);
        let cart = alpha(&out[cart_at(&out)]);
        let case = out
            .iter()
            .find_map(|d| is_housing(d).then(|| alpha(d)))
            .expect("no case drawn");
        assert!(
            (cart - case).abs() < 0.001,
            "at {t} the cart is at {cart} and the case at {case}: it is not leaving with it"
        );
        assert!(out.iter().any(is_game_layer), "no picture at {t}");
    }
}

/// A dark panel shows nothing. A game layer listed at zero power is a black rectangle over
/// the cart for the whole of its travel.
#[test]
fn a_dark_screen_lists_no_game_layer() {
    let mut out = Vec::new();
    draw_inserting(&cart(), 0.5, &mut out);
    assert!(
        !out.iter().any(is_game_layer),
        "the picture is drawn with the screen off"
    );
}

#[test]
fn the_chrome_does_not_scale_with_the_screen() {
    let mut out = Vec::new();
    draw_powering_on(0.3, &mut out);
    let lip = out.iter().find_map(as_lip).expect("no lip");
    let mut settled = Vec::new();
    draw_powering_on(1.0, &mut settled);
    let lip2 = settled.iter().find_map(as_lip).unwrap();
    assert_eq!(
        (lip.y, lip.h),
        (lip2.y, lip2.h),
        "the lip moved with the picture"
    );
}

/// There is nothing behind the housing until the screen comes up, so it has to be solid
/// until then and gone once the picture fills the frame. A band left painted over a live
/// game is a black bar across the bottom of it.
#[test]
fn the_slot_is_solid_until_the_picture_is_behind_it() {
    let dark = bands(0.0);
    // Count deliberately not pinned: the slot is a band with a hole cut in it, so the number
    // of pieces is an implementation detail. What matters is that none of it is see through.
    assert!(dark.len() >= 3, "the slot lost its bands");
    assert!(
        dark.iter().all(opaque),
        "the backdrop shows through the slot"
    );
    assert!(
        bands(1.0).iter().all(|d| alpha(d) == 0.0),
        "the slot is still painted over the picture"
    );
}

/// The refusal symbol is drawn on the cart, so it has to fit on one. Bigger and it hangs off
/// a 240x135 face into the housing, which reads as a badge on the slot rather than on the
/// cart the slot would not take. Only the compositor can mint a `TexId`, so the quad itself
/// never reaches a unit test and the size is what can be held here.
#[test]
fn the_alert_fits_on_the_cart_face() {
    let (w, h) = icon_box(ALERT_PX);
    assert!(
        w < CART_W && h < CART_H,
        "a {w}x{h} alert on a {CART_W}x{CART_H} cart"
    );
    assert!(
        h * 4 > CART_H,
        "a {h} px alert on a {CART_H} px cart is a speck"
    );
}

/// Measured against the drop through the lip rather than against the catch, which is by
/// design the slowest stretch of the travel.
#[test]
fn the_travel_eases_out_into_the_seat() {
    for (name, c) in both() {
        let last = (cart_y(&c, 1.0) - cart_y(&c, 0.9)).abs();
        let through = (cart_y(&c, 0.85) - cart_y(&c, 0.70)).abs() / 1.5;
        assert!(
            last < through * 0.5,
            "{name} covers {last} px in the last tenth against {through} while dropping \
             through: it arrives at speed and stops dead"
        );
    }
}

/// The slot is one object whether or not something is going into it. The recess is a hole in
/// the front pieces, so a shelf that drew only the front had a hole onto the backdrop where
/// the inside of the machine should be, and the recess appeared out of nowhere the moment an
/// insert started.
#[test]
fn the_empty_slot_is_the_same_slot_the_chrome_draws() {
    let mut empty = Vec::new();
    draw_empty_slot(&mut empty);

    let c = cart();
    let mut seated = Vec::new();
    SlotChrome {
        cart: &c,
        face: None,
        rest: CENTRED,
        seat: 1.0,
        alert: None,
        dim: 0.0,
        screen: 0.0,
        game: false,
    }
    .draw(&mut seated);

    // The chrome carries a cart as well, so compare the slot's own pieces.
    let slot_of = |list: &[Draw]| -> Vec<(Quad, f32)> {
        list.iter()
            .filter(|d| is_housing(d) || is_lip(d) || is_mouth(d) || tinted(d, recess()))
            .map(|d| (quad(d), alpha(d)))
            .collect()
    };
    assert_eq!(
        slot_of(&empty),
        slot_of(&seated),
        "the slot is not the same object on the two screens"
    );
    assert!(
        empty.iter().any(|d| tinted(d, recess())),
        "the empty slot has no recess, so it is a hole onto the backdrop"
    );
}

/// The rule the slot is built on. Dark is a hole and always goes behind the cart. Plastic
/// mostly goes in front, but not the slot's top edge, so the test for it is geometric: from
/// wherever the cart starts being covered it stays covered to the bottom of the screen.
/// Anything else is a bar drawn across the cart, and every wrong version of this slot failed
/// exactly that way: first the opening, then the thumb scoop, then the top edge itself.
#[test]
fn nothing_is_ever_ruled_across_the_cart() {
    // Across the whole travel, not one frame of it: the pieces are the same every frame but
    // only the screen says so, and one frame proves nothing about the rest. Both cartridges,
    // because a taller one is over the slot's pieces for a different stretch of the travel.
    for (_, c) in both() {
        for step in 0..=20 {
            let seat = step as f32 / 20.0;
            let out = chrome(&c, seat);
            let cart = out.iter().position(is_cart).expect("no cart drawn");
            for (i, d) in out.iter().enumerate() {
                if is_mouth(d) || tinted(d, recess()) {
                    assert!(
                        i < cart,
                        "at {seat} a hole at {i} is over the cart at {cart}"
                    );
                }
            }
            assert!(
                out[..cart].iter().filter(|d| is_mouth(d)).count() > 4,
                "the scoop is not part of the hole"
            );

            // Plastic is not held to the same rule: the slot's own top bar is plastic and draws
            // behind on purpose. What has to hold instead is that whatever covers the cart
            // covers it from somewhere down, in one piece, to the bottom of the screen. A band
            // with clear space under it is a bar ruled across the cart, and two pixels of the
            // slot's top edge was exactly that, drawing a line across the label the whole way
            // down the travel.
            let c = quad(&out[cart]);
            let mut x = c.x + 1.0;
            while x < c.x + c.w {
                let mut cover: Vec<(f32, f32)> = out[cart + 1..]
                    .iter()
                    .filter(|d| opaque(d))
                    .map(quad)
                    .filter(|q| q.x <= x && q.x + q.w > x)
                    .map(|q| (q.y, q.y + q.h))
                    .collect();
                cover.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                let mut run: Option<(f32, f32)> = None;
                for (top, bottom) in cover {
                    run = Some(match run {
                        None => (top, bottom),
                        Some((t, b)) => {
                            assert!(
                                top <= b + 0.01,
                                "at {seat}, x {x}: cover breaks at {b} and resumes at {top}, so \
                             something is ruled across the cart"
                            );
                            (t, b.max(bottom))
                        }
                    });
                }
                if let Some((_, bottom)) = run {
                    assert!(
                        bottom >= OUT_H as f32 - 0.01,
                        "at {seat}, x {x}: cover stops at {bottom}, short of the bottom"
                    );
                }
                x += 4.0;
            }

            // And the hole is one hole. A piece of plastic lying across the middle of it cuts a
            // band out of the cart, which reads as the label being sliced in half.
            let recess = out[..cart]
                .iter()
                .find_map(|d| tinted(d, recess()).then(|| quad(d)))
                .expect("no recess drawn");
            for d in &out[cart + 1..] {
                let q = quad(d);
                let inside = q.y > recess.y + 0.01 && q.y + q.h < recess.y + recess.h - 0.01;
                let over_cart = q.x < c.x + c.w && q.x + q.w > c.x;
                // Wide enough to be a band rather than the arc's own edge, which is cut a
                // column at a time and legitimately lives inside the recess.
                let band = q.w > c.w / 2.0;
                assert!(
                    !(opaque(d) && inside && over_cart && band),
                    "at {seat}, {q:?} cuts a band out of the cart inside the recess"
                );
            }
        }
    }
}

/// The GBA SP's thumb scoop: one broad arc across the *middle* of the near wall, deepest at
/// the centre, which is how you get hold of a cart to pull it out. It used to be two small
/// notches at the ends, which is a shape the SP does not have.
#[test]
fn the_slot_has_a_thumb_scoop_across_its_middle() {
    let out = chrome(&cart(), 0.5);
    let dark: Vec<_> = out.iter().filter(|d| is_mouth(d)).map(quad).collect();
    let slit = dark
        .iter()
        .copied()
        .max_by(|a, b| a.w.partial_cmp(&b.w).unwrap())
        .expect("no slot drawn");
    // The arc is cut column by column, so each piece is a vertical span and only the shape
    // they make together is an arc.
    let below: Vec<_> = dark
        .iter()
        .copied()
        .filter(|q| q.y >= slit.y + slit.h)
        .collect();
    assert!(below.len() >= 8, "the scoop is not cut as a curve");

    let span_of = |pick: &dyn Fn(&Quad) -> bool| -> (f32, f32) {
        let l = below
            .iter()
            .filter(|q| pick(q))
            .map(|q| q.x)
            .fold(f32::INFINITY, f32::min);
        let r = below
            .iter()
            .filter(|q| pick(q))
            .map(|q| q.x + q.w)
            .fold(f32::NEG_INFINITY, f32::max);
        (l, r)
    };
    let (l, r) = span_of(&|_| true);
    let centre = OUT_W as f32 / 2.0;
    assert!(
        ((l + r) / 2.0 - centre).abs() < 1.0,
        "the scoop runs {l}..{r} and is not centred"
    );
    assert!(r - l < slit.w, "the scoop is as wide as the opening itself");

    let deepest = below.iter().map(|q| q.y + q.h).fold(0.0, f32::max);
    let (dl, dr) = span_of(&|q| q.y + q.h > deepest - 0.5);
    assert!(
        dr - dl < (r - l) * 0.6,
        "the scoop is a rectangle, not an arc: {}px across at its deepest against {}px at \
         the top",
        dr - dl,
        r - l
    );
    assert!(
        ((dl + dr) / 2.0 - centre).abs() < 1.0,
        "the deepest part of the scoop is off to one side"
    );

    // Smooth: no step along the curve wider than a pixel, which is what the arc looked like
    // when it was cut by depth instead of by column.
    let mut edges: Vec<(f32, f32)> = below.iter().map(|q| (q.x, q.y + q.h)).collect();
    edges.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    for pair in edges.windows(2) {
        let step = (pair[1].1 - pair[0].1).abs();
        assert!(
            step <= 1.01,
            "the curve steps {step}px at x {}: that reads as a jag",
            pair[1].0
        );
    }

    assert!(
        out.iter().any(|d| is_lip(d) && quad(d).y > slit.y),
        "the scoop has no lit rim, so the plastic has no edge"
    );
}

/// The slot sits in a bay stepped down from the shell, so the opening is a recess in a
/// surface rather than a stripe painted on a flat one.
#[test]
fn the_slot_sits_in_a_recessed_bay() {
    let out = chrome(&cart(), 0.5);
    let bay = out
        .iter()
        .find_map(|d| tinted(d, recess()).then(|| quad(d)))
        .expect("no bay drawn");
    let slit = out
        .iter()
        .filter(|d| is_mouth(d))
        .map(quad)
        .max_by(|a, b| a.w.partial_cmp(&b.w).unwrap())
        .expect("no slot drawn");
    assert!(bay.w > slit.w, "the bay is no wider than the opening in it");
    assert!(
        bay.x < slit.x && bay.x + bay.w > slit.x + slit.w,
        "the opening is not inside the bay"
    );
    let step = |a: [f32; 4], b: [f32; 4]| (0..3).map(|i| a[i] - b[i]).sum::<f32>();
    assert!(
        step(housing(), recess()) > 0.0 && step(recess(), opening()) > 0.0,
        "the bay does not read as a step between the shell and the opening"
    );
}

/// It catches where a real cart would: its bottom edge meeting the top edge of the slot.
#[test]
fn the_cart_catches_on_the_top_edge_of_the_slot() {
    let lip = OUT_H as f32 - MOUTH_H;
    for (name, c) in both() {
        let bottom_at = |t: f32| {
            let out = chrome(&c, t);
            match out[cart_at(&out)] {
                Draw::Rect { y, h, .. } | Draw::Tex { y, h, .. } => y + h,
                ref other => panic!("the cart is not a quad: {other:?}"),
            }
        };
        // Find where travel slows to its minimum and check the cart's bottom is at the lip.
        let mut slowest = (f32::MAX, 0.0f32);
        let mut t = 0.05;
        while t < 0.95 {
            let d = bottom_at(t + 0.05) - bottom_at(t);
            if d < slowest.0 {
                slowest = (d, t);
            }
            t += 0.05;
        }
        let at = bottom_at(slowest.1);
        // Tight, and in pixels rather than in a share of a cart's height: the foot lands on the
        // lip and creeps a few pixels past it, and that is the same few pixels whatever is
        // standing on top of it. A tolerance measured against the cartridge would be twice as
        // forgiving for a pak, which is exactly the cartridge it needs to hold hardest for.
        assert!(
            (at - lip).abs() < 16.0,
            "{name} hesitates at {at} but the slot's top edge is {lip}"
        );
    }
}

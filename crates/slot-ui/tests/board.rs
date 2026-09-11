use slot_store::{Cart, Core};
use slot_ui::{
    board_at, board_face, chip_face, chip_shadow_face, lid_at, on_board, padded, rom_marking,
    rom_marking_face, shelf_cart, shell_for, socket_face, CartFace, Placed, BOARD_H, BOARD_W,
    CHIP_H, CHIP_W, DEFAULT_SHELL, LID_TURN, ROM_H, ROM_W, SHADOW_H, SHADOW_W, SOCKET_H, SOCKET_W,
    TURN_PAD,
};

fn cart(stem: &str, code: &str) -> Cart {
    Cart {
        stem: stem.into(),
        rom: format!("Games/{stem}.gba").into(),
        label: None,
        code: code.into(),
        title: stem.to_uppercase(),
    }
}

fn rgb(face: &CartFace, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * face.w + x) * 4) as usize;
    [face.rgba[i], face.rgba[i + 1], face.rgba[i + 2]]
}

fn near(a: [u8; 3], b: [u8; 3]) -> bool {
    (0..3).all(|k| (a[k] as i32 - b[k] as i32).abs() <= 3)
}

const EMERALD: &str = "Pokemon - Emerald Version (USA, Europe)";

/// The chip is tall and narrow, so the name goes on it a few words at a time, and the dump's
/// bracketed facts go underneath in smaller type — the way a mask ROM carries a part number
/// over a date code.
#[test]
fn the_marking_stacks_the_title_and_puts_the_tags_beneath() {
    let m = rom_marking(EMERALD);
    assert_eq!(m.title, ["POKEMON", "EMERALD", "VERSION"]);
    assert_eq!(m.tags.as_deref(), Some("USA, EUROPE"));
}

/// Three lines is all the chip has. A fourth would run off the bottom, so the rest joins the
/// third and the fitter shrinks it.
#[test]
fn a_long_title_folds_its_tail_into_the_third_line() {
    let m = rom_marking("Advance Wars 2 - Black Hole Rising");
    assert_eq!(m.title, ["ADVANCE", "WARS 2", "BLACK HOLE RISING"]);
    assert_eq!(m.tags, None);
}

/// One tag per bracketed group, as `label_tags` has it: `(USA, Europe)` is one release.
#[test]
fn every_bracketed_group_is_its_own_tag() {
    let m = rom_marking("Pokemon - LeafGreen Version (USA, Europe) (Rev 1)");
    assert_eq!(m.tags.as_deref(), Some("USA, EUROPE · REV 1"));
}

#[test]
fn a_title_without_brackets_has_no_tag_line() {
    let m = rom_marking("Metroid Fusion");
    assert_eq!(m.title, ["METROID", "FUSION"]);
    assert_eq!(m.tags, None);
}

/// Ink in the outermost column is type that ran off the chip and onto the legs.
#[test]
fn the_marking_is_drawn_and_stays_on_the_chip() {
    for stem in [
        EMERALD,
        "Metroid Fusion",
        "Advance Wars 2 - Black Hole Rising",
        "Pokemon - LeafGreen Version (USA, Europe) (Rev 1)",
    ] {
        let face = rom_marking_face(stem);
        assert_eq!((face.w, face.h), (ROM_W, ROM_H));
        let alpha = |x: u32, y: u32| face.rgba[((y * face.w + x) * 4 + 3) as usize];
        assert!(
            (0..face.h).any(|y| (0..face.w).any(|x| alpha(x, y) > 128)),
            "{stem} left no marking"
        );
        for y in 0..face.h {
            assert_eq!(alpha(0, y), 0, "{stem} ran off the left of the chip");
            assert_eq!(
                alpha(face.w - 1, y),
                0,
                "{stem} ran off the right of the chip"
            );
        }
    }
}

/// Rasterised at the size it is shown at, since that is the only size a face is sharp at.
#[test]
fn the_board_is_the_size_it_is_shown_at() {
    let face = board_face(&cart(EMERALD, "BPEE"));
    assert_eq!((face.w, face.h), (BOARD_W, BOARD_H));
    assert_eq!((BOARD_W, BOARD_H), (372, 209));
}

/// The back of the cart is the same plastic as its front on the shelf. The wall at board unit
/// (9, 45) is clear of the clips, the floor and every shadow.
#[test]
fn the_back_shell_is_the_carts_own_plastic() {
    let emerald = board_face(&cart(EMERALD, "BPEE"));
    assert!(
        near(rgb(&emerald, 14, 70), shell_for("BPEE").colour),
        "Emerald's wall is {:?}",
        rgb(&emerald, 14, 70)
    );
    let unknown = board_face(&cart("Homebrew", ""));
    assert!(
        near(rgb(&unknown, 14, 70), DEFAULT_SHELL.colour),
        "an unknown cart's wall is {:?}",
        rgb(&unknown, 14, 70)
    );
}

/// A board that failed to parse comes back transparent and every other test here would pass on
/// nothing. Board unit (200, 70) is bare solder mask.
#[test]
fn the_board_itself_is_drawn() {
    let face = board_face(&cart(EMERALD, "BPEE"));
    assert!(
        near(rgb(&face, 310, 108), [0x3a, 0x9a, 0x3c]),
        "the board is {:?}",
        rgb(&face, 310, 108)
    );
}

fn alpha(face: &CartFace, x: u32, y: u32) -> u8 {
    face.rgba[((y * face.w + x) * 4 + 3) as usize]
}

/// The open cart grows out of the cart on the shelf, so its first frame has to be that cart
/// exactly, and its last the place the mockup put it.
#[test]
fn the_open_cart_starts_as_the_shelf_cart_and_lands_where_the_mockup_has_it() {
    assert_eq!(
        shelf_cart(),
        Placed {
            x: 240.0,
            y: 172.5,
            w: 240.0,
            h: 135.0
        }
    );
    assert_eq!(board_at(0.0), shelf_cart());
    assert_eq!(
        board_at(1.0),
        Placed {
            x: 174.0,
            y: 150.0,
            w: 372.0,
            h: 209.0
        }
    );
    assert_eq!(board_at(1.7), board_at(1.0), "an overshoot kept growing");
}

#[test]
fn the_lid_leaves_level_and_rests_turned() {
    assert_eq!(lid_at(0.0), (shelf_cart(), 0.0));
    let (rest, turn) = lid_at(1.0);
    assert_eq!(
        rest,
        Placed {
            x: 288.0,
            y: 30.0,
            w: 144.0,
            h: 81.0
        }
    );
    assert_eq!(turn, LID_TURN);
    assert!(
        (LID_TURN.to_degrees() + 5.0).abs() < 1e-4,
        "{}",
        LID_TURN.to_degrees()
    );
}

#[test]
fn board_units_land_on_the_panel_at_one_and_a_half_times() {
    let board = board_at(1.0);
    assert_eq!(on_board(board, 0.0, 0.0), (174.0, 150.0));
    assert_eq!(on_board(board, 240.0, 135.0), (546.0, 359.0));
}

/// An empty socket names the core it is for, quietly: the chip's own name is the loud one.
#[test]
fn a_socket_names_its_core_at_half_strength() {
    for core in Core::ALL {
        let face = socket_face(core);
        assert_eq!((face.w, face.h), (SOCKET_W, SOCKET_H));
        let strongest = (12..34)
            .flat_map(|y| (16..48).map(move |x| (x, y)))
            .map(|(x, y)| alpha(&face, x, y))
            .max()
            .unwrap();
        assert!(
            (60..=150).contains(&strongest),
            "{core:?}'s name is at alpha {strongest}"
        );
    }
}

/// The outline's left edge is the socket SVG's closing stroke, sitting right at `x = 0` for
/// the straight run of it: a partly covered pixel there is the outline ink, `#eef5e6`, at some
/// fraction of full alpha. Premultiplied, the compositor's own blend then multiplies that
/// fraction in a second time and darkens it; straight alpha keeps the ink's own brightness at
/// any coverage.
#[test]
fn a_partly_covered_socket_edge_pixel_keeps_the_outline_inks_brightness() {
    let face = socket_face(Core::Mgba);
    let (r, a) = (6..(face.h - 6))
        .find_map(|y| {
            let i = ((y * face.w) * 4) as usize;
            let a = face.rgba[i + 3];
            (a > 0 && a < 255).then(|| (face.rgba[i], a))
        })
        .expect("no partly covered pixel on the socket's left edge");
    assert!(
        r > 200,
        "the outline ink darkened at partial coverage: R={r} at alpha={a}"
    );
}

/// A turned quad's edge is not antialiased, so the chip's outline has to sit inside the
/// texture: the outermost `TURN_PAD` pixels are nothing.
#[test]
fn the_chip_is_padded_clear_on_every_side() {
    for core in [None, Some(Core::Mgba), Some(Core::Gpsp)] {
        let face = chip_face(core);
        assert_eq!(
            (face.w, face.h),
            (CHIP_W + 2 * TURN_PAD, CHIP_H + 2 * TURN_PAD)
        );
        for y in 0..face.h {
            for x in 0..face.w {
                let ring = x < TURN_PAD
                    || y < TURN_PAD
                    || x >= face.w - TURN_PAD
                    || y >= face.h - TURN_PAD;
                if ring {
                    assert_eq!(alpha(&face, x, y), 0, "{core:?} is not clear at {x},{y}");
                }
            }
        }
    }
}

/// Seated, the chip is the loudest word on the board; in flight it says nothing, so the two
/// sockets' names are the only ones on screen.
#[test]
fn a_seated_chip_wears_its_name_and_a_flying_one_is_blank() {
    let light = |face: &CartFace| {
        face.rgba
            .chunks_exact(4)
            .filter(|p| p[0] > 200 && p[1] > 200 && p[2] > 200 && p[3] > 200)
            .count()
    };
    assert!(
        light(&chip_face(Some(Core::Mgba))) > 20,
        "mGBA left no name"
    );
    assert_eq!(light(&chip_face(None)), 0, "the blank chip says something");
    assert_ne!(
        chip_face(Some(Core::Mgba)).rgba,
        chip_face(Some(Core::Gpsp)).rgba,
        "both cores wear the same mark"
    );
}

#[test]
fn padding_moves_nothing() {
    let face = chip_face(Some(Core::Gpsp));
    let more = padded(&face, 3);
    assert_eq!((more.w, more.h), (face.w + 6, face.h + 6));
    for (x, y) in [(0, 0), (10, 7), (face.w - 1, face.h - 1)] {
        let a = ((y * face.w + x) * 4) as usize;
        let b = (((y + 3) * more.w + x + 3) * 4) as usize;
        assert_eq!(face.rgba[a..a + 4], more.rgba[b..b + 4]);
    }
}

#[test]
fn the_shadow_is_darkest_in_the_middle_and_gone_at_the_corners() {
    let s = chip_shadow_face();
    assert_eq!((s.w, s.h), (SHADOW_W, SHADOW_H));
    assert!(alpha(&s, SHADOW_W / 2, SHADOW_H / 2) > alpha(&s, SHADOW_W / 8, SHADOW_H / 2));
    assert_eq!(alpha(&s, 0, 0), 0);
}

/// The seated chip's name sits on the chip's own opaque black body, not on nothing, so its
/// edges have to blend toward the body colour instead of snapping straight to full ink. Diffed
/// against the blank chip, which carries the same body art, so only the name's own pixels are
/// in play — not the legs, the pin-1 dot or the body's rounded corners, which are antialiased
/// on both chips already.
#[test]
fn the_seated_chips_name_is_antialiased_against_the_body() {
    let red = |face: &CartFace, x: u32, y: u32| face.rgba[((y * face.w + x) * 4) as usize];
    for core in [Core::Mgba, Core::Gpsp] {
        let named = chip_face(Some(core));
        let blank = chip_face(None);
        let soft = (0..named.h)
            .flat_map(|y| (0..named.w).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                alpha(&named, x, y) == 255
                    && red(&named, x, y) != red(&blank, x, y)
                    && (0x40..0xc0).contains(&red(&named, x, y))
            })
            .count();
        assert!(
            soft > 5,
            "{core:?}'s name has only {soft} antialiased pixels"
        );
    }
}

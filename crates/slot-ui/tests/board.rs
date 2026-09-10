use slot_store::Cart;
use slot_ui::{
    board_face, rom_marking, rom_marking_face, shell_for, CartFace, BOARD_H, BOARD_W,
    DEFAULT_SHELL, ROM_H, ROM_W,
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

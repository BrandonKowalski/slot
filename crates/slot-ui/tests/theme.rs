//! Its own binary on purpose: `set_theme` is process wide and once only, so a test that sets
//! one cannot share a process with the tests that check the default palette.

use slot_store::Theme;
use slot_ui::{draw_empty_slot, edge, housing, opening, recess, set_theme, Draw};

#[test]
fn the_card_s_palette_is_what_reaches_the_screen() {
    let t = Theme {
        housing: [0x11, 0x22, 0x33],
        recess: [0x44, 0x55, 0x66],
        opening: [0x77, 0x88, 0x99],
        edge: [0xaa, 0xbb, 0xcc],
    };
    set_theme(t);
    let hex = |c: [f32; 4]| {
        [
            (c[0] * 255.0).round() as u8,
            (c[1] * 255.0).round() as u8,
            (c[2] * 255.0).round() as u8,
        ]
    };
    assert_eq!(hex(housing()), t.housing);
    assert_eq!(hex(recess()), t.recess);
    assert_eq!(hex(opening()), t.opening);
    assert_eq!(hex(edge()), t.edge);

    // And it is the slot that is painted with it, not just the accessors.
    let mut out = Vec::new();
    draw_empty_slot(&mut out);
    let has = |c: [f32; 4]| {
        out.iter().any(|d| match *d {
            Draw::Rect { colour, .. } => (0..3).all(|i| (colour[i] - c[i]).abs() < 0.001),
            _ => false,
        })
    };
    for (c, name) in [
        (housing(), "housing"),
        (recess(), "recess"),
        (opening(), "opening"),
        (edge(), "edge"),
    ] {
        assert!(has(c), "the slot is not drawn in the theme's {name}");
    }
}

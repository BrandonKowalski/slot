use slot_ui::{
    arrows_hint_face, arrows_hint_width, hint_face, hint_width, title_face, UndoFace, ARROW_GAP,
    CAP, CAP_GAP, HINT_GAP, HINT_H, TITLE_H, TITLE_W,
};

fn opaque(face: &UndoFace, x: u32, y: u32) -> bool {
    face.rgba[((y * face.w + x) * 4 + 3) as usize] > 0
}

fn ink(face: &UndoFace, x: u32, y: u32) -> bool {
    let i = ((y * face.w + x) * 4) as usize;
    face.rgba[i] > 128 && face.rgba[i + 3] > 128
}

/// Both plates are translucent over a screenshot, so a hint has to be a cap and some type on
/// nothing else. A filled background would be a solid block sitting on the picture.
#[test]
fn the_hint_is_a_key_cap_and_type_on_nothing_else() {
    let face = hint_face("B", "Back");
    assert_eq!((face.w, face.h), (hint_width("B", "Back"), HINT_H));
    assert!(opaque(&face, 1, HINT_H / 2), "the key cap is not filled");
    assert!(
        !opaque(&face, face.w - 1, 0),
        "the hint has a background behind its type"
    );
}

/// The face is small and the label is not, so the fitter is the only thing keeping the type
/// off the edge. Ink in the last column is a label that has overrun.
#[test]
fn the_label_is_drawn_and_stays_inside_the_hint() {
    for label in ["Back", "Undo save", "Undo an interminable action"] {
        let face = hint_face("X", label);
        assert!(
            (0..face.h).any(|y| (0..face.w).any(|x| ink(&face, x, y))),
            "{label} was never drawn"
        );
        for y in 0..face.h {
            assert!(!ink(&face, face.w - 1, y), "{label} overran its face");
        }
    }
}

/// The title is the one sentence on screen. A relative time that ran off the plate would be
/// the switcher naming a state the user cannot read.
#[test]
fn the_title_is_drawn_and_stays_inside_its_face() {
    for text in [
        "just now",
        "4 min ago",
        "2026-08-08 20:00",
        "2026-08-09_14-32-05",
    ] {
        let face = title_face(text);
        assert_eq!((face.w, face.h), (TITLE_W, TITLE_H));
        assert!(
            (0..face.h).any(|y| (0..face.w).any(|x| ink(&face, x, y))),
            "{text} was never drawn"
        );
        for y in 0..face.h {
            assert!(!ink(&face, face.w - 1, y), "{text} overran its face");
        }
    }
}

/// A legend is pairs, not a run of tokens. The space inside a pair has to stay clearly
/// smaller than the space between pairs, whatever the two are tuned to. Both screens build
/// hints from these same constants, so holding the ratio here holds it everywhere.
#[test]
fn grouping_reads_as_pairs() {
    // Both are constants, which clippy rightly notices. The point of the test is to fail
    // the build if someone retunes one without the other, so the comparison is forced to
    // happen at runtime.
    let inside = std::hint::black_box(CAP_GAP) as f32;
    let between = std::hint::black_box(HINT_GAP);
    assert!(
        between >= inside * 2.0,
        "gap inside a pair is {CAP_GAP}px and between pairs {HINT_GAP}px: they will not group"
    );
    assert!(inside >= 4.0, "the cap is crowding its word at {inside}px");
}

fn cap_pixels(face: &UndoFace, cap_x: u32) -> Vec<u8> {
    let top = (HINT_H - CAP) / 2;
    (top..top + CAP)
        .flat_map(|y| (cap_x..cap_x + CAP).map(move |x| (x, y)))
        .flat_map(|(x, y)| {
            let i = ((y * face.w + x) * 4) as usize;
            face.rgba[i..i + 4].to_vec()
        })
        .collect()
}

/// Each caret cap has to carry a dark glyph. A codepoint the symbols font lacks rasterises to
/// nothing, and a blank cap reads as a layout bug rather than as a missing arrow.
#[test]
fn both_arrow_caps_carry_a_glyph() {
    let face = arrows_hint_face("Swap");
    assert_eq!((face.w, face.h), (arrows_hint_width("Swap"), HINT_H));
    for cap_x in [0, CAP + ARROW_GAP] {
        let dark = cap_pixels(&face, cap_x)
            .chunks_exact(4)
            .filter(|p| p[0] < 100 && p[3] > 200)
            .count();
        assert!(
            dark > 6,
            "the cap at x={cap_x} has no glyph ({dark} dark pixels)"
        );
    }
}

/// Left and right are two keys pointing two ways, not one mark drawn twice.
#[test]
fn the_two_arrows_point_different_ways() {
    let face = arrows_hint_face("Swap");
    assert_ne!(cap_pixels(&face, 0), cap_pixels(&face, CAP + ARROW_GAP));
}

/// The word the two keys share is set like any other hint's, and stays inside its face.
#[test]
fn the_arrows_label_is_drawn_and_stays_inside() {
    let face = arrows_hint_face("Swap");
    let from = 2 * CAP + ARROW_GAP;
    assert!(
        (0..face.h).any(|y| (from..face.w).any(|x| ink(&face, x, y))),
        "Swap was never drawn"
    );
    for y in 0..face.h {
        assert!(!ink(&face, face.w - 1, y), "Swap overran its face");
    }
}

use slot_ui::{
    hint_face, hint_width, title_face, UndoFace, CAP_GAP, HINT_GAP, HINT_H, TITLE_H, TITLE_W,
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

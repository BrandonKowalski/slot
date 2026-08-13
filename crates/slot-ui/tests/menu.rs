use slot_ui::{Draw, Menu, MenuItem};

/// Clamped, not wrapped, like the clock picker's fields: a short list is a line to walk along,
/// and rolling off the end of two entries reads as a bug rather than a convenience.
#[test]
fn the_cursor_walks_the_list_and_stops_at_both_ends() {
    let mut m = Menu::new();
    assert_eq!(m.selected(), MenuItem::RelinkAdb);
    m.up();
    assert_eq!(m.selected(), MenuItem::RelinkAdb, "walked off the top");
    m.down();
    assert_eq!(m.selected(), MenuItem::About);
    m.down();
    assert_eq!(m.selected(), MenuItem::About, "walked off the bottom");
}

/// The binary rasterises these and hands them back in this order, so a row is labelled by its
/// position. Out of step and every row wears its neighbour's name.
#[test]
fn the_labels_are_in_declaration_order() {
    assert_eq!(MenuItem::ALL.len(), 2);
    assert_eq!(MenuItem::ALL[0].label(), "relink adb");
    assert_eq!(MenuItem::ALL[1].label(), "about");
    for (i, item) in MenuItem::ALL.iter().enumerate() {
        assert_eq!(item.index(), i, "{item:?} is out of order in ALL");
    }
}

/// The panel holds its shape before the faces arrive, for the same reason a cart with no face
/// still holds its place on the shelf: a menu that pops into existence a frame late reads as
/// a glitch.
#[test]
fn a_menu_with_no_faces_draws_its_panel_and_no_rows() {
    let m = Menu::new();
    let mut out = Vec::new();
    m.draw(&mut out);
    let texs = out.iter().filter(|d| matches!(d, Draw::Tex { .. })).count();
    assert_eq!(texs, 0, "a row was drawn with no face to draw");
    assert!(!out.is_empty(), "the panel itself was not drawn");
}

/// Once the faces are there, every row draws one.
#[test]
fn every_row_draws_its_face() {
    let mut m = Menu::new();
    m.set_faces(vec![
        slot_gfx::TexId::from_raw(1),
        slot_gfx::TexId::from_raw(2),
    ]);
    let mut out = Vec::new();
    m.draw(&mut out);
    let texs = out.iter().filter(|d| matches!(d, Draw::Tex { .. })).count();
    assert_eq!(texs, MenuItem::ALL.len(), "one face per row");
}

/// The mark follows the cursor. Counting draws cannot show this — the mark is one rect either
/// way — so the two lists are compared instead: a menu that draws the same picture whichever
/// row is selected has no cursor at all.
#[test]
fn the_mark_moves_with_the_cursor() {
    let mut m = Menu::new();
    let mut top = Vec::new();
    m.draw(&mut top);
    m.down();
    let mut bottom = Vec::new();
    m.draw(&mut bottom);
    assert_eq!(
        top.len(),
        bottom.len(),
        "the mark should move, not multiply"
    );
    assert_ne!(
        format!("{top:?}"),
        format!("{bottom:?}"),
        "the same picture for both rows: nothing says which is selected"
    );
}

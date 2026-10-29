use slot_store::GuideSpot;
use slot_ui::{
    cell, fitting_zoom, page_box, Canvas, Guide, GuideGlyphs, GUIDE_COLUMNS, GUIDE_ZOOMS,
};

const WIDE: Canvas = Canvas { w: 720.0, h: 480.0 };
const SQUARE: Canvas = Canvas { w: 640.0, h: 480.0 };

fn lines(n: usize, width: usize) -> Vec<String> {
    (0..n)
        .map(|i| format!("{i:<width$}").replace(' ', "x"))
        .collect()
}

#[test]
fn a_new_guide_shows_eighty_columns_at_the_largest_size_that_fits() {
    for canvas in [WIDE, SQUARE] {
        let z = fitting_zoom(canvas) as usize;
        let g = Guide::new(lines(10, 80), None, canvas);
        assert!(g.cols(canvas) >= GUIDE_COLUMNS, "{canvas:?}");
        if let Some(&bigger) = GUIDE_ZOOMS.get(z + 1) {
            let (_, _, w, _) = page_box(canvas);
            assert!(cell(bigger).0 * GUIDE_COLUMNS as f32 > w as f32);
        }
    }
    assert!(fitting_zoom(WIDE) > fitting_zoom(SQUARE));
}

#[test]
fn scrolling_stops_at_the_last_full_page() {
    let mut g = Guide::new(lines(100, 10), None, WIDE);
    let rows = g.rows(WIDE);
    g.scroll(-5, WIDE);
    assert_eq!(g.spot().line, 0);
    g.scroll(1000, WIDE);
    assert_eq!(g.spot().line as usize, 100 - rows);
    g.page(-1, WIDE);
    assert_eq!(g.spot().line as usize, 100 - rows - (rows - 1));
}

#[test]
fn a_short_guide_does_not_scroll_or_pan() {
    let mut g = Guide::new(lines(3, 10), None, WIDE);
    g.scroll(1, WIDE);
    g.pan(1, WIDE);
    assert_eq!((g.spot().line, g.spot().col), (0, 0));
}

#[test]
fn panning_reaches_the_end_of_the_widest_line() {
    let mut g = Guide::new(lines(3, 200), None, SQUARE);
    let cols = g.cols(SQUARE);
    g.pan(100, SQUARE);
    assert_eq!(g.spot().col as usize, 200 - cols);
}

#[test]
fn zoom_stops_at_both_ends_and_keeps_the_view_in_range() {
    let mut g = Guide::new(lines(100, 90), None, WIDE);
    while g.zoom_by(-1, WIDE) {}
    assert_eq!(g.spot().zoom, 0);
    g.scroll(1000, WIDE);
    let top = g.spot().line;
    while g.zoom_by(1, WIDE) {}
    assert_eq!(g.spot().zoom as usize, GUIDE_ZOOMS.len() - 1);
    assert_eq!(g.spot().line, top, "zooming in moved the top line");
    g.scroll(1000, WIDE);
    while g.zoom_by(-1, WIDE) {}
    assert_eq!(g.spot().line as usize, 100 - g.rows(WIDE));
}

#[test]
fn a_saved_spot_out_of_range_is_pulled_back() {
    let spot = GuideSpot {
        line: 9999,
        col: 9999,
        zoom: 200,
    };
    let g = Guide::new(lines(50, 100), Some(spot), WIDE);
    assert_eq!(g.spot().zoom as usize, GUIDE_ZOOMS.len() - 1);
    assert_eq!(g.spot().line as usize, 50 - g.rows(WIDE));
    assert_eq!(g.spot().col as usize, 100 - g.cols(WIDE));
}

fn inked_rows(face: &slot_ui::UndoFace) -> Vec<bool> {
    (0..face.h as usize)
        .map(|y| {
            let row = &face.rgba[y * face.w as usize * 4..(y + 1) * face.w as usize * 4];
            row.chunks(4).any(|p| p[0] > 128)
        })
        .collect()
}

#[test]
fn the_page_inks_the_text_and_nothing_below_it() {
    let canvas = WIDE;
    let mut text = vec!["#".repeat(80); 2];
    text.push(String::new());
    text.push("#".repeat(80));
    let g = Guide::new(text, None, canvas);
    let face = g.render(canvas, &mut GuideGlyphs::default());
    let (_, _, w, h) = page_box(canvas);
    assert_eq!((face.w, face.h), (w, h));
    assert!(face.rgba.chunks(4).all(|p| p[3] == 255));
    let line_h = cell(g.px()).1 as usize;
    let rows = inked_rows(&face);
    let band = |r: usize| rows[r * line_h..(r + 1) * line_h].iter().any(|&b| b);
    assert!(band(0) && band(1) && band(3));
    assert!(!band(2), "the blank line has ink");
    assert!(!rows[4 * line_h..].iter().any(|&b| b), "ink below the text");
}

#[test]
fn the_page_follows_the_scroll() {
    let canvas = WIDE;
    let mut text = vec![String::new(); 60];
    text[1] = "#".repeat(10);
    let mut g = Guide::new(text, None, canvas);
    let mut glyphs = GuideGlyphs::default();
    let line_h = cell(g.px()).1 as usize;
    let before = inked_rows(&g.render(canvas, &mut glyphs));
    assert!(before[line_h..2 * line_h].iter().any(|&b| b));
    g.scroll(1, canvas);
    let after = inked_rows(&g.render(canvas, &mut glyphs));
    assert!(after[..line_h].iter().any(|&b| b));
    assert!(!after[line_h..].iter().any(|&b| b));
}

#[test]
fn the_legend_is_back_and_help_only() {
    let g = Guide::new(lines(3, 10), None, WIDE);
    let keys: Vec<(&str, String)> = g.hints().into_iter().map(|h| (h.key, h.label)).collect();
    assert_eq!(
        keys,
        [("B", "Back".to_string()), ("MENU", "Help".to_string())]
    );
}

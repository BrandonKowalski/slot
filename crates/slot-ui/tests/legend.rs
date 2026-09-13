use slot_ui::{centred_hints, TexId, HINT_EDGE, LEGEND_GAP, OUT_W};

/// A row of hints is centred on the panel as one legend, measured by what shows of each: the
/// transparent strip every hint face carries after its label is not counted, the gap between
/// them is, and every hint lands on a whole pixel.
#[test]
fn a_row_of_hints_is_centred_by_what_shows_of_each() {
    let (a, b) = (TexId::from_raw(1), TexId::from_raw(2));
    let (aw, bw) = (71 + HINT_EDGE, 110 + HINT_EDGE);
    let placed = centred_hints(&[(a, aw), (b, bw)], LEGEND_GAP);
    let x = ((OUT_W as f32 - (71.0 + LEGEND_GAP + 110.0)) / 2.0).round();
    assert_eq!(
        placed,
        vec![(a, aw, x), (b, bw, (x + 71.0 + LEGEND_GAP).round())]
    );
}

#[test]
fn a_lone_hint_is_centred_by_what_shows_of_it() {
    let a = TexId::from_raw(1);
    let aw = 150 + HINT_EDGE;
    assert_eq!(
        centred_hints(&[(a, aw)], LEGEND_GAP),
        vec![(a, aw, ((OUT_W as f32 - 150.0) / 2.0).round())]
    );
}

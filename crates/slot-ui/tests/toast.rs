use slot_ui::{toast_face, toast_rect, Draw, Hud, HudKind, Toast, OUT_W, PLATE_H};

#[test]
fn saving_and_loading_say_which_one_happened() {
    assert_eq!(Toast::StateSaved.text(), "State Saved");
    assert_eq!(Toast::StateLoaded.text(), "State Loaded");
}

#[test]
fn a_toast_fades_on_the_same_curve_as_the_bar() {
    let mut h = Hud::new();
    h.toast(Toast::StateSaved, 1_000);
    assert!(h.toast_visible(2_499));
    assert!(!h.toast_visible(2_500));
}

#[test]
fn a_toast_is_centred() {
    let (x, _, w, _) = toast_rect();
    assert_eq!(x + w / 2.0, OUT_W as f32 / 2.0);
}

/// Nothing backs the type, so the type carries its own contrast or it disappears on a white
/// game frame. Same halo the badge beside it uses.
#[test]
fn a_toast_carries_its_own_halo() {
    let f = toast_face(Toast::StateSaved);
    let dark = f
        .rgba
        .chunks(4)
        .any(|p| p[3] > 0 && p[0] < 0x40 && p[1] < 0x40 && p[2] < 0x40);
    assert!(dark, "there is nothing dark behind the type");
}

/// The toast reads against the same plate the level bar does, in the same place. It used to
/// sit below the band with no backing, which put two different treatments on one screen.
#[test]
fn a_toast_sits_in_the_plate_band_and_is_backed_by_it() {
    let mut h = Hud::new();
    h.toast(Toast::StateSaved, 0);
    let mut out = Vec::new();
    h.draw(0, &mut out);

    let plate = out
        .iter()
        .find(|d| matches!(d, Draw::Rect { w, .. } if *w == OUT_W as f32))
        .expect("the toast has nothing to be read against");
    let Draw::Rect { colour, h: ph, .. } = plate else {
        unreachable!()
    };
    assert!(colour[3] > 0.6, "the plate is too faint to give contrast");
    assert!((*ph - PLATE_H).abs() < 0.01, "the plate is not the band");

    let (_, y, _, th) = toast_rect();
    assert!(
        y >= 0.0 && y + th <= PLATE_H,
        "the toast at {y} is outside the band"
    );
}

/// They share one strip, so only one can have it. A toast names something that just
/// happened; a level is visible in its own effect.
#[test]
fn a_toast_takes_the_band_from_the_bar() {
    let mut h = Hud::new();
    h.show(HudKind::Volume, 50, false, 0);
    let mut bar_only = Vec::new();
    h.draw(0, &mut bar_only);
    let bars = bar_only.len();

    h.toast(Toast::StateSaved, 0);
    let mut both = Vec::new();
    h.draw(0, &mut both);
    assert!(
        both.len() < bars,
        "the bar is still drawn underneath the toast"
    );
}

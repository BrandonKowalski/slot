use slot_ui::{toast_face, toast_rect, Draw, Hud, HudKind, Toast, OUT_W, PLATE_H};

#[test]
fn saving_and_loading_say_which_one_happened() {
    assert_eq!(Toast::StateSaved.text(), "State Saved");
    assert_eq!(Toast::StateLoaded.text(), "State Loaded");
}

/// The link shortcut on a core that cannot link says which one can, in the same banner.
#[test]
fn the_link_shortcut_on_the_wrong_core_says_to_switch() {
    assert_eq!(Toast::NeedsGpsp.text(), "Please switch to gpSP");
    let f = toast_face(Toast::NeedsGpsp);
    assert!(f.rgba.chunks(4).any(|p| p[3] > 0), "the banner is blank");
}

/// gpSP fakes named protocols and has no generic cable, so for any other cart there is no link
/// to make. The shortcut says so in the same banner rather than bringing a radio up for a game
/// that will never see a packet.
#[test]
fn a_cart_gpsp_cannot_link_says_there_is_no_link() {
    assert_eq!(Toast::NoLink.text(), "No link support");
    let f = toast_face(Toast::NoLink);
    assert!(f.rgba.chunks(4).any(|p| p[3] > 0), "the banner is blank");
}

/// Every banner the HUD can raise answers something the user just did, and none of them merely
/// describes what is already on screen. The carousel briefly had three that named the shelf it
/// had moved to; that is the plate corner's job now — see `slot_ui::mark` — and a banner saying it
/// as well would be the same fact told twice. So the list is held at six, by name, because the
/// way a line like that comes back is one variant at a time.
#[test]
fn the_banner_says_what_happened_and_never_what_is_on_screen() {
    assert_eq!(
        Toast::ALL,
        [
            Toast::StateSaved,
            Toast::StateLoaded,
            Toast::NeedsGpsp,
            Toast::NoLink,
            Toast::LinkEnded,
            Toast::PeerEnded,
        ],
        "a banner was added or dropped: every face is uploaded by its place in this list"
    );
    for (i, t) in Toast::ALL.iter().enumerate() {
        assert_eq!(t.index(), i, "{t:?} does not answer to its own place");
        let f = toast_face(*t);
        assert!(
            f.rgba.chunks(4).any(|p| p[3] > 0),
            "{t:?} rastered to a blank banner"
        );
    }
}

/// Every line is rastered into one box at one size, so a line too long for it would be shrunk on
/// its own and read as a different banner from the others. Measured off the pixels rather than
/// off the layout: the type is uppercased and has no descenders, so a banner set at the same
/// size covers the same rows.
///
/// To within a row, which is the typeface rather than the size. Round and pointed capitals
/// overshoot the flat ones — the S and A of STATE SAVED sit a row below the K and D of LINK
/// ENDED at 16 px — so a line built only from flat capitals is a row shorter while being set at
/// exactly the same size. A shrunk line is not a row out; it is four, which is what the bound
/// below actually catches. `toast.rs`'s own unit test holds the size itself, against both the
/// full size and the fallback.
#[test]
fn no_toast_is_shrunk_to_fit_its_box() {
    let rows = |t: Toast| {
        let f = toast_face(t);
        let inked: Vec<usize> = (0..f.h as usize)
            .filter(|y| (0..f.w as usize).any(|x| f.rgba[(y * f.w as usize + x) * 4 + 3] > 0))
            .collect();
        let first = *inked.first().expect("the banner is blank");
        let last = *inked.last().expect("the banner is blank");
        (first, last)
    };
    let (top, bottom) = rows(Toast::StateSaved);
    for t in Toast::ALL {
        let (a, b) = rows(t);
        assert!(
            a.abs_diff(top) <= 1 && b.abs_diff(bottom) <= 1,
            "{t:?} sits on rows {a}..{b} where the others sit on {top}..{bottom}, so it was shrunk to fit"
        );
    }
}

#[test]
fn a_toast_fades_on_the_same_curve_as_the_bar() {
    let mut h = Hud::new();
    h.toast(Toast::StateSaved, 1_000);
    assert!(h.toast_visible(2_499));
    assert!(!h.toast_visible(2_500));
}

/// Saying something twice re-shows the one banner rather than queueing a second behind it: the
/// HUD holds one line and one clock, and a second call re-stamps that clock. It matters wherever
/// a button can be worked faster than a banner fades, which is every button that raises one.
///
/// Held here rather than through the app, which is where it used to be: the three shelf banners
/// were what exercised it, and the shelf says which system it is in the plate's corner now.
#[test]
fn saying_the_same_thing_twice_re_shows_it_rather_than_stacking() {
    let mut h = Hud::new();
    h.toast(Toast::StateSaved, 1_000);
    h.toast(Toast::StateSaved, 2_400);
    // Past the first stamp's own fade and short of the second's, so the banner is up here only
    // because the second call moved the clock forward.
    assert_eq!(h.said(3_400), Some(Toast::StateSaved), "it did not re-show");
    assert_eq!(h.said(3_900), None, "it never faded");
}

/// A different banner replaces the one showing rather than waiting behind it. The same one slot,
/// read the other way round: what is on screen is always the last thing that happened.
#[test]
fn a_second_banner_replaces_the_first() {
    let mut h = Hud::new();
    h.toast(Toast::StateSaved, 1_000);
    h.toast(Toast::LinkEnded, 1_100);
    assert_eq!(h.said(1_200), Some(Toast::LinkEnded));
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

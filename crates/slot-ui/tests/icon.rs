use slot_ui::{icon_box, icon_face, Icon, HUD_ICON_PX};

/// Faces are uploaded in `ALL` order and looked up by `index`, which is the discriminant.
/// Reordering `ALL` alone would silently put the wrong glyph on the HUD.
#[test]
fn icons_are_indexed_in_declaration_order() {
    for (i, icon) in Icon::ALL.iter().enumerate() {
        assert_eq!(icon.index(), i, "{icon:?} is out of order in ALL");
    }
}

#[test]
fn the_shared_box_is_the_one_faces_come_back_in() {
    let f = icon_face(Icon::Rewind, 18.0, [255, 255, 255]);
    assert_eq!(icon_box(18.0), (f.w, f.h));
}

#[test]
fn every_icon_rasterises_to_something_visible() {
    for icon in Icon::ALL {
        let f = icon_face(icon, 18.0, [255, 255, 255]);
        let lit = f.rgba.chunks(4).filter(|p| p[3] > 0).count();
        assert!(
            lit > 20,
            "{icon:?} rasterised to {lit} opaque pixels, glyph is missing"
        );
    }
}

/// A missing glyph renders as tofu or nothing, and both look like a layout bug rather than
/// a font problem. This is the test that catches picking a codepoint the font lacks: every
/// pair, because two glyphs the font is missing come back as the same box.
#[test]
fn icons_are_distinguishable_from_each_other() {
    let faces: Vec<_> = Icon::ALL
        .iter()
        .map(|i| (i, icon_face(*i, 18.0, [255, 255, 255]).rgba))
        .collect();
    for (n, (a, fa)) in faces.iter().enumerate() {
        for (b, fb) in &faces[n + 1..] {
            assert_ne!(fa, fb, "{a:?} and {b:?} rasterised identically");
        }
    }
}

/// The whole reason for the Mono variant: a HUD row must not reflow as the glyph under it
/// changes.
#[test]
fn all_icons_share_one_box() {
    let first = icon_face(Icon::Volume, 18.0, [255, 255, 255]);
    for icon in Icon::ALL {
        let f = icon_face(icon, 18.0, [255, 255, 255]);
        assert_eq!(
            (f.w, f.h),
            (first.w, first.h),
            "{icon:?} is {}x{}, the row would reflow",
            f.w,
            f.h
        );
    }
}

/// The raster is cached and the tint is not, so a second colour must not come back wearing
/// the first one's.
#[test]
fn the_icon_is_tinted_with_the_colour_asked_for() {
    icon_face(Icon::Volume, 18.0, [255, 255, 255]);
    let f = icon_face(Icon::Volume, 18.0, [200, 30, 10]);
    // Halo edges blend, so only fully covered pixels are exactly one colour or the other.
    let opaque: Vec<_> = f.rgba.chunks(4).filter(|p| p[3] == 255).collect();
    assert!(
        opaque.iter().any(|p| p[..3] == [200, 30, 10]),
        "the tint did not reach the glyph"
    );
    assert!(
        !opaque.iter().any(|p| p[..3] == [255, 255, 255]),
        "the cache served the previous colour"
    );
}

#[test]
fn a_bigger_size_gives_a_bigger_face() {
    let small = icon_face(Icon::Brightness, 18.0, [255, 255, 255]);
    let big = icon_face(Icon::Brightness, 36.0, [255, 255, 255]);
    assert!(
        big.h > small.h && big.w > small.w,
        "36 px rasterised to {}x{} against 18 px at {}x{}, the size was ignored",
        big.w,
        big.h,
        small.w,
        small.h
    );
    assert_eq!(
        icon_face(Icon::Brightness, 18.0, [255, 255, 255]).rgba,
        small.rgba,
        "the cache handed back the wrong size"
    );
}

/// The fast forward badge has no plate under it, so the icon has to survive being drawn on
/// a white game frame. A dilated dark halo is what makes that work.
#[test]
fn icons_carry_a_halo_so_they_read_without_a_plate() {
    let f = icon_face(Icon::FastForward, 18.0, [255, 255, 255]);
    let opaque = |p: &[u8]| p[3] > 128;
    let dark = f.rgba.chunks(4).filter(|p| opaque(p) && p[0] < 60).count();
    let light = f.rgba.chunks(4).filter(|p| opaque(p) && p[0] > 200).count();
    assert!(dark > 8, "no halo, only {dark} dark pixels");
    assert!(
        light > 8,
        "the glyph itself is gone, only {light} light pixels"
    );
}

/// The frame is the union of every glyph's extent, so a new icon can quietly grow the box
/// that all of them are rastered into and shove the whole HUD row sideways. The bolt has to
/// fit inside the box the existing eight already agreed on.
#[test]
fn a_new_icon_does_not_resize_the_box_the_others_share() {
    // Tracks HUD_ICON_PX: this is the union at 24 px. If it moves without the size moving,
    // the new glyph is wider or taller than every icon before it.
    assert_eq!(
        icon_box(HUD_ICON_PX),
        (26, 27),
        "if this moved, the new glyph is wider or taller than every icon before it"
    );
}

/// Callers lay out against `icon_box` before they know which glyph lands in it, so it has to
/// include the halo or every icon overflows its slot by a pixel.
#[test]
fn icon_box_accounts_for_the_halo() {
    let (bw, bh) = icon_box(18.0);
    let f = icon_face(Icon::Volume, 18.0, [255, 255, 255]);
    assert_eq!((bw, bh), (f.w, f.h));
}

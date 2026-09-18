use slot_gfx::OUT_W;
use slot_store::Platform;

use crate::art::render_svg;
use crate::hud::HUD_INK;
use crate::icon::{haloed, HALO_PX};
use crate::CartFace;

/// One per shelf, and the reason the carousel no longer has to say its shelf's name out loud:
/// the machine each shelf's cartridges were made for, drawn in the top plate's right corner
/// where a live session draws its link badge. Line art from the Noun Project under CC BY — each
/// file names its creator and `licenses/README.md` carries the attribution the stripped credit
/// line used to.
const GBA_SVG: &str = include_str!("../assets/platform_gba.svg");
const GB_SVG: &str = include_str!("../assets/platform_gb.svg");
const GBC_SVG: &str = include_str!("../assets/platform_gbc.svg");

/// How tall a mark is drawn, in offscreen pixels.
///
/// It was 24 on the case band and 32 in the corner, and both came back off the device as too
/// small — the second time with the corner itself called too tight as well. Rendered at 32, 36,
/// 40, 44 and 48 into the real shelf frame and looked at: 36 is not far enough from 32 to be
/// worth the move, 40 reads cleanly, 44 is confident, and 48 is the largest that is still an
/// annotation rather than a subject — beside a cart's own face it stays plainly the smaller
/// thing. 48 is what a third pass on "still too small" should be: 1.5 times the last size and
/// twice the first. It is also the size the physical panel argues for, since 480 px over a 53 mm
/// screen is about 9 px to the millimetre and 48 px is a 5.3 mm badge in the hand.
///
/// Multiples of 8 only, so `MARK_W` stays exact: `render_svg` scales x and y independently, and
/// a height that leaves `MARK_H * 5 / 8` with a remainder squeezes every drawing by the rounding.
pub const MARK_H: u32 = 48;

/// How far the mark's box is held off the right edge and the top of the screen.
///
/// The right margin is the case's own, the one `draw_footer` prints the gauge and the clock at,
/// rather than the 12 px `badge_at` holds the link badge off at. That is the whole of what made
/// this read as tucked into the corner: the mark sat half as far in as the clock directly below
/// it, and two things at the same end of the screen inset by different amounts read as one of
/// them being wrong. They now share an edge — the clock's type and the mark's ink both stop
/// 24 px short, give or take the halo's pixel.
///
/// The top margin has nothing above it to line up with, so it is the clock's own air read the
/// other way up: the band the footer prints its type in stops 17 px short of the bottom edge,
/// and 16 px of box plus the halo puts a mark's first lit pixel 17 px down from the top. Off the
/// rendered frame the two corners come out 17 above and 22 below, since the digits do not fill
/// their band — near enough that the corners read as a pair. Rendered at 8, 12, 16 and 20 and
/// looked at: 8 is still the complaint, 12 is comfortable, 16 is plainly deliberate, and by 20
/// the mark has come away from its corner and floats.
const MARK_MARGIN: f32 = 24.0;
const MARK_TOP: f32 = 16.0;

/// Where a mark of this size goes. Its own rule rather than `badge_at`, which it used to share.
///
/// The two are still the same idea — one corner, saying the single thing worth knowing about
/// where you are — but they are measured against different things, and at this size the shared
/// rule could no longer hold both. A badge is drawn inside the HUD's plate, only ever over a
/// running game, and 12 px in and centred in 40 px of depth is the plate's own margin. A mark is
/// drawn on the carousel, where there is no plate at all: the frame it is read against is the
/// screen, and everything else the screen prints — the gauge, the clock — is 24 px in.
///
/// That is also why the size is not bounded by the plate any more. Centred in 40 px the mark's
/// air is `(40 - box) / 2`, so every pixel of growth costs half a pixel of padding at each end,
/// and "bigger, with more room above it" cannot both be had: at today's 50 px box it is already
/// 5 px of overhang before any padding, and 16 px of air inside the plate would leave room for a
/// 6 px machine. So the mark keeps to the screen's margins and the plate, when the HUD raises
/// one, passes behind it. Over the shelf's black backdrop nothing shows; over a wallpaper the
/// plate's lower edge crosses the drawing, for the second and a half the bar is up.
///
/// Width alone, where `badge_at` takes both: a badge is centred in the plate and so has to know
/// how tall it is, and a mark is hung from the top of the screen and does not.
pub fn mark_at(w: f32) -> (f32, f32) {
    (OUT_W as f32 - MARK_MARGIN - w, MARK_TOP)
}

/// What the coverage is raised to before it is tinted, which is the whole of the fix for marks
/// that came off the device looking like a different, dimmer class of thing than the type beside
/// them. The ink was never the problem — it is `HUD_INK`, the same near-white the percent and the
/// clock are set in. The problem is that these are line drawings whose strokes are around 2 units
/// of a 100-unit box: even at 32 px that is two thirds of a pixel, so the rasteriser reports two
/// thirds coverage and two thirds of near-white on a dark ground is grey.
///
/// Raising the coverage is the same trick a font rasteriser's stem darkening is, and it is honest
/// for the same reason: the line is really there, the pixel grid is what cannot hold it. Solid
/// areas are untouched (1 to any power is 1) and so is empty space, so only the thin work moves.
///
/// 0.5 by eye against 1.0, 0.6, 0.45 and 0.4 at 32 px, magnified ten times: over the lit pixels
/// of a mark it takes the mean from 135 to 165 against a line of HUD type's own 189, which is
/// most of the gap closed while leaving a mark reading a shade under a glyph — which is what it
/// is. Below 0.45 the boost starts to be a defect rather than a correction: the DMG's screen
/// bezel begins to merge into its body and the Colour's buttons run together.
const INK_GAMMA: f32 = 0.5;

/// 5 to 8, which is what all three drawings' viewBoxes were re-fitted to. Their machines are
/// not the same shape — a DMG is 0.622 wide for its height, an SP 0.563, a Colour 0.598 — so
/// The square a mark is fitted inside, whichever way it is long.
///
/// The three marks used to share one 5:8 box, each drawing's viewBox padded out about its own
/// centre to the widest of them, so that ringing through the shelves moved nothing in the corner.
/// That held while all three were upright handhelds. The Advance's mark is the wide model now, at
/// 1.76:1, and padding a landscape drawing into an upright box leaves it either squashed or
/// drawn at a third the height of the others.
///
/// So the box is the mark's own, fitted inside a square: an upright drawing is limited by its
/// height and comes out exactly the 30x48 all three were before, and a wide one is limited by its
/// width. `mark_at` places a mark by its right edge, so the two upright marks do not move at all
/// and only the wide one reaches further in.
pub const MARK_BOX: u32 = MARK_H;

/// Kept as the width of an upright mark, which is what every caller that asks for one number
/// wants and what the two Game Boy marks still measure.
pub const MARK_W: u32 = MARK_H * 5 / 8;

/// How wide and tall this shelf's mark is drawn, from the drawing's own proportions.
pub fn mark_size(platform: Platform) -> (u32, u32) {
    let a = aspect(svg_for(platform));
    match a >= 1.0 {
        true => (MARK_BOX, (MARK_BOX as f32 / a).round().max(1.0) as u32),
        false => ((MARK_BOX as f32 * a).round().max(1.0) as u32, MARK_BOX),
    }
}

/// The drawing's width over its height, read off the file rather than written down twice.
fn aspect(svg: &str) -> f32 {
    let attr = |name: &str| -> Option<f32> {
        let at = svg.find(&format!("{name}=\""))? + name.len() + 2;
        svg[at..].split('"').next()?.parse().ok()
    };
    match (attr("width"), attr("height")) {
        (Some(w), Some(h)) if h > 0.0 => w / h,
        _ => MARK_W as f32 / MARK_H as f32,
    }
}

fn svg_for(platform: Platform) -> &'static str {
    match platform {
        Platform::Gba => GBA_SVG,
        Platform::Gb => GB_SVG,
        Platform::Gbc => GBC_SVG,
    }
}

/// The box a mark is rastered into, halo included, for callers placing one in the corner.
pub fn mark_box(platform: Platform) -> (u32, u32) {
    let (w, h) = mark_size(platform);
    (w + 2 * HALO_PX, h + 2 * HALO_PX)
}

/// The mark for a shelf, tinted and haloed exactly as the link badge that shares its corner is.
/// The drawings are black on nothing, so what is kept from the raster is its coverage alone —
/// boosted by `INK_GAMMA`, since sub-pixel line work under-reports itself — and the HUD's own ink
/// is put through it. The same two steps `icon_face` takes, for the same reason: what a mark is
/// drawn over is dark, and a mark in the artist's black would be a hole in it.
///
/// One ink for all three, deliberately. A per-platform tint was built and rendered beside this
/// one — the Advance's indigo, the DMG's pea green, the Colour's berry — and at real size it is
/// three saturated chips in a corner of a bar whose whole job is to be ignorable, with the indigo
/// coming out darker than the grey it was meant to replace. The plate is monochrome and this
/// stays monochrome with it.
pub fn mark_face(platform: Platform) -> CartFace {
    let (w, h) = mark_size(platform);
    let Some(rgba) = render_svg(svg_for(platform), w, h) else {
        return CartFace {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        };
    };
    let cov: Vec<u8> = rgba.chunks_exact(4).map(|px| boosted(px[3])).collect();
    haloed(&cov, w, h, HUD_INK)
}

/// One coverage byte through `INK_GAMMA`. Kept apart from the loop so a test can hold the curve
/// itself to its two fixed points rather than inferring them from a rastered machine.
fn boosted(cov: u8) -> u8 {
    if cov == 0 || cov == u8::MAX {
        return cov;
    }
    let a = f32::from(cov) / 255.0;
    (a.powf(INK_GAMMA) * 255.0).round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every shelf gets a mark, every mark is the size the corner reserves for that shelf, and
    /// every one has ink in it. A drawing that failed to parse comes back as an empty face rather
    /// than as a panic, which on the band would be a shelf that silently stopped saying what it
    /// was.
    ///
    /// The three used to be one size. They are not now: the Advance's is the wide model, so its
    /// mark is limited by width where the two upright ones are limited by height.
    #[test]
    fn every_shelf_has_a_mark_the_size_its_corner_reserves() {
        for p in Platform::ALL {
            let (w, h) = mark_box(p);
            let face = mark_face(p);
            assert_eq!(
                (face.w, face.h),
                (w, h),
                "{p:?}'s mark is not the size the band reserves for it"
            );
            let inked = face.rgba.chunks_exact(4).filter(|px| px[3] > 0).count();
            assert!(inked > 0, "{p:?}'s mark rastered to nothing at all");
            let (mw, mh) = mark_size(p);
            assert!(
                mw <= MARK_BOX && mh <= MARK_BOX,
                "{p:?}'s mark is {mw}x{mh}, outside the {MARK_BOX} square every mark fits"
            );
            assert!(
                mw == MARK_BOX || mh == MARK_BOX,
                "{p:?}'s mark is {mw}x{mh}, not touching either side of the square it fits"
            );
        }
    }

    /// The two upright marks are exactly the size all three used to be, so switching the Advance
    /// to the wide model moved nothing else in the corner.
    #[test]
    fn the_upright_marks_are_the_size_they_always_were() {
        assert_eq!(mark_size(Platform::Gb), (MARK_W, MARK_H));
        assert_eq!(mark_size(Platform::Gbc), (MARK_W, MARK_H));
    }

    /// The box is the 5:8 the three viewBoxes were re-fitted to, exactly, with no rounding left
    /// over. `render_svg` scales x and y independently, so a height that leaves `MARK_H * 5 / 8`
    /// with a remainder does not letterbox the drawing — it squeezes it, by up to a pixel, in
    /// every mark at once. Nothing about that would look broken on screen or fail a test; the
    /// machines would simply come out a little narrower than they were drawn.
    #[test]
    fn the_box_is_whole_pixels_of_the_ratio_the_drawings_were_fitted_to() {
        assert_eq!(
            MARK_W * 8,
            MARK_H * 5,
            "a {MARK_W}x{MARK_H} mark is not 5:8: pick a height that is a multiple of 8"
        );
    }

    /// The three are different pictures. Cheap to get wrong — the three files are one paste
    /// apart — and on the band it would look exactly like a mark that never changes, which is
    /// the one thing this feature has to not do.
    #[test]
    fn no_two_shelves_show_the_same_mark() {
        let faces: Vec<CartFace> = Platform::ALL.iter().map(|p| mark_face(*p)).collect();
        for (i, a) in faces.iter().enumerate() {
            for (j, b) in faces.iter().enumerate().skip(i + 1) {
                assert_ne!(
                    a.rgba,
                    b.rgba,
                    "{:?} and {:?} draw the same mark",
                    Platform::ALL[i],
                    Platform::ALL[j]
                );
            }
        }
    }

    /// The curve's two fixed points, which are the whole of what makes it a correction rather
    /// than a wash: nothing that the rasteriser said was empty gains ink, nothing it said was
    /// solid can gain any more, and every partial pixel in between comes up. A gamma above 1,
    /// or an off-by-one that inverted it, would darken exactly the line work this exists to
    /// lift and would still pass every other test in this file.
    #[test]
    fn the_boost_lifts_the_antialiased_middle_and_leaves_both_ends_alone() {
        assert_eq!(boosted(0), 0, "empty space gained ink");
        assert_eq!(boosted(255), 255, "solid ink was pushed past solid");
        for cov in 1..255u8 {
            assert!(
                boosted(cov) >= cov,
                "{cov} coverage came back as {}, darker than it went in",
                boosted(cov)
            );
        }
        // And actually moved, rather than merely not gone backwards. The last few steps are
        // excluded because they cannot move: the curve owes 254 half a level and rounding takes
        // it back, which is arithmetic rather than a mark that failed to brighten.
        for cov in 1..=250u8 {
            assert!(
                boosted(cov) > cov,
                "{cov} coverage came back as {}, no brighter",
                boosted(cov)
            );
        }
    }

    /// And what that does to a real drawing: every mark comes off the rasteriser brighter than
    /// it went in, without the thin work closing up into a block. The ceiling is the honest half
    /// of this — the failure a lower gamma would produce is not a dim mark but a solid one, and
    /// a test that only checked for "brighter" would wave that through.
    #[test]
    fn every_mark_comes_up_brighter_without_filling_its_box() {
        for p in Platform::ALL {
            let raw = render_svg(svg_of(p), MARK_W, MARK_H).expect("the drawing rasterises");
            let raw: Vec<u8> = raw.chunks_exact(4).map(|px| px[3]).collect();
            let lit: Vec<usize> = (0..raw.len()).filter(|i| raw[*i] > 0).collect();
            let mean = |c: &[u8]| -> f32 {
                lit.iter().map(|i| f32::from(c[*i])).sum::<f32>() / lit.len() as f32
            };
            let boost: Vec<u8> = raw.iter().map(|c| boosted(*c)).collect();
            let (before, after) = (mean(&raw), mean(&boost));
            assert!(
                after > before + 20.0,
                "{p:?} came up at {after:.0} against {before:.0}: the boost is not doing \
                 enough to be worth having"
            );
            let solid = boost.iter().filter(|c| **c == 255).count();
            assert!(
                solid < boost.len() / 2,
                "{p:?} is {solid} solid pixels of {}: the drawing has flooded rather than \
                 firmed up",
                boost.len()
            );
        }
    }

    fn svg_of(p: Platform) -> &'static str {
        match p {
            Platform::Gba => GBA_SVG,
            Platform::Gb => GB_SVG,
            Platform::Gbc => GBC_SVG,
        }
    }

    /// The credit the free download baked in sat below the drawing, at y 115 and y 120 of a
    /// 125-unit box that the drawing itself only reached y 100 of. Strip the type without
    /// re-fitting the box and every mark renders squashed into its top four fifths with a band
    /// of nothing under it — which is not a crash, not a test failure, and plainly wrong on
    /// screen. So the bottom row of each drawing is required to carry ink: it can only do that
    /// if the box ends where the machine does.
    #[test]
    fn the_box_was_re_fitted_to_the_drawing_and_not_left_holding_the_credit() {
        for p in Platform::ALL {
            let face = mark_face(p);
            let pad = HALO_PX;
            let row = |y: u32| {
                (pad..pad + MARK_W).any(|x| face.rgba[((y * face.w + x) * 4 + 3) as usize] > 0)
            };
            assert!(row(pad), "{p:?} is not drawn to the top of its box");
            assert!(
                row(pad + MARK_H - 1),
                "{p:?} stops short of the bottom of its box: the viewBox still holds the \
                 credit line the drawing was stripped of"
            );
        }
    }
}

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

/// How tall a mark is drawn, in offscreen pixels. It is a corner of the `PLATE_H` top plate now
/// rather than a slot on the case band, and the plate is 40 px deep, so the size is no longer
/// set by the height of a row of type: 32 leaves three pixels of air above and below the haloed
/// box, which is the same air the plate gives the level bar.
///
/// It was 24 on the band, and that was too small on the device. Rendered at 24, 28, 32 and 36
/// beside a line of HUD type and looked at: 24 is a chip you have to go and read, 28 is better
/// and still fiddly, 32 is the first size at which the SP's clamshell, the DMG's two round
/// buttons and the Colour's rounded shoulders are each simply there, and 36 reads best of all
/// but its haloed box is 38 px in a 40 px plate, which leaves it touching both edges.
pub const MARK_H: u32 = 32;

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
/// each viewBox was widened about the drawing's own centre to the widest of the three, rounded
/// to 5:8 so the box is whole pixels. That is what lets one box hold all three: every mark
/// draws at one height, in its own proportions, and ringing the shoulders through the shelves
/// moves nothing else in the corner.
pub const MARK_W: u32 = MARK_H * 5 / 8;

/// The box a mark is rastered into, halo included, for callers placing one in the corner before
/// they know which shelf is showing.
pub fn mark_box() -> (u32, u32) {
    (MARK_W + 2 * HALO_PX, MARK_H + 2 * HALO_PX)
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
    let svg = match platform {
        Platform::Gba => GBA_SVG,
        Platform::Gb => GB_SVG,
        Platform::Gbc => GBC_SVG,
    };
    let Some(rgba) = render_svg(svg, MARK_W, MARK_H) else {
        return CartFace {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        };
    };
    let cov: Vec<u8> = rgba.chunks_exact(4).map(|px| boosted(px[3])).collect();
    haloed(&cov, MARK_W, MARK_H, HUD_INK)
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

    /// Every shelf gets a mark, every mark lands in the one box, and every one of them has ink
    /// in it. A drawing that failed to parse comes back as an empty face rather than as a
    /// panic, which on the band would be a shelf that silently stopped saying what it was.
    #[test]
    fn every_shelf_has_a_mark_and_they_all_share_one_box() {
        let (w, h) = mark_box();
        for p in Platform::ALL {
            let face = mark_face(p);
            assert_eq!(
                (face.w, face.h),
                (w, h),
                "{p:?}'s mark is not the size the band reserves for it"
            );
            let inked = face.rgba.chunks_exact(4).filter(|px| px[3] > 0).count();
            assert!(inked > 0, "{p:?}'s mark rastered to nothing at all");
        }
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

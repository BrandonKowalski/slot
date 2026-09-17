use slot_store::Platform;

use crate::art::render_svg;
use crate::hud::HUD_INK;
use crate::icon::{haloed, HALO_PX};
use crate::CartFace;

/// One per shelf, and the reason the carousel no longer has to say its shelf's name out loud:
/// the machine each shelf's cartridges were made for, drawn on the case band where the charge
/// and the time are. Line art from the Noun Project under CC BY — each file names its creator
/// and `licenses/README.md` carries the attribution the stripped credit line used to.
const GBA_SVG: &str = include_str!("../assets/platform_gba.svg");
const GB_SVG: &str = include_str!("../assets/platform_gb.svg");
const GBC_SVG: &str = include_str!("../assets/platform_gbc.svg");

/// How tall a mark is drawn, in offscreen pixels: the whole of the `HINT_H` row the band's type
/// is set in, which is as much as it can have without stopping being part of that printed line.
/// It needs all of it. These are line drawings of whole machines and not single glyphs, and they
/// were rendered at 20, 24 and 28 px and looked at: at 20 the SP's hinge and the Colour's pad
/// are close to gone and telling the three apart is work, at 24 the clamshell, the DMG's two
/// round buttons and the Colour's rounded shoulders are each there to be seen, and 28 adds
/// little while overrunning the row. The strokes are around 2 units of a 100-unit drawing, so
/// even here they land under a pixel and antialias to grey: the marks read lighter than the type
/// beside them, which is what line art at this size does rather than something to tune out.
pub const MARK_H: u32 = 24;

/// 5 to 8, which is what all three drawings' viewBoxes were re-fitted to. Their machines are
/// not the same shape — a DMG is 0.622 wide for its height, an SP 0.563, a Colour 0.598 — so
/// each viewBox was widened about the drawing's own centre to the widest of the three, rounded
/// to 5:8 so the box is whole pixels. That is what lets one box hold all three: every mark
/// draws at one height, in its own proportions, and ringing the shoulders through the shelves
/// moves nothing else on the band.
pub const MARK_W: u32 = MARK_H * 5 / 8;

/// The box a mark is rastered into, halo included, for callers laying the band out before they
/// know which shelf is showing.
pub fn mark_box() -> (u32, u32) {
    (MARK_W + 2 * HALO_PX, MARK_H + 2 * HALO_PX)
}

/// The mark for a shelf, tinted and haloed exactly as the charging bolt beside it is. The
/// drawings are black on nothing, so what is kept from the raster is its coverage alone and the
/// HUD's own ink is put through it — the same two steps `icon_face` takes, for the same reason:
/// the band is dark, and a mark drawn in the artist's black would be a hole in it.
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
    let cov: Vec<u8> = rgba.chunks_exact(4).map(|px| px[3]).collect();
    haloed(&cov, MARK_W, MARK_H, HUD_INK)
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

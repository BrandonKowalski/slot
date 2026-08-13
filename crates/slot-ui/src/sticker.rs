//! The about screen, which is the back label of a Game Boy Advance SP with this build's
//! details where the regulatory text was.
//!
//! Rasterised whole rather than assembled from draw rects: the original has a rounded body, a
//! die-cut notch and a white panel inset into one corner, and none of those are shapes a list
//! of rectangles gets right. The binary uploads the face and re-rasterises it when the text
//! changes, which is the same deal the shelf clock has.

use slot_gfx::{Draw, TexId};

use crate::barcode::{code39, CODE39_NARROW, CODE39_WIDE};
use crate::plate::UndoFace;
use crate::text;

/// The body and the barcode panel, traced from the article. The die cut and both corner radii
/// come from here rather than from constants: the outline is the one part of this that is a
/// drawing rather than a layout, and hand-fitting rectangles to it never quite landed.
const STICKER_SVG: &str = include_str!("../assets/sticker.svg");

/// ANBERNIC RG SP, set as outlines. Where the article puts the console's own logo.
const WORDMARK_SVG: &str = include_str!("../assets/wordmark.svg");

/// How wide the lockup sits on the label. Set by its keyline rather than by the block it sits
/// in: the outline is what makes it that logotype, and below about this width it thins to
/// nothing and the whole thing collapses into solid letters.
const WORDMARK_W: u32 = 250;

/// The traced outline's own aspect, so it rasterises unstretched.
pub const STICKER_W: u32 = 660;
pub const STICKER_H: u32 = 228;

const BLACK: [u8; 3] = [0x23, 0x1f, 0x20];
const WHITE: [u8; 3] = [0xff, 0xff, 0xff];

/// Where the panel sits, as fractions of the traced artwork, so the type follows the shape
/// rather than a second set of numbers that can drift from it.
const PANEL_FX: f32 = 62.133 / 205.762;
const PANEL_FY: f32 = 3.81 / 71.116;
const PANEL_FW: f32 = 140.315 / 205.762;
const PANEL_FH: f32 = 35.433 / 71.116;

/// Stands in for the direct current symbol in the text, and is drawn rather than set. Kept as
/// the real codepoint so the line reads correctly to anything but the rasteriser.
pub const DC: char = '\u{2393}';

const MARGIN: f32 = 11.0;
const HEAD_PX: f32 = 11.0;
const BODY_PX: f32 = 8.5;
const SERIAL_PX: f32 = 26.0;
const SMALL_PX: f32 = 9.0;

/// Everything the label says that is not fixed for the life of the binary.
pub struct StickerFields<'a> {
    /// `None` where the board has no gauge, which is a device without one rather than a flat
    /// battery.
    pub battery: Option<u8>,
    /// The build's short hash, which is what the barcode encodes and what names this unit —
    /// the way a serial names a real one. There is no version row: the hash is more specific
    /// than a version and the plate has one identifier, not two.
    pub serial: &'a str,
    /// Boxed beside the serial, where the article boxes a check digit. Says whether the tree
    /// had uncommitted changes when this was built.
    pub dirty_digit: char,
}

/// What the article gives ten lines of regulatory small print to. Set to the width of the
/// column rather than to the sentence: the original's type is condensed and Open Sans is not,
/// so the same wording at the same size would run out from under the barcode panel.
///
/// This is what README.md credits, in the space a label has for it.
pub const CREDITS: [&str; 10] = [
    "EMULATION BY MGBA, THROUGH",
    "LIBRETRO. AGS-102 IS A FORK OF",
    "BASEOS BY PVAIBHAV. TYPE IS",
    "OPEN SANS AND NERD FONTS",
    "SYMBOLS BY RYAN L MCINTYRE.",
    "THE PANEL MASK IS DERIVED",
    "FROM GIGAHERZ'S LCD3X. THE",
    "SOUNDS ARE MY CHILDHOOD",
    "GAMEBOY. I WASTED WATER",
    "WITH CLAUDE CODE.",
];

/// The article's own origin row, kept word for word. It is the one place the joke is funnier
/// left alone than rewritten.
pub const ORIGIN: [&str; 2] = ["S/LOT-USA", "MADE IN ITHACA"];

/// The bottom right block, under the lockup. The copyright sign is a real glyph here; the
/// article's circled M beside it is not, and is not true of this anyway.
pub const COPYRIGHT: &str = "\u{a9} 2026 BRANDON T. KOWALSKI";

pub const HOME: &str = "SEE README.";

/// The three headline rows: what the plate says about this unit. Only the gauge moves.
pub fn head_rows(f: &StickerFields) -> [String; 3] {
    [
        // The next model number after the backlit SP, which is what this is pretending to be.
        "MODEL NO. AGS-102".into(),
        // The real symbol. No font here has it — Open Sans lacks it and the Nerd Font's
        // nearest codepoint is a pair of squares — so the renderer draws it instead of asking
        // for a glyph. It is a solid bar over three dashes and nothing more.
        format!("INPUT : 5V{DC}1.5A"),
        match f.battery {
            Some(p) => format!("BATTERY : LI-ION ({p}%)"),
            // The row stays: a label with a gap where a heading was reads as a rendering fault.
            None => "BATTERY : LI-ION".to_string(),
        },
    ]
}

/// Every line on the label, in reading order. The face draws from the blocks above rather than
/// from this, so nothing depends on a position in it.
pub fn sticker_lines(f: &StickerFields) -> Vec<String> {
    let mut out: Vec<String> = head_rows(f).into();
    out.extend(CREDITS.iter().map(|s| s.to_string()));
    out.extend(ORIGIN.iter().map(|s| s.to_string()));
    out.push(format!("{} {}", f.serial, f.dirty_digit));
    out.push(COPYRIGHT.to_string());
    out.push(HOME.to_string());
    out
}

struct Canvas {
    px: Vec<u8>,
    w: u32,
    h: u32,
}

impl Canvas {
    /// The traced outline, rasterised. Everything else is set on top of it.
    fn shape(w: u32, h: u32) -> Canvas {
        let px = render_svg(STICKER_SVG, w, h).unwrap_or_else(|| vec![0; (w * h * 4) as usize]);
        Canvas { px, w, h }
    }

    fn set(&mut self, x: u32, y: u32, c: [u8; 3], a: u8) {
        if x >= self.w || y >= self.h {
            return;
        }
        let at = ((y * self.w + x) * 4) as usize;
        self.px[at..at + 3].copy_from_slice(&c);
        self.px[at + 3] = a;
    }

    fn rect(&mut self, x: u32, y: u32, w: u32, h: u32, c: [u8; 3]) {
        for yy in y..(y + h).min(self.h) {
            for xx in x..(x + w).min(self.w) {
                self.set(xx, yy, c, 255);
            }
        }
    }

    /// What `print` would take, without drawing it. Needed wherever something is placed
    /// against its own right edge rather than its left.
    fn print_measure(&self, s: &str, px: f32) -> f32 {
        let Some(font) = text::label_font() else {
            return 0.0;
        };
        let layout = text::fit(font, s, f32::MAX, 1, px, px);
        layout
            .lines
            .first()
            .map(|l| text::line_width(font, l, px, layout.tracking))
            .unwrap_or(0.0)
    }

    /// The direct current symbol: a solid bar with three dashes beneath it. Drawn, because no
    /// font in this crate carries U+2393 and a missing glyph rasterises to nothing — a rating
    /// line that quietly loses its middle.
    fn dc(&mut self, x: f32, y: f32, px: f32, c: [u8; 3]) -> f32 {
        let bar_w = px * 0.78;
        let t = (px / 8.0).round().max(1.0);
        // The two halves need daylight between them or they read as one thick smudge at this
        // size — which is exactly what a mark this small fails at first.
        let gap = (t * 2.0).max(3.0);
        // Centred on the cap band of the digits either side, which sits from 0.35 to 1.02 of
        // the size below the box top. Hung any lower and the dashes land on the baseline,
        // where the mark stops reading as a symbol and starts reading as an ellipsis.
        let bar_y = (y + px * 0.685 - (t * 2.0 + gap) / 2.0).round();
        self.rect(x as u32, bar_y as u32, bar_w as u32, t as u32, c);
        let dash = bar_w / 5.0;
        for n in 0..3 {
            let dx = x + n as f32 * dash * 2.0;
            self.rect(
                dx as u32,
                (bar_y + t + gap) as u32,
                dash.ceil() as u32,
                t as u32,
                c,
            );
        }
        bar_w
    }

    /// Rasterised artwork composited at a position, over whatever is already there.
    fn blit(&mut self, x: u32, y: u32, src: &[u8], sw: u32, sh: u32) {
        for row in 0..sh {
            for col in 0..sw {
                let s = ((row * sw + col) * 4) as usize;
                let a = src[s + 3] as u32;
                if a == 0 {
                    continue;
                }
                let (dx, dy) = (x + col, y + row);
                if dx >= self.w || dy >= self.h {
                    continue;
                }
                let d = ((dy * self.w + dx) * 4) as usize;
                for k in 0..3 {
                    let under = self.px[d + k] as u32;
                    self.px[d + k] =
                        ((src[s + k] as u32 * 255 + under * (255 - a)) / 255).min(255) as u8;
                }
                self.px[d + 3] = 255;
            }
        }
    }

    /// A hollow rectangle, for the boxed digit.
    fn outline(&mut self, x: f32, y: f32, w: f32, h: f32, t: f32, c: [u8; 3]) {
        let (x, y, w, h, t) = (x as u32, y as u32, w as u32, h as u32, t as u32);
        self.rect(x, y, w, t, c);
        self.rect(x, y + h - t, w, t, c);
        self.rect(x, y, t, h, c);
        self.rect(x + w - t, y, t, h, c);
    }

    /// One line of type at a position, left aligned. `coverage` centres its line in the box
    /// it is given, so a box cut to the line's own width puts the pen at zero — which is the
    /// only way to left align through this layer. The box is sized from the text rather than
    /// from the column, which is also what stops `fit` shrinking and then breaking a line
    /// mid-word to make it match.
    fn print(&mut self, x: f32, y: f32, s: &str, px: f32, c: [u8; 3]) -> f32 {
        let Some(font) = text::label_font() else {
            return y;
        };
        let layout = text::fit(font, s, f32::MAX, 1, px, px);
        let measured = layout
            .lines
            .first()
            .map(|l| text::line_width(font, l, px, layout.tracking))
            .unwrap_or(0.0);
        let (bw, bh) = ((measured.ceil() as u32).max(1), (px * 1.6).ceil() as u32);
        let cov = text::coverage(bw, bh, &layout);
        for row in 0..bh {
            for col in 0..bw {
                let a = cov[(row * bw + col) as usize];
                if a == 0 {
                    continue;
                }
                let (dx, dy) = (x as u32 + col, y as u32 + row);
                if dx < self.w && dy < self.h {
                    let at = ((dy * self.w + dx) * 4) as usize;
                    let inv = 255 - a as u32;
                    for (k, ink) in c.iter().enumerate() {
                        let under = self.px[at + k] as u32;
                        self.px[at + k] = ((*ink as u32 * a as u32 + under * inv) / 255) as u8;
                    }
                    self.px[at + 3] = 255;
                }
            }
        }
        y + px * 1.35
    }
}

/// The whole sticker, ready to upload. Re-rasterised only when its text changes.
pub fn sticker_face(f: &StickerFields) -> UndoFace {
    // The body, its die cut and the barcode panel all arrive together: they are one drawing,
    // and the only thing left to do here is set type on it.
    let mut c = Canvas::shape(STICKER_W, STICKER_H);
    let panel_x = (PANEL_FX * STICKER_W as f32).round() as u32;
    let panel_y = (PANEL_FY * STICKER_H as f32).round() as u32;
    let panel_w = (PANEL_FW * STICKER_W as f32).round() as u32;
    let panel_h = (PANEL_FH * STICKER_H as f32).round() as u32;

    let left = MARGIN;
    let mut y = MARGIN;
    for (n, line) in head_rows(f).iter().enumerate() {
        // The rating row is the one line with a symbol no font here can set, so it is laid out
        // by hand around it rather than printed whole.
        if n == 1 {
            if let Some((before, after)) = line.split_once(DC) {
                let bw = c.print_measure(before, HEAD_PX);
                c.print(left, y, before, HEAD_PX, WHITE);
                // Space either side, as a character would have. Without it the mark collides
                // with the digits and the row reads as damage rather than a rating.
                let dw = c.dc(left + bw + 4.0, y, HEAD_PX, WHITE);
                y = c.print(left + bw + dw + 8.0, y, after, HEAD_PX, WHITE);
                continue;
            }
        }
        y = c.print(left, y, line, HEAD_PX, WHITE);
    }
    y += 3.0;
    for line in CREDITS {
        y = c.print(left, y, line, BODY_PX, WHITE);
    }
    y += 3.0;
    // Two items on one row, as the article's is: one against each edge of the column.
    let col_right = panel_x as f32 - MARGIN;
    c.print(left, y, ORIGIN[0], BODY_PX, WHITE);
    let maker_w = c.print_measure(ORIGIN[1], BODY_PX);
    c.print(col_right - maker_w, y, ORIGIN[1], BODY_PX, WHITE);

    // The barcode, over the white panel, with a quiet zone either side.
    let bars_y = panel_y + 14;
    let bars_h = 62;
    // Model, unit and the dirty marker: a scan says everything the label does about which
    // build this is, the way a product barcode carries model and serial together.
    let payload = format!("SLOT-{}-{}", f.serial, f.dirty_digit);
    if let Some(run) = code39(&format!("*{payload}*")) {
        // The widest bars that still fit the panel with its quiet zones. Computed rather than
        // constant: a longer payload otherwise runs off the panel with no complaint, and bars
        // too fine to read are not a barcode.
        let syms = run.len() as f32 / 9.0;
        let per_sym = 3.0 * CODE39_WIDE + 7.0 * CODE39_NARROW;
        let scale = ((panel_w as f32 - 12.0) / (syms * per_sym + 20.0 * CODE39_NARROW)).min(2.0);
        let narrow = ((CODE39_NARROW * scale).round() as u32).max(1);
        let wide = ((CODE39_WIDE * scale).round() as u32).max(narrow * 2);
        let total: u32 = run
            .iter()
            .map(|w| if *w { wide } else { narrow })
            .sum::<u32>()
            + (run.len() as u32 / 9) * narrow;
        let mut x = panel_x + panel_w.saturating_sub(total) / 2;
        for chunk in run.chunks_exact(9) {
            for (n, is_wide) in chunk.iter().enumerate() {
                let ew = if *is_wide { wide } else { narrow };
                if n.is_multiple_of(2) {
                    c.rect(x, bars_y, ew, bars_h, BLACK);
                }
                x += ew;
            }
            x += narrow;
        }
    }

    // The serial under the bars, in the panel, with the dirty digit boxed off as the real
    // label boxes its check digit.
    // The hash, then the dirty marker in a box of its own — the article boxes its check digit
    // the same way, and it is the one glyph here that is a flag rather than an identifier.
    let sy = (bars_y + bars_h + 4) as f32;
    let hash_w = c.print_measure(f.serial, SERIAL_PX);
    let box_w = SERIAL_PX * 0.9;
    let total = hash_w + 10.0 + box_w;
    let sx = panel_x as f32 + (panel_w as f32 - total) / 2.0;
    c.print(sx, sy, f.serial, SERIAL_PX, BLACK);
    let bx = sx + hash_w + 10.0;
    c.outline(bx, sy + 1.0, box_w, SERIAL_PX * 1.25, 2.0, BLACK);
    c.print(
        bx + box_w * 0.28,
        sy + 2.0,
        &f.dirty_digit.to_string(),
        SERIAL_PX * 0.85,
        BLACK,
    );

    // The bottom right block, where the console's own name and the copyright are.
    let mut ry = panel_y as f32 + panel_h as f32 + 10.0;
    let right_edge = (panel_x + panel_w) as f32;
    if let Some(mark) = wordmark(WORDMARK_W) {
        let (mw, mh) = mark.1;
        c.blit(
            (right_edge - mw as f32 - 10.0) as u32,
            ry as u32,
            &mark.0,
            mw,
            mh,
        );
        ry += mh as f32 + 4.0;
    }
    // The copyright and where the thing actually is. The article's second mark was the
    // circled M, which says a trademark is registered; this one is not, so the line is the
    // copyright alone and the address goes under it.
    for line in [COPYRIGHT, HOME] {
        let lw = c.print_measure(line, SMALL_PX);
        ry = c.print(right_edge - lw - 10.0, ry, line, SMALL_PX, WHITE);
    }

    UndoFace {
        rgba: c.px,
        w: STICKER_W,
        h: STICKER_H,
    }
}

/// The traced shape, at the size the face wants. `tiny_skia` hands back premultiplied RGBA,
/// which is the same thing straight through wherever alpha is 0 or 255 — and this artwork has
/// no partial coverage except on its own antialiased edges, where premultiplied is what the
/// compositor wants anyway.
fn render_svg(svg: &str, w: u32, h: u32) -> Option<Vec<u8>> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
    let size = tree.size();
    let scale =
        resvg::tiny_skia::Transform::from_scale(w as f32 / size.width(), h as f32 / size.height());
    resvg::render(&tree, scale, &mut pixmap.as_mut());
    Some(pixmap.data().to_vec())
}

/// The lockup at a given width, and the size it came back. `None` where the artwork will not
/// parse, which draws no logo rather than no label.
fn wordmark(w: u32) -> Option<(Vec<u8>, (u32, u32))> {
    let tree = usvg::Tree::from_str(WORDMARK_SVG, &usvg::Options::default()).ok()?;
    let size = tree.size();
    let h = (w as f32 * size.height() / size.width()).round() as u32;
    Some((render_svg(WORDMARK_SVG, w, h)?, (w, h)))
}

/// Centred on screen, at its own size. The label is an object being looked at rather than a
/// screen being laid out, so it is not stretched to fit.
pub fn draw_sticker(face: Option<TexId>, out: &mut Vec<Draw>) {
    out.push(Draw::Rect {
        x: 0.0,
        y: 0.0,
        w: slot_gfx::OUT_W as f32,
        h: slot_gfx::OUT_H as f32,
        colour: [0.04, 0.04, 0.05, 1.0],
    });
    let Some(tex) = face else {
        return;
    };
    out.push(Draw::Tex {
        x: (slot_gfx::OUT_W as f32 - STICKER_W as f32) / 2.0,
        y: (slot_gfx::OUT_H as f32 - STICKER_H as f32) / 2.0,
        w: STICKER_W as f32,
        h: STICKER_H as f32,
        tex,
        alpha: 1.0,
    });
}

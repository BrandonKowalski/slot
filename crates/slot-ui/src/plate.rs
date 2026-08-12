use crate::draw::{Draw, TexId};
use crate::text;

/// A hint is a key cap and what that key does, sized to what it says: four of them share the
/// bottom plate with the dots, and a fixed width wide enough for the longest would leave the
/// short ones swimming.
pub const HINT_H: u32 = 24;
/// The smallest a cap gets, and the size of every single letter one, so B and X sit
/// identically on theirs.
pub const CAP: u32 = 20;
/// Blank either side of a key that needs more room than the square.
const CAP_PAD: u32 = 4;
/// A cap is never wider than this. Only the layout uses it; nothing names a key this long.
const CAP_MAX_W: f32 = 64.0;
/// Tight. Any more and the cap and its word read as two separate things rather than one
/// label.
/// Between one hint and the next. Lives here rather than with either screen, so the shelf
/// and the switcher cannot drift apart.
pub const HINT_GAP: f32 = 14.0;

/// Between a key cap and the word it belongs to. Small, but it has to stay clearly smaller
/// than the gap between one hint and the next, or the row reads as an alternating run of
/// caps and words rather than as pairs. `grouping_reads_as_pairs` holds that ratio.
pub const CAP_GAP: u32 = 5;
/// Blank column past the type, so a label that filled its band cannot touch the next hint.
const EDGE: u32 = 2;
/// The longest a label may rasterise to before the fitter shrinks it. The undo is the only
/// one that comes from outside this file.
const LABEL_MAX_W: f32 = 140.0;

pub const TITLE_W: u32 = 360;
pub const TITLE_H: u32 = 24;

const INK: [u8; 3] = [0xf6, 0xf4, 0xef];
/// A cap is light with a dark letter on it, which is what a key looks like and the only
/// thing separating the key from its label at this size.
const CAP_INK: [u8; 3] = [0x1a, 0x19, 0x17];
const KEY_PX: f32 = 14.0;
const LABEL_PX: f32 = 16.0;
const LABEL_MIN_PX: f32 = 10.0;
const TITLE_PX: f32 = 20.0;
const TITLE_MIN_PX: f32 = 12.0;

pub struct UndoFace {
    pub rgba: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

/// A key cap and what it does. Every screen names its own buttons; the binary rasterises what
/// it is told and hands back a texture.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Hint {
    pub key: &'static str,
    pub label: String,
}

/// A legend as hints, in the order it will be drawn.
pub fn hint_row(legend: &[(&'static str, &'static str)]) -> Vec<Hint> {
    legend
        .iter()
        .map(|(key, label)| Hint {
            key,
            label: label.to_string(),
        })
        .collect()
}

/// How wide the cap has to be for the key on it. Square for a single letter; wider for one
/// that names two buttons, since shrinking `L / R` into the square would make the shelf's only
/// legend the smallest type on screen.
pub fn cap_width(key: &str) -> u32 {
    let Some(font) = text::label_font() else {
        return CAP;
    };
    let layout = text::fit(font, key, CAP_MAX_W, 1, KEY_PX, KEY_PX);
    let ink = layout
        .lines
        .iter()
        .map(|l| text::line_width(font, l, layout.px, layout.tracking))
        .fold(0.0, f32::max);
    CAP.max(ink.ceil() as u32 + 2 * CAP_PAD)
}

/// What `hint_face` will rasterise to. The screens lay their legends out from these, so the
/// two have to agree or every hint after the first sits beside someone else's type.
pub fn hint_width(key: &str, label: &str) -> u32 {
    cap_width(key) + CAP_GAP + band_width(label) + EDGE
}

/// Transparent apart from the cap and the type: both plates are translucent over a
/// screenshot, and a filled face would be a solid block sitting on one.
pub fn hint_face(key: &str, label: &str) -> UndoFace {
    let w = hint_width(key, label);
    let mut rgba = vec![0u8; (w * HINT_H * 4) as usize];

    let cap_w = cap_width(key);
    let mut cap = Vec::with_capacity((cap_w * CAP * 4) as usize);
    for _ in 0..cap_w * CAP {
        cap.extend_from_slice(&[INK[0], INK[1], INK[2], 255]);
    }
    if let Some(font) = text::label_font() {
        let layout = text::fit(font, key, cap_w as f32, 1, KEY_PX, KEY_PX);
        text::draw_centred(&mut cap, cap_w, CAP, &layout, CAP_INK);
    }
    blit(&mut rgba, w, &cap, cap_w, CAP, 0, (HINT_H - CAP) / 2);

    let text_w = band_width(label);
    let mut band = vec![0u8; (text_w * HINT_H * 4) as usize];
    if let Some(font) = text::label_font() {
        let layout = text::fit(font, label, text_w as f32, 1, LABEL_PX, LABEL_MIN_PX);
        text::draw_centred(&mut band, text_w, HINT_H, &layout, INK);
    }
    blit(&mut rgba, w, &band, text_w, HINT_H, cap_w + CAP_GAP, 0);

    UndoFace { rgba, w, h: HINT_H }
}

/// The same type as a hint's label, with no key cap in front of it. What the shelf prints on
/// the case: the wordmark and the time, in the font the buttons are labelled in.
pub fn word_width(text: &str) -> u32 {
    band_width(text)
}

pub fn word_face(text: &str) -> UndoFace {
    let w = word_width(text);
    let mut rgba = vec![0u8; (w * HINT_H * 4) as usize];
    if let Some(font) = text::label_font() {
        let layout = text::fit(font, text, w as f32, 1, LABEL_PX, LABEL_MIN_PX);
        text::draw_centred(&mut rgba, w, HINT_H, &layout, INK);
    }
    UndoFace { rgba, w, h: HINT_H }
}

/// A hint's slot is held whether or not its face arrived, for the same reason a cart with no
/// face still holds its place on the shelf: a missing affordance is worse than a blank one.
pub fn hint_quad(x: f32, y: f32, w: f32, face: Option<TexId>) -> Draw {
    let h = HINT_H as f32;
    match face {
        Some(tex) => Draw::Tex {
            x,
            y,
            w,
            h,
            tex,
            alpha: 1.0,
        },
        None => Draw::Rect {
            x,
            y,
            w,
            h,
            colour: [1.0, 1.0, 1.0, 0.12],
        },
    }
}

/// How wide the type alone comes out, which is what the hint is sized around. A label the
/// fitter had to break lands at `LABEL_MAX_W`, since that is the width it was broken to.
fn band_width(label: &str) -> u32 {
    let Some(font) = text::label_font() else {
        return LABEL_MAX_W as u32;
    };
    let layout = text::fit(font, label, LABEL_MAX_W, 1, LABEL_PX, LABEL_MIN_PX);
    let ink = layout
        .lines
        .iter()
        .map(|l| text::line_width(font, l, layout.px, layout.tracking))
        .fold(0.0, f32::max);
    (ink.ceil() as u32).clamp(1, LABEL_MAX_W as u32)
}

/// One line of type for the top plate, naming the entry under the eye.
pub fn title_face(text: &str) -> UndoFace {
    let mut rgba = vec![0u8; (TITLE_W * TITLE_H * 4) as usize];
    if let Some(font) = text::label_font() {
        let layout = text::fit(font, text, TITLE_W as f32, 1, TITLE_PX, TITLE_MIN_PX);
        text::draw_centred(&mut rgba, TITLE_W, TITLE_H, &layout, INK);
    }
    UndoFace {
        rgba,
        w: TITLE_W,
        h: TITLE_H,
    }
}

pub(crate) fn blit(dst: &mut [u8], dst_w: u32, src: &[u8], src_w: u32, src_h: u32, x: u32, y: u32) {
    for row in 0..src_h {
        let from = ((row * src_w) * 4) as usize;
        let to = (((y + row) * dst_w + x) * 4) as usize;
        dst[to..to + (src_w * 4) as usize].copy_from_slice(&src[from..from + (src_w * 4) as usize]);
    }
}

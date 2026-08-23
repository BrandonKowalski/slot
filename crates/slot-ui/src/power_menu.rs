use crate::plate::UndoFace;
use crate::text;

/// What a held POWER offers. Restart, because a device you develop on wants one that is not
/// "off, then find the button again" — and off, which is the only other thing this hardware
/// can honestly do.
///
/// There is no Standby. The board suspends well, under 45 mA, but it cannot wake itself: the
/// RTC alarm arms, reads back, and never fires — measured on a fully awake machine as well as
/// a suspended one, and unrelated to Super Standby, which was the first two things I blamed.
/// A standby nothing can end is a slow leak with a nicer name, so the lid and the button run
/// a timer and then power off properly instead.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PowerChoice {
    Restart,
    PowerOff,
}

impl PowerChoice {
    /// Restart first: it is the one that costs nothing to pick by mistake.
    pub const ALL: [PowerChoice; 2] = [PowerChoice::Restart, PowerChoice::PowerOff];

    /// Position in `ALL`, which is the order the faces are uploaded in.
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn text(self) -> &'static str {
        match self {
            PowerChoice::Restart => "Restart",
            PowerChoice::PowerOff => "Power Off",
        }
    }
}

/// The menu is read at arm's length on a 720x480 panel while the user is deciding something
/// they cannot undo, so it is set well above the key-caption type the rest of the chrome
/// uses. The shutdown line that follows a choice is rastered at the same size: the words
/// change but the voice should not.
const MENU_PX: f32 = 30.0;
const MENU_MIN_PX: f32 = 18.0;
const MENU_H: u32 = 40;
/// Breathing room either side of the ink, which is also what the highlight bar is padded by
/// so the bar hugs the words rather than the panel.
pub const MENU_PAD: u32 = 18;
const MENU_INK: [u8; 3] = [0xf6, 0xf4, 0xef];

/// Sized to its own text rather than to a fixed box, so a caller can put a bar behind it
/// that fits the words. A fixed width would make the bar the same size under "Restart" and
/// "Power Off", which is the thing that looks wrong when the selection moves.
pub fn menu_face(label: &str) -> UndoFace {
    let Some(font) = text::label_font() else {
        return UndoFace {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        };
    };
    let ink = text::line_width(font, label, MENU_PX, 0.0).ceil() as u32;
    let w = ink + 2 * MENU_PAD;
    let mut rgba = vec![0u8; (w * MENU_H * 4) as usize];
    let layout = text::fit(font, label, w as f32, 1, MENU_PX, MENU_MIN_PX);
    text::draw_centred(&mut rgba, w, MENU_H, &layout, MENU_INK);
    UndoFace { rgba, w, h: MENU_H }
}

/// The cart's name over the core picker. Set above the rows it labels rather than under
/// them: the rows are two short words the eye already knows, and the name is the long
/// unfamiliar one doing the actual identifying, so it is the thing that has to be legible
/// first. Wide enough for a full "Pokemon - LeafGreen Version (USA, Europe) (Rev 1)" before
/// the fitter starts shrinking, which is the shape a real ROM filename takes.
const PICKER_TITLE_PX: f32 = 36.0;
const PICKER_TITLE_MIN_PX: f32 = 18.0;
const PICKER_TITLE_W: u32 = 640;
const PICKER_TITLE_H: u32 = 48;

pub fn picker_title_face(label: &str) -> UndoFace {
    let mut rgba = vec![0u8; (PICKER_TITLE_W * PICKER_TITLE_H * 4) as usize];
    if let Some(font) = text::label_font() {
        let layout = text::fit(
            font,
            label,
            PICKER_TITLE_W as f32,
            1,
            PICKER_TITLE_PX,
            PICKER_TITLE_MIN_PX,
        );
        text::draw_centred(&mut rgba, PICKER_TITLE_W, PICKER_TITLE_H, &layout, MENU_INK);
    }
    UndoFace {
        rgba,
        w: PICKER_TITLE_W,
        h: PICKER_TITLE_H,
    }
}

/// The word between the cart's name and the cores, saying what the two rows underneath are.
/// Without it the menu is a game's name over two proper nouns and no verb: whether picking
/// one runs it, deletes it or renames it is left to the player to guess.
///
/// Dimmer and much smaller than either — it is a label, and a label that competes with the
/// thing it labels has failed at being one.
const CAPTION_PX: f32 = 16.0;
const CAPTION_MIN_PX: f32 = 12.0;
const CAPTION_H: u32 = 22;
const CAPTION_W: u32 = 360;
const CAPTION_INK: [u8; 3] = [0x8e, 0x8a, 0x84];

pub fn picker_caption_face(label: &str) -> UndoFace {
    let mut rgba = vec![0u8; (CAPTION_W * CAPTION_H * 4) as usize];
    if let Some(font) = text::label_font() {
        let layout = text::fit(font, label, CAPTION_W as f32, 1, CAPTION_PX, CAPTION_MIN_PX);
        text::draw_centred(&mut rgba, CAPTION_W, CAPTION_H, &layout, CAPTION_INK);
    }
    UndoFace {
        rgba,
        w: CAPTION_W,
        h: CAPTION_H,
    }
}

use crate::plate::UndoFace;
use crate::text;

/// What a held POWER offers. Three states rather than two, because a device you develop on
/// wants a restart that is not "off, then find the button again", and because a menu of two
/// reads as a confirmation dialog rather than a choice.
///
/// `Standby` is deliberately not called Sleep, and not called Hibernate. The kernel here
/// offers only `freeze` and `mem` — `/sys/power/state` has no `disk`, so hibernation is not
/// available on this SoC at all — and what this does is suspend-to-RAM: memory held powered,
/// instant resume, measured under 45 mA against 400-700 mA awake. Standby is what the
/// hardware calls it too: the PMIC bit that makes it cheap is Super Standby.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PowerChoice {
    Standby,
    Restart,
    PowerOff,
}

impl PowerChoice {
    /// Standby first: it is the one that costs nothing to pick by mistake, and the one a
    /// user reaching for the power button most often means.
    pub const ALL: [PowerChoice; 3] = [
        PowerChoice::Standby,
        PowerChoice::Restart,
        PowerChoice::PowerOff,
    ];

    /// Position in `ALL`, which is the order the faces are uploaded in.
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn text(self) -> &'static str {
        match self {
            PowerChoice::Standby => "Standby",
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
/// that fits the words. A fixed width would make the bar the same size under "Standby" and
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

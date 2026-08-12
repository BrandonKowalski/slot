use slot_gfx::{Draw, TexId, OUT_W};
use slot_store::{BLUE_LIGHT_MAX, BRIGHTNESS_MAX, VOLUME_MAX};

use crate::icon::icon_box;
use crate::toast::toast_rect;
use crate::{Icon, Toast};

/// Milliseconds on the same monotonic clock the gesture machines run on. Spelled again
/// here because `slot-ui` cannot see `slot-input` and must not learn to.
pub type Millis = u64;

pub const HUD_MS: Millis = 1500;
/// The tail of that window, spent ramping out. A hard cut at 1500 ms reads as a glitch.
const FADE_MS: Millis = 250;

/// The bar has no backing of its own, so over a bright frame a white fill on a translucent
/// white track vanishes. The plate is what it is read against. Shared with the refusal and
/// with the switcher's two plates, so everything that ever sits on top of a picture reads as
/// one system.
pub const PLATE_H: f32 = 40.0;
pub(crate) const PLATE: [f32; 4] = [0.0, 0.0, 0.0, 0.72];

pub const HUD_ICON_PX: f32 = 24.0;
/// One ink for the glyph and the fill, so the row reads as one control.
pub const HUD_INK: [u8; 3] = [0xf5, 0xf2, 0xef];
const ICON_GAP: f32 = 10.0;

/// The badge sits in the corner the bar never reaches, so the two never have to negotiate.
const BADGE_MARGIN: f32 = 12.0;

const BAR_W: f32 = 320.0;
const BAR_H: f32 = 6.0;
const BAR_Y: f32 = (PLATE_H - BAR_H) / 2.0;
const TRACK: [f32; 4] = [1.0, 1.0, 1.0, 0.18];
const FILL: [f32; 4] = [
    HUD_INK[0] as f32 / 255.0,
    HUD_INK[1] as f32 / 255.0,
    HUD_INK[2] as f32 / 255.0,
    1.0,
];

#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
pub enum HudKind {
    #[default]
    Brightness,
    BlueLight,
    Volume,
    /// Not a level: the value is how much rewind history is left, as a percentage.
    Rewind,
}

impl HudKind {
    /// Silence is a state, not a low level: a bar at zero looks the same as a bar that has
    /// not been dragged down yet, so the glyph is what has to say it.
    pub fn icon(self, value: u8, muted: bool) -> Icon {
        match self {
            HudKind::Brightness => Icon::Brightness,
            HudKind::BlueLight => Icon::BlueLight,
            HudKind::Volume if muted => Icon::VolumeMuted,
            HudKind::Volume if value == 0 => Icon::VolumeZero,
            HudKind::Volume => Icon::Volume,
            HudKind::Rewind => Icon::Rewind,
        }
    }

    fn max(self) -> u8 {
        match self {
            HudKind::Brightness => BRIGHTNESS_MAX,
            HudKind::BlueLight => BLUE_LIGHT_MAX,
            HudKind::Volume => VOLUME_MAX,
            HudKind::Rewind => 100,
        }
    }
}

/// Fast forward is a mode rather than a moment, so the badge is state the HUD is told and
/// holds, not something it starts a clock on.
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
pub enum FfState {
    #[default]
    Off,
    Held,
    Latched,
}

/// One glyph, never two. The outline and solid variants are the same shape at the same
/// width, so a latch neither widens the badge nor moves it.
pub fn ff_badge(state: FfState) -> Option<Icon> {
    match state {
        FfState::Off => None,
        FfState::Held => Some(Icon::FastForward),
        FfState::Latched => Some(Icon::FastForwardLatched),
    }
}

/// One bar for all three levels, drawn identically whichever it is: the user knows what
/// they just pressed, and three styles would read as three different controls.
#[derive(Default)]
pub struct Hud {
    pub kind: HudKind,
    pub value: u8,
    /// `None` until the first adjustment. A zero here would put a bar on screen for the
    /// first 1.5 s of every boot.
    pub shown_at: Option<Millis>,
    /// The rewind bar answers to the button, not to the clock, so while this is set the fade
    /// never starts.
    held: bool,
    /// Silenced rather than turned down. The bar is empty either way, so this is the only
    /// thing that can tell the two apart on screen.
    muted: bool,
    ff: FfState,
    /// The last thing the HUD said, and when. On its own clock rather than the bar's: the
    /// two are triggered by different actions and either can outlive the other.
    said: Option<(Toast, Millis)>,
    /// One face per icon, in `Icon::ALL` order. Empty until the binary uploads them, since
    /// only the compositor can mint a `TexId`.
    icons: Vec<TexId>,
    /// One face per toast, in `Toast::ALL` order.
    toasts: Vec<TexId>,
}

impl Hud {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_icons(&mut self, icons: Vec<TexId>) {
        self.icons = icons;
    }

    /// In `Toast::ALL` order.
    pub fn set_toasts(&mut self, toasts: Vec<TexId>) {
        self.toasts = toasts;
    }

    pub fn toast(&mut self, toast: Toast, now: Millis) {
        self.said = Some((toast, now));
    }

    pub fn toast_visible(&self, now: Millis) -> bool {
        self.toast_alpha(now) > 0.0
    }

    /// What the HUD is saying, if anything. The app reads this rather than tracking the
    /// grace period twice.
    pub fn said(&self, now: Millis) -> Option<Toast> {
        self.toast_visible(now)
            .then(|| self.said.map(|(t, _)| t))
            .flatten()
    }

    pub fn show(&mut self, kind: HudKind, value: u8, muted: bool, now: Millis) {
        self.kind = kind;
        self.value = value;
        self.muted = muted;
        self.shown_at = Some(now);
        self.held = kind == HudKind::Rewind;
    }

    /// L2 let go. A level bar shown during the rewind is on its own timer and stays.
    pub fn release_rewind(&mut self) {
        if self.kind == HudKind::Rewind {
            self.shown_at = None;
        }
        self.held = false;
    }

    /// The glyph on the bar, whether or not a face for it has been uploaded.
    pub fn glyph(&self) -> Icon {
        self.kind.icon(self.value, self.muted)
    }

    /// The glyph the badge is showing, which is not the same question as what reached the
    /// screen: without an uploaded face it draws nothing at all.
    pub fn badge(&self) -> Option<Icon> {
        ff_badge(self.ff)
    }

    /// Pushed from outside because the badge answers to the gesture machine's latch, which
    /// nothing on this side of the app can see.
    pub fn set_ff(&mut self, ff: FfState) {
        self.ff = ff;
    }

    pub fn visible(&self, now: Millis) -> bool {
        self.alpha(now) > 0.0
    }

    pub fn draw(&self, now: Millis, out: &mut Vec<Draw>) {
        let alpha = self.alpha(now);
        let toast = self.toast_alpha(now);
        // The plate belongs to whatever is being read against it, bar or toast. The badge
        // never gets one: fast forward can be latched for minutes, and a full width plate
        // over the game for all of it is a worse trade than the halo it carries.
        if alpha > 0.0 || toast > 0.0 {
            out.push(Draw::Rect {
                x: 0.0,
                y: 0.0,
                w: OUT_W as f32,
                h: PLATE_H,
                colour: faded(PLATE, alpha.max(toast)),
            });
        }
        // They share the band, so only one at a time. A toast is a specific thing that just
        // happened and outranks a level the user can see the effect of anyway.
        if toast > 0.0 {
            self.draw_toast(now, out);
        } else if alpha > 0.0 {
            self.draw_bar(alpha, out);
        }
        self.draw_badge(ff_badge(self.ff), out);
    }

    fn draw_bar(&self, alpha: f32, out: &mut Vec<Draw>) {
        let x = (OUT_W as f32 - BAR_W) / 2.0;
        if let Some(tex) = self.icon() {
            let (w, h) = icon_box(HUD_ICON_PX);
            out.push(Draw::Tex {
                x: x - ICON_GAP - w as f32,
                y: (PLATE_H - h as f32) / 2.0,
                w: w as f32,
                h: h as f32,
                tex,
                alpha,
            });
        }
        out.push(Draw::Rect {
            x,
            y: BAR_Y,
            w: BAR_W,
            h: BAR_H,
            colour: faded(TRACK, alpha),
        });
        out.push(Draw::Rect {
            x,
            y: BAR_Y,
            w: BAR_W * self.fraction(),
            h: BAR_H,
            colour: faded(FILL, alpha),
        });
    }

    fn draw_badge(&self, badge: Option<Icon>, out: &mut Vec<Draw>) {
        let Some(icon) = badge else {
            return;
        };
        let Some(tex) = self.icons.get(icon.index()).copied() else {
            return;
        };
        let (w, h) = icon_box(HUD_ICON_PX);
        let (w, h) = (w as f32, h as f32);
        out.push(Draw::Tex {
            x: OUT_W as f32 - BADGE_MARGIN - w,
            y: (PLATE_H - h) / 2.0,
            w,
            h,
            tex,
            alpha: 1.0,
        });
    }

    /// No placeholder. A toast is type, and absent type is absent rather than a bar sitting
    /// where two words should be.
    fn draw_toast(&self, now: Millis, out: &mut Vec<Draw>) {
        let alpha = self.toast_alpha(now);
        if alpha <= 0.0 {
            return;
        }
        let Some(tex) = self
            .said
            .and_then(|(t, _)| self.toasts.get(t.index()))
            .copied()
        else {
            return;
        };
        let (x, y, w, h) = toast_rect();
        out.push(Draw::Tex {
            x,
            y,
            w,
            h,
            tex,
            alpha,
        });
    }

    fn icon(&self) -> Option<TexId> {
        self.icons.get(self.glyph().index()).copied()
    }

    fn fraction(&self) -> f32 {
        let max = self.kind.max();
        if max == 0 {
            return 0.0;
        }
        self.value.min(max) as f32 / max as f32
    }

    fn alpha(&self, now: Millis) -> f32 {
        let Some(shown) = self.shown_at else {
            return 0.0;
        };
        if self.held {
            return 1.0;
        }
        fade(shown, now)
    }

    fn toast_alpha(&self, now: Millis) -> f32 {
        match self.said {
            Some((_, shown)) => fade(shown, now),
            None => 0.0,
        }
    }
}

/// One curve for everything the HUD shows, so the bar and the toast leave together when they
/// arrived together.
fn fade(shown: Millis, now: Millis) -> f32 {
    let age = now.saturating_sub(shown);
    if age >= HUD_MS {
        return 0.0;
    }
    ((HUD_MS - age) as f32 / FADE_MS as f32).min(1.0)
}

fn faded(colour: [f32; 4], alpha: f32) -> [f32; 4] {
    [colour[0], colour[1], colour[2], colour[3] * alpha]
}

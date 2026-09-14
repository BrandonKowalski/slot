use std::path::{Path, PathBuf};

use crate::atomic::atomic_write;

pub const BRIGHTNESS_MAX: u8 = 9;
pub const BLUE_LIGHT_MAX: u8 = 9;
pub const VOLUME_MAX: u8 = 100;

/// What a real zone can be, in minutes. The card keeps UTC because the base system's clock
/// and its ntp both assume it; this is the only thing that turns it into the time on the
/// shelf. Minutes rather than hours: several zones are offset by thirty and forty five.
pub const UTC_OFFSET_MIN: i16 = -720;
pub const UTC_OFFSET_MAX: i16 = 840;

/// The fast forward ceilings the quick menu offers, in game frames per screen refresh. One is
/// not fast at all, and above four the row offers adaptive instead of a number.
pub const FF_SPEED_MIN: u8 = 2;
pub const FF_SPEED_MAX: u8 = 4;

/// What `ff_speed` says when the user chose ADAPTIVE: no ceiling of their own, only the
/// emulator's safety cap (`FAST_STEPS_MAX`), which `EmuHandle::set_fast_steps` clamps this down
/// to. Deliberately larger than any ceiling the hardware could serve, so it needs no separate
/// field to travel in and cannot be mistaken for a step count anywhere it is read.
///
/// 255 rather than a new key, because it degrades correctly in the build that matters. Every
/// slot that has ever shipped reads this line as "a number from 2 to 4, anything else is not
/// mine", so a card written by this build and read by an older one falls back to the default,
/// 4× — the fastest fixed ceiling there was, which is the closest thing to adaptive it can do.
/// A card can therefore move between builds without either one finding a speed it cannot
/// explain. The cost, accepted: an older build that goes on to *write* the card spells 4 here,
/// so a round trip through it quietly forgets the choice. A second key would have had the same
/// failure and added a way for the two to disagree with each other.
pub const FF_SPEED_ADAPTIVE: u8 = 255;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SlotState {
    /// Filename stem. `None` is an empty slot, which is the shelf.
    pub cart: Option<String>,
    pub brightness: u8,
    pub blue_light: u8,
    pub volume: u8,
    /// Silence on top of the level rather than instead of it, so unmuting gives back the
    /// number the user last chose.
    pub muted: bool,
    /// Whether anyone has ever confirmed the wall clock. The marker for slot's own first
    /// launch, and the one field a fresh card must read as false.
    pub clock_set: bool,
    /// Minutes to add to the card's UTC to get local time. Zero is a device that never left
    /// Greenwich, which is also what a card that has never been asked reads as.
    pub utc_offset_min: i16,
    /// Whether the motor may move. Off, a game still asks for it and is simply never obeyed.
    pub rumble: bool,
    /// The most game frames a screen refresh runs while fast forwarding: `FF_SPEED_MIN` to
    /// `FF_SPEED_MAX`, or `FF_SPEED_ADAPTIVE` for no ceiling but the emulator's own.
    pub ff_speed: u8,
    /// Whether fast forward is heard, sped up, rather than dropped.
    pub ff_sound: bool,
}

/// Not derived. `read_slot_state` falls back here on a first boot, and all zeroes would
/// be a device with the backlight off and the mixer muted. The quick menu's three settings
/// default to what slot did before they were settings.
impl Default for SlotState {
    fn default() -> Self {
        SlotState {
            cart: None,
            brightness: 5,
            blue_light: 0,
            volume: 60,
            muted: false,
            clock_set: false,
            utc_offset_min: 0,
            rumble: true,
            ff_speed: FF_SPEED_MAX,
            ff_sound: false,
        }
    }
}

fn state_path(root: &Path) -> PathBuf {
    root.join("System").join("slot.state")
}

pub fn read_slot_state(root: &Path) -> SlotState {
    std::fs::read(state_path(root))
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .and_then(|s| parse(&s))
        .unwrap_or_default()
}

pub fn write_slot_state(root: &Path, s: &SlotState) -> std::io::Result<()> {
    let text = format!(
        "cart={}\nbrightness={}\nblue_light={}\nvolume={}\nmuted={}\nclock_set={}\nutc_offset_min={}\nrumble={}\nff_speed={}\nff_sound={}\n",
        s.cart.as_deref().unwrap_or(""),
        s.brightness,
        s.blue_light,
        s.volume,
        s.muted as u8,
        s.clock_set as u8,
        s.utc_offset_min,
        s.rumble as u8,
        s.ff_speed,
        s.ff_sound as u8
    );
    atomic_write(&state_path(root), text.as_bytes())
}

/// The lines every build has written are all or nothing. A file missing one of those, or
/// holding one out of range, is not one we wrote, and inheriting the missing fields from the
/// defaults would hide the corruption behind plausible values.
///
/// Everything else is forgiven. A line this build does not know was written by a later one,
/// and is skipped rather than costing the user their levels and their clock. The quick menu's
/// settings arrived after cards were already in use, so each of those that is missing or
/// unreadable reads as its own default and leaves the rest of the card alone.
fn parse(text: &str) -> Option<SlotState> {
    let mut cart = None;
    let mut brightness = None;
    let mut blue_light = None;
    let mut volume = None;
    let mut muted = None;
    let mut clock_set = None;
    let mut utc_offset_min = None;
    let mut rumble = None;
    let mut ff_speed = None;
    let mut ff_sound = None;
    for line in text.lines().filter(|l| !l.is_empty()) {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "cart" => cart = Some(value.to_string()),
            "brightness" => brightness = Some(level(value, BRIGHTNESS_MAX)?),
            "blue_light" => blue_light = Some(level(value, BLUE_LIGHT_MAX)?),
            "volume" => volume = Some(level(value, VOLUME_MAX)?),
            "muted" => muted = Some(level(value, 1)? == 1),
            "clock_set" => clock_set = Some(level(value, 1)? == 1),
            "utc_offset_min" => utc_offset_min = Some(offset(value)?),
            "rumble" => rumble = flag(value),
            "ff_speed" => ff_speed = ff_speed_value(value),
            "ff_sound" => ff_sound = flag(value),
            _ => {}
        }
    }
    let cart = cart?;
    let fallback = SlotState::default();
    Some(SlotState {
        cart: (!cart.is_empty()).then_some(cart),
        brightness: brightness?,
        blue_light: blue_light?,
        volume: volume?,
        muted: muted?,
        clock_set: clock_set?,
        utc_offset_min: utc_offset_min?,
        rumble: rumble.unwrap_or(fallback.rumble),
        ff_speed: ff_speed.unwrap_or(fallback.ff_speed),
        ff_sound: ff_sound.unwrap_or(fallback.ff_sound),
    })
}

fn offset(value: &str) -> Option<i16> {
    value
        .parse()
        .ok()
        .filter(|n| (UTC_OFFSET_MIN..=UTC_OFFSET_MAX).contains(n))
}

/// A fast forward ceiling the menu offers, or the adaptive sentinel. Anything else was not
/// written by a build of slot and reads as the default, the way every other quick menu setting
/// out of range does.
fn ff_speed_value(value: &str) -> Option<u8> {
    match value.parse().ok()? {
        FF_SPEED_ADAPTIVE => Some(FF_SPEED_ADAPTIVE),
        n if (FF_SPEED_MIN..=FF_SPEED_MAX).contains(&n) => Some(n),
        _ => None,
    }
}

fn level(value: &str, max: u8) -> Option<u8> {
    value.parse().ok().filter(|n| *n <= max)
}

fn flag(value: &str) -> Option<bool> {
    level(value, 1).map(|n| n == 1)
}

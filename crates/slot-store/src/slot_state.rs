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
}

/// Not derived. `read_slot_state` falls back here on a first boot, and all zeroes would
/// be a device with the backlight off and the mixer muted.
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
        "cart={}\nbrightness={}\nblue_light={}\nvolume={}\nmuted={}\nclock_set={}\nutc_offset_min={}\n",
        s.cart.as_deref().unwrap_or(""),
        s.brightness,
        s.blue_light,
        s.volume,
        s.muted as u8,
        s.clock_set as u8,
        s.utc_offset_min
    );
    atomic_write(&state_path(root), text.as_bytes())
}

/// All or nothing. A file we only half recognise is not one we wrote, and inheriting the
/// missing fields from the defaults would hide the corruption behind plausible values.
fn parse(text: &str) -> Option<SlotState> {
    let mut cart = None;
    let mut brightness = None;
    let mut blue_light = None;
    let mut volume = None;
    let mut muted = None;
    let mut clock_set = None;
    let mut utc_offset_min = None;
    for line in text.lines().filter(|l| !l.is_empty()) {
        let (key, value) = line.split_once('=')?;
        match key {
            "cart" => cart = Some(value.to_string()),
            "brightness" => brightness = Some(level(value, BRIGHTNESS_MAX)?),
            "blue_light" => blue_light = Some(level(value, BLUE_LIGHT_MAX)?),
            "volume" => volume = Some(level(value, VOLUME_MAX)?),
            "muted" => muted = Some(level(value, 1)? == 1),
            "clock_set" => clock_set = Some(level(value, 1)? == 1),
            "utc_offset_min" => utc_offset_min = Some(offset(value)?),
            _ => return None,
        }
    }
    let cart = cart?;
    Some(SlotState {
        cart: (!cart.is_empty()).then_some(cart),
        brightness: brightness?,
        blue_light: blue_light?,
        volume: volume?,
        muted: muted?,
        clock_set: clock_set?,
        utc_offset_min: utc_offset_min?,
    })
}

fn offset(value: &str) -> Option<i16> {
    value
        .parse()
        .ok()
        .filter(|n| (UTC_OFFSET_MIN..=UTC_OFFSET_MAX).contains(n))
}

fn level(value: &str, max: u8) -> Option<u8> {
    value.parse().ok().filter(|n| *n <= max)
}

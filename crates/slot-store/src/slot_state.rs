use std::path::{Path, PathBuf};

use crate::atomic::atomic_write;
use crate::platform::Platform;

pub const BRIGHTNESS_MAX: u8 = 9;
pub const BLUE_LIGHT_MAX: u8 = 9;
pub const VOLUME_MAX: u8 = 100;

/// What a real zone can be, in minutes. The card keeps UTC because the base system's clock
/// and its ntp both assume it; this is the only thing that turns it into the time on the
/// shelf. Minutes rather than hours: several zones are offset by thirty and forty five.
pub const UTC_OFFSET_MIN: i16 = -720;
pub const UTC_OFFSET_MAX: i16 = 840;

/// The fast forward ceilings the quick menu offers, in game frames per screen refresh, left to
/// right along the row. These four and nothing else are what `ff_speed` may hold.
///
/// A list rather than a range. The row steps 2, 3, 4, 6 because past four a single frame of
/// difference is not a speed anyone can tell apart, so 5 is not on it — and a `MIN..=MAX`
/// check, which is what this used to be, would have quietly accepted it.
///
/// It stops at six because eight was measured and bought nothing. On mGBA gameplay a ceiling of
/// eight ran 281 game frames a second against six's 280, while presents that ran past 16.67 ms
/// went from 1% to 7% and the loop started a frame it could not finish in 56% of presents
/// rather than 19%. On the heaviest content it changed nothing at all: Pokémon under mGBA held
/// 2.1 frames a present at four, six and eight alike. A row should not offer a ceiling only one
/// core can reach.
///
/// What an older build does with the number six, said plainly rather than left to be found out:
/// every slot that shipped before this row reads this line as "a number from 2 to 4, anything
/// else is not mine", so a card written here at 6 falls back to that build's default, 4×, when
/// read by it. That is acceptable — 4× was the fastest speed it had, so it is as close to what
/// was asked as that build can get, and neither build ever finds a speed it cannot explain. A
/// card written at 8 by a build from the night this row had five ceilings falls back the same
/// way, through the same list, exactly as the 255 an adaptive-era card can still hold. The cost,
/// accepted: an older build that goes on to *write* the card spells 4 here, so a round trip
/// through one forgets the choice.
pub const FF_SPEEDS: [u8; 4] = [2, 3, 4, 6];

/// The speed a card that never chose one gets, and the one the row opens on.
///
/// Six, chosen on the device rather than reasoned about, and now the top of the row as well as
/// its default. Eight was on the row for a few hours and measured worth nothing: the same speed
/// as six on mGBA gameplay, no speed at all on heavy content, and less steady on both. Six is
/// the one that reads as fast without feeling like a different game, and it is a ceiling both
/// cores can actually reach. The old default of 4x was inherited from when gpSP ran its
/// interpreter and could not serve more; the core slot builds now runs its dynarec, so 4x had
/// stopped being what the hardware could do and become merely what it was told.
///
/// This deliberately sits outside the 2..=4 an older build accepts, which the default used to
/// stay inside so the common card read identically everywhere. A fresh card written here reads
/// as 4x on any build that predates this row — the same fallback 6x takes as a choice.
/// Accepted knowingly: the compatibility being given up is with builds nobody runs, and pinning
/// the default to what the oldest build could read would keep a number the hardware outgrew.
pub const FF_SPEED_DEFAULT: u8 = 6;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SlotState {
    /// Filename stem. `None` is an empty slot, which is the shelf.
    pub cart: Option<String>,
    /// Which shelf `cart` stands on, when the card says so. A stem is ambiguous exactly when it
    /// collides across the three platform folders — `Tetris.gb` beside `Tetris.gba` is two
    /// cartridges under one name — and this is the only thing that says which of them was in
    /// the slot.
    ///
    /// `None` is a card that never said, which is every card written before this line existed
    /// and every card written by a build that does not know it. That is deliberately *not* the
    /// same as a stated `Gba`: unstated means "look the stem up across the shelves and take the
    /// first that has it", which is what slot has always done and which puts Game Boy Advance
    /// first, while a stated platform means that shelf and no other. A card written before this
    /// change held only GBA carts, so the old behaviour is what the line meant when it was
    /// written, and it is what a missing line still means.
    pub cart_platform: Option<Platform>,
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
    /// The most game frames a screen refresh runs while fast forwarding: one of `FF_SPEEDS`.
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
            cart_platform: None,
            brightness: 5,
            blue_light: 0,
            volume: 60,
            muted: false,
            clock_set: false,
            utc_offset_min: 0,
            rumble: true,
            ff_speed: FF_SPEED_DEFAULT,
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
        "cart={}\ncart_platform={}\nbrightness={}\nblue_light={}\nvolume={}\nmuted={}\nclock_set={}\nutc_offset_min={}\nrumble={}\nff_speed={}\nff_sound={}\n",
        s.cart.as_deref().unwrap_or(""),
        s.cart_platform.map_or(String::new(), platform_key),
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
    let mut cart_platform = None;
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
            "cart_platform" => cart_platform = platform_value(value),
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
        // Assigned rather than resolved against the fallback, which the three settings below do,
        // because for this one key the fallback *is* `None`: absent is a meaning of its own —
        // "the card never said, look the stem up" — and not a value waiting on a default. It is
        // forgiven in exactly the same way for exactly the same reason: a line this build cannot
        // read costs its own answer and takes nothing else on the card with it.
        cart_platform,
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

/// How a platform is spelled on this line: its own directory name in lower case, so the file
/// says `cart_platform=gb` for the cart that lives in `Games/GB/`. Built from `dir_name` rather
/// than from a second list of three strings, which is what stops the state file and the card's
/// own folders coming to disagree about what a platform is called.
fn platform_key(platform: Platform) -> String {
    platform.dir_name().to_ascii_lowercase()
}

/// The reverse, and forgiving of case for the same reason the extension check is: the line is
/// plain text on a card anyone can open in an editor, and `GB` means what `gb` means. Anything
/// that is not one of the three — an empty value, a platform a later build added, a typo — is
/// `None`, which reads as a card that never said.
fn platform_value(value: &str) -> Option<Platform> {
    Platform::ALL
        .into_iter()
        .find(|p| value.eq_ignore_ascii_case(p.dir_name()))
}

fn offset(value: &str) -> Option<i16> {
    value
        .parse()
        .ok()
        .filter(|n| (UTC_OFFSET_MIN..=UTC_OFFSET_MAX).contains(n))
}

/// A fast forward ceiling the menu offers. Anything else was not written by a build of slot and
/// reads as the default, the way every other quick menu setting out of range does — 5 and 7
/// included, which sit between the row's ends without being on it.
fn ff_speed_value(value: &str) -> Option<u8> {
    value.parse().ok().filter(|n| FF_SPEEDS.contains(n))
}

fn level(value: &str, max: u8) -> Option<u8> {
    value.parse().ok().filter(|n| *n <= max)
}

fn flag(value: &str) -> Option<bool> {
    level(value, 1).map(|n| n == 1)
}

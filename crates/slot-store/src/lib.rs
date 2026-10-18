mod atomic;
mod bindings;
pub mod cart_shell;
mod config;
mod core;
pub mod gb;
mod gb_palette;
mod gba;
pub mod ini;
mod platform;
mod ring;
mod scan;
mod slot_state;
mod stamp;

pub use atomic::atomic_write;
pub use bindings::{
    read_carousel, read_controls, read_shortcuts, write_carousel, write_controls, write_shortcuts,
    Binding, CarouselAction, CarouselKeys, Controls, Device, GameButton, Modifier, Shortcut,
    ShortcutKeys,
};
pub use cart_shell::{Outline, ShellChoice, ShellFinish, CART_SHELL_FILE, LABELS_SHELL_FILE};
pub use config::{move_config, CONFIG_DIR};
pub use core::{
    core_for, core_for_platform, read_selected_cores, write_selected_core, Core, SELECTED_CORE_FILE,
};
pub use gb_palette::GbPalette;
pub use gba::{header_clean, header_code, header_title, header_title_code};
pub use platform::Platform;
pub use ring::{StateEntry, StateRing, RING_MAX};
pub use scan::{initial, is_hidden, listing, scan, sort_key, Cart, StoreError};
pub use slot_state::{
    read_slot_state, write_slot_state, Appearance, BezelPick, Recent, Shader, SlotState,
    WallpaperPick, WallpaperTint, BLUE_LIGHT_MAX, BRIGHTNESS_MAX, FF_SPEEDS, FF_SPEED_DEFAULT,
    LID_OFF_MIN, RECENTS_MAX, UTC_OFFSET_MAX, UTC_OFFSET_MIN, VOLUME_MAX,
};
pub use stamp::{
    civil_from_days, days_from_civil, days_in_month, format_stamp, parse_stamp, stamp_now,
};

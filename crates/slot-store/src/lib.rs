mod atomic;
mod core;
mod gba;
mod migrate;
mod ring;
mod scan;
mod slot_state;
mod stamp;
mod theme;

pub use atomic::atomic_write;
pub use core::{core_for, read_selected_cores, write_selected_core, Core, SELECTED_CORE_FILE};
pub use gba::{header_clean, header_code, header_title};
pub use migrate::{migrate_states, MigrationReport};
pub use ring::{StateEntry, StateRing, RING_MAX};
pub use scan::{is_hidden, scan, Cart, StoreError};
pub use slot_state::{
    read_slot_state, write_slot_state, SlotState, BLUE_LIGHT_MAX, BRIGHTNESS_MAX,
    FF_SPEED_ADAPTIVE, FF_SPEED_MAX, FF_SPEED_MIN, UTC_OFFSET_MAX, UTC_OFFSET_MIN, VOLUME_MAX,
};
pub use stamp::{
    civil_from_days, days_from_civil, days_in_month, format_stamp, parse_stamp, stamp_now,
};
pub use theme::{Theme, THEME_FILE};

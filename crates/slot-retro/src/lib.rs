mod core;
mod ffi;
mod mgba;
mod mock;
mod rumble;

pub use core::{AvInfo, ButtonMask, CoreError, RetroCore, GBA_H, GBA_W};
pub use mgba::MgbaCore;
pub use mock::MockCore;
pub use rumble::Rumble;

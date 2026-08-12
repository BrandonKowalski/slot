//! Chrome geometry is declared by the compositor that consumes it, so UI code and GL code
//! cannot drift into two shapes of the same list.

pub use slot_gfx::{Draw, TexId, OUT_H, OUT_W};

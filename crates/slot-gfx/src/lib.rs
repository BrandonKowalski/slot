mod draw;
mod fbdev;
mod fbo;
mod gl;
mod grade;
#[cfg(target_os = "macos")]
mod headless;
#[cfg(feature = "host")]
mod host;
mod lcd3x;
mod pipeline;
mod power;
mod quad;
mod shaders;
mod surface;

pub use draw::{Draw, TexId};
// Built on the host too, so the port stays under the type checker and the linter that only
// ever run there. Opening it away from the device fails at the first dlopen, not at compile.
pub use fbdev::{egl_error, panel_mode, panel_size, FbdevSurface};
pub use fbo::{Compositor, BACKDROP};
pub use grade::{blue_light_gain, BLUE_LIGHT_MAX};
#[cfg(target_os = "macos")]
pub use headless::HeadlessSurface;
#[cfg(feature = "host")]
pub use host::HostSurface;
pub use lcd3x::{lcd3x_mask, mask_texture_rgba8};
pub use pipeline::{SCALE, SRC_H, SRC_W};
pub use power::{screen_brightness, screen_scale, screen_width};
pub use surface::{blit_rect, blit_rect_fit, fit_rect, fit_scale, GfxError, Surface, OUT_H, OUT_W};

use crate::surface::{GfxError, Surface};
use glutin::config::{Config, ConfigTemplateBuilder, GlConfig};
use glutin::context::{
    ContextApi, ContextAttributesBuilder, GlProfile, NotCurrentGlContext, PossiblyCurrentContext,
    PossiblyCurrentGlContext, Version,
};
use glutin::display::{Display, GetGlDisplay, GlDisplay};
use glutin::surface::{GlSurface, SwapInterval, WindowSurface};
use glutin_winit::{DisplayBuilder, GlWindow};
use raw_window_handle::HasWindowHandle;
use std::ffi::{c_void, CString};
use std::num::NonZeroU32;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

/// Two times the 720x480 output, the size the device panel maps to on a desktop.
const DEFAULT_W: u32 = 1440;
const DEFAULT_H: u32 = 960;

pub struct HostSurface {
    window: Window,
    display: Display,
    gl_surface: glutin::surface::Surface<WindowSurface>,
    context: PossiblyCurrentContext,
}

fn err<E: std::fmt::Display>(e: E) -> GfxError {
    GfxError::Context(e.to_string())
}

/// `SLOT_BARE=1` drops the title bar and the border, leaving nothing on screen but the
/// panel. For screenshots and capture, where a macOS title bar is the only thing between
/// a window and a picture of the device.
///
/// Off by default, and deliberately not the only way to run: an undecorated window
/// cannot be dragged, resized or closed with the mouse, so it is quit with Cmd+Q.
fn bare() -> bool {
    std::env::var_os("SLOT_BARE").is_some_and(|v| v != "0")
}

impl HostSurface {
    pub fn new(events: &ActiveEventLoop) -> Result<Self, GfxError> {
        let mut attrs = Window::default_attributes()
            .with_title("slot.")
            .with_inner_size(winit::dpi::PhysicalSize::new(DEFAULT_W, DEFAULT_H));
        if bare() {
            attrs = attrs.with_decorations(false);
        }
        let (window, config) = DisplayBuilder::new()
            .with_window_attributes(Some(attrs))
            .build(events, ConfigTemplateBuilder::new(), pick_config)
            .map_err(err)?;
        let window = window.ok_or_else(|| GfxError::Context("no window was created".into()))?;

        let display = config.display();
        let handle = window.window_handle().map_err(err)?.as_raw();
        let context_attrs = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
            .with_profile(GlProfile::Core)
            .build(Some(handle));
        let context = unsafe { display.create_context(&config, &context_attrs) }.map_err(err)?;

        let surface_attrs = window
            .build_surface_attributes(Default::default())
            .map_err(err)?;
        let gl_surface =
            unsafe { display.create_window_surface(&config, &surface_attrs) }.map_err(err)?;
        let context = context.make_current(&gl_surface).map_err(err)?;

        // Present is locked to the panel; the 0.456% GBA-to-panel drift is absorbed by
        // audio rate control, never by dropping vsync.
        let _ = gl_surface.set_swap_interval(&context, SwapInterval::Wait(NonZeroU32::MIN));

        Ok(HostSurface {
            window,
            display,
            gl_surface,
            context,
        })
    }

    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    pub fn resize(&self, size: winit::dpi::PhysicalSize<u32>) {
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        self.gl_surface.resize(&self.context, w, h);
    }
}

fn pick_config(configs: Box<dyn Iterator<Item = Config> + '_>) -> Config {
    let mut best: Option<Config> = None;
    for c in configs {
        let better = match &best {
            None => true,
            Some(b) => c.num_samples() < b.num_samples(),
        };
        if better {
            best = Some(c);
        }
    }
    // The iterator is non empty whenever `build` succeeds, so this cannot be reached.
    best.unwrap_or_else(|| unreachable!("glutin yielded no configs"))
}

impl Surface for HostSurface {
    fn make_current(&mut self) -> Result<(), GfxError> {
        self.context.make_current(&self.gl_surface).map_err(err)
    }

    fn window_size(&self) -> (u32, u32) {
        let s = self.window.inner_size();
        (s.width, s.height)
    }

    fn swap(&mut self) -> Result<(), GfxError> {
        self.gl_surface.swap_buffers(&self.context).map_err(err)
    }

    fn proc_address(&self, name: &str) -> *const c_void {
        match CString::new(name) {
            Ok(c) => self.display.get_proc_address(&c),
            Err(_) => std::ptr::null(),
        }
    }
}

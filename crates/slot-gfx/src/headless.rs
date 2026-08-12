//! An offscreen GL context, for readback checks that must not open a window. CGL is used
//! directly rather than through glutin because glutin's macOS path hops to the main thread,
//! which a test harness never services.

use crate::surface::{GfxError, Surface, OUT_H, OUT_W};
use libloading::Library;
use std::ffi::{c_void, CString};

const FRAMEWORK: &str = "/System/Library/Frameworks/OpenGL.framework/OpenGL";

const PFA_COLOR_SIZE: i32 = 8;
const PFA_ALPHA_SIZE: i32 = 11;
const PFA_OPENGL_PROFILE: i32 = 99;
const PROFILE_GL4_CORE: i32 = 0x4100;

type PixelFormatObj = *mut c_void;
type ContextObj = *mut c_void;

#[link(name = "OpenGL", kind = "framework")]
extern "C" {
    fn CGLChoosePixelFormat(attrs: *const i32, pix: *mut PixelFormatObj, npix: *mut i32) -> i32;
    fn CGLDestroyPixelFormat(pix: PixelFormatObj) -> i32;
    fn CGLCreateContext(pix: PixelFormatObj, share: ContextObj, ctx: *mut ContextObj) -> i32;
    fn CGLDestroyContext(ctx: ContextObj) -> i32;
    fn CGLSetCurrentContext(ctx: ContextObj) -> i32;
}

pub struct HeadlessSurface {
    context: ContextObj,
    lib: Library,
}

impl HeadlessSurface {
    /// Creates the context and makes it current on the calling thread. A CGL context is
    /// per thread, so it must be used from the thread that built it.
    pub fn new() -> Result<Self, GfxError> {
        let lib = unsafe { Library::new(FRAMEWORK) }
            .map_err(|e| GfxError::Context(format!("opengl framework: {e}")))?;
        let attrs = [
            PFA_OPENGL_PROFILE,
            PROFILE_GL4_CORE,
            PFA_COLOR_SIZE,
            24,
            PFA_ALPHA_SIZE,
            8,
            0,
        ];
        let mut pix: PixelFormatObj = std::ptr::null_mut();
        let mut count = 0;
        let mut context: ContextObj = std::ptr::null_mut();
        unsafe {
            if CGLChoosePixelFormat(attrs.as_ptr(), &mut pix, &mut count) != 0 || pix.is_null() {
                return Err(GfxError::Context("no core profile pixel format".into()));
            }
            let created = CGLCreateContext(pix, std::ptr::null_mut(), &mut context);
            CGLDestroyPixelFormat(pix);
            if created != 0 || context.is_null() {
                return Err(GfxError::Context(format!("cgl context: error {created}")));
            }
            if CGLSetCurrentContext(context) != 0 {
                CGLDestroyContext(context);
                return Err(GfxError::Context(
                    "could not make the context current".into(),
                ));
            }
        }
        Ok(HeadlessSurface { context, lib })
    }
}

impl Surface for HeadlessSurface {
    fn make_current(&mut self) -> Result<(), GfxError> {
        match unsafe { CGLSetCurrentContext(self.context) } {
            0 => Ok(()),
            e => Err(GfxError::Context(format!("cgl make current: error {e}"))),
        }
    }

    fn window_size(&self) -> (u32, u32) {
        (OUT_W, OUT_H)
    }

    fn swap(&mut self) -> Result<(), GfxError> {
        unsafe { gl::Flush() };
        Ok(())
    }

    fn proc_address(&self, name: &str) -> *const c_void {
        let Ok(sym) = CString::new(name) else {
            return std::ptr::null();
        };
        unsafe {
            match self.lib.get::<*const c_void>(sym.as_bytes_with_nul()) {
                Ok(f) => f.into_raw().into_raw() as *const c_void,
                Err(_) => std::ptr::null(),
            }
        }
    }
}

impl Drop for HeadlessSurface {
    fn drop(&mut self) {
        unsafe {
            CGLSetCurrentContext(std::ptr::null_mut());
            CGLDestroyContext(self.context);
        }
    }
}

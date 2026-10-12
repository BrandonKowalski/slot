use crate::power::{screen_brightness, screen_rect};
use crate::quad::Quad;
use crate::shaders::{RECT_VERT, VIDEO_FRAG};
use crate::surface::{GfxError, OUT_H, OUT_W};

const TEX: u32 = 1024;

/// How a frame that is bigger than the 240x160 panel texture is scaled to the screen.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum VideoFilter {
    Crisp,
    Sharp,
    Smooth,
    Scanlines,
    Crt,
}

impl VideoFilter {
    fn mode(self) -> f32 {
        self as u8 as f32
    }
}

/// Draws a game frame at its own size, scaled by the GPU, with the picture kept at 4:3.
pub struct VideoPass {
    prog: gl::types::GLuint,
    tex: gl::types::GLuint,
    u_rect: gl::types::GLint,
    u_bright: gl::types::GLint,
    u_frame: gl::types::GLint,
    u_scale: gl::types::GLint,
    u_mode: gl::types::GLint,
    u_frac: gl::types::GLint,
    frame: (u32, u32),
}

impl VideoPass {
    pub fn new() -> Result<Self, GfxError> {
        let prog = crate::shaders::program(RECT_VERT, VIDEO_FRAG)?;
        let tex = crate::gl::texture(TEX, TEX, gl::LINEAR, gl::CLAMP_TO_EDGE, gl::BGRA, None);
        let (u_rect, u_bright, u_frame, u_scale, u_mode, u_frac);
        unsafe {
            gl::UseProgram(prog);
            gl::Uniform1i(crate::gl::uniform_location(prog, "u_game"), 0);
            gl::Uniform2f(
                crate::gl::uniform_location(prog, "u_target"),
                OUT_W as f32,
                OUT_H as f32,
            );
            gl::Uniform2f(
                crate::gl::uniform_location(prog, "u_tex"),
                TEX as f32,
                TEX as f32,
            );
            u_rect = crate::gl::uniform_location(prog, "u_rect");
            u_bright = crate::gl::uniform_location(prog, "u_bright");
            u_frame = crate::gl::uniform_location(prog, "u_frame");
            u_scale = crate::gl::uniform_location(prog, "u_scale");
            u_mode = crate::gl::uniform_location(prog, "u_mode");
            u_frac = crate::gl::uniform_location(prog, "u_frac");
        }
        Ok(VideoPass {
            prog,
            tex,
            u_rect,
            u_bright,
            u_frame,
            u_scale,
            u_mode,
            u_frac,
            frame: (0, 0),
        })
    }

    pub fn has_frame(&self) -> bool {
        self.frame.0 > 0 && self.frame.1 > 0
    }

    pub fn upload(&mut self, w: u32, h: u32, bgra: &[u8]) {
        if w == 0 || h == 0 || w > TEX || h > TEX || bgra.len() < (w * h * 4) as usize {
            return;
        }
        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, self.tex);
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
            gl::TexSubImage2D(
                gl::TEXTURE_2D,
                0,
                0,
                0,
                w as i32,
                h as i32,
                gl::BGRA,
                gl::UNSIGNED_BYTE,
                bgra.as_ptr() as *const std::ffi::c_void,
            );
        }
        self.frame = (w, h);
    }

    pub fn draw(&self, quad: &Quad, filter: VideoFilter, fill: bool, power: f32, fbo: [f32; 2]) {
        if !self.has_frame() {
            return;
        }
        let (x, y, w, h) = screen_rect(power);
        let frac = match fill {
            true => 1.0,
            false => ((OUT_H as f32 * 4.0 / 3.0) / OUT_W as f32).min(1.0),
        };
        let scale_x = frac * OUT_W as f32 * fbo[0] / self.frame.0 as f32;
        let scale_y = OUT_H as f32 * fbo[1] / self.frame.1 as f32;
        unsafe {
            gl::UseProgram(self.prog);
            gl::Uniform4f(self.u_rect, x, y, w, h);
            gl::Uniform1f(self.u_bright, screen_brightness(power));
            gl::Uniform2f(self.u_frame, self.frame.0 as f32, self.frame.1 as f32);
            gl::Uniform2f(self.u_scale, scale_x, scale_y);
            gl::Uniform1f(self.u_mode, filter.mode());
            gl::Uniform1f(self.u_frac, frac);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.tex);
        }
        quad.draw();
    }
}

impl Drop for VideoPass {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteTextures(1, &self.tex);
            gl::DeleteProgram(self.prog);
        }
    }
}

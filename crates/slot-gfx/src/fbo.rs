use crate::draw::{Draw, Sprites, TexId};
use crate::grade::blue_light_gain;
use crate::pipeline::GamePass;
use crate::quad::Quad;
use crate::shaders::{BLIT_FRAG, BLIT_VERT};
use crate::surface::{blit_rect, GfxError, Surface, OUT_H, OUT_W};

/// Backdrop behind everything drawn into the offscreen target. Distinct from the black
/// letterbox so the blit rect is visible even with nothing else on screen.
/// Black. A grey backdrop was within 6/765 of the default cart shell, which made every
/// ordinary cart on the shelf invisible against it. Black also lets a coloured shell read as
/// plastic rather than as a tinted panel.
pub const BACKDROP: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

pub struct Compositor {
    fbo: gl::types::GLuint,
    tex: gl::types::GLuint,
    blit: gl::types::GLuint,
    u_gain: gl::types::GLint,
    gain: [f32; 3],
    shake: f32,
    quad: Quad,
    game: GamePass,
    sprites: Sprites,
}

impl Compositor {
    pub fn new(surface: &dyn Surface) -> Result<Self, GfxError> {
        crate::gl::load(surface);
        let blit = crate::shaders::program(BLIT_VERT, BLIT_FRAG)?;
        // Nearest and clamped: the blit is an integer multiply, never a resample.
        let tex = crate::gl::texture(OUT_W, OUT_H, gl::NEAREST, gl::CLAMP_TO_EDGE, gl::RGBA, None);
        unsafe {
            let mut fbo = 0;
            gl::GenFramebuffers(1, &mut fbo);
            gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                tex,
                0,
            );
            let status = gl::CheckFramebufferStatus(gl::FRAMEBUFFER);
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            if status != gl::FRAMEBUFFER_COMPLETE {
                gl::DeleteFramebuffers(1, &fbo);
                gl::DeleteTextures(1, &tex);
                gl::DeleteProgram(blit);
                return Err(GfxError::Framebuffer(status));
            }
            gl::UseProgram(blit);
            gl::Uniform1i(crate::gl::uniform_location(blit, "u_tex"), 0);
            let u_gain = crate::gl::uniform_location(blit, "u_gain");

            Ok(Compositor {
                fbo,
                tex,
                blit,
                u_gain,
                gain: blue_light_gain(0),
                shake: 0.0,
                quad: Quad::new(),
                game: GamePass::new()?,
                sprites: Sprites::new()?,
            })
        }
    }

    pub fn upload_game(&mut self, xrgb8888: &[u8]) {
        self.game.upload(xrgb8888);
    }

    pub fn draw_game(&mut self) {
        self.game.draw(&self.quad);
    }

    /// Sprites in order, with the game pass drawn wherever the list asks for it. The split is
    /// what lets the picture come up over a seated cart and stay under the HUD.
    pub fn draw_list(&mut self, items: &[Draw]) {
        let mut from = 0;
        for (i, item) in items.iter().enumerate() {
            match *item {
                Draw::Game => {
                    self.sprites.draw(&items[from..i], &self.quad);
                    self.game.draw(&self.quad);
                }
                Draw::Shot { tex } => {
                    self.sprites.draw(&items[from..i], &self.quad);
                    // A shot naming a texture nobody made draws nothing, as a sprite does.
                    if let Some(tex) = self.sprites.source(tex) {
                        self.game.draw_still(tex, &self.quad);
                    }
                }
                _ => continue,
            }
            from = i + 1;
        }
        self.sprites.draw(&items[from..], &self.quad);
    }

    pub fn create_texture(&mut self, w: u32, h: u32, rgba: &[u8]) -> TexId {
        self.sprites.create_texture(w, h, rgba)
    }

    pub fn create_texture_nearest(&mut self, w: u32, h: u32, rgba: &[u8]) -> TexId {
        self.sprites.create_texture_nearest(w, h, rgba)
    }

    pub fn update_texture(&mut self, id: TexId, w: u32, h: u32, rgba: &[u8]) {
        self.sprites.update_texture(id, w, h, rgba);
    }

    pub fn set_blue_light(&mut self, step: u8) {
        self.gain = blue_light_gain(step);
    }

    /// 0.0 dark, 1.0 fully on. Scales and brightens the game layer, nothing else: the chrome
    /// stays where it is while the picture blooms out from behind it.
    pub fn set_screen_power(&mut self, t: f32) {
        self.game.set_power(t);
    }

    /// Pixels, in offscreen space, applied to the whole presented image. On the blit rather
    /// than on the draw list, so game, chrome and HUD move together as one picture. Applied
    /// inside the offscreen target it would shake the chrome against a game that stayed
    /// still. Edges reveal the letterbox for the duration, which is what a jolt looks like.
    pub fn set_shake(&mut self, dx: f32) {
        self.shake = dx;
    }

    /// The offscreen target, top row first. glReadPixels hands back the framebuffer in
    /// memory order, which is bottom up.
    pub fn read_frame(&self) -> Vec<u8> {
        let stride = OUT_W as usize * 4;
        let mut buf = vec![0u8; stride * OUT_H as usize];
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.fbo);
            gl::PixelStorei(gl::PACK_ALIGNMENT, 1);
            gl::ReadPixels(
                0,
                0,
                OUT_W as i32,
                OUT_H as i32,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                buf.as_mut_ptr() as *mut std::ffi::c_void,
            );
        }
        let mut top_down = Vec::with_capacity(buf.len());
        for row in buf.chunks_exact(stride).rev() {
            top_down.extend_from_slice(row);
        }
        top_down
    }

    pub fn begin_frame(&mut self) {
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.fbo);
            gl::Viewport(0, 0, OUT_W as i32, OUT_H as i32);
            gl::ClearColor(BACKDROP[0], BACKDROP[1], BACKDROP[2], BACKDROP[3]);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }
    }

    pub fn end_frame(&mut self, window: (u32, u32)) {
        let (x, y, w, h) = blit_rect(window, self.shake);
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::Viewport(0, 0, window.0 as i32, window.1 as i32);
            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
            gl::Viewport(x, y, w, h);
            gl::UseProgram(self.blit);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.tex);
            gl::Uniform3f(self.u_gain, self.gain[0], self.gain[1], self.gain[2]);
        }
        self.quad.draw();
    }
}

impl Drop for Compositor {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteFramebuffers(1, &self.fbo);
            gl::DeleteTextures(1, &self.tex);
            gl::DeleteProgram(self.blit);
        }
    }
}

use crate::draw::{Draw, Sprites, TexId};
use crate::grade::blue_light_gain;
use crate::pipeline::GamePass;
use crate::quad::Quad;
use crate::shaders::{BEZEL_FRAG, BLIT_FRAG, BLIT_VERT};
use crate::surface::{blit_rect, GfxError, Surface, OUT_H, OUT_W};

pub const BACKDROP: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

pub struct Compositor {
    fbo: gl::types::GLuint,
    tex: gl::types::GLuint,
    size: (u32, u32),
    blit: gl::types::GLuint,
    u_gain: gl::types::GLint,
    u_blit_uv: gl::types::GLint,
    split: Option<f32>,
    gain: [f32; 3],
    shake: f32,
    lift: f32,
    screen: f32,
    quad: Quad,
    game: GamePass,
    sprites: Sprites,
    bezel_prog: gl::types::GLuint,
    u_bezel_gain: gl::types::GLint,
    u_bezel_alpha: gl::types::GLint,
    bezel: Option<gl::types::GLuint>,
}

impl Compositor {
    pub fn new(surface: &dyn Surface) -> Result<Self, GfxError> {
        crate::gl::load(surface);
        let blit = crate::shaders::program(BLIT_VERT, BLIT_FRAG)?;
        let bezel_prog = crate::shaders::program(BLIT_VERT, BEZEL_FRAG)?;
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
                gl::DeleteProgram(bezel_prog);
                return Err(GfxError::Framebuffer(status));
            }
            gl::UseProgram(blit);
            gl::Uniform1i(crate::gl::uniform_location(blit, "u_tex"), 0);
            let u_gain = crate::gl::uniform_location(blit, "u_gain");
            let u_blit_uv = crate::gl::uniform_location(blit, "u_uv");
            gl::Uniform4f(u_blit_uv, 0.0, 0.0, 1.0, 1.0);
            gl::UseProgram(bezel_prog);
            gl::Uniform1i(crate::gl::uniform_location(bezel_prog, "u_tex"), 0);
            let u_bezel_gain = crate::gl::uniform_location(bezel_prog, "u_gain");
            let u_bezel_alpha = crate::gl::uniform_location(bezel_prog, "u_alpha");
            gl::Uniform4f(
                crate::gl::uniform_location(bezel_prog, "u_uv"),
                0.0,
                0.0,
                1.0,
                1.0,
            );

            Ok(Compositor {
                fbo,
                tex,
                size: (OUT_W, OUT_H),
                blit,
                u_gain,
                u_blit_uv,
                split: None,
                gain: blue_light_gain(0),
                shake: 0.0,
                lift: 1.0,
                screen: 0.0,
                quad: Quad::new(),
                game: GamePass::new()?,
                sprites: Sprites::new()?,
                bezel_prog,
                u_bezel_gain,
                u_bezel_alpha,
                bezel: None,
            })
        }
    }

    pub fn upload_game(&mut self, xrgb8888: &[u8]) {
        self.game.upload(xrgb8888);
    }

    pub fn draw_game(&mut self) {
        self.game.draw(&self.quad);
    }

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

    pub fn set_screen_effect(&mut self, effect: crate::ScreenEffect) {
        self.game.set_effect(effect);
    }

    pub fn set_picture(&mut self, rect: [f32; 4]) {
        self.game.set_picture(rect);
    }

    pub fn set_paper(&mut self, size: u32, rgba: &[u8]) {
        self.game.set_paper(size, rgba);
    }

    pub fn fit(&mut self, window: (u32, u32)) {
        let size = canvas_size(window);
        if size == self.size {
            return;
        }
        let tex = crate::gl::texture(
            size.0,
            size.1,
            gl::NEAREST,
            gl::CLAMP_TO_EDGE,
            gl::RGBA,
            None,
        );
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.fbo);
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                tex,
                0,
            );
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::DeleteTextures(1, &self.tex);
        }
        self.tex = tex;
        self.size = size;
    }

    pub fn set_frame_split(&mut self, canvas_y: Option<f32>) {
        self.split = canvas_y;
    }

    pub fn set_frame_lift(&mut self, t: f32) {
        self.lift = t;
    }

    pub fn set_screen_power(&mut self, t: f32) {
        self.screen = t;
        self.game.set_power(t);
    }

    pub fn set_game_source_rect(&mut self, rect: [f32; 4]) {
        self.game.set_source_rect(rect);
    }

    pub fn set_shake(&mut self, dx: f32) {
        self.shake = dx;
    }

    pub fn read_frame(&self) -> Vec<u8> {
        let (w, h) = self.size;
        let stride = w as usize * 4;
        let mut buf = vec![0u8; stride * h as usize];
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.fbo);
            gl::PixelStorei(gl::PACK_ALIGNMENT, 1);
            gl::ReadPixels(
                0,
                0,
                w as i32,
                h as i32,
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
        self.game.set_fbo_scale(
            self.size.0 as f32 / OUT_W as f32,
            self.size.1 as f32 / OUT_H as f32,
        );
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.fbo);
            gl::Viewport(0, 0, self.size.0 as i32, self.size.1 as i32);
            gl::ClearColor(BACKDROP[0], BACKDROP[1], BACKDROP[2], BACKDROP[3]);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }
    }

    pub fn set_bezel(&mut self, w: u32, h: u32, rgba: &[u8]) {
        let stride = w as usize * 4;
        let mut bottom_up = Vec::with_capacity(rgba.len());
        for row in rgba.chunks_exact(stride).rev() {
            bottom_up.extend_from_slice(row);
        }
        let tex = crate::gl::texture(
            w,
            h,
            gl::LINEAR,
            gl::CLAMP_TO_EDGE,
            gl::RGBA,
            Some(&bottom_up),
        );
        if let Some(old) = self.bezel.replace(tex) {
            unsafe { gl::DeleteTextures(1, &old) };
        }
    }

    pub fn end_frame(&mut self, window: (u32, u32)) {
        let framed = crate::bezel::framed(window);
        let (x, mut y, w, h) = blit_rect(window, self.shake);
        let drop = if framed {
            crate::bezel::lifted_rect(window, self.lift).1
        } else {
            0
        };
        y -= drop;
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::Viewport(0, 0, window.0 as i32, window.1 as i32);
            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
            gl::UseProgram(self.blit);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::Uniform3f(self.u_gain, self.gain[0], self.gain[1], self.gain[2]);
        }
        unsafe { gl::BindTexture(gl::TEXTURE_2D, self.tex) };
        let cut = self
            .split
            .filter(|_| framed && drop == 0)
            .map(|c| (c * h as f32 / OUT_H as f32).round() as i32)
            .filter(|c| (1..h).contains(c));
        match cut {
            Some(cut) => {
                let ch = h as f32;
                let gap = window.1 as i32 - h;
                let below = h - cut;
                let top = window.1 as i32 - cut;
                self.blit_part(
                    x,
                    top,
                    w,
                    cut,
                    [0.0, below as f32 / ch, 1.0, cut as f32 / ch],
                );
                self.blit_part(x, 0, w, below, [0.0, 0.0, 1.0, below as f32 / ch]);
                let row = (below as f32 + 0.5) / ch;
                self.blit_part(x, below, w, gap, [0.0, row, 1.0, 0.0]);
                unsafe { gl::Uniform4f(self.u_blit_uv, 0.0, 0.0, 1.0, 1.0) };
            }
            None => self.blit_part(x, y, w, h, [0.0, 0.0, 1.0, 1.0]),
        }
        let Some(bezel) = self.bezel.filter(|_| framed && self.screen > 0.0) else {
            return;
        };
        let alpha = self.screen.clamp(0.0, 1.0);
        unsafe {
            gl::Viewport(0, -drop, window.0 as i32, window.1 as i32);
            gl::UseProgram(self.bezel_prog);
            gl::Uniform3f(self.u_bezel_gain, self.gain[0], self.gain[1], self.gain[2]);
            gl::Uniform1f(self.u_bezel_alpha, alpha);
            gl::BindTexture(gl::TEXTURE_2D, bezel);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        }
        self.quad.draw();
        unsafe { gl::Disable(gl::BLEND) };
    }
}

impl Compositor {
    fn blit_part(&self, x: i32, y: i32, w: i32, h: i32, uv: [f32; 4]) {
        unsafe {
            gl::Viewport(x, y, w, h);
            gl::Uniform4f(self.u_blit_uv, uv[0], uv[1], uv[2], uv[3]);
        }
        self.quad.draw();
    }
}

pub fn canvas_size(window: (u32, u32)) -> (u32, u32) {
    let (_, _, w, h) = blit_rect(window, 0.0);
    let (w, h) = (w.max(1) as u32, h.max(1) as u32);
    if w % OUT_W == 0 && h % OUT_H == 0 && w / OUT_W == h / OUT_H {
        (OUT_W, OUT_H)
    } else {
        (w, h)
    }
}

impl Drop for Compositor {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteFramebuffers(1, &self.fbo);
            gl::DeleteTextures(1, &self.tex);
            gl::DeleteProgram(self.blit);
            gl::DeleteProgram(self.bezel_prog);
            if let Some(tex) = self.bezel {
                gl::DeleteTextures(1, &tex);
            }
        }
    }
}

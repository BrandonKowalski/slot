use crate::quad::Quad;
use crate::shaders::{RECT_VERT, SPRITE_FRAG};
use crate::surface::{GfxError, OUT_H, OUT_W};

/// Handle to a texture the compositor owns. UI code never sees a GL name.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct TexId(usize);

#[cfg(feature = "test-support")]
impl TexId {
    /// A handle by index, for callers that need to name a texture without a live context —
    /// tests and tools. Gated behind `test-support`, which no consumer enables outside its
    /// own `[dev-dependencies]`, so a forged handle cannot reach the device build and alias
    /// whatever real texture happens to sit at that index.
    pub const fn from_raw(id: usize) -> Self {
        TexId(id)
    }
}

/// Chrome geometry, in offscreen pixels with the origin at the top left.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Draw {
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        colour: [f32; 4],
    },
    Tex {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        tex: TexId,
        alpha: f32,
    },
    /// Where the game layer goes. It carries no geometry: the pass owns its own rect, since
    /// the power on squeezes it. A marker rather than a pass of its own before the list,
    /// because the panel is the front surface of the device and has to be able to come up
    /// over a cart that is already in the slot.
    Game,
    /// A 240x160 still on the panel, through the game pass. Geometry comes from the pass for
    /// the same reason `Game`'s does: it is the same screen, showing a picture instead of a
    /// live frame, so it wears the same mask at the same scale.
    Shot { tex: TexId },
}

/// One program for both variants: a rect is a textured quad sampling a white texel, so a
/// draw list never has to change program mid list.
pub struct Sprites {
    prog: gl::types::GLuint,
    white: gl::types::GLuint,
    textures: Vec<gl::types::GLuint>,
    u_rect: gl::types::GLint,
    u_colour: gl::types::GLint,
}

impl Sprites {
    pub fn new() -> Result<Self, GfxError> {
        let prog = crate::shaders::program(RECT_VERT, SPRITE_FRAG)?;
        let white = crate::gl::texture(
            1,
            1,
            gl::NEAREST,
            gl::CLAMP_TO_EDGE,
            gl::RGBA,
            Some(&[255u8; 4]),
        );
        let (u_rect, u_colour);
        unsafe {
            gl::UseProgram(prog);
            gl::Uniform1i(crate::gl::uniform_location(prog, "u_tex"), 0);
            gl::Uniform2f(
                crate::gl::uniform_location(prog, "u_target"),
                OUT_W as f32,
                OUT_H as f32,
            );
            u_rect = crate::gl::uniform_location(prog, "u_rect");
            u_colour = crate::gl::uniform_location(prog, "u_colour");
        }
        Ok(Sprites {
            prog,
            white,
            textures: Vec::new(),
            u_rect,
            u_colour,
        })
    }

    pub fn create_texture(&mut self, w: u32, h: u32, rgba: &[u8]) -> TexId {
        self.push(w, h, rgba, gl::LINEAR)
    }

    /// For a texture drawn at a whole number magnification, where a linear tap lands between
    /// texels and blurs the pixel grid the source is defined on. The switcher's screenshot is
    /// the only one: everything else is drawn at its own size or smaller.
    pub fn create_texture_nearest(&mut self, w: u32, h: u32, rgba: &[u8]) -> TexId {
        self.push(w, h, rgba, gl::NEAREST)
    }

    fn push(&mut self, w: u32, h: u32, rgba: &[u8], filter: gl::types::GLenum) -> TexId {
        let tex = crate::gl::texture(w, h, filter, gl::CLAMP_TO_EDGE, gl::RGBA, Some(rgba));
        self.textures.push(tex);
        TexId(self.textures.len() - 1)
    }

    /// Replaces a texture's contents in place. The polaroid switcher redraws the same ten
    /// cards every time it opens, and creating a fresh set each time would grow the pool
    /// for the life of the session.
    pub fn update_texture(&mut self, id: TexId, w: u32, h: u32, rgba: &[u8]) {
        let Some(tex) = self.textures.get(id.0) else {
            return;
        };
        if rgba.len() < (w * h * 4) as usize {
            return;
        }
        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, *tex);
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                crate::gl::internal_format(gl::RGBA),
                w as i32,
                h as i32,
                0,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                rgba.as_ptr() as *const std::ffi::c_void,
            );
        }
    }

    /// For the passes that sample a sprite texture without drawing it as a sprite.
    pub fn source(&self, id: TexId) -> Option<gl::types::GLuint> {
        self.textures.get(id.0).copied()
    }

    pub fn draw(&self, items: &[Draw], quad: &Quad) {
        unsafe {
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
            gl::UseProgram(self.prog);
            gl::ActiveTexture(gl::TEXTURE0);
        }
        for item in items {
            let (x, y, w, h, tex, colour) = match *item {
                Draw::Rect { x, y, w, h, colour } => (x, y, w, h, self.white, colour),
                Draw::Tex {
                    x,
                    y,
                    w,
                    h,
                    tex,
                    alpha,
                } => match self.textures.get(tex.0) {
                    Some(t) => (x, y, w, h, *t, [1.0, 1.0, 1.0, alpha]),
                    None => continue,
                },
                // The compositor splits the list on these and draws the game pass itself.
                Draw::Game | Draw::Shot { .. } => continue,
            };
            unsafe {
                gl::BindTexture(gl::TEXTURE_2D, tex);
                gl::Uniform4f(self.u_rect, x, y, w, h);
                gl::Uniform4f(self.u_colour, colour[0], colour[1], colour[2], colour[3]);
            }
            quad.draw();
        }
        unsafe { gl::Disable(gl::BLEND) };
    }
}

impl Drop for Sprites {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteTextures(self.textures.len() as i32, self.textures.as_ptr());
            gl::DeleteTextures(1, &self.white);
            gl::DeleteProgram(self.prog);
        }
    }
}

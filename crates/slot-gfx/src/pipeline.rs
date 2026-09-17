use crate::lcd3x::mask_texture_rgba8;
use crate::power::{screen_brightness, screen_rect};
use crate::quad::Quad;
use crate::shaders::{GAME_FRAG, RECT_VERT};
use crate::surface::{GfxError, OUT_H, OUT_W};

/// The only scale that exists. 240x160 to 720x480, nearest, which is what collapses LCD3x
/// to a 3x3 mask tiled once per source pixel.
pub const SCALE: u32 = 3;
pub const SRC_W: u32 = OUT_W / SCALE;
pub const SRC_H: u32 = OUT_H / SCALE;

/// Origin then size, in texture coordinates: everything there is. The default, what a GBA
/// picture is always drawn with, and what a still is always drawn with.
pub const WHOLE_TEXTURE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

pub struct GamePass {
    prog: gl::types::GLuint,
    game: gl::types::GLuint,
    mask: gl::types::GLuint,
    u_rect: gl::types::GLint,
    u_bright: gl::types::GLint,
    u_uv: gl::types::GLint,
    /// A compositor with nobody driving it is a screen that is on.
    power: f32,
    /// The part of the live game's texture the panel shows: origin then size, in texture
    /// coordinates. The whole texture until somebody says otherwise, which is every GBA
    /// picture and every Game Boy one at actual size. A still never reads it — see
    /// `draw_still`.
    src: [f32; 4],
}

impl GamePass {
    pub fn new() -> Result<Self, GfxError> {
        let prog = crate::shaders::program(RECT_VERT, GAME_FRAG)?;
        let game = crate::gl::texture(SRC_W, SRC_H, gl::NEAREST, gl::CLAMP_TO_EDGE, gl::BGRA, None);
        let mask = crate::gl::texture(
            3,
            3,
            gl::NEAREST,
            gl::REPEAT,
            gl::RGBA,
            Some(&mask_texture_rgba8()),
        );
        let (u_rect, u_bright, u_uv);
        unsafe {
            // The other two are fixed for the life of the program: the mask always tiles once
            // per source pixel and the target is always the offscreen frame.
            gl::UseProgram(prog);
            gl::Uniform1i(crate::gl::uniform_location(prog, "u_game"), 0);
            gl::Uniform1i(crate::gl::uniform_location(prog, "u_mask"), 1);
            gl::Uniform2f(
                crate::gl::uniform_location(prog, "u_src"),
                SRC_W as f32,
                SRC_H as f32,
            );
            gl::Uniform2f(
                crate::gl::uniform_location(prog, "u_target"),
                OUT_W as f32,
                OUT_H as f32,
            );
            u_rect = crate::gl::uniform_location(prog, "u_rect");
            u_bright = crate::gl::uniform_location(prog, "u_bright");
            u_uv = crate::gl::uniform_location(prog, "u_uv");
        }
        Ok(GamePass {
            prog,
            game,
            mask,
            u_rect,
            u_bright,
            u_uv,
            power: 1.0,
            src: WHOLE_TEXTURE,
        })
    }

    pub fn set_power(&mut self, t: f32) {
        self.power = t.clamp(0.0, 1.0);
    }

    /// Which part of the *live game's* texture fills the panel, as origin then size in texture
    /// coordinates. `WHOLE_TEXTURE` is the default and draws exactly what this pass drew before
    /// there was a sub-rect at all; a Game Boy's own 160x144 window inside the 240x160 buffer is
    /// what the fullscreen mode asks for. The grille is unaffected either way — see `GAME_FRAG`.
    ///
    /// A still does not take it. See `draw_still`.
    pub fn set_source_rect(&mut self, rect: [f32; 4]) {
        self.src = rect;
    }

    pub fn upload(&mut self, xrgb8888: &[u8]) {
        if xrgb8888.len() < (SRC_W * SRC_H * 4) as usize {
            return;
        }
        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, self.game);
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
            gl::TexSubImage2D(
                gl::TEXTURE_2D,
                0,
                0,
                0,
                SRC_W as i32,
                SRC_H as i32,
                gl::BGRA,
                gl::UNSIGNED_BYTE,
                xrgb8888.as_ptr() as *const std::ffi::c_void,
            );
        }
    }

    pub fn draw(&self, quad: &Quad) {
        self.draw_source(self.game, quad, self.src);
    }

    /// The same pass over a still. A saved shot is a picture of this panel at exactly the
    /// scale the mask is built for, so it is filtered at draw time rather than blitted flat
    /// beside a game that is filtered.
    ///
    /// `WHOLE_TEXTURE` explicitly, and never `self.src`: a still is a *photograph*, taken at
    /// some earlier moment, and `thumb::png` encodes the whole 240x160 buffer — so a Game Boy
    /// polaroid is the centred picture with black at its sides whatever the panel is set to
    /// now. Cropping it to the current mode would render the same stored image differently
    /// depending on a setting that has nothing to do with when it was taken: a state saved
    /// before the player ever pressed L would come back stretched because of a preference set
    /// afterwards.
    ///
    /// This does mean a polaroid does not match a stretched game showing behind the switcher.
    /// That is the honest inconsistency, and it is deliberate: a polaroid is a picture of the
    /// game, not of the display setting it was viewed at, and looking different is how it says
    /// so. A photograph on a shelf does not change shape when you rearrange the room. Do not
    /// "fix" this for consistency.
    pub fn draw_still(&self, tex: gl::types::GLuint, quad: &Quad) {
        self.draw_source(tex, quad, WHOLE_TEXTURE);
    }

    fn draw_source(&self, tex: gl::types::GLuint, quad: &Quad, src: [f32; 4]) {
        let (x, y, w, h) = screen_rect(self.power);
        unsafe {
            gl::UseProgram(self.prog);
            gl::Uniform4f(self.u_rect, x, y, w, h);
            // Here rather than in `set_source_rect`, so the value is whatever this draw asked
            // for on whichever program the caller left bound — the same discipline `u_rect` and
            // `u_bright` already follow. Taken as an argument rather than read off the field,
            // because the game and a still deliberately want different answers.
            gl::Uniform4f(self.u_uv, src[0], src[1], src[2], src[3]);
            gl::Uniform1f(self.u_bright, screen_brightness(self.power));
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, tex);
            gl::ActiveTexture(gl::TEXTURE1);
            gl::BindTexture(gl::TEXTURE_2D, self.mask);
            gl::ActiveTexture(gl::TEXTURE0);
        }
        quad.draw();
    }
}

impl Drop for GamePass {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteTextures(1, &self.game);
            gl::DeleteTextures(1, &self.mask);
            gl::DeleteProgram(self.prog);
        }
    }
}

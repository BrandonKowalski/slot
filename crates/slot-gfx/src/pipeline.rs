use crate::lcd3x::ScreenEffect;
use crate::power::{screen_brightness, screen_rect_above};
use crate::quad::Quad;
use crate::shaders::{GAME_FRAG, RECT_VERT};
use crate::surface::{GfxError, OUT_H, OUT_W};

pub const SCALE: u32 = 3;
pub const SRC_W: u32 = OUT_W / SCALE;
pub const SRC_H: u32 = OUT_H / SCALE;

pub const WHOLE_TEXTURE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

pub struct GamePass {
    prog: gl::types::GLuint,
    game: gl::types::GLuint,
    u_rect: gl::types::GLint,
    u_bright: gl::types::GLint,
    u_uv: gl::types::GLint,
    u_mode: gl::types::GLint,
    u_paper_size: gl::types::GLint,
    u_fbo: gl::types::GLint,
    fbo_scale: [f32; 2],
    u_origin: gl::types::GLint,
    origin: [f32; 2],
    u_pic: gl::types::GLint,
    pic: [f32; 4],
    paper: gl::types::GLuint,
    paper_size: f32,
    effect: ScreenEffect,
    power: f32,
    fills: bool,
    fit: Option<(u32, u32)>,
    /// Pixels kept for the carousel footer. The picture scales in the space above them.
    bottom: f32,
    src: [f32; 4],
}

impl GamePass {
    pub fn new() -> Result<Self, GfxError> {
        let prog = crate::shaders::program(RECT_VERT, GAME_FRAG)?;
        let game = crate::gl::texture(SRC_W, SRC_H, gl::NEAREST, gl::CLAMP_TO_EDGE, gl::BGRA, None);
        let paper = crate::gl::texture(
            1,
            1,
            gl::NEAREST,
            gl::REPEAT,
            crate::gl::gray_format(),
            Some(&[255]),
        );
        let (u_rect, u_bright, u_uv, u_mode, u_paper_size, u_pic, u_fbo, u_origin);
        unsafe {
            gl::UseProgram(prog);
            gl::Uniform1i(crate::gl::uniform_location(prog, "u_game"), 0);
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
            gl::Uniform1i(crate::gl::uniform_location(prog, "u_paper"), 1);
            u_mode = crate::gl::uniform_location(prog, "u_mode");
            u_paper_size = crate::gl::uniform_location(prog, "u_paper_size");
            u_fbo = crate::gl::uniform_location(prog, "u_fbo");
            u_pic = crate::gl::uniform_location(prog, "u_pic");
            u_origin = crate::gl::uniform_location(prog, "u_origin");
        }
        Ok(GamePass {
            prog,
            game,
            u_rect,
            u_bright,
            u_uv,
            u_mode,
            u_paper_size,
            u_fbo,
            fbo_scale: [1.0, 1.0],
            u_origin,
            origin: [0.0, 0.0],
            u_pic,
            pic: WHOLE_TEXTURE,
            paper,
            paper_size: 1.0,
            effect: ScreenEffect::Lcd3x,
            power: 1.0,
            fills: false,
            fit: None,
            bottom: 0.0,
            src: WHOLE_TEXTURE,
        })
    }

    pub fn set_power(&mut self, t: f32) {
        self.power = t.clamp(0.0, 1.0);
    }

    pub fn set_bottom(&mut self, px: f32) {
        self.bottom = px.max(0.0);
    }

    pub fn frame(&self) -> (f32, f32, f32, f32) {
        self.placed()
    }

    fn placed(&self) -> (f32, f32, f32, f32) {
        if let Some(panel) = self.fit {
            return fitted_rect(
                panel,
                (self.src[2] * SRC_W as f32, self.src[3] * SRC_H as f32),
                if self.fills { 1.0 } else { self.power },
                self.bottom,
            );
        }
        let foot = self.bottom;
        match self.fills {
            true => (0.0, 0.0, OUT_W as f32, OUT_H as f32 - foot),
            false => screen_rect_above(self.power, foot),
        }
    }

    pub fn set_fit(&mut self, panel: Option<(u32, u32)>) {
        self.fit = panel;
    }

    pub fn set_fills_frame(&mut self, on: bool) {
        self.fills = on;
    }

    pub fn set_picture(&mut self, rect: [f32; 4]) {
        self.pic = rect;
    }

    pub fn set_fbo_scale(&mut self, x: f32, y: f32) {
        self.fbo_scale = [x, y];
    }

    pub fn set_origin(&mut self, x: f32, y: f32) {
        self.origin = [x, y];
    }

    pub fn set_effect(&mut self, effect: ScreenEffect) {
        self.effect = effect;
    }

    pub fn set_paper(&mut self, size: u32, gray: &[u8]) {
        let tex = crate::gl::texture(
            size,
            size,
            gl::NEAREST,
            gl::REPEAT,
            crate::gl::gray_format(),
            Some(gray),
        );
        unsafe { gl::DeleteTextures(1, &self.paper) };
        self.paper = tex;
        self.paper_size = size as f32;
    }

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
        let src = match self.effect {
            ScreenEffect::Grid if self.fit.is_none() => {
                whole_scale_source(self.src, self.fbo_scale)
            }
            _ => self.src,
        };
        self.draw_source(self.game, quad, src);
    }

    pub fn draw_still(&self, tex: gl::types::GLuint, quad: &Quad) {
        self.draw_source(
            tex,
            quad,
            if self.fit.is_some() {
                self.src
            } else {
                WHOLE_TEXTURE
            },
        );
    }

    fn draw_source(&self, tex: gl::types::GLuint, quad: &Quad, src: [f32; 4]) {
        let (x, y, w, h) = self.placed();
        let bright = if self.fills {
            1.0
        } else {
            screen_brightness(self.power)
        };
        // The footer covers the bottom of a full-frame picture. Keep the part that sat above
        // it, so opening the scale does not squash the picture into the shorter rect.
        let mut src = src;
        if self.bottom > 0.0 && self.fit.is_none() {
            src[3] *= (OUT_H as f32 - self.bottom) / OUT_H as f32;
        }
        unsafe {
            gl::UseProgram(self.prog);
            gl::Uniform4f(self.u_rect, x, y, w, h);
            gl::Uniform4f(self.u_uv, src[0], src[1], src[2], src[3]);
            gl::Uniform1f(self.u_bright, bright);
            gl::Uniform1f(self.u_mode, self.effect.mode());
            gl::Uniform1f(self.u_paper_size, self.paper_size);
            gl::Uniform2f(self.u_fbo, self.fbo_scale[0], self.fbo_scale[1]);
            gl::Uniform2f(self.u_origin, self.origin[0], self.origin[1]);
            gl::Uniform4f(
                self.u_pic,
                self.pic[0],
                self.pic[1],
                self.pic[2],
                self.pic[3],
            );
            gl::ActiveTexture(gl::TEXTURE1);
            gl::BindTexture(gl::TEXTURE_2D, self.paper);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, tex);
        }
        quad.draw();
    }
}

fn fitted_rect(
    panel: (u32, u32),
    picture: (f32, f32),
    power: f32,
    footer: f32,
) -> (f32, f32, f32, f32) {
    let (pw, ph) = (panel.0.max(1) as f32, panel.1.max(1) as f32);
    let view = (ph * (1.0 - footer.clamp(0.0, OUT_H as f32) / OUT_H as f32)).floor();
    let scale = (pw / picture.0).min(view / picture.1);
    let (w, h) = ((picture.0 * scale).round(), (picture.1 * scale).round());
    let (x, y, sw, sh) = screen_rect_above(power, 0.0);
    (
        (((pw - w) / 2.0).floor() + x / OUT_W as f32 * w) * OUT_W as f32 / pw,
        (((view - h) / 2.0).floor() + y / OUT_H as f32 * h) * OUT_H as f32 / ph,
        w * sw / pw,
        h * sh / ph,
    )
}

fn whole_scale_source(src: [f32; 4], fbo: [f32; 2]) -> [f32; 4] {
    let (x, w) = whole_scale(src[0], src[2], SRC_W as f32, OUT_W as f32 * fbo[0]);
    let (y, h) = whole_scale(src[1], src[3], SRC_H as f32, OUT_H as f32 * fbo[1]);
    [x, y, w, h]
}

fn whole_scale(at: f32, len: f32, texels: f32, out: f32) -> (f32, f32) {
    let shown = len * texels;
    let scale = (out / shown).ceil();
    let trim = scale * shown - out;
    if trim >= 0.1 * shown {
        return (at, len);
    }
    let top = (trim / 2.0).floor();
    (at + top / scale / texels, out / scale / texels)
}

impl Drop for GamePass {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteTextures(1, &self.game);
            gl::DeleteTextures(1, &self.paper);
            gl::DeleteProgram(self.prog);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_power_curves_stay_centred_inside_the_fitted_picture() {
        for panel in [(640, 480), (720, 720), (1280, 720), (320, 480)] {
            for footer in [0.0, 58.0] {
                let (x, y, w, h) = fitted_rect(panel, (160.0, 144.0), 1.0, footer);
                for power in [0.0, 0.06, 0.12, 0.5, 1.0] {
                    let (ax, ay, aw, ah) = fitted_rect(panel, (160.0, 144.0), power, footer);
                    assert!((ax + aw / 2.0 - x - w / 2.0).abs() < 0.001);
                    assert!((ay + ah / 2.0 - y - h / 2.0).abs() < 0.001);
                    assert!((aw / w - crate::power::screen_width(power)).abs() < 0.0001);
                    assert!((ah / h - crate::power::screen_scale(power)).abs() < 0.0001);
                    assert!(ay + ah <= OUT_H as f32 - footer + 0.001);
                }
            }
        }
    }

    #[test]
    fn fit_reserves_logical_footer_pixels_without_squashing_the_picture() {
        for panel in [(640, 480), (720, 720), (1280, 720), (320, 480)] {
            for picture in [(160.0, 144.0), (240.0, 160.0)] {
                for footer in [0.0, 29.5, 58.0, OUT_H as f32, OUT_H as f32 + 1.0] {
                    let (x, y, w, h) = fitted_rect(panel, picture, 1.0, footer);
                    let pw = w * panel.0 as f32 / OUT_W as f32;
                    let ph = h * panel.1 as f32 / OUT_H as f32;
                    assert!(x >= 0.0 && y >= 0.0 && w >= 0.0 && h >= 0.0);
                    assert!(y + h <= OUT_H as f32 - footer.min(OUT_H as f32) + 0.001);
                    assert!((pw - ph * picture.0 / picture.1).abs() <= 1.0);
                    if footer >= OUT_H as f32 {
                        assert_eq!((w, h), (0.0, 0.0));
                    }
                }
            }
        }
        // The footer consumes existing letterboxing before it shrinks a width-limited picture.
        let full = fitted_rect((320, 480), (160.0, 144.0), 1.0, 0.0);
        let above = fitted_rect((320, 480), (160.0, 144.0), 1.0, 58.0);
        assert_eq!((full.2, full.3), (above.2, above.3));
        assert!(above.1 < full.1);
    }

    #[test]
    fn fit_maximizes_and_centres_the_full_picture() {
        for (panel, expected) in [
            ((640, 480), (53, 0, 533, 480)),
            ((720, 480), (93, 0, 533, 480)),
            ((720, 720), (0, 36, 720, 648)),
            ((1280, 720), (240, 0, 800, 720)),
            ((320, 480), (0, 96, 320, 288)),
        ] {
            let (x, y, w, h) = fitted_rect(panel, (160.0, 144.0), 1.0, 0.0);
            let actual = (
                (x * panel.0 as f32 / OUT_W as f32).round() as i32,
                (y * panel.1 as f32 / OUT_H as f32).round() as i32,
                (w * panel.0 as f32 / OUT_W as f32).round() as i32,
                (h * panel.1 as f32 / OUT_H as f32).round() as i32,
            );
            assert_eq!(actual, expected, "{panel:?}");
            assert!(actual.0 >= 0 && actual.1 >= 0);
            assert!(actual.0 + actual.2 <= panel.0 as i32);
            assert!(actual.1 + actual.3 <= panel.1 as i32);
            assert!((actual.2 as f32 - actual.3 as f32 * 160.0 / 144.0).abs() <= 0.5);
        }
    }
}

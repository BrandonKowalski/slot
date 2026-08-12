//! Shader sources are GLSL ES 1.00 so the device build compiles them unchanged. Only the
//! preamble differs: the device supplies `precision` defaults and
//! `#define FRAG_COLOR gl_FragColor`, the host maps the ES names onto GL 3.3 core.

use crate::surface::GfxError;

const VERT_PREAMBLE: &str = "#version 330 core\n#define attribute in\n#define varying out\n";

const FRAG_PREAMBLE: &str = "#version 330 core\n#define varying in\n\
                             #define texture2D texture\nout vec4 FRAG_COLOR;\n";

/// ES 1.00 is the language these are written in, so the device adds nothing but the name of
/// the output. A `#version` line is omitted rather than set: 100 is the default, and the
/// drivers that reject `#version 100` outnumber the ones that require it.
const VERT_PREAMBLE_ES: &str = "";
const FRAG_PREAMBLE_ES: &str = "#define FRAG_COLOR gl_FragColor\n";

pub fn program(vert: &str, frag: &str) -> Result<gl::types::GLuint, GfxError> {
    let (vp, fp) = match crate::gl::es() {
        true => (VERT_PREAMBLE_ES, FRAG_PREAMBLE_ES),
        false => (VERT_PREAMBLE, FRAG_PREAMBLE),
    };
    crate::gl::program(&format!("{vp}{vert}"), &format!("{fp}{frag}"))
}

/// Unit quad to a rect in target pixels, origin top left. The y flip lives here, so every
/// pass drawing into the offscreen target thinks in screen coordinates and only the blit
/// deals with the framebuffer being stored bottom up.
pub const RECT_VERT: &str = r#"
attribute vec2 a_pos;
uniform vec4 u_rect;
uniform vec2 u_target;
varying vec2 v_uv;
void main() {
    v_uv = a_pos;
    vec2 p = (u_rect.xy + a_pos * u_rect.zw) / u_target;
    gl_Position = vec4(p.x * 2.0 - 1.0, 1.0 - p.y * 2.0, 0.0, 1.0);
}
"#;

/// `u_src` is the source size in pixels, which is also the number of times the 3x3 mask
/// tiles across the target: one RGB triad per source pixel, exactly.
pub const GAME_FRAG: &str = r#"
precision mediump float;
uniform sampler2D u_game;
uniform sampler2D u_mask;
uniform vec2 u_src;
uniform float u_bright;
varying vec2 v_uv;
void main() {
    vec3 rgb = texture2D(u_game, v_uv).rgb * texture2D(u_mask, v_uv * u_src).rgb;
    FRAG_COLOR = vec4(rgb * u_bright, 1.0);
}
"#;

pub const SPRITE_FRAG: &str = r#"
precision mediump float;
uniform sampler2D u_tex;
uniform vec4 u_colour;
varying vec2 v_uv;
void main() {
    FRAG_COLOR = texture2D(u_tex, v_uv) * u_colour;
}
"#;

pub const BLIT_VERT: &str = r#"
attribute vec2 a_pos;
varying vec2 v_uv;
void main() {
    v_uv = a_pos;
    gl_Position = vec4(a_pos * 2.0 - 1.0, 0.0, 1.0);
}
"#;

pub const BLIT_FRAG: &str = r#"
precision mediump float;
uniform sampler2D u_tex;
uniform vec3 u_gain;
varying vec2 v_uv;
void main() {
    FRAG_COLOR = vec4(texture2D(u_tex, v_uv).rgb * u_gain, 1.0);
}
"#;

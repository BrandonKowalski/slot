use crate::surface::{GfxError, Surface};
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};

/// Whether the context this process loaded is GLES rather than desktop GL. The two disagree
/// on two things the tree uses: sized internal formats, and whether a draw needs a vertex
/// array object. Read from the driver rather than from a build feature, so a host build
/// pointed at a GLES context still draws.
static ES: AtomicBool = AtomicBool::new(false);

pub fn load(surface: &dyn Surface) {
    gl::load_with(|name| surface.proc_address(name));
    let version = unsafe { gl::GetString(gl::VERSION) };
    let es = !version.is_null()
        && unsafe { std::ffi::CStr::from_ptr(version as *const std::ffi::c_char) }
            .to_string_lossy()
            .starts_with("OpenGL ES");
    ES.store(es, Ordering::Relaxed);
}

pub fn es() -> bool {
    ES.load(Ordering::Relaxed)
}

/// ES 2.0 has no sized internal formats: the internal format has to be spelled exactly as
/// the data's, and `RGBA8` is a hard error rather than a hint it ignores.
pub fn internal_format(format: gl::types::GLenum) -> gl::types::GLint {
    match es() {
        true => format as gl::types::GLint,
        false => gl::RGBA8 as gl::types::GLint,
    }
}

fn shader(kind: gl::types::GLenum, src: &str) -> Result<gl::types::GLuint, GfxError> {
    let Ok(csrc) = CString::new(src) else {
        return Err(GfxError::Shader("source contains a nul byte".into()));
    };
    unsafe {
        let id = gl::CreateShader(kind);
        gl::ShaderSource(id, 1, &csrc.as_ptr(), std::ptr::null());
        gl::CompileShader(id);
        let mut ok = 0;
        gl::GetShaderiv(id, gl::COMPILE_STATUS, &mut ok);
        if ok == 0 {
            let log = info_log(id, gl::GetShaderiv, gl::GetShaderInfoLog);
            gl::DeleteShader(id);
            return Err(GfxError::Shader(log));
        }
        Ok(id)
    }
}

pub fn program(vert: &str, frag: &str) -> Result<gl::types::GLuint, GfxError> {
    let vs = shader(gl::VERTEX_SHADER, vert)?;
    let fs = match shader(gl::FRAGMENT_SHADER, frag) {
        Ok(fs) => fs,
        Err(e) => {
            unsafe { gl::DeleteShader(vs) };
            return Err(e);
        }
    };
    unsafe {
        let p = gl::CreateProgram();
        gl::AttachShader(p, vs);
        gl::AttachShader(p, fs);
        if let Ok(pos) = CString::new("a_pos") {
            gl::BindAttribLocation(p, crate::quad::POS_LOCATION, pos.as_ptr());
        }
        gl::LinkProgram(p);
        // Shaders stay alive until the program is linked, then the program owns the code.
        gl::DeleteShader(vs);
        gl::DeleteShader(fs);
        let mut ok = 0;
        gl::GetProgramiv(p, gl::LINK_STATUS, &mut ok);
        if ok == 0 {
            let log = info_log(p, gl::GetProgramiv, gl::GetProgramInfoLog);
            gl::DeleteProgram(p);
            return Err(GfxError::Shader(log));
        }
        Ok(p)
    }
}

unsafe fn info_log(
    id: gl::types::GLuint,
    get_iv: unsafe fn(gl::types::GLuint, gl::types::GLenum, *mut gl::types::GLint),
    get_log: unsafe fn(
        gl::types::GLuint,
        gl::types::GLsizei,
        *mut gl::types::GLsizei,
        *mut gl::types::GLchar,
    ),
) -> String {
    let mut len = 0;
    get_iv(id, gl::INFO_LOG_LENGTH, &mut len);
    if len <= 0 {
        return "no info log".into();
    }
    let mut buf = vec![0u8; len as usize];
    get_log(
        id,
        len,
        std::ptr::null_mut(),
        buf.as_mut_ptr() as *mut gl::types::GLchar,
    );
    while buf.last() == Some(&0) {
        buf.pop();
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// An RGBA8 texture. `format` is the layout of `data`, which the GBA frame needs because
/// libretro hands over XRGB8888, little endian, so its bytes arrive B, G, R, X.
pub fn texture(
    w: u32,
    h: u32,
    filter: gl::types::GLenum,
    wrap: gl::types::GLenum,
    format: gl::types::GLenum,
    data: Option<&[u8]>,
) -> gl::types::GLuint {
    let pixels = match data {
        Some(d) if d.len() >= (w * h * 4) as usize => d.as_ptr() as *const std::ffi::c_void,
        _ => std::ptr::null(),
    };
    unsafe {
        let mut tex = 0;
        gl::GenTextures(1, &mut tex);
        gl::BindTexture(gl::TEXTURE_2D, tex);
        gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
        gl::TexImage2D(
            gl::TEXTURE_2D,
            0,
            internal_format(format),
            w as i32,
            h as i32,
            0,
            format,
            gl::UNSIGNED_BYTE,
            pixels,
        );
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, filter as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, filter as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, wrap as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, wrap as i32);
        tex
    }
}

pub fn uniform_location(program: gl::types::GLuint, name: &str) -> gl::types::GLint {
    match CString::new(name) {
        Ok(c) => unsafe { gl::GetUniformLocation(program, c.as_ptr()) },
        Err(_) => -1,
    }
}

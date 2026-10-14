use std::path::Path;

use slot_gfx::{canvas_is_whole_texture, game_rect, OUT_H, OUT_W, SRC_H, SRC_W, WHOLE_TEXTURE};
use slot_store::{ini, Platform};

pub const VIDEO_MODE_FILE: &str = "Config/video_mode.txt";

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum VideoMode {
    #[default]
    Actual,
    Stretch,
}

impl VideoMode {
    pub fn as_str(self) -> &'static str {
        match self {
            VideoMode::Actual => "actual",
            VideoMode::Stretch => "stretch",
        }
    }

    pub fn parse(s: &str) -> Option<VideoMode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "actual" => Some(VideoMode::Actual),
            "stretch" => Some(VideoMode::Stretch),
            _ => None,
        }
    }
}

pub fn video_mode_for(root: &Path, stem: &str) -> VideoMode {
    ini::value(root, VIDEO_MODE_FILE, stem)
        .as_deref()
        .and_then(VideoMode::parse)
        .unwrap_or_default()
}

pub fn write_video_mode(root: &Path, stem: &str, mode: VideoMode) -> std::io::Result<()> {
    ini::write(root, VIDEO_MODE_FILE, stem, mode.as_str())
}

/// The texels of the game texture that reach the canvas.
///
/// On the 720x480 canvas the whole texture is drawn edge to edge at 3x, so an "actual" Game Boy
/// picture is the 160x144 sitting inside it. On any other canvas only the platform's picture is
/// shown, and `dest_rect` places it.
pub fn shown(platform: Platform, mode: VideoMode) -> (u32, u32) {
    let (w, h) = platform.picture();
    if mode == VideoMode::Actual && canvas_is_whole_texture() {
        return (SRC_W, SRC_H);
    }
    (w, h)
}

/// Where the shown texels land on the canvas. The whole canvas on 720x480; elsewhere "actual" is
/// the largest whole scale that fits and "stretch" fills the canvas with the aspect kept.
pub fn dest_rect(platform: Platform, mode: VideoMode) -> (f32, f32, f32, f32) {
    if canvas_is_whole_texture() {
        return (0.0, 0.0, OUT_W as f32, OUT_H as f32);
    }
    game_rect(shown(platform, mode), mode == VideoMode::Stretch)
}

pub fn source_rect(platform: Platform, mode: VideoMode) -> [f32; 4] {
    let (w, h) = shown(platform, mode);
    if (w, h) == (SRC_W, SRC_H) {
        return WHOLE_TEXTURE;
    }
    let x = SRC_W.saturating_sub(w) / 2;
    let y = SRC_H.saturating_sub(h) / 2;
    [
        x as f32 / SRC_W as f32,
        y as f32 / SRC_H as f32,
        w as f32 / SRC_W as f32,
        h as f32 / SRC_H as f32,
    ]
}

use std::path::Path;

use slot_gfx::{OUT_H, OUT_W, SRC_H, SRC_W, WHOLE_TEXTURE};
use slot_store::{ini, Platform};

pub const VIDEO_MODE_FILE: &str = "Config/video_mode.txt";

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum VideoMode {
    #[default]
    Actual,
    Fit,
    Stretch,
}

impl VideoMode {
    pub fn as_str(self) -> &'static str {
        match self {
            VideoMode::Actual => "actual",
            VideoMode::Fit => "fit",
            VideoMode::Stretch => "stretch",
        }
    }

    pub fn bigger(self) -> VideoMode {
        match self {
            VideoMode::Actual => VideoMode::Fit,
            VideoMode::Fit | VideoMode::Stretch => VideoMode::Stretch,
        }
    }

    pub fn smaller(self) -> VideoMode {
        match self {
            VideoMode::Stretch => VideoMode::Fit,
            VideoMode::Fit | VideoMode::Actual => VideoMode::Actual,
        }
    }

    pub fn parse(s: &str) -> Option<VideoMode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "actual" => Some(VideoMode::Actual),
            "fit" => Some(VideoMode::Fit),
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

pub fn source_rect(platform: Platform, mode: VideoMode) -> [f32; 4] {
    let (w, h) = platform.picture();
    if (w, h) == (SRC_W, SRC_H) {
        return WHOLE_TEXTURE;
    }
    match mode {
        VideoMode::Actual => WHOLE_TEXTURE,
        VideoMode::Fit => {
            // The picture's rows fill the screen height; the columns follow at the same scale.
            let cols = h * OUT_W / OUT_H;
            let (x, y) = (SRC_W.saturating_sub(cols) / 2, (SRC_H - h) / 2);
            [
                x as f32 / SRC_W as f32,
                y as f32 / SRC_H as f32,
                cols as f32 / SRC_W as f32,
                h as f32 / SRC_H as f32,
            ]
        }
        VideoMode::Stretch => {
            let x = SRC_W.saturating_sub(w) / 2;
            [x as f32 / SRC_W as f32, 0.0, w as f32 / SRC_W as f32, 1.0]
        }
    }
}

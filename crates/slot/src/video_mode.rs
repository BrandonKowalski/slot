//! Whether a picture is drawn at its own size or stretched over the whole panel, and which of
//! the two each cart was last left in.
//!
//! The Game Boy and the Game Boy Color had no shoulder buttons, so on one of their carts slot
//! takes L and R: L stretches the picture to fill the panel, R gives back the largest whole
//! multiple of it, centred. On a GBA cart the two are the GBA's own and this file has nothing
//! to say — a GBA picture already fills the panel at exactly 3x, and there is no second size
//! for it to have.
//!
//! The stretch distorts, deliberately. 160x144 is 10:9 against a 3:2 panel, so a fullscreen
//! Game Boy picture comes out about 35% wider than it is tall. That is what a Game Boy picture
//! blown up to fill a television looked like, and it is the mode the user asked for by name.
//! The aspect-correct alternative — 533x480 with 93 px bars either side — is neither fullscreen
//! nor period-correct, and it would cost the panel-locked grille as well (see `GAME_FRAG`), so
//! it is not offered.

use std::path::Path;

use slot_gfx::{SRC_H, SRC_W, WHOLE_TEXTURE};
use slot_store::{ini, Platform};

/// A sibling of `selected_core.ini`, in the same `<stem> = <value>` shape and read by the same
/// parser. Flat and stem-keyed like that one, which means a `GB/Tetris.gb` and a
/// `GBC/Tetris.gbc` share a line. Deliberately: it is a two-value cosmetic preference that
/// cannot lose anybody's data, and the worst it can do is open a Colour cart stretched because
/// its same-named sibling was.
pub const VIDEO_MODE_FILE: &str = "System/video_mode.ini";

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum VideoMode {
    /// The picture at the largest whole multiple of itself the panel holds, centred. A Game
    /// Boy's 160x144 comes out 480x432 with 120 px of nothing either side and 24 top and
    /// bottom, and every source pixel sits under exactly one mask cell.
    #[default]
    Actual,
    /// The picture over the whole panel, aspect and all.
    Stretch,
}

impl VideoMode {
    /// The ini's spelling, meant to be typed by hand into a text editor on a computer.
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

/// The mode one cart was last left in. A cart the file does not name reads `Actual`, and so
/// does a cart whose line nobody can parse — exactly as `core_for` answers for a cart with no
/// line.
pub fn video_mode_for(root: &Path, stem: &str) -> VideoMode {
    ini::value(root, VIDEO_MODE_FILE, stem)
        .as_deref()
        .and_then(VideoMode::parse)
        .unwrap_or_default()
}

/// Set one cart's mode, leaving the rest of the file exactly as it was.
pub fn write_video_mode(root: &Path, stem: &str, mode: VideoMode) -> std::io::Result<()> {
    ini::write(root, VIDEO_MODE_FILE, stem, mode.as_str())
}

/// The part of the frame buffer the panel shows, as origin then size in texture coordinates.
///
/// The whole texture unless a Game Boy cart has been stretched, and that default is what keeps
/// every GBA pixel where it has always been: it is the arithmetic the game pass did before
/// there was a sub-rect to ask for.
///
/// The window comes from `Platform::picture` and the same centring `video_refresh` applies, so
/// the two cannot drift: whatever size a platform says its picture is, this is where that
/// picture was put.
pub fn source_rect(platform: Platform, mode: VideoMode) -> [f32; 4] {
    let (w, h) = platform.picture();
    if mode == VideoMode::Actual || (w, h) == (SRC_W, SRC_H) {
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

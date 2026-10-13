use std::path::Path;

use crate::art;

/// A bezel at its own pixel size, and the clear window over the picture that
/// is already on screen, as fractions of that image.
pub struct GameBezelFace {
    pub rgba: Vec<u8>,
    pub w: u32,
    pub h: u32,
    pub hole: [f32; 4],
}

pub fn game_bezel_face(path: &Path) -> Option<GameBezelFace> {
    let (rgba, w, h) = art::read_rgba(path)?;
    let hole = clear_window(&rgba, w, h)?;
    Some(GameBezelFace { rgba, w, h, hole })
}

/// The bounding box of fully clear pixels. A bezel's screen is that window.
fn clear_window(rgba: &[u8], w: u32, h: u32) -> Option<[f32; 4]> {
    let mut min_x = w;
    let mut min_y = h;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    let mut any = false;
    for y in 0..h {
        for x in 0..w {
            if rgba[((y * w + x) * 4 + 3) as usize] != 0 {
                continue;
            }
            any = true;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    if !any {
        return None;
    }
    Some([
        min_x as f32 / w as f32,
        min_y as f32 / h as f32,
        (max_x - min_x + 1) as f32 / w as f32,
        (max_y - min_y + 1) as f32 / h as f32,
    ])
}

#[cfg(test)]
mod tests {
    use slot_gfx::{OUT_H, OUT_W};

    use super::*;

    fn px(face: &GameBezelFace) -> (u32, u32, u32, u32) {
        (
            (face.hole[0] * face.w as f32).round() as u32,
            (face.hole[1] * face.h as f32).round() as u32,
            (face.hole[2] * face.w as f32).round() as u32,
            (face.hole[3] * face.h as f32).round() as u32,
        )
    }

    #[test]
    fn the_shipped_bezels_open_on_the_same_clear_screen() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for name in ["bezels/GB/bezel_gb.png", "bezels/GBC/bezel_gbc.png"] {
            let face = game_bezel_face(&root.join(name)).expect(name);
            assert_eq!((face.w, face.h), (OUT_W, OUT_H), "{name}");
            assert_eq!(px(&face), (127, 33, 466, 418), "{name}");
        }
    }

    #[test]
    fn the_640_bezels_keep_their_own_clear_screen() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for name in [
            "bezels 640/GB/bezel_gb_640.png",
            "bezels 640/GBC/bezel_gbc_640.png",
        ] {
            let face = game_bezel_face(&root.join(name)).expect(name);
            assert_eq!((face.w, face.h), (640, 480), "{name}");
            assert_eq!(px(&face), (87, 33, 466, 418), "{name}");
        }
    }
}

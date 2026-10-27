use crate::bezel::{framed, screen_rect};
use crate::surface::{OUT_H, OUT_W};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sizing {
    Band,
    Integer,
    Fit,
    Panel,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GameLayout {
    pub panel: bool,
    pub rect: [f32; 4],
}

impl GameLayout {
    pub const BAND: GameLayout = GameLayout {
        panel: false,
        rect: [0.0, 0.0, OUT_W as f32, OUT_H as f32],
    };
}

pub fn game_layout(window: (u32, u32), picture: (u32, u32), sizing: Sizing) -> GameLayout {
    let framed = framed(window);
    let panel = match framed {
        true => window,
        false => (OUT_W, OUT_H),
    };
    let band = match framed {
        true => (panel.0, screen_rect(panel).3 as u32),
        false => panel,
    };
    let (pw, ph) = (picture.0.max(1), picture.1.max(1));
    let size = match sizing {
        Sizing::Band => return GameLayout::BAND,
        Sizing::Panel => {
            return GameLayout {
                panel: framed,
                ..GameLayout::BAND
            }
        }
        Sizing::Integer => {
            let k = (panel.0 / pw).min(panel.1 / ph).max(1);
            (pw * k, ph * k)
        }
        Sizing::Fit => {
            let s = (panel.0 as f32 / pw as f32).min(panel.1 as f32 / ph as f32);
            (
                (pw as f32 * s).round() as u32,
                (ph as f32 * s).round() as u32,
            )
        }
    };
    let area = match size.0 <= band.0 && size.1 <= band.1 {
        true => band,
        false => panel,
    };
    let x = area.0.saturating_sub(size.0) / 2;
    let y = area.1.saturating_sub(size.1) / 2;
    let (sx, sy) = (OUT_W as f32 / area.0 as f32, OUT_H as f32 / area.1 as f32);
    GameLayout {
        panel: framed && area != band,
        rect: [
            x as f32 * sx,
            y as f32 * sy,
            size.0 as f32 * sx,
            size.1 as f32 * sy,
        ],
    }
}

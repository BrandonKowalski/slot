use std::f32::consts::PI;

const BRIGHTEN_SCANLINES: f32 = 16.0;
const BRIGHTEN_LCD: f32 = 4.0;
const OFFSETS: [f32; 3] = [PI / 2.0, PI * (0.5 - 2.0 / 3.0), PI * (0.5 - 4.0 / 3.0)];

pub fn lcd3x_factors(x: f32, y: f32) -> [f32; 3] {
    let yfactor = (BRIGHTEN_SCANLINES + (2.0 * PI * y).sin()) / (BRIGHTEN_SCANLINES + 1.0);
    OFFSETS.map(|o| yfactor * (BRIGHTEN_LCD + (2.0 * PI * x + o).sin()) / (BRIGHTEN_LCD + 1.0))
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ScreenEffect {
    None,
    Lcd3x,
    Grid,
    Dot,
    Simpletex,
    Transparent,
}

impl ScreenEffect {
    pub fn mode(self) -> f32 {
        self as u8 as f32
    }
}

pub fn lcd3x_mask() -> [[[f32; 3]; 3]; 3] {
    let mut mask = [[[0.0f32; 3]; 3]; 3];
    for (oy, row) in mask.iter_mut().enumerate() {
        for (ox, cell) in row.iter_mut().enumerate() {
            *cell = lcd3x_factors((ox as f32 + 0.5) / 3.0, (oy as f32 + 0.5) / 3.0);
        }
    }
    mask
}

#[cfg(test)]
mod tests {
    use super::ScreenEffect;

    #[test]
    fn each_effect_has_the_mode_the_game_shader_branches_on() {
        // GAME_FRAG picks its branch from these numbers, highest first.
        assert_eq!(ScreenEffect::None.mode(), 0.0);
        assert_eq!(ScreenEffect::Lcd3x.mode(), 1.0);
        assert_eq!(ScreenEffect::Grid.mode(), 2.0);
        assert_eq!(ScreenEffect::Dot.mode(), 3.0);
        assert_eq!(ScreenEffect::Simpletex.mode(), 4.0);
        assert_eq!(ScreenEffect::Transparent.mode(), 5.0);
        assert!(crate::shaders::GAME_FRAG.contains("u_mode > 4.5"));
    }
}

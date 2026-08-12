/// Warmest step. Red is untouched and blue falls hardest, which is what makes the panel
/// read as warmer rather than dimmer.
const WARMEST: [f32; 3] = [1.0, 0.82, 0.62];

pub const BLUE_LIGHT_MAX: u8 = 9;

/// RGB gain for a blue light step, linear between neutral and [`WARMEST`]. Applied in the
/// final blit so it grades the chrome as well as the game.
pub fn blue_light_gain(step: u8) -> [f32; 3] {
    let t = step.min(BLUE_LIGHT_MAX) as f32 / BLUE_LIGHT_MAX as f32;
    let mut gain = [1.0f32; 3];
    for (g, warm) in gain.iter_mut().zip(WARMEST) {
        *g = 1.0 + (warm - 1.0) * t;
    }
    gain
}

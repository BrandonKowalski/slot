//! The link screen: its sprites, motion and drawing.

use slot_ui::TexId;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Sprite {
    pub tex: TexId,
    pub w: u32,
    pub h: u32,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct LinkSprites {
    pub port: Sprite,
    pub plug_host: Sprite,
    pub plug_join: Sprite,
    pub adapter: Sprite,
    pub glow_host: Sprite,
    pub glow_neutral: Sprite,
    pub arcs_right: [Sprite; 3],
    pub arcs_left: [Sprite; 3],
    pub clicks: Sprite,
    pub arrow_left: Sprite,
    pub arrow_right: Sprite,
}

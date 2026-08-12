use slot_input::{Action, Btn};
use slot_retro::ButtonMask;

/// The buttons the gesture layer let through to the game, held as a libretro mask. It is
/// level state, not edges, because that is what the core is polled for every frame.
#[derive(Default)]
pub struct Pad {
    mask: u16,
}

impl Pad {
    pub fn apply(&mut self, action: Action) {
        match action {
            Action::GbaDown(b) => {
                if let Some(bit) = bit(b) {
                    self.mask |= bit;
                }
            }
            Action::GbaUp(b) => {
                if let Some(bit) = bit(b) {
                    self.mask &= !bit;
                }
            }
            _ => {}
        }
    }

    pub fn mask(&self) -> ButtonMask {
        ButtonMask(self.mask)
    }

    /// Buttons pressed for the switcher are not the game's. Without this the game resumes
    /// holding whatever was down when the switcher took the input.
    pub fn clear(&mut self) {
        self.mask = 0;
    }
}

fn bit(btn: Btn) -> Option<u16> {
    Some(match btn {
        Btn::Up => ButtonMask::UP,
        Btn::Down => ButtonMask::DOWN,
        Btn::Left => ButtonMask::LEFT,
        Btn::Right => ButtonMask::RIGHT,
        Btn::A => ButtonMask::A,
        Btn::B => ButtonMask::B,
        Btn::L1 => ButtonMask::L,
        Btn::R1 => ButtonMask::R,
        Btn::Start => ButtonMask::START,
        Btn::Select => ButtonMask::SELECT,
        _ => return None,
    })
}

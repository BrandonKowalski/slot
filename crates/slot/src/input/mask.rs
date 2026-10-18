use slot_input::{Action, Btn};
use slot_retro::ButtonMask;
use slot_store::{Controls, GameButton, Modifier};

use super::bind::device_btn;

#[derive(Default)]
pub struct Pad {
    held: Vec<Btn>,
    chorded: Vec<Btn>,
}

impl Pad {
    pub fn apply(&mut self, action: Action) {
        let (list, b, down) = match action {
            Action::GbaDown(b) => (&mut self.held, b, true),
            Action::GbaUp(b) => (&mut self.held, b, false),
            Action::MenuChordDown(b) => (&mut self.chorded, b, true),
            Action::MenuChordUp(b) => (&mut self.chorded, b, false),
            _ => return,
        };
        list.retain(|x| *x != b);
        if down {
            list.push(b);
        }
    }

    pub fn mask(&self, controls: &Controls) -> ButtonMask {
        let bits = GameButton::ALL
            .into_iter()
            .filter(|g| {
                controls.game(*g).is_some_and(|binding| {
                    let b = device_btn(binding.button);
                    match binding.modifier {
                        None => self.held.contains(&b),
                        Some(Modifier::Menu) => self.chorded.contains(&b),
                        Some(Modifier::Select) => false,
                    }
                })
            })
            .fold(0, |bits, g| bits | bit(g));
        ButtonMask(bits)
    }

    pub fn clear(&mut self) {
        self.held.clear();
        self.chorded.clear();
    }
}

fn bit(g: GameButton) -> u16 {
    match g {
        GameButton::Up => ButtonMask::UP,
        GameButton::Down => ButtonMask::DOWN,
        GameButton::Left => ButtonMask::LEFT,
        GameButton::Right => ButtonMask::RIGHT,
        GameButton::A => ButtonMask::A,
        GameButton::B => ButtonMask::B,
        GameButton::Select => ButtonMask::SELECT,
        GameButton::Start => ButtonMask::START,
        GameButton::L => ButtonMask::L,
        GameButton::R => ButtonMask::R,
        GameButton::TurboA => ButtonMask::X,
        GameButton::TurboB => ButtonMask::Y,
    }
}

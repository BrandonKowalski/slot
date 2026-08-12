use gilrs::Button;
use slot_input::Btn;

/// Face buttons map by position, not by name: the RG SP has A to the right of B, which is
/// East and South on a pad.
pub fn pad_to_btn(button: Button) -> Option<Btn> {
    Some(match button {
        Button::DPadUp => Btn::Up,
        Button::DPadDown => Btn::Down,
        Button::DPadLeft => Btn::Left,
        Button::DPadRight => Btn::Right,
        Button::East => Btn::A,
        Button::South => Btn::B,
        // The top face button, where the RG SP puts X.
        Button::North => Btn::X,
        Button::West => Btn::Y,
        Button::LeftTrigger => Btn::L1,
        Button::RightTrigger => Btn::R1,
        Button::LeftTrigger2 => Btn::L2,
        Button::RightTrigger2 => Btn::R2,
        Button::Start => Btn::Start,
        Button::Select => Btn::Select,
        Button::Mode => Btn::Menu,
        _ => return None,
    })
}

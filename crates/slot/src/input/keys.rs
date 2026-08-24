use slot_input::Btn;
use winit::keyboard::KeyCode;

/// Physical positions, not layout characters, so the map holds on a non QWERTY keyboard.
pub fn key_to_btn(code: KeyCode) -> Option<Btn> {
    Some(match code {
        KeyCode::ArrowUp => Btn::Up,
        KeyCode::ArrowDown => Btn::Down,
        KeyCode::ArrowLeft => Btn::Left,
        KeyCode::ArrowRight => Btn::Right,
        KeyCode::KeyZ => Btn::A,
        KeyCode::KeyX => Btn::B,
        // Not physical X, which is already GBA's B.
        KeyCode::KeyC => Btn::X,
        KeyCode::KeyV => Btn::Y,
        KeyCode::KeyA => Btn::L1,
        KeyCode::KeyS => Btn::R1,
        KeyCode::KeyQ => Btn::L2,
        KeyCode::KeyW => Btn::R2,
        KeyCode::Enter => Btn::Start,
        KeyCode::ShiftRight => Btn::Select,
        // Backquote as well as Tab: macOS drives its key view loop from Tab and beeps when
        // nothing consumes it, which on a 2 s hold autorepeats into a stream of beeps.
        KeyCode::Tab | KeyCode::Backquote => Btn::Menu,
        KeyCode::Equal => Btn::VolUp,
        KeyCode::Minus => Btn::VolDown,
        // Deliberate reaches, both of them, and that is the whole reason they are here.
        // These are the only two buttons that end a live link session — the device treats
        // power and the lid as "this session is over", correctly, and a link cannot be
        // resumed once it is. On hardware neither is reachable by accident: power is a
        // dedicated button and the lid is a hinge. On a keyboard they were Escape and L,
        // which is to say the universal back-out key and a letter, and both ended a running
        // link the moment a finger strayed. Escape especially: the instinct that leaves a
        // menu should not be the one that drops your friend.
        KeyCode::Backslash => Btn::Power,
        KeyCode::BracketRight => Btn::Lid,
        _ => return None,
    })
}

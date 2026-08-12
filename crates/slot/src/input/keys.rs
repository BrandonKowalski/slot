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
        KeyCode::Escape => Btn::Power,
        KeyCode::KeyL => Btn::Lid,
        _ => return None,
    })
}

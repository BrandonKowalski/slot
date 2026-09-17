use gilrs::{EventType, Gilrs};
use slot_input::{Btn, InputSource, Millis, RawEvent};
use winit::event::WindowEvent;
use winit::keyboard::{KeyCode, PhysicalKey};

use super::keys::key_to_btn;
use super::pad::pad_to_btn;

pub struct HostInput {
    pending: Vec<RawEvent>,
    pad: Option<Gilrs>,
    lid_closed: bool,
    /// Which buttons the *keyboard* is currently holding down. Kept only so they can be let go
    /// of when the window stops being the one receiving keys — see `on_window_event`. The
    /// gamepad is not in here: gilrs reads the device rather than the window, so its releases
    /// arrive whether or not slot has focus.
    held_keys: Vec<Btn>,
}

impl HostInput {
    pub fn new() -> Self {
        HostInput {
            pending: Vec::new(),
            pad: Gilrs::new().ok(),
            lid_closed: false,
            held_keys: Vec::new(),
        }
    }

    pub fn on_window_event(&mut self, event: &WindowEvent) {
        match event {
            // A window that loses focus does not get the releases for the keys that were down
            // in it: the key-up is delivered to whatever took focus, and this never hears about
            // it. Nothing downstream can tell that apart from a finger that is still there, so
            // the bit stays set on the pad for the rest of the session and the game goes on
            // holding it — cmd-tab away with Z down and the game is still pressing A when the
            // window comes back. Pressing and releasing that one key again is the only thing
            // that clears it, and by then the player is hunting for which key it was.
            //
            // So the keys are let go of here instead, which is what actually happened: the
            // window stopped being the thing the keyboard was talking to.
            WindowEvent::Focused(false) => self.release_keys(),
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                self.key(code, event.state.is_pressed(), event.repeat);
            }
            _ => {}
        }
    }

    /// Every key still down, released once each. Nothing is released twice: the list is taken
    /// rather than read, so a second focus loss with no key pressed in between has nothing to
    /// let go of.
    fn release_keys(&mut self) {
        for btn in std::mem::take(&mut self.held_keys) {
            self.pending.push(RawEvent::Up(btn));
        }
    }

    pub fn key(&mut self, code: KeyCode, pressed: bool, repeat: bool) {
        // Autorepeat is a stream of presses with no release, which would re-arm every hold
        // and double tap window downstream.
        if repeat {
            return;
        }
        let Some(btn) = key_to_btn(code) else {
            return;
        };
        if btn == Btn::Lid {
            // The host has no hinge, so L latches: press to close, press again to open.
            if pressed {
                self.lid_closed = !self.lid_closed;
                self.pending.push(if self.lid_closed {
                    RawEvent::Down(Btn::Lid)
                } else {
                    RawEvent::Up(Btn::Lid)
                });
            }
            return;
        }
        // Written down before it is sent, so a focus loss knows what is still down. The lid is
        // above this and stays out of it: it latches rather than being held, so there is no
        // finger on it to come off.
        match pressed {
            true if !self.held_keys.contains(&btn) => self.held_keys.push(btn),
            true => {}
            false => self.held_keys.retain(|b| *b != btn),
        }
        self.push(btn, pressed);
    }

    fn push(&mut self, btn: Btn, pressed: bool) {
        self.pending.push(if pressed {
            RawEvent::Down(btn)
        } else {
            RawEvent::Up(btn)
        });
    }

    fn drain_pad(&mut self) {
        let Some(pad) = self.pad.as_mut() else {
            return;
        };
        let mut edges = Vec::new();
        while let Some(ev) = pad.next_event() {
            match ev.event {
                EventType::ButtonPressed(b, _) => edges.push((b, true)),
                EventType::ButtonReleased(b, _) => edges.push((b, false)),
                _ => {}
            }
        }
        for (b, pressed) in edges {
            if let Some(btn) = pad_to_btn(b) {
                self.push(btn, pressed);
            }
        }
    }
}

impl Default for HostInput {
    fn default() -> Self {
        HostInput::new()
    }
}

impl InputSource for HostInput {
    fn poll(&mut self, _now: Millis) -> Vec<RawEvent> {
        self.drain_pad();
        std::mem::take(&mut self.pending)
    }
}

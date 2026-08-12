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
}

impl HostInput {
    pub fn new() -> Self {
        HostInput {
            pending: Vec::new(),
            pad: Gilrs::new().ok(),
            lid_closed: false,
        }
    }

    pub fn on_window_event(&mut self, event: &WindowEvent) {
        let WindowEvent::KeyboardInput { event, .. } = event else {
            return;
        };
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        self.key(code, event.state.is_pressed(), event.repeat);
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

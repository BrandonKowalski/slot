use std::fmt;
use std::path::Path;

use crate::link::Link;
use crate::rumble::Rumble;

pub const GBA_W: u32 = 240;
pub const GBA_H: u32 = 160;

/// libretro `RETRO_DEVICE_ID_JOYPAD` bit order. Y and X have no GBA equivalent, so bits 1
/// and 9 are never set.
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
pub struct ButtonMask(pub u16);

impl ButtonMask {
    pub const B: u16 = 1 << 0;
    pub const SELECT: u16 = 1 << 2;
    pub const START: u16 = 1 << 3;
    pub const UP: u16 = 1 << 4;
    pub const DOWN: u16 = 1 << 5;
    pub const LEFT: u16 = 1 << 6;
    pub const RIGHT: u16 = 1 << 7;
    pub const A: u16 = 1 << 8;
    pub const L: u16 = 1 << 10;
    pub const R: u16 = 1 << 11;
}

#[derive(Copy, Clone, Debug)]
pub struct AvInfo {
    pub fps: f64,
    pub sample_rate: f64,
}

#[derive(Debug)]
pub enum CoreError {
    Io(std::io::Error),
    Load(String),
    Unsupported(String),
    State(String),
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreError::Io(e) => write!(f, "io: {e}"),
            CoreError::Load(m) => write!(f, "load: {m}"),
            CoreError::Unsupported(m) => write!(f, "unsupported: {m}"),
            CoreError::State(m) => write!(f, "state: {m}"),
        }
    }
}

impl std::error::Error for CoreError {}

impl From<std::io::Error> for CoreError {
    fn from(e: std::io::Error) -> Self {
        CoreError::Io(e)
    }
}

pub trait RetroCore: Send {
    fn load(&mut self, rom: &Path) -> Result<(), CoreError>;
    fn run_frame(&mut self, input: ButtonMask);
    /// One frame with two players' buttons: `p1` on input port 0, `p2` on port 1. Only a core
    /// running two linked GBAs reads port 1, so the default runs `p1` alone and drops `p2`.
    fn run_frame_linked(&mut self, p1: ButtonMask, _p2: ButtonMask) {
        self.run_frame(p1);
    }
    /// Whether the *next* `run_frame` should emulate without drawing a picture. A skipped frame
    /// advances the machine exactly as a drawn one does and leaves `video_xrgb8888` holding the
    /// last picture that was drawn.
    ///
    /// Called before every frame of a fast forward present so only the frame that will actually
    /// be shown costs a render — the one change the spike measured that moves the speed cap at
    /// all. It has to be said *before* the frame runs, because that is the only moment either
    /// real core can still be told: both decide whether to draw the frame they are about to run
    /// at the top of `retro_run`.
    ///
    /// A core with no way to skip a render draws every frame, which is correct but slower, so
    /// the default does nothing.
    fn set_frame_skip(&mut self, _skip: bool) {}
    /// `GBA_W * GBA_H * 4` bytes, little endian XRGB8888, so the byte order is B, G, R, unused.
    fn video_xrgb8888(&self) -> &[u8];
    fn take_audio(&mut self) -> Vec<i16>;
    fn serialize(&mut self) -> Result<Vec<u8>, CoreError>;
    fn unserialize(&mut self, data: &[u8]) -> Result<(), CoreError>;
    fn save_ram(&self) -> Option<Vec<u8>>;
    fn load_save_ram(&mut self, data: &[u8]) -> Result<(), CoreError>;
    fn av_info(&self) -> AvInfo;
    /// Where this core's rumble lands. A core that was never offered the interface, or one
    /// that turned it down, hands back a cell nothing ever writes.
    fn rumble(&self) -> Rumble {
        Rumble::default()
    }
    /// Where this core's serial traffic — link cable, wireless adapter — goes. Only gpSP ever
    /// answers this for real; a core that never registers libretro's netpacket interface
    /// hands back a handle nothing ever reads from or writes to, the same shape `rumble`
    /// above uses for a core with no motor.
    fn net(&self) -> Link {
        Link::default()
    }
    /// Begins a netpacket session on this core, if it carries one. `client_id` is libretro's
    /// own: 0 the host, 1 the joiner — the only two this product has (RFU supports four; see
    /// the plan for why this stays at two). A core with no serial traffic of its own — every
    /// core but gpSP — has nothing to start, so the default does nothing.
    fn start_link(&mut self, _client_id: u16) {}
    /// Once per frame: hand the core anything the transport put in its inbound queue since
    /// the last call, then let the core do its own polling. A core that never registered
    /// netpacket has nothing to drain, so the default is a no-op — only `LibretroCore` (gpSP)
    /// ever overrides this.
    fn pump_link(&mut self) {}
    /// Ends a netpacket session on this core: `start_link`'s counterpart, called once
    /// whatever was carrying the session's traffic is going away. libretro documents `stop`
    /// as OPTIONAL — unlike `start`, a spec-compliant core may leave it NULL — so a core that
    /// never offered one simply has nothing to hear this through, and the default does
    /// nothing.
    fn stop_link(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records what `run_frame` was handed, to see what the default `run_frame_linked` passes on.
    #[derive(Default)]
    struct Recorder(Vec<ButtonMask>);

    impl RetroCore for Recorder {
        fn load(&mut self, _rom: &Path) -> Result<(), CoreError> {
            Ok(())
        }
        fn run_frame(&mut self, input: ButtonMask) {
            self.0.push(input);
        }
        fn video_xrgb8888(&self) -> &[u8] {
            &[]
        }
        fn take_audio(&mut self) -> Vec<i16> {
            Vec::new()
        }
        fn serialize(&mut self) -> Result<Vec<u8>, CoreError> {
            Ok(Vec::new())
        }
        fn unserialize(&mut self, _data: &[u8]) -> Result<(), CoreError> {
            Ok(())
        }
        fn save_ram(&self) -> Option<Vec<u8>> {
            None
        }
        fn load_save_ram(&mut self, _data: &[u8]) -> Result<(), CoreError> {
            Ok(())
        }
        fn av_info(&self) -> AvInfo {
            AvInfo {
                fps: 60.0,
                sample_rate: 48_000.0,
            }
        }
    }

    #[test]
    fn a_core_without_link_mode_runs_player_1_alone() {
        let mut core = Recorder::default();
        core.run_frame_linked(ButtonMask(ButtonMask::A), ButtonMask(ButtonMask::B));
        assert_eq!(core.0, vec![ButtonMask(ButtonMask::A)]);
    }
}

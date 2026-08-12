use std::fmt;
use std::sync::Arc;

use super::ring::Ring;

#[derive(Debug)]
pub enum AudioError {
    NoDevice,
    Config(String),
    Device(String),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioError::NoDevice => write!(f, "no default output device"),
            AudioError::Config(m) => write!(f, "unusable output config: {m}"),
            AudioError::Device(m) => write!(f, "audio device: {m}"),
        }
    }
}

impl std::error::Error for AudioError {}

/// The device, and nothing else. Everything about occupancy, rate and muting belongs to the
/// ring it drains, which is what lets the emulator and the UI both write to it without
/// either of them owning the hardware.
pub trait AudioSink: Send {
    /// The rate is a preference. A device that will not take it opens at its own, which the
    /// ring then reports and the resampler converts to.
    fn open(&mut self, sample_rate: u32) -> Result<(), AudioError>;
    fn ring(&self) -> Arc<Ring>;
}

mod alsa;
#[cfg(feature = "host")]
mod host;
mod ring;
mod sfx;
mod sink;
mod stub;
pub mod volume;

pub use alsa::AlsaSink;
#[cfg(feature = "host")]
pub use host::HostAudio;
pub use ring::{ring_capacity, Ring};
pub use sfx::Sfx;
pub use sink::{AudioError, AudioSink};
pub use stub::StubSink;

/// The GBA's own rate. slot plays nothing else, so the device is opened for it before there
/// is a core to ask, and a device that takes it needs no resampling at all.
pub const GBA_HZ: u32 = 32_768;

/// The sink this build talks to: cpal on a desktop, ALSA on the device. A sink that fails to
/// open is not a failure to boot either way, since a ring nothing drains still lets the
/// emulator run.
#[cfg(feature = "host")]
pub fn open_sink() -> Box<dyn AudioSink> {
    Box::new(HostAudio::new())
}

#[cfg(not(feature = "host"))]
pub fn open_sink() -> Box<dyn AudioSink> {
    Box::new(AlsaSink::new())
}

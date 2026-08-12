use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use super::ring::Ring;
use super::sink::{AudioError, AudioSink};

pub struct HostAudio {
    ring: Arc<Ring>,
    device: Option<Device>,
}

/// A cpal stream is not `Send` on every backend, so it is built, played and dropped on a
/// thread of its own; the sink itself carries nothing but the ring.
struct Device {
    stop: mpsc::Sender<()>,
    join: JoinHandle<()>,
}

impl HostAudio {
    pub fn new() -> Self {
        HostAudio {
            ring: Arc::new(Ring::new(0)),
            device: None,
        }
    }

    fn close(&mut self) {
        if let Some(d) = self.device.take() {
            drop(d.stop);
            let _ = d.join.join();
        }
    }
}

impl Default for HostAudio {
    fn default() -> Self {
        HostAudio::new()
    }
}

impl Drop for HostAudio {
    fn drop(&mut self) {
        self.close();
    }
}

impl AudioSink for HostAudio {
    fn open(&mut self, sample_rate: u32) -> Result<(), AudioError> {
        self.close();
        let ring = self.ring.clone();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let join = std::thread::Builder::new()
            .name("slot-audio".into())
            .spawn(move || match open_stream(&ring, sample_rate) {
                Ok(stream) => {
                    let _ = ready_tx.send(Ok(()));
                    let _ = stop_rx.recv();
                    drop(stream);
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            })
            .map_err(|e| AudioError::Device(e.to_string()))?;
        match ready_rx.recv() {
            Ok(Ok(())) => {
                self.device = Some(Device {
                    stop: stop_tx,
                    join,
                });
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = join.join();
                Err(e)
            }
            Err(_) => {
                let _ = join.join();
                Err(AudioError::Device("output thread stopped".into()))
            }
        }
    }

    fn ring(&self) -> Arc<Ring> {
        self.ring.clone()
    }
}

fn open_stream(ring: &Arc<Ring>, sample_rate: u32) -> Result<cpal::Stream, AudioError> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or(AudioError::NoDevice)?;
    let fallback = device
        .default_output_config()
        .map_err(|e| AudioError::Device(e.to_string()))?;
    let mut last = None;
    // The core's rate is preferred so no resampling is needed, but a device that refuses it
    // is not a fatal error: DRC resamples to whatever rate the device did accept.
    for config in [exact_rate(&device, sample_rate), Some(fallback)]
        .into_iter()
        .flatten()
    {
        match build(&device, config, ring) {
            Ok(stream) => return Ok(stream),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or(AudioError::NoDevice))
}

fn exact_rate(device: &cpal::Device, sample_rate: u32) -> Option<cpal::SupportedStreamConfig> {
    let want = cpal::SampleRate(sample_rate);
    device.supported_output_configs().ok()?.find_map(|r| {
        let usable = r.channels() == 2
            && r.sample_format() == cpal::SampleFormat::F32
            && r.min_sample_rate() <= want
            && want <= r.max_sample_rate();
        usable.then(|| r.with_sample_rate(want))
    })
}

/// Ask for a period rather than taking whatever the device felt like, so the ring's
/// occupancy target is a known quantity. About 11 ms at 48 kHz, a sixth of the target.
const PERIOD_FRAMES: u32 = 512;

fn build(
    device: &cpal::Device,
    config: cpal::SupportedStreamConfig,
    ring: &Arc<Ring>,
) -> Result<cpal::Stream, AudioError> {
    if config.channels() != 2 || config.sample_format() != cpal::SampleFormat::F32 {
        return Err(AudioError::Config(format!(
            "{} channels, {}",
            config.channels(),
            config.sample_format()
        )));
    }
    ring.reopen(config.sample_rate().0);
    let mut cfg = config.config();
    let fixed = match *config.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max } => Some(PERIOD_FRAMES.clamp(min, max)),
        cpal::SupportedBufferSize::Unknown => None,
    };
    let mut last = None;
    // A device that will not take the size it advertised is still a device worth having.
    for size in [
        fixed.map(cpal::BufferSize::Fixed),
        Some(cpal::BufferSize::Default),
    ]
    .into_iter()
    .flatten()
    {
        cfg.buffer_size = size;
        match start(device, &cfg, ring) {
            Ok(s) => return Ok(s),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or(AudioError::NoDevice))
}

fn start(
    device: &cpal::Device,
    cfg: &cpal::StreamConfig,
    ring: &Arc<Ring>,
) -> Result<cpal::Stream, AudioError> {
    let ring = ring.clone();
    let mut scratch: Vec<i16> = Vec::new();
    let stream = device
        .build_output_stream(
            cfg,
            move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                if scratch.len() < out.len() {
                    scratch.resize(out.len(), 0);
                }
                let taken = &mut scratch[..out.len()];
                ring.fill(taken);
                for (o, s) in out.iter_mut().zip(taken.iter()) {
                    *o = f32::from(*s) / 32768.0;
                }
            },
            |e| eprintln!("slot: audio: {e}"),
            None,
        )
        .map_err(|e| AudioError::Device(e.to_string()))?;
    stream
        .play()
        .map_err(|e| AudioError::Device(e.to_string()))?;
    Ok(stream)
}

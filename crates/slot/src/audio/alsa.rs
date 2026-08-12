//! ALSA through `dlopen`, exactly as the libretro core is loaded. Linking `libasound` would
//! put a rootfs dependency on the cross build for a library the device already has, and the
//! PCM entry points needed here are six.

use std::ffi::{c_char, c_int, c_uint, c_void, CStr, CString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;

use libloading::Library;

use super::ring::Ring;
use super::sink::{AudioError, AudioSink};

const SND_PCM_STREAM_PLAYBACK: c_int = 0;
const SND_PCM_FORMAT_S16_LE: c_int = 2;
const SND_PCM_ACCESS_RW_INTERLEAVED: c_int = 3;
const CHANNELS: c_uint = 2;

/// What ALSA is asked to buffer. Two of these are about what the ring targets, so the device
/// and the emulator agree on how far ahead the audio runs.
const LATENCY_US: c_uint = 40_000;

/// Frames per write. One video frame at the GBA's rate, so the writer wakes at about the
/// rate the emulator produces rather than in bursts.
const PERIOD_FRAMES: usize = 512;

/// Most specific first. `plug:default` leads because both halves are needed and neither is
/// optional: `default` is where the card's `asound.conf` lives, and on the H700 that file is
/// what switches the codec's speaker and line out on — open anything else and the PCM streams
/// perfectly into outputs that are still muted. The `plug:` wrapper is what makes `default`
/// reachable at all, since it resolves to a raw `hw:` slave that cannot convert the GBA's
/// 32768 Hz to the 32000 the codec runs at, and a bare `default` fails on the rate alone.
const DEVICES: [&str; 4] = ["plug:default", "default", "plughw:0,0", "hw:0,0"];

type PcmOpen = unsafe extern "C" fn(*mut *mut c_void, *const c_char, c_int, c_int) -> c_int;
type PcmSetParams =
    unsafe extern "C" fn(*mut c_void, c_int, c_int, c_uint, c_uint, c_int, c_uint) -> c_int;
type PcmWritei = unsafe extern "C" fn(*mut c_void, *const c_void, u64) -> i64;
type PcmRecover = unsafe extern "C" fn(*mut c_void, c_int, c_int) -> c_int;
type PcmDrop = unsafe extern "C" fn(*mut c_void) -> c_int;
type PcmClose = unsafe extern "C" fn(*mut c_void) -> c_int;
type StrError = unsafe extern "C" fn(c_int) -> *const c_char;

struct Alsa {
    open: PcmOpen,
    set_params: PcmSetParams,
    writei: PcmWritei,
    recover: PcmRecover,
    drop: PcmDrop,
    close: PcmClose,
    strerror: StrError,
    _lib: Library,
}

impl Alsa {
    fn load() -> Result<Self, AudioError> {
        let lib = unsafe { Library::new("libasound.so.2") }
            .or_else(|_| unsafe { Library::new("libasound.so") })
            .map_err(|e| AudioError::Device(format!("libasound: {e}")))?;
        macro_rules! get {
            ($name:literal) => {
                *lib.get(concat!($name, "\0").as_bytes())
                    .map_err(|e| AudioError::Device(format!("{}: {e}", $name)))?
            };
        }
        unsafe {
            Ok(Alsa {
                open: get!("snd_pcm_open"),
                set_params: get!("snd_pcm_set_params"),
                writei: get!("snd_pcm_writei"),
                recover: get!("snd_pcm_recover"),
                drop: get!("snd_pcm_drop"),
                close: get!("snd_pcm_close"),
                strerror: get!("snd_strerror"),
                _lib: lib,
            })
        }
    }

    fn message(&self, err: c_int) -> String {
        let text = unsafe { (self.strerror)(err) };
        match text.is_null() {
            true => format!("error {err}"),
            false => unsafe { CStr::from_ptr(text) }
                .to_string_lossy()
                .into_owned(),
        }
    }

    fn open_pcm(&self, rate: u32) -> Result<*mut c_void, AudioError> {
        let mut last = AudioError::NoDevice;
        for name in DEVICES {
            let Ok(cname) = CString::new(name) else {
                continue;
            };
            let mut pcm: *mut c_void = std::ptr::null_mut();
            let err = unsafe {
                (self.open)(
                    &mut pcm,
                    cname.as_ptr(),
                    SND_PCM_STREAM_PLAYBACK,
                    // Blocking: the write is what paces the worker, and a nonblocking one
                    // would need a poll loop to do the same job.
                    0,
                )
            };
            if err < 0 || pcm.is_null() {
                last = AudioError::Device(format!("{name}: {}", self.message(err)));
                continue;
            }
            // Resampling is asked of ALSA rather than of the frontend: the GBA's 32768 Hz is
            // not a rate every codec offers, and one conversion is better than two.
            let err = unsafe {
                (self.set_params)(
                    pcm,
                    SND_PCM_FORMAT_S16_LE,
                    SND_PCM_ACCESS_RW_INTERLEAVED,
                    CHANNELS,
                    rate,
                    1,
                    LATENCY_US,
                )
            };
            if err < 0 {
                unsafe { (self.close)(pcm) };
                last = AudioError::Config(format!("{name}: {}", self.message(err)));
                continue;
            }
            eprintln!("slot: audio {name} at {rate} Hz");
            return Ok(pcm);
        }
        Err(last)
    }
}

pub struct AlsaSink {
    ring: Arc<Ring>,
    device: Option<Device>,
}

struct Device {
    stop: Arc<AtomicBool>,
    join: JoinHandle<()>,
}

impl AlsaSink {
    pub fn new() -> Self {
        AlsaSink {
            ring: Arc::new(Ring::new(0)),
            device: None,
        }
    }

    fn close(&mut self) {
        if let Some(d) = self.device.take() {
            d.stop.store(true, Ordering::Relaxed);
            let _ = d.join.join();
        }
    }
}

impl Default for AlsaSink {
    fn default() -> Self {
        AlsaSink::new()
    }
}

impl Drop for AlsaSink {
    fn drop(&mut self) {
        self.close();
    }
}

impl AudioSink for AlsaSink {
    /// The PCM is opened on the thread that writes to it, so the handle never crosses one.
    fn open(&mut self, sample_rate: u32) -> Result<(), AudioError> {
        self.close();
        let ring = self.ring.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let (ready_tx, ready_rx) = mpsc::channel();
        let join = std::thread::Builder::new()
            .name("slot-audio".into())
            .spawn(move || match play(&ring, sample_rate) {
                Ok(device) => {
                    let _ = ready_tx.send(Ok(()));
                    device.run(&ring, &flag);
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            })
            .map_err(|e| AudioError::Device(e.to_string()))?;
        match ready_rx.recv() {
            Ok(Ok(())) => {
                self.device = Some(Device { stop, join });
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

/// The open PCM and the library it came from, both owned by the writing thread.
struct Playback {
    alsa: Alsa,
    pcm: *mut c_void,
}

fn play(ring: &Arc<Ring>, sample_rate: u32) -> Result<Playback, AudioError> {
    let alsa = Alsa::load()?;
    let pcm = alsa.open_pcm(sample_rate)?;
    ring.reopen(sample_rate);
    Ok(Playback { alsa, pcm })
}

impl Playback {
    /// A blocking write per period, which is what paces the whole frontend: the emulator
    /// runs against the ring and the ring drains at exactly the rate the codec plays.
    fn run(&self, ring: &Ring, stop: &AtomicBool) {
        let mut buf = vec![0i16; PERIOD_FRAMES * CHANNELS as usize];
        while !stop.load(Ordering::Relaxed) {
            ring.fill(&mut buf);
            let mut written = 0;
            while written < PERIOD_FRAMES {
                let at = written * CHANNELS as usize;
                let frames = unsafe {
                    (self.alsa.writei)(
                        self.pcm,
                        buf[at..].as_ptr() as *const c_void,
                        (PERIOD_FRAMES - written) as u64,
                    )
                };
                if frames < 0 {
                    // An underrun is recoverable and routine on a device that just came back
                    // from a doze. Anything else ends the stream.
                    let err = unsafe { (self.alsa.recover)(self.pcm, frames as c_int, 1) };
                    if err < 0 {
                        eprintln!("slot: audio: {}", self.alsa.message(err));
                        return;
                    }
                    continue;
                }
                written += frames as usize;
            }
        }
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        unsafe {
            // Drop rather than drain: what is still queued is audio for a session that has
            // already ended, and draining would block the close on playing all of it.
            (self.alsa.drop)(self.pcm);
            (self.alsa.close)(self.pcm);
        }
    }
}

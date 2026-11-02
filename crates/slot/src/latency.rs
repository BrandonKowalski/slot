use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

pub const TRACE_VAR: &str = "SLOT_TRACE_LATENCY";
pub const FLAG_FILE: &str = "System/trace-latency.on";

static READ: AtomicU64 = AtomicU64::new(0);
static APPLIED: AtomicU64 = AtomicU64::new(0);
static EMU: AtomicU64 = AtomicU64::new(0);
static PUBLISHED: AtomicU64 = AtomicU64::new(0);
static TAKEN: AtomicU64 = AtomicU64::new(0);
static BITS: AtomicU16 = AtomicU16::new(0);

struct ResponseTrace {
    file: File,
    selected: Option<[u8; 3]>,
    shown: Option<[u8; 3]>,
    failed: bool,
}

impl ResponseTrace {
    fn picture(&mut self, frame: &[u8]) {
        if let Some(pixel) = frame.get(..4) {
            self.selected = Some([pixel[2], pixel[1], pixel[0]]);
        }
    }

    fn swapped(&mut self, monotonic_ns: u64) -> io::Result<()> {
        if let Some([r, g, b]) = self.selected.filter(|colour| Some(*colour) != self.shown) {
            writeln!(self.file, "{monotonic_ns},{r},{g},{b}")?;
            self.shown = self.selected;
        }
        Ok(())
    }
}

fn response() -> Option<&'static Mutex<ResponseTrace>> {
    static RESPONSE: OnceLock<Option<Mutex<ResponseTrace>>> = OnceLock::new();
    RESPONSE
        .get_or_init(|| {
            let path = std::env::var_os("SLOT_TRACE_RESPONSE")?;
            match OpenOptions::new().create(true).append(true).open(&path) {
                Ok(file) => Some(Mutex::new(ResponseTrace {
                    file,
                    selected: None,
                    shown: None,
                    failed: false,
                })),
                Err(error) => {
                    eprintln!(
                        "slot: response trace: cannot open {}: {error}",
                        std::path::Path::new(&path).display()
                    );
                    None
                }
            }
        })
        .as_ref()
}

/// Track the source image selected for the next composition, before its swap.
pub fn picture(frame: &[u8]) {
    if let Some(trace) = response() {
        let mut trace = trace.lock().unwrap_or_else(|error| error.into_inner());
        if !trace.failed {
            trace.picture(frame);
        }
    }
}

fn response_swapped() {
    let Some(trace) = response() else {
        return;
    };
    let mut trace = trace.lock().unwrap_or_else(|error| error.into_inner());
    if trace.failed || trace.selected == trace.shown {
        return;
    }
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: time is a writable timespec and CLOCK_MONOTONIC requires no other state.
    if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) } != 0 {
        trace.failed = true;
        eprintln!(
            "slot: response trace: monotonic clock: {}",
            io::Error::last_os_error()
        );
        return;
    }
    let monotonic_ns = time.tv_sec as u64 * 1_000_000_000 + time.tv_nsec as u64;
    if let Err(error) = trace.swapped(monotonic_ns) {
        trace.failed = true;
        eprintln!("slot: response trace: write failed: {error}");
    }
}

fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        let card = std::env::var_os("SLOT_ROOT").unwrap_or_else(|| "/mnt/sdcard".into());
        std::env::var_os(TRACE_VAR).is_some_and(|v| v != "0")
            || std::path::Path::new(&card).join(FLAG_FILE).is_file()
    })
}

fn now() -> u64 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed().as_nanos() as u64 + 1
}

fn stamp(slot: &AtomicU64, after: &AtomicU64) -> bool {
    if after.load(Ordering::Acquire) == 0 {
        return false;
    }
    slot.compare_exchange(0, now(), Ordering::AcqRel, Ordering::Relaxed)
        .is_ok()
}

pub fn read() {
    if on() {
        let _ = READ.compare_exchange(0, now(), Ordering::AcqRel, Ordering::Relaxed);
    }
}

pub fn applied(before: u16, after: u16) {
    let added = after & !before;
    if on() && added != 0 && READ.load(Ordering::Acquire) != 0 {
        BITS.store(added, Ordering::Release);
        stamp(&APPLIED, &READ);
    }
}

pub fn emu_frame(mask: u16) {
    if on() && mask & BITS.load(Ordering::Acquire) != 0 {
        stamp(&EMU, &APPLIED);
    }
}

pub fn published() {
    if on() {
        stamp(&PUBLISHED, &EMU);
    }
}

pub fn taken() {
    if on() {
        stamp(&TAKEN, &PUBLISHED);
    }
}

pub fn swapped(swap_ms: f64) {
    response_swapped();
    if !on() || TAKEN.load(Ordering::Acquire) == 0 {
        return;
    }
    let end = now();
    let [r, a, e, p, t] =
        [&READ, &APPLIED, &EMU, &PUBLISHED, &TAKEN].map(|s| s.swap(0, Ordering::AcqRel));
    BITS.store(0, Ordering::Release);
    let ms = |from: u64, to: u64| to.saturating_sub(from) as f64 / 1e6;
    eprintln!(
        "slot: latency: read>pad {:.1} pad>emu {:.1} emu>pub {:.1} pub>take {:.1} take>swapped {:.1} (swap {:.1}) total {:.1} ms",
        ms(r, a),
        ms(a, e),
        ms(e, p),
        ms(p, t),
        ms(t, end),
        swap_ms,
        ms(r, end)
    );
}

pub fn tracing() -> bool {
    on()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_rows_follow_swapped_source_colour_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("response.csv");
        let mut trace = ResponseTrace {
            file: File::create(&path).unwrap(),
            selected: None,
            shown: None,
            failed: false,
        };
        trace.swapped(100).unwrap();
        trace.picture(&[0, 0, 255, 0]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
        trace.swapped(200).unwrap();
        trace.swapped(300).unwrap();
        trace.picture(&[0, 127, 128, 0]);
        trace.swapped(400).unwrap();
        trace.picture(&[0, 255, 0, 0]);
        trace.picture(&[0, 255, 0, 0]);
        trace.swapped(500).unwrap();
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "200,255,0,0\n400,128,127,0\n500,0,255,0\n"
        );
    }
}

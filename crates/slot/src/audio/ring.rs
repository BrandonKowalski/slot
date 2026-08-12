use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// About eight video frames of audio. Four left the device one late emulator frame from
/// empty and the emulator one late callback from full, and both ends click.
pub fn ring_capacity(sample_rate: u32) -> usize {
    sample_rate as usize * 8 / 60
}

/// The longest the emulator may be held waiting for room. Past this the device is not
/// draining at all, and a stall there has to cost the audio rather than the game.
const MAX_WAIT: Duration = Duration::from_millis(50);

struct Inner {
    buf: Vec<i16>,
    head: usize,
    len: usize,
}

/// Interleaved stereo between the emulator thread and the device callback.
pub struct Ring {
    inner: Mutex<Inner>,
    /// Signalled by every device read, waited on by a producer with nowhere to put a block.
    room: Condvar,
    /// Mirror of `Inner::len` so occupancy can be read without contending with the callback.
    queued: AtomicUsize,
    capacity: AtomicUsize,
    rate: AtomicU32,
    muted: AtomicBool,
    /// Whether anything is expected to be feeding this ring. Nothing is while the core is
    /// held still, and a dry ring is only a fault when something should be filling it.
    idle: AtomicBool,
    overruns: AtomicU64,
    underruns: AtomicU64,
    /// Set by every device read, cleared when a wait for room expires. Pacing against a ring
    /// nothing drains would hold the emulator up for a device that is not coming back.
    consuming: AtomicBool,
    /// Rebuilding the cushion after running dry. The device is given clean silence meanwhile
    /// rather than whatever part of a period the producer has managed, because alternating
    /// the two is what a ramp sounds like from outside.
    priming: AtomicBool,
}

impl Ring {
    pub fn new(capacity_frames: usize) -> Self {
        Ring {
            inner: Mutex::new(Inner {
                buf: vec![0; capacity_frames * 2],
                head: 0,
                len: 0,
            }),
            room: Condvar::new(),
            queued: AtomicUsize::new(0),
            idle: AtomicBool::new(false),
            capacity: AtomicUsize::new(capacity_frames),
            rate: AtomicU32::new(0),
            muted: AtomicBool::new(false),
            overruns: AtomicU64::new(0),
            underruns: AtomicU64::new(0),
            consuming: AtomicBool::new(true),
            priming: AtomicBool::new(false),
        }
    }

    /// Resize for the rate the device actually opened at, discarding whatever the old device
    /// had not played. Opens at the DRC target rather than empty: the device pulls its first
    /// block before the core has run a frame, so an empty ring is a guaranteed underrun and
    /// occupancy would then have to climb to target through it.
    pub fn reopen(&self, sample_rate: u32) {
        let frames = ring_capacity(sample_rate);
        let mut i = self.lock();
        i.buf = vec![0; frames * 2];
        i.head = 0;
        i.len = frames;
        self.queued.store(i.len, Ordering::Relaxed);
        self.capacity.store(frames, Ordering::Relaxed);
        self.rate.store(sample_rate, Ordering::Relaxed);
        self.overruns.store(0, Ordering::Relaxed);
        self.underruns.store(0, Ordering::Relaxed);
        self.consuming.store(true, Ordering::Relaxed);
        // Opened at the target already, so there is nothing to rebuild.
        self.priming.store(false, Ordering::Relaxed);
    }

    /// Samples past capacity are dropped rather than evicting queued audio, so what the
    /// device is about to play stays contiguous.
    pub fn push(&self, samples: &[i16]) {
        let mut i = self.lock();
        let lost = write_into(&mut i, samples).len();
        self.queued.store(i.len, Ordering::Relaxed);
        drop(i);
        // Muted output is discarded by design, and a ring with no buffer has no device
        // behind it. Neither is the race this counter watches for.
        if lost > 0 && !self.muted() && self.capacity_frames() > 0 {
            self.overruns.fetch_add(lost as u64, Ordering::Relaxed);
        }
    }

    /// Emulator side. Waits for the device to make room rather than discarding, which is what
    /// keeps a worker running slightly fast in step with the device instead of clipping
    /// samples off every frame it wins.
    pub fn push_blocking(&self, samples: &[i16]) {
        if self.muted() || self.capacity_frames() == 0 || !self.consuming.load(Ordering::Relaxed) {
            self.push(samples);
            return;
        }
        let deadline = Instant::now() + MAX_WAIT;
        let mut rest = samples;
        let mut i = self.lock();
        loop {
            rest = write_into(&mut i, rest);
            self.queued.store(i.len, Ordering::Relaxed);
            // Mute mid wait: what is left is silence the device will never play.
            if rest.is_empty() || self.muted() {
                return;
            }
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                self.consuming.store(false, Ordering::Relaxed);
                self.overruns
                    .fetch_add(rest.len() as u64, Ordering::Relaxed);
                return;
            };
            i = self
                .room
                .wait_timeout(i, left)
                .map(|(guard, _)| guard)
                .unwrap_or_else(|e| e.into_inner().0);
        }
    }

    /// Adds on top of what is already queued, extending the ring only where the sound runs
    /// past it. A UI sound appended behind the game's audio would arrive a whole buffer after
    /// the thing it is meant to be the sound of.
    pub fn mix(&self, samples: &[i16]) {
        // Nothing is going to build a cushion behind a cart sound: on the shelf there is no
        // core at all, so a rebuild in progress would swallow it whole.
        self.priming.store(false, Ordering::Relaxed);
        let mut i = self.lock();
        let cap = i.buf.len();
        if cap == 0 {
            return;
        }
        let n = samples.len().min(cap);
        for (k, s) in samples[..n].iter().enumerate() {
            let at = (i.head + k) % cap;
            // Past the queued run the buffer holds samples the device has already played,
            // so those are overwritten rather than added to.
            i.buf[at] = if k < i.len {
                i.buf[at].saturating_add(*s)
            } else {
                *s
            };
        }
        i.len = i.len.max(n);
        self.queued.store(i.len, Ordering::Relaxed);
    }

    /// Device side. Pads an underrun with silence. Muted output still drains, so occupancy
    /// keeps meaning the same thing to DRC while the sink is silent.
    pub fn fill(&self, out: &mut [i16]) {
        let mut i = self.lock();
        let cap = i.buf.len();
        // Half the buffer in samples is the DRC target in frames, which is the cushion the
        // rest of the pipeline is tuned around. Holding below it hands over clean silence and
        // leaves the ring to fill, rather than draining a fragment that cannot cover a period.
        let holding = self.priming.load(Ordering::Relaxed) && cap > 0 && i.len < cap / 2;
        self.priming.store(holding, Ordering::Relaxed);
        let n = if holding { 0 } else { out.len().min(i.len) };
        if cap > 0 && !holding {
            let first = n.min(cap - i.head);
            out[..first].copy_from_slice(&i.buf[i.head..i.head + first]);
            out[first..n].copy_from_slice(&i.buf[..n - first]);
            i.head = (i.head + n) % cap;
            i.len -= n;
        }
        out[n..].fill(0);
        self.queued.store(i.len, Ordering::Relaxed);
        drop(i);
        // Even a read that got nothing proves the device is alive.
        self.consuming.store(true, Ordering::Relaxed);
        self.room.notify_all();
        let short = out.len() - n;
        if short > 0 && !self.muted() && !self.idle.load(Ordering::Relaxed) && cap > 0 {
            self.underruns.fetch_add(short as u64, Ordering::Relaxed);
            // Ride it out on silence and come back with a full cushion. One clean gap is a
            // beat of quiet; a period of audio per period of padding is a scratch.
            self.priming.store(true, Ordering::Relaxed);
        }
        if self.muted() {
            out.fill(0);
        }
    }

    pub fn queued_frames(&self) -> usize {
        self.queued.load(Ordering::Relaxed) / 2
    }

    pub fn capacity_frames(&self) -> usize {
        self.capacity.load(Ordering::Relaxed)
    }

    /// Samples dropped for want of room. Any nonzero value is audible.
    pub fn overruns(&self) -> u64 {
        self.overruns.load(Ordering::Relaxed)
    }

    /// Samples the device asked for and got silence.
    pub fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }

    /// Zeroed by whoever takes the ring over. The device outlives every cart and an empty
    /// slot drains as silence, so without this a cart inherits however long the shelf sat.
    pub fn clear_faults(&self) {
        self.overruns.store(0, Ordering::Relaxed);
        self.underruns.store(0, Ordering::Relaxed);
    }

    /// Zero until the device opens. The resampler treats that as "no device yet".
    pub fn sample_rate(&self) -> u32 {
        self.rate.load(Ordering::Relaxed)
    }

    /// Whether anything is expected to be feeding this ring. A paused core is not, so a dry
    /// ring while the cart is sliding in is the normal state of things rather than the device
    /// going hungry, and counting it as a starve reports a fault on every single insert.
    pub fn set_idle(&self, idle: bool) {
        self.idle.store(idle, Ordering::Relaxed);
    }

    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
        // A producer parked on a sink that has just gone silent has nothing left to wait for.
        self.room.notify_all();
    }

    pub fn muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    /// A poisoned lock means a writer panicked mid copy. One garbled buffer is a better
    /// outcome than losing audio for the rest of the session.
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Writes what fits and hands back what did not, leaving the caller to decide between
/// waiting for room and giving the samples up.
fn write_into<'a>(i: &mut Inner, samples: &'a [i16]) -> &'a [i16] {
    let cap = i.buf.len();
    if cap == 0 {
        return samples;
    }
    let n = samples.len().min(cap - i.len);
    let start = (i.head + i.len) % cap;
    let first = n.min(cap - start);
    i.buf[start..start + first].copy_from_slice(&samples[..first]);
    i.buf[..n - first].copy_from_slice(&samples[first..n]);
    i.len += n;
    &samples[n..]
}

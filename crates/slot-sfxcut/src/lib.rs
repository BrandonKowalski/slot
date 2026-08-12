//! Cuts the two cartridge noises out of a recording of the real thing. Not part of the
//! device build: it writes `crates/slot/assets/*.pcm`, which are committed, and the frontend
//! reads those.
//!
//! The recording is the take; nothing here invents sound. All this does is find the sharp
//! events in it, cut a fixed window around the one you pick, and line that window up so the
//! transient lands exactly where the cartridge animation expects it.

mod wav;

pub use wav::{read_wav, WavError};

pub const HZ: f32 = 48_000.0;

/// Exactly what the frontend expects. `Sfx::tail` derives from the file length, so these
/// changing changes the animation.
pub const INSERT_LEN: usize = 11_520;
pub const EJECT_LEN: usize = 15_120;

/// How far into each clip the contacts are, mirroring `Sfx::lead`. The caller starts the
/// clip this long before the cart reaches them, so the transient has to land here.
pub const INSERT_LEAD: f32 = 0.097;
pub const EJECT_LEAD: f32 = 0.021;

/// Peak levels the frontend has always played at, so a new take does not change how loud the
/// thing is next to the game audio.
pub const INSERT_PEAK: f32 = 11_397.0;
pub const EJECT_PEAK: f32 = 12_000.0;

/// A sharp event in the recording: one cart going in, or one coming out.
#[derive(Clone, Copy, Debug)]
pub struct Take {
    /// Seconds from the start of the recording.
    pub at: f32,
    /// Loudest sample in the 30 ms around it, 0.0 to 1.0.
    pub peak: f32,
    /// Silence before it. A take with a neighbour too close cannot be cut cleanly.
    pub clear_before: f32,
    /// Room after it, to the next take or the end of the recording.
    pub clear_after: f32,
}

impl Take {
    /// Whether a clip of `len` samples leading by `lead` fits around this take without
    /// running into the next one or off either end.
    pub fn fits(&self, lead: f32, len: usize) -> bool {
        let after = len as f32 / HZ - lead;
        self.clear_before >= lead && self.clear_after >= after
    }
}

/// Every sharp event in the recording, loudest first is not the order: these come out in
/// time order, because the eject you want is usually the one after the insert you want.
///
/// A transient is a jump in short window energy well above what the preceding tenth of a
/// second was doing. That finds cart noises and also finds coughs, so the caller listens to
/// what it picked rather than trusting the list.
pub fn takes(pcm: &[f32]) -> Vec<Take> {
    let rms: Vec<f32> = (0..pcm.len().saturating_sub(WIN))
        .step_by(HOP)
        .map(|i| {
            let w = &pcm[i..i + WIN];
            (w.iter().map(|v| v * v).sum::<f32>() / WIN as f32).sqrt()
        })
        .collect();

    let mut hits: Vec<usize> = Vec::new();
    for (k, &now) in rms.iter().enumerate().skip(HISTORY) {
        if now < FLOOR {
            continue;
        }
        let before = &rms[k - HISTORY..k];
        let quiet = before.iter().fold(f32::MAX, |m, &v| m.min(v)).max(1.0e-6);
        if 20.0 * (now / quiet).log10() < JUMP_DB {
            continue;
        }
        // One cart makes several hops loud. Keep the first, and let 80 ms pass before
        // anything counts as a separate event.
        if hits
            .last()
            .is_some_and(|&p| (k - p) * HOP < (0.080 * HZ) as usize)
        {
            continue;
        }
        hits.push(k);
    }

    let peak_around = |centre: usize| {
        let lo = centre.saturating_sub((0.015 * HZ) as usize);
        let hi = (centre + (0.015 * HZ) as usize).min(pcm.len());
        pcm[lo..hi].iter().fold(0.0f32, |m, v| m.max(v.abs()))
    };

    let onsets: Vec<usize> = hits.iter().map(|&k| onset(pcm, k * HOP)).collect();

    // One cart going in is not one transient. It is the rails, then the contacts, then the
    // stop, then the shell settling, and the detector sees all of them. Group anything
    // within 400 ms into a single take and anchor it on the loudest, because the loudest is
    // the contact and the contact is what `lead` is measured to.
    let mut grouped: Vec<(usize, usize)> = Vec::new(); // (anchor, last onset in the group)
    for &sample in &onsets {
        match grouped.last_mut() {
            Some((anchor, last)) if (sample - *last) as f32 / HZ < GROUP => {
                *last = sample;
                if peak_around(sample) > peak_around(*anchor) {
                    *anchor = sample;
                }
            }
            _ => grouped.push((sample, sample)),
        }
    }
    let mut anchors: Vec<usize> = grouped.into_iter().map(|(a, _)| a).collect();

    // Handling the device makes small noises that clear the detector but are not takes.
    let loudest = anchors
        .iter()
        .map(|&a| peak_around(a))
        .fold(0.0f32, f32::max);
    anchors.retain(|&a| peak_around(a) >= loudest * 0.10);

    let total = pcm.len() as f32 / HZ;
    anchors
        .iter()
        .enumerate()
        .map(|(n, &sample)| {
            let at = sample as f32 / HZ;
            Take {
                at,
                peak: peak_around(sample),
                clear_before: match n {
                    0 => at,
                    _ => at - anchors[n - 1] as f32 / HZ,
                },
                clear_after: match anchors.get(n + 1) {
                    Some(&next) => next as f32 / HZ - at,
                    None => total - at,
                },
            }
        })
        .collect()
}

const HOP: usize = 240; // 5 ms
const WIN: usize = 480; // 10 ms
const HISTORY: usize = 20; // 100 ms of hops
const JUMP_DB: f32 = 12.0;
const FLOOR: f32 = 0.004; // about -48 dBFS, below which it is room tone
const GROUP: f32 = 0.400; // transients closer than this are one cart action

/// The hop grid is 5 ms, which is coarse next to a 21 ms lead, and the window that trips the
/// detector starts up to one window *before* the sound does. So once a hop has flagged an
/// event, find the sample the rise actually starts on: the first one nearby that crosses a
/// third of the local peak. Being 5 ms out here is 5 ms of sound against picture.
fn onset(pcm: &[f32], hop_start: usize) -> usize {
    let lo = hop_start.saturating_sub(WIN);
    let hi = (hop_start + 3 * WIN).min(pcm.len());
    if lo >= hi {
        return hop_start;
    }
    let gate = pcm[lo..hi].iter().fold(0.0f32, |m, v| m.max(v.abs())) / 3.0;
    (lo..hi)
        .find(|&i| pcm[i].abs() >= gate)
        .unwrap_or(hop_start)
}

#[derive(Debug)]
pub enum CutError {
    /// The window would start before the recording does.
    NotEnoughBefore { want: f32, have: f32 },
    /// The window would run off the end.
    NotEnoughAfter { want: f32, have: f32 },
}

impl std::fmt::Display for CutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotEnoughBefore { want, have } => write!(
                f,
                "needs {:.0} ms before the transient, the take has {:.0} ms",
                want * 1000.0,
                have * 1000.0
            ),
            Self::NotEnoughAfter { want, have } => write!(
                f,
                "needs {:.0} ms after the transient, the take has {:.0} ms",
                want * 1000.0,
                have * 1000.0
            ),
        }
    }
}

/// Cut `len` samples that put the transient at `at` exactly `lead` in, then normalise to
/// `peak` and ease both ends. The alignment is the whole point: the frontend starts the clip
/// `lead` before the cart reaches the contacts, so this is what keeps sound and picture
/// together.
///
/// `lift_db` raises everything before the transient, easing back to unity over the last
/// 15 ms so there is no step at the join. A hand pushing a cart is much quieter than the
/// cart hitting its stop, and a phone a hand's width away records that gap faithfully;
/// closing some of it is the difference between hearing the rails and hearing only a clunk.
/// It changes the balance of the take, not its content. Zero leaves the take alone.
pub fn cut(
    pcm: &[f32],
    at: f32,
    lead: f32,
    len: usize,
    peak: f32,
    lift_db: f32,
) -> Result<Vec<i16>, CutError> {
    let start = (at - lead) * HZ;
    if start < 0.0 {
        return Err(CutError::NotEnoughBefore {
            want: lead,
            have: at,
        });
    }
    let start = start as usize;
    if start + len > pcm.len() {
        return Err(CutError::NotEnoughAfter {
            want: len as f32 / HZ - lead,
            have: (pcm.len() - start.min(pcm.len())) as f32 / HZ - lead,
        });
    }
    let mut buf = pcm[start..start + len].to_vec();
    lift(&mut buf, lead, lift_db);
    Ok(finish(&mut buf, peak))
}

/// Raise everything before the transient by `db`, easing back to unity over the 15 ms in
/// front of it. Applied before the peak is set, so the transient still decides the gain.
fn lift(buf: &mut [f32], lead: f32, db: f32) {
    if db == 0.0 {
        return;
    }
    let gain = 10.0f32.powf(db / 20.0);
    let at = (lead * HZ) as usize;
    let ramp = (0.015 * HZ) as usize;
    for (i, s) in buf.iter_mut().enumerate().take(at) {
        // Full lift until the ramp, then back to unity by the time the transient arrives.
        let u = ((at - i) as f32 / ramp as f32).min(1.0);
        *s *= 1.0 + (gain - 1.0) * raised_cosine(u);
    }
}

/// Ease the two ends to nothing, scale so the loudest sample lands exactly on `peak`, then
/// to signed 16 bit.
///
/// The fades are not cosmetic. A cut lands wherever the recording happened to be, and a clip
/// that starts or stops partway through room tone is a click on every insert, which is what
/// `neither_clip_starts_or_ends_on_a_step` over in the frontend exists to catch.
fn finish(buf: &mut [f32], peak: f32) -> Vec<i16> {
    const IN: usize = 96; // 2 ms
    const OUT: usize = 900; // 19 ms

    let n = buf.len();
    for (i, s) in buf.iter_mut().enumerate() {
        let head = (i as f32 / IN as f32).min(1.0);
        let tail = ((n - 1 - i) as f32 / OUT as f32).min(1.0);
        *s *= raised_cosine(head) * raised_cosine(tail);
    }

    let loudest = buf.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let gain = if loudest > 0.0 { peak / loudest } else { 0.0 };
    buf.iter()
        .map(|v| (v * gain).round().clamp(-32_768.0, 32_767.0) as i16)
        .collect()
}

/// 0 to 1 with both ends flat, so a fade neither starts nor stops abruptly.
fn raised_cosine(u: f32) -> f32 {
    0.5 * (1.0 - (std::f32::consts::PI * u).cos())
}

/// Mono signed 16 bit little endian, which is what `include_bytes!` on the other side
/// expects to find.
pub fn to_le_bytes(pcm: &[i16]) -> Vec<u8> {
    pcm.iter().flat_map(|s| s.to_le_bytes()).collect()
}

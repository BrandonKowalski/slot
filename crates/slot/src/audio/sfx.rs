/// Cut from a recording of the real thing by `slot-sfxcut`, mono signed 16 bit little endian
/// at 48 kHz. One continuous take per direction, played as recorded: no stretching, no
/// envelope, nothing joined. Everything the sound needs to do it already does, and the sync
/// is a question of when it starts rather than what is done to it.
const INSERT: &[u8] = include_bytes!("../../assets/insert.pcm");
const EJECT: &[u8] = include_bytes!("../../assets/eject.pcm");
const ASSET_HZ: f32 = 48_000.0;

/// A noise the frontend makes itself, as opposed to anything coming out of a core.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Sfx {
    /// The whole of a cart going in: the shell down the rails, then the contacts.
    Insert,
    /// The whole of one coming out: the contacts letting go, then the shell back up.
    Eject,
}

impl Sfx {
    /// How far into the clip the contacts are. The caller starts the clip this long before
    /// the cart reaches them, which is the whole of how the two are kept together.
    pub fn lead(self) -> f32 {
        match self {
            Sfx::Insert => 0.097,
            Sfx::Eject => 0.021,
        }
    }

    /// What is left of the clip after the contacts: the shell settling on the way in, the
    /// shell still moving on the way out. Nothing should cut across it.
    pub fn tail(self) -> f32 {
        let pcm = match self {
            Sfx::Insert => INSERT,
            Sfx::Eject => EJECT,
        };
        pcm.len() as f32 / 2.0 / ASSET_HZ - self.lead()
    }

    /// Interleaved stereo, mono content, at the sink's rate.
    pub fn render(self, sample_rate: u32) -> Vec<i16> {
        let pcm = match self {
            Sfx::Insert => INSERT,
            Sfx::Eject => EJECT,
        };
        let mut out = Vec::with_capacity(pcm.len());
        for v in resampled(pcm, sample_rate) {
            let s = v.clamp(-32768.0, 32767.0) as i16;
            out.push(s);
            out.push(s);
        }
        out
    }
}

/// Linear, from the asset's 48 kHz to whatever the sink runs at. Both are the same rate on
/// every platform this has met so far, so this is a safeguard rather than a hot path.
fn resampled(pcm: &[u8], sample_rate: u32) -> Vec<f32> {
    let src: Vec<f32> = pcm
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32)
        .collect();
    if sample_rate == ASSET_HZ as u32 || src.is_empty() {
        return src;
    }
    let ratio = sample_rate as f32 / ASSET_HZ;
    let n = (src.len() as f32 * ratio) as usize;
    (0..n)
        .map(|i| {
            let x = i as f32 / ratio;
            let a = x as usize;
            let f = x - a as f32;
            let b = (a + 1).min(src.len() - 1);
            src[a] * (1.0 - f) + src[b] * f
        })
        .collect()
}

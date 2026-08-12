//! Just enough RIFF to read what a phone and `afconvert` produce. Anything it will not open
//! it says so about, rather than guessing and cutting silence.

use std::path::Path;

use crate::HZ;

#[derive(Debug)]
pub enum WavError {
    Io(std::io::Error),
    NotRiff,
    NoFmt,
    NoData,
    /// Compressed, or a bit depth this does not handle.
    Unsupported(String),
}

impl std::fmt::Display for WavError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::NotRiff => write!(f, "not a RIFF/WAVE file"),
            Self::NoFmt => write!(f, "no fmt chunk"),
            Self::NoData => write!(f, "no data chunk"),
            Self::Unsupported(what) => write!(f, "unsupported: {what}"),
        }
    }
}

impl From<std::io::Error> for WavError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Mono f32 in -1.0 to 1.0 at 48 kHz, whatever the file was. Stereo is summed rather than
/// one channel taken, because a phone lying next to the device does not put the cart in
/// either channel in particular.
pub fn read_wav(path: &Path) -> Result<Vec<f32>, WavError> {
    let b = std::fs::read(path)?;
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return Err(WavError::NotRiff);
    }

    let mut fmt: Option<(u16, u16, u32, u16)> = None; // format, channels, rate, bits
    let mut data: Option<&[u8]> = None;

    let mut i = 12;
    while i + 8 <= b.len() {
        let id = &b[i..i + 4];
        let size = u32::from_le_bytes([b[i + 4], b[i + 5], b[i + 6], b[i + 7]]) as usize;
        let body = i + 8;
        let end = (body + size).min(b.len());
        match id {
            b"fmt " if size >= 16 => {
                fmt = Some((
                    u16::from_le_bytes([b[body], b[body + 1]]),
                    u16::from_le_bytes([b[body + 2], b[body + 3]]),
                    u32::from_le_bytes([b[body + 4], b[body + 5], b[body + 6], b[body + 7]]),
                    u16::from_le_bytes([b[body + 14], b[body + 15]]),
                ));
            }
            b"data" => data = Some(&b[body..end]),
            _ => {}
        }
        // Chunks are word aligned, and a stray odd size otherwise walks the parser off.
        i = body + size + (size & 1);
    }

    let (format, channels, rate, bits) = fmt.ok_or(WavError::NoFmt)?;
    let data = data.ok_or(WavError::NoData)?;
    let channels = channels.max(1) as usize;

    // 1 is integer PCM, 3 is IEEE float, 0xFFFE is extensible and its real format lives in
    // the chunk extension; treating it by bit depth covers what afconvert emits.
    let frames: Vec<f32> = match (format, bits) {
        (1 | 0xFFFE, 16) => data
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32_768.0)
            .collect(),
        (1 | 0xFFFE, 24) => data
            .chunks_exact(3)
            .map(|c| {
                let v = i32::from_le_bytes([0, c[0], c[1], c[2]]) >> 8;
                v as f32 / 8_388_608.0
            })
            .collect(),
        (3 | 0xFFFE, 32) => data
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
        (f, bits) => {
            return Err(WavError::Unsupported(format!(
                "format {f}, {bits} bit. Convert first: \
                 afconvert -f WAVE -d LEI16@48000 -c 1 <in> <out.wav>"
            )))
        }
    };

    let mono: Vec<f32> = match channels {
        1 => frames,
        n => frames
            .chunks_exact(n)
            .map(|f| f.iter().sum::<f32>() / n as f32)
            .collect(),
    };

    Ok(resampled(mono, rate))
}

/// Linear, to the 48 kHz everything downstream assumes. A phone records at 44.1 or 48 and
/// either is fine; this is here so the tool never silently retimes a take.
fn resampled(src: Vec<f32>, rate: u32) -> Vec<f32> {
    if rate == HZ as u32 || src.is_empty() {
        return src;
    }
    let ratio = HZ / rate as f32;
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

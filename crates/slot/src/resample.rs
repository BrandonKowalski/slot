/// Linear interpolation from the core rate to whatever rate the device actually opened at,
/// with the DRC ratio bending the step. Position is carried across calls and the previous
/// buffer's last frame is kept, so a core frame boundary is not a discontinuity.
pub struct Resampler {
    step: f64,
    scaled: f64,
    pos: f64,
    prev: [i16; 2],
}

impl Resampler {
    pub fn new(src_hz: f64, dst_hz: f64) -> Self {
        let step = if dst_hz > 0.0 { src_hz / dst_hz } else { 1.0 };
        Resampler {
            step,
            scaled: step,
            pos: 0.0,
            prev: [0, 0],
        }
    }

    /// Above 1.0 takes smaller steps through the source, so more output frames come out of
    /// the same input and a starved device catches up.
    pub fn set_ratio(&mut self, ratio: f64) {
        if ratio > 0.0 {
            self.scaled = self.step / ratio;
        }
    }

    pub fn process(&mut self, src: &[i16], out: &mut Vec<i16>) {
        out.clear();
        let frames = src.len() / 2;
        if frames == 0 {
            return;
        }
        while self.pos < frames as f64 {
            let i = self.pos as usize;
            let frac = self.pos - i as f64;
            for c in 0..2 {
                let a = tap(&self.prev, src, i, c);
                let b = tap(&self.prev, src, i + 1, c);
                out.push((a + (b - a) * frac).round() as i16);
            }
            self.pos += self.scaled;
        }
        self.pos -= frames as f64;
        self.prev = [src[(frames - 1) * 2], src[(frames - 1) * 2 + 1]];
    }
}

/// Index 0 is the last frame of the previous call, so an output frame landing between two
/// buffers still has both of its endpoints.
fn tap(prev: &[i16; 2], src: &[i16], frame: usize, channel: usize) -> f64 {
    if frame == 0 {
        prev[channel] as f64
    } else {
        src[(frame - 1) * 2 + channel] as f64
    }
}

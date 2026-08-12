/// Volume is stored 0 to 100 because that is what an ALSA mixer takes on the device. The
/// host has no mixer, so the level is applied to the samples on their way to the sink.
///
/// Squared rather than linear: loudness is roughly perceived that way, and a linear ramp
/// spends most of its travel in a range that already sounds like full volume.
pub fn gain(volume: u8) -> f32 {
    let v = volume.min(100) as f32 / 100.0;
    v * v
}

pub fn apply(samples: &mut [i16], volume: u8) {
    match volume.min(100) {
        100 => {}
        0 => samples.fill(0),
        v => {
            let g = gain(v);
            for s in samples.iter_mut() {
                *s = (*s as f32 * g) as i16;
            }
        }
    }
}

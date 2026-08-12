/// Widest correction the resampler is allowed to apply. The panel runs 60 Hz against the
/// GBA's 59.7275, a standing 0.456% surplus, so the bound has to sit just above that or the
/// buffer would climb with nothing able to pull it back.
const MAX_CORRECTION: f64 = 0.005;

/// How far the device buffer is from where it should sit, as a resampling ratio. Above 1.0
/// stretches the stream to cover a starved device, below 1.0 compresses it to drain a full one.
pub fn drc_ratio(queued_frames: usize, target_frames: usize) -> f64 {
    if target_frames == 0 {
        return 1.0;
    }
    let error = target_frames as f64 - queued_frames as f64;
    let ratio = 1.0 + MAX_CORRECTION * error / target_frames as f64;
    ratio.clamp(1.0 - MAX_CORRECTION, 1.0 + MAX_CORRECTION)
}

/// Half the device buffer, which leaves equal room to absorb a late emulator frame and a
/// late device callback.
pub fn drc_target(capacity_frames: usize) -> usize {
    capacity_frames / 2
}

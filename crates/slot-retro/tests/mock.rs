use slot_retro::{ButtonMask, MockCore, RetroCore};

#[test]
fn mock_core_round_trips_state() {
    let mut c = MockCore::new();
    c.load(std::path::Path::new("unused")).unwrap();
    for _ in 0..10 {
        c.run_frame(ButtonMask::default());
    }
    let s = c.serialize().unwrap();
    let a = c.video_xrgb8888().to_vec();
    for _ in 0..10 {
        c.run_frame(ButtonMask::default());
    }
    assert_ne!(c.video_xrgb8888(), &a[..]);
    c.unserialize(&s).unwrap();
    assert_eq!(c.video_xrgb8888(), &a[..]);
}

#[test]
fn audio_tracks_the_frame_rate_without_drift() {
    let mut c = MockCore::new();
    c.load(std::path::Path::new("unused")).unwrap();
    let info = c.av_info();
    let frames = 597;
    let mut samples = 0;
    for _ in 0..frames {
        c.run_frame(ButtonMask::default());
        let chunk = c.take_audio();
        assert_eq!(chunk.len() % 2, 0, "audio must be interleaved stereo");
        samples += chunk.len();
    }
    let want = (frames as f64 / info.fps * info.sample_rate) as usize;
    let got = samples / 2;
    assert!(
        got.abs_diff(want) <= 1,
        "{got} stereo frames over {frames} video frames, expected {want}"
    );
}

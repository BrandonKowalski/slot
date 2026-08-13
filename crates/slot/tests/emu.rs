use std::path::PathBuf;
use std::time::{Duration, Instant};

use slot::audio::{AudioSink, StubSink};
use slot::emu::{CoreState, EmuHandle, Speed, FAST_STEPS};
use slot::persist::Snapshot;
use slot_retro::MockCore;

fn spawn() -> EmuHandle {
    spawn_with(None)
}

fn spawn_with(sav: Option<Vec<u8>>) -> EmuHandle {
    spawn_into(StubSink::new(), sav)
}

/// The worker now waits for the device to make room, so a sink nothing drains would hold it
/// up. This is the device: it always has room, which keeps these tests about the emulator.
fn drain(sink: StubSink) {
    std::thread::spawn(move || loop {
        sink.device_drain();
        std::thread::sleep(Duration::from_millis(2));
    });
}

fn spawn_into(mut sink: StubSink, sav: Option<Vec<u8>>) -> EmuHandle {
    sink.open(32_768).expect("the stub refused to open");
    drain(sink.clone());
    let emu = EmuHandle::spawn(
        Box::new(MockCore::new()),
        PathBuf::from("mock"),
        sink.ring(),
        sav,
        None,
    );
    // A worker starts paused now, because one spawned during an insert must not run the
    // first frames of the bios boot where nobody can see them. A test that wants a running
    // core asks for one.
    emu.set_speed(Speed::Normal);
    assert!(
        wait_for(|| emu.state() != CoreState::Loading),
        "the core never finished loading"
    );
    assert_eq!(emu.state(), CoreState::Ready);
    emu
}

fn wait_for(cond: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if cond() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// MockCore serializes to its frame counter, so a state doubles as "how far has it run".
fn frame_count(emu: &EmuHandle) -> u64 {
    let bytes = emu
        .request_state()
        .recv_timeout(Duration::from_secs(2))
        .expect("the worker never answered a state request");
    u64::from_le_bytes(bytes.try_into().expect("mock state is a u64 counter"))
}

#[test]
fn the_worker_runs_until_it_is_paused_and_resumes_where_it_stopped() {
    let emu = spawn();
    let early = frame_count(&emu);
    std::thread::sleep(Duration::from_millis(200));
    let later = frame_count(&emu);
    assert!(
        later > early,
        "the core is not stepping: {early} then {later}"
    );

    emu.set_speed(Speed::Paused);
    std::thread::sleep(Duration::from_millis(100));
    let held = frame_count(&emu);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(frame_count(&emu), held, "a paused core kept stepping");

    emu.set_speed(Speed::Normal);
    std::thread::sleep(Duration::from_millis(200));
    assert!(frame_count(&emu) > held, "the core did not resume");
}

#[test]
fn a_requested_load_rewinds_the_core() {
    let emu = spawn();
    let state = emu
        .request_state()
        .recv_timeout(Duration::from_secs(2))
        .expect("the worker never answered a state request");
    let at_save = u64::from_le_bytes(state.clone().try_into().expect("mock state is a counter"));

    std::thread::sleep(Duration::from_millis(200));
    assert!(frame_count(&emu) > at_save);

    emu.set_speed(Speed::Paused);
    emu.request_load(state);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(frame_count(&emu), at_save);
}

/// Holding L2 walks the core back through the snapshots it took while playing, and letting
/// go plays on from where it landed rather than from where the rewind started.
#[test]
fn rewinding_walks_the_core_backwards_and_then_plays_on() {
    let emu = spawn();
    std::thread::sleep(Duration::from_millis(300));
    let played = frame_count(&emu);

    emu.set_rewinding(true);
    std::thread::sleep(Duration::from_millis(200));
    emu.set_rewinding(false);
    let rewound = frame_count(&emu);
    assert!(
        rewound < played,
        "the core did not rewind: {played} then {rewound}"
    );

    std::thread::sleep(Duration::from_millis(200));
    assert!(
        frame_count(&emu) > rewound,
        "the core did not resume after the rewind"
    );
}

/// Fast forward drops the core's audio, so what is left in the ring is stale by up to four
/// frames. Muting is what stops the device playing it out.
#[test]
fn fast_forward_mutes_and_normal_speed_unmutes() {
    let sink = StubSink::new();
    let emu = spawn_into(sink.clone(), None);
    emu.set_speed(Speed::Fast);
    assert!(
        wait_for(|| sink.muted()),
        "fast forward did not mute the sink"
    );
    emu.set_speed(Speed::Normal);
    assert!(
        wait_for(|| !sink.muted()),
        "the sink stayed muted at normal speed"
    );
}

/// The device outlives the cart, so a worker that stopped while it was muted would take
/// every sound after it down with it: the next cart's, and the slot's own.
#[test]
fn a_worker_that_stops_while_muted_leaves_the_sink_audible() {
    let sink = StubSink::new();
    let emu = spawn_into(sink.clone(), None);
    emu.set_speed(Speed::Fast);
    assert!(wait_for(|| sink.muted()), "fast forward did not mute");
    drop(emu);
    assert!(!sink.muted(), "the sink stayed muted after the cart left");
}

/// `FAST_STEPS` core frames per present, which is the only fast forward speed there is. Read
/// from the constant rather than repeated here: a step count that moves and a test that does
/// not would leave the test passing on the old number.
#[test]
fn fast_forward_runs_fast_steps_core_frames_per_present() {
    let emu = spawn();
    let from = frame_count(&emu);
    std::thread::sleep(Duration::from_millis(300));
    let normal = frame_count(&emu) - from;

    emu.set_speed(Speed::Fast);
    let from = frame_count(&emu);
    std::thread::sleep(Duration::from_millis(300));
    let fast = frame_count(&emu) - from;

    let n = normal as u32;
    assert!(
        fast as u32 >= n * (FAST_STEPS - 1) && fast as u32 <= n * (FAST_STEPS + 1),
        "{fast} frames fast against {normal} normal is not {FAST_STEPS}x"
    );
}

/// The battery save has to reach the core after the rom is loaded, since before that
/// there is no save ram to copy it into, and come back out unchanged.
#[test]
fn battery_save_ram_reaches_the_core_and_comes_back() {
    let mut sav = vec![0u8; 8 * 1024];
    sav[7] = 0xab;
    let emu = spawn_with(Some(sav.clone()));
    let got = emu
        .snapshot()
        .save_ram()
        .expect("the core reported no save ram");
    assert_eq!(got, sav);
}

/// The card is a picture of the game, not of the panel. The LCD mask is applied when the
/// switcher draws the shot, so what the worker captures has to be the core's own frame: a
/// mask baked in here would be wrong the moment the mask changes.
#[test]
fn a_captured_thumbnail_is_the_unfiltered_core_frame() {
    let emu = spawn();
    assert!(wait_for(|| emu.has_published()), "no frame was published");
    // Paused, so the frame waiting for the renderer is also the one the core is sitting on.
    emu.set_speed(Speed::Paused);
    std::thread::sleep(Duration::from_millis(100));
    let frame = emu.latest_frame().expect("the published frame went away");
    let png = emu
        .snapshot()
        .thumb()
        .expect("the snapshot carried no thumbnail");

    let mut reader = png::Decoder::new(png.as_slice())
        .read_info()
        .expect("read info");
    let mut buf = vec![0u8; reader.output_buffer_size()];
    reader.next_frame(&mut buf).expect("decode");

    // The core's frame is XRGB8888 little endian, so its bytes are B, G, R, X.
    for (i, px) in frame.chunks_exact(4).enumerate() {
        assert_eq!(
            &buf[i * 3..i * 3 + 3],
            &[px[2], px[1], px[0]],
            "thumbnail pixel {i} is not the frame's"
        );
    }
}

#[test]
fn the_renderer_is_handed_whole_gba_frames() {
    let emu = spawn();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(frame) = emu.latest_frame() {
            assert_eq!(frame.len(), 240 * 160 * 4);
            return;
        }
        assert!(Instant::now() < deadline, "no frame was published in 2s");
        std::thread::sleep(Duration::from_millis(2));
    }
}

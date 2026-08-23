use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use slot::audio::{AudioSink, StubSink};
use slot::emu::{CoreState, EmuHandle, Speed, FAST_STEPS};
use slot::persist::Snapshot;
use slot_retro::{
    AvInfo, ButtonMask, CoreError, LinkChannel, MockCore, RetroCore, NETPACKET_RELIABLE,
};

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

/// Fast forward has to leave a trail behind it. Snapshotting only at normal speed put a
/// hole in the history: the newest state was whatever was recorded before the trigger, so
/// the first pop of a rewind collapsed the entire fast forwarded stretch in one step
/// rather than walking back through it.
#[test]
fn fast_forward_still_records_rewind_history() {
    let emu = spawn();
    std::thread::sleep(Duration::from_millis(200));
    let before_ff = frame_count(&emu);

    emu.set_speed(Speed::Fast);
    std::thread::sleep(Duration::from_millis(300));
    emu.set_speed(Speed::Normal);
    let after_ff = frame_count(&emu);
    assert!(
        after_ff > before_ff,
        "fast forward did not advance the core: {before_ff} then {after_ff}"
    );

    // A brief rewind is a handful of pops. It should walk back a little way into the
    // stretch that was run at speed, not fall off the far side of it.
    emu.set_rewinding(true);
    std::thread::sleep(Duration::from_millis(60));
    emu.set_rewinding(false);
    let rewound = frame_count(&emu);

    assert!(
        rewound > before_ff,
        "a short rewind fell back past where the fast forward began, so nothing was \
         recorded while it ran: {before_ff} -> {after_ff} -> {rewound}"
    );
}

// --- the link pump --------------------------------------------------------------------
//
// `MockCore` never registers netpacket (it does not exercise the FFI, `slot-retro`'s own
// tests already cover that seam exhaustively), so `RetroCore::pump_link`'s default is a
// no-op here — meaning any packet the worker moves in either direction has to be the
// worker's own loop doing it, not the core. That is exactly the wiring this task adds: the
// worker draining a transport into `push_inbound`, and draining `take_outbound` back onto
// it, every present, regardless of what the core does with either queue.

/// Two `LinkChannel`s wired to each other over a channel pair, standing in for a peer:
/// whatever one side sends, the other's `try_recv` eventually returns. `LoopbackLink` cannot
/// do this — it echoes a send back to the same handle — and a real socket is more than this
/// test needs to prove the worker's own loop moves bytes in both directions.
struct PairedLink {
    tx: mpsc::Sender<Vec<u8>>,
    rx: mpsc::Receiver<Vec<u8>>,
}

fn paired_links() -> (PairedLink, PairedLink) {
    let (tx_a, rx_b) = mpsc::channel();
    let (tx_b, rx_a) = mpsc::channel();
    (
        PairedLink { tx: tx_a, rx: rx_a },
        PairedLink { tx: tx_b, rx: rx_b },
    )
}

impl LinkChannel for PairedLink {
    fn send(&mut self, _flags: i32, buf: &[u8]) {
        let _ = self.tx.send(buf.to_vec());
    }

    fn try_recv(&mut self) -> Option<Vec<u8>> {
        self.rx.try_recv().ok()
    }
}

fn wait_for_packet(mut poll: impl FnMut() -> Option<Vec<u8>>) -> Option<Vec<u8>> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(p) = poll() {
            return Some(p);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Inbound: whatever the transport hands back reaches the core's `Link` — `push_inbound` —
/// every present, with no session-specific behaviour from `MockCore` involved at all.
#[test]
fn a_transport_packet_reaches_the_cores_inbound_queue() {
    let emu = spawn();
    let (mut here, there) = paired_links();
    emu.begin_link(0, Box::new(there));
    assert!(
        wait_for(|| emu.net().is_active()),
        "begin_link must mark the session active"
    );

    here.send(NETPACKET_RELIABLE, b"from the peer");
    let got = wait_for_packet(|| emu.net().take_inbound());
    assert_eq!(got.as_deref(), Some(&b"from the peer"[..]));
}

/// Outbound: whatever lands in `Link`'s outbound queue — here pushed directly, standing in
/// for what the trampoline would do for a real netpacket core — reaches the transport.
#[test]
fn the_cores_outbound_queue_reaches_the_transport() {
    let emu = spawn();
    let (mut here, there) = paired_links();
    emu.begin_link(1, Box::new(there));

    emu.net().push_outbound(b"from the core".to_vec());
    let got = wait_for_packet(|| here.try_recv());
    assert_eq!(got.as_deref(), Some(&b"from the core"[..]));
}

/// `end_link` drops the transport (which is what actually closes a real wire — see
/// `TcpLink`'s `Drop`) and marks the session no longer active, so nothing pumped after it
/// still reaches a peer that has moved on.
#[test]
fn end_link_marks_the_session_inactive() {
    let emu = spawn();
    let (_here, there) = paired_links();
    emu.begin_link(0, Box::new(there));
    assert!(wait_for(|| emu.net().is_active()), "begin_link never took");

    emu.end_link();
    assert!(
        wait_for(|| !emu.net().is_active()),
        "end_link must mark the session no longer active"
    );
}

/// Wraps `MockCore` and counts calls into the `RetroCore` link methods the worker is
/// responsible for invoking on its own schedule — `start_link`, `pump_link` and `stop_link`.
/// `MockCore` itself overrides none of the three (see its own doc comment on `net()`'s
/// default: a fresh, unrelated `Link` on every call, which is also why this spy does not try
/// to override `net()` either — nothing here needs the core's own link state, only proof the
/// worker actually called through). Without a spy like this, none of the worker's own calls
/// into the core are visible from `slot`'s tests at all — the mutation run that found this
/// (I7) deleted `pump_link()` from the worker loop, `start_link(client_id)` from
/// `Cmd::BeginLink`, and made `Cmd::EndLink` keep the transport instead of dropping it, and
/// all 652 tests stayed green because nothing was watching any of the three.
struct SpyLinkCore {
    inner: MockCore,
    start_calls: Arc<Mutex<Vec<u16>>>,
    pump_calls: Arc<AtomicUsize>,
    stop_calls: Arc<AtomicUsize>,
}

impl Default for SpyLinkCore {
    fn default() -> Self {
        SpyLinkCore {
            inner: MockCore::new(),
            start_calls: Arc::new(Mutex::new(Vec::new())),
            pump_calls: Arc::new(AtomicUsize::new(0)),
            stop_calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl RetroCore for SpyLinkCore {
    fn load(&mut self, rom: &Path) -> Result<(), CoreError> {
        self.inner.load(rom)
    }
    fn run_frame(&mut self, input: ButtonMask) {
        self.inner.run_frame(input)
    }
    fn video_xrgb8888(&self) -> &[u8] {
        self.inner.video_xrgb8888()
    }
    fn take_audio(&mut self) -> Vec<i16> {
        self.inner.take_audio()
    }
    fn serialize(&mut self) -> Result<Vec<u8>, CoreError> {
        self.inner.serialize()
    }
    fn unserialize(&mut self, data: &[u8]) -> Result<(), CoreError> {
        self.inner.unserialize(data)
    }
    fn save_ram(&self) -> Option<Vec<u8>> {
        self.inner.save_ram()
    }
    fn load_save_ram(&mut self, data: &[u8]) -> Result<(), CoreError> {
        self.inner.load_save_ram(data)
    }
    fn av_info(&self) -> AvInfo {
        self.inner.av_info()
    }
    fn start_link(&mut self, client_id: u16) {
        self.start_calls.lock().unwrap().push(client_id);
    }
    fn pump_link(&mut self) {
        self.pump_calls.fetch_add(1, Ordering::Relaxed);
    }
    fn stop_link(&mut self) {
        self.stop_calls.fetch_add(1, Ordering::Relaxed);
    }
}

fn spawn_with_core(core: Box<dyn RetroCore>) -> EmuHandle {
    let mut sink = StubSink::new();
    sink.open(32_768).expect("the stub refused to open");
    drain(sink.clone());
    let emu = EmuHandle::spawn(core, PathBuf::from("mock"), sink.ring(), None, None);
    emu.set_speed(Speed::Normal);
    assert!(
        wait_for(|| emu.state() != CoreState::Loading),
        "the core never finished loading"
    );
    emu
}

/// The libretro counterpart to `start_link`: `Cmd::EndLink` must reach the core's own
/// `stop_link`, not just drop the transport and flip `Link`'s active flag — a core left
/// believing a session is live keeps producing packets nobody is left to carry.
#[test]
fn ending_a_link_tells_the_cores_own_stop_link() {
    let core = SpyLinkCore::default();
    let stop_calls = core.stop_calls.clone();
    let emu = spawn_with_core(Box::new(core));
    let (_here, there) = paired_links();
    emu.begin_link(0, Box::new(there));
    assert!(wait_for(|| emu.net().is_active()), "begin_link never took");

    emu.end_link();
    assert!(
        wait_for(|| stop_calls.load(Ordering::Relaxed) > 0),
        "end_link must call the core's own stop_link"
    );
}

/// I7 mutation 2: `Cmd::BeginLink` must call the core's own `start_link`, with the
/// `client_id` it was actually given — `MockCore` never overrides `start_link`, so nothing
/// short of a spy shows whether the worker forgot to call through at all.
#[test]
fn beginning_a_link_calls_the_cores_own_start_link() {
    let core = SpyLinkCore::default();
    let start_calls = core.start_calls.clone();
    let emu = spawn_with_core(Box::new(core));
    let (_here, there) = paired_links();

    emu.begin_link(1, Box::new(there));
    assert!(
        wait_for(|| !start_calls.lock().unwrap().is_empty()),
        "Cmd::BeginLink must call the core's own start_link"
    );
    assert_eq!(start_calls.lock().unwrap().as_slice(), &[1]);
}

/// I7 mutation 1: the worker must call `core.pump_link()` every present, regardless of
/// speed or phase or whether a session is even live — see the comment above the call site in
/// `emu.rs` for why. A spawn with no `begin_link` at all is the strictest version of that
/// claim: it has to keep happening with nothing wired up yet.
#[test]
fn the_worker_pumps_the_core_every_present() {
    let core = SpyLinkCore::default();
    let pump_calls = core.pump_calls.clone();
    let emu = spawn_with_core(Box::new(core));

    assert!(
        wait_for(|| pump_calls.load(Ordering::Relaxed) > 3),
        "the worker must call core.pump_link() every present"
    );
    drop(emu);
}

/// I7 mutation 3: `Cmd::EndLink` must actually drop the transport, not merely mark the
/// session inactive — `is_active()` going false is necessary but not sufficient, since a
/// mutation that keeps the transport alive while still flipping the flag would pass every
/// other test here. A `Drop` flag is what proves the transport itself is gone, the same way
/// the module doc for `TcpLink` explains a socket drop matters for a real one.
#[test]
fn end_link_drops_the_transport_not_just_marks_it_inactive() {
    struct DropSignal {
        inner: PairedLink,
        dropped: Arc<AtomicBool>,
    }
    impl LinkChannel for DropSignal {
        fn send(&mut self, flags: i32, buf: &[u8]) {
            self.inner.send(flags, buf)
        }
        fn try_recv(&mut self) -> Option<Vec<u8>> {
            self.inner.try_recv()
        }
    }
    impl Drop for DropSignal {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::Relaxed);
        }
    }

    let emu = spawn();
    let (_here, there) = paired_links();
    let dropped = Arc::new(AtomicBool::new(false));
    let transport = DropSignal {
        inner: there,
        dropped: dropped.clone(),
    };
    emu.begin_link(0, Box::new(transport));
    assert!(wait_for(|| emu.net().is_active()), "begin_link never took");

    emu.end_link();
    assert!(
        wait_for(|| dropped.load(Ordering::Relaxed)),
        "Cmd::EndLink must drop the transport, not merely mark the session inactive"
    );
}

/// I2: a packet that arrived just before the session ended must not survive to be handed to
/// the next one. Proven directly against `Link`, since `MockCore::pump_link` is a no-op and
/// cannot itself drain this for a probe to observe — `slot-retro`'s own tests cover the
/// `drain_link`/`netpacket_poll_receive` half of this at the ABI seam.
#[test]
fn ending_a_link_clears_stale_packets_for_the_next_session() {
    let emu = spawn();
    let (_here, there) = paired_links();
    emu.begin_link(0, Box::new(there));
    assert!(wait_for(|| emu.net().is_active()), "begin_link never took");

    emu.net()
        .push_inbound(b"stale from the old session".to_vec());

    emu.end_link();
    // No settle needed: `Cmd::EndLink` clears both queues before flipping `is_active`, and
    // `Link::set_active`'s `Release` store is paired with `is_active`'s own `Acquire` load —
    // so observing the flag fall here is itself the guarantee the clear already happened,
    // not merely a likely one a fixed sleep would only approximate.
    assert!(wait_for(|| !emu.net().is_active()), "end_link never took");

    assert!(
        emu.net().take_inbound().is_none(),
        "a packet queued before the session ended survived into the next one"
    );
}

mod common;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use slot::audio::Ring;
use slot::emu::{CoreState, EmuHandle, Speed};
use slot::persist::Snapshot;
use slot_retro::{LinkChannel, MockCore};

#[derive(Default)]
struct Wire {
    a: VecDeque<Vec<u8>>,
    b: VecDeque<Vec<u8>>,
}

struct End {
    wire: Arc<Mutex<Wire>>,
    first: bool,
}

impl LinkChannel for End {
    fn send(&mut self, _flags: i32, buf: &[u8]) {
        let mut w = self.wire.lock().unwrap();
        match self.first {
            true => w.a.push_back(buf.to_vec()),
            false => w.b.push_back(buf.to_vec()),
        }
    }
    fn try_recv(&mut self) -> Option<Vec<u8>> {
        let mut w = self.wire.lock().unwrap();
        match self.first {
            true => w.b.pop_front(),
            false => w.a.pop_front(),
        }
    }
}

fn spawn(ring: Arc<Ring>) -> EmuHandle {
    let emu = EmuHandle::spawn(
        Box::new(MockCore::new()),
        PathBuf::from("mock"),
        ring,
        None,
        None,
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while emu.state() == CoreState::Loading {
        assert!(Instant::now() < deadline, "the core never settled");
        std::thread::sleep(Duration::from_millis(5));
    }
    emu
}

#[test]
fn a_cable_session_steps_both_consoles() {
    let wire = Arc::new(Mutex::new(Wire::default()));
    let host = spawn(Arc::new(Ring::new(4096)));
    let join = spawn(Arc::new(Ring::new(4096)));

    host.begin_cable(
        0,
        Box::new(End {
            wire: wire.clone(),
            first: true,
        }),
    );
    join.begin_cable(
        1,
        Box::new(End {
            wire: wire.clone(),
            first: false,
        }),
    );
    host.set_speed(Speed::Normal);
    join.set_speed(Speed::Normal);

    let deadline = Instant::now() + Duration::from_secs(5);
    while host.linked_frames() < 30 || join.linked_frames() < 30 {
        assert!(
            Instant::now() < deadline,
            "the pair stalled at host {} / joiner {} linked frames",
            host.linked_frames(),
            join.linked_frames()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_joiner_runs_the_hosts_machine_and_not_its_own() {
    let wire = Arc::new(Mutex::new(Wire::default()));
    let host = spawn(Arc::new(Ring::new(4096)));
    let join = spawn(Arc::new(Ring::new(4096)));

    join.set_speed(Speed::Normal);
    let deadline = Instant::now() + Duration::from_secs(5);
    while join.published_count() < 20 {
        assert!(Instant::now() < deadline, "the joiner never ran alone");
        std::thread::sleep(Duration::from_millis(10));
    }
    join.set_speed(Speed::Paused);

    host.begin_cable(
        0,
        Box::new(End {
            wire: wire.clone(),
            first: true,
        }),
    );
    join.begin_cable(
        1,
        Box::new(End {
            wire: wire.clone(),
            first: false,
        }),
    );
    host.set_speed(Speed::Normal);
    join.set_speed(Speed::Normal);

    let deadline = Instant::now() + Duration::from_secs(10);
    while join.linked_frames() < 20 {
        assert!(
            Instant::now() < deadline,
            "the joiner never started: {} linked frames",
            join.linked_frames()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let (joined, hosted) = (join.snapshot(), host.snapshot());
    assert!(
        !joined.save_ram_trusted() && !joined.resume_trusted(),
        "the joiner would save the host's game over its own"
    );
    assert!(
        hosted.save_ram_trusted() && hosted.resume_trusted(),
        "the host stopped saving its own game"
    );
}

#[test]
fn a_peer_that_never_speaks_stalls_rather_than_guessing() {
    let wire = Arc::new(Mutex::new(Wire::default()));
    let lonely = spawn(Arc::new(Ring::new(4096)));
    lonely.begin_cable(
        0,
        Box::new(End {
            wire: wire.clone(),
            first: true,
        }),
    );
    lonely.set_speed(Speed::Normal);

    let deadline = Instant::now() + Duration::from_secs(5);
    while !lonely.link_lost() {
        assert!(
            Instant::now() < deadline,
            "a silent peer was never reported lost; linked frames {}",
            lonely.linked_frames()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        lonely.linked_frames() <= slot::cable::DELAY,
        "it ran {} frames with nobody on the other end",
        lonely.linked_frames()
    );
}

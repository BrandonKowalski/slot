//! The emulated cable through the emulator worker: two devices, one process, one wire.
//!
//! `cable.rs`'s own tests cover the frame clock without a core. This covers the wiring: that a
//! session begun with `begin_cable` actually reaches `run_frame_linked`, and that two ends fed
//! the same buttons stay on the same frame.

mod common;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use slot::audio::Ring;
use slot::emu::{CoreState, EmuHandle, Speed};
use slot_retro::{LinkChannel, MockCore};

/// Two ends of one wire, in memory. Nothing is dropped or reordered, which is what TCP gives the
/// real transport too, so what this leaves untested is the network rather than the protocol.
#[derive(Default)]
struct Wire {
    a: VecDeque<Vec<u8>>,
    b: VecDeque<Vec<u8>>,
}

struct End {
    wire: Arc<Mutex<Wire>>,
    /// Which queue this end writes to; it reads the other.
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

/// A cable session runs. Before the wiring, `begin_cable` had nowhere to go and the worker kept
/// calling `run_frame`, so the linked frame count stayed at zero however long it ran.
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

    // Long enough for the seeded frames to be used up and real exchanged masks to carry it.
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

/// The joiner starts from the host's machine, not its own. Both devices simulate both consoles,
/// so a joiner that began from whatever its own card had would play a different game from
/// identical inputs: on hardware that showed as one SP waiting for a cable while the other was
/// already choosing a character.
#[test]
fn a_joiner_runs_the_hosts_machine_and_not_its_own() {
    let wire = Arc::new(Mutex::new(Wire::default()));
    let host = spawn(Arc::new(Ring::new(4096)));
    let join = spawn(Arc::new(Ring::new(4096)));

    // Run the joiner on alone first, so its machine is demonstrably somewhere else.
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

    // The joiner only reaches linked frames at all once it has restored what the host sent: an
    // unprimed cable refuses even the seeded frames, which need no peer mask and would otherwise
    // have run on the wrong machine.
    let deadline = Instant::now() + Duration::from_secs(10);
    while join.linked_frames() < 20 {
        assert!(
            Instant::now() < deadline,
            "the joiner never started: {} linked frames",
            join.linked_frames()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A peer that says nothing stalls the frame rather than being guessed at, and is eventually
/// reported lost. Guessing would desync the two devices silently, which is worse than stopping.
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
    // It ran the seeded frames and then stopped, rather than running on without the peer.
    assert!(
        lonely.linked_frames() <= slot::cable::DELAY,
        "it ran {} frames with nobody on the other end",
        lonely.linked_frames()
    );
}

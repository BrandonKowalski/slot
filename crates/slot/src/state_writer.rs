use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use slot_store::{write_slot_state, SlotState};

#[derive(Default)]
struct Queue {
    next: Option<(PathBuf, SlotState)>,
    busy: bool,
    closed: bool,
}

type Shared = Arc<(Mutex<Queue>, Condvar)>;

pub struct StateWriter {
    shared: Shared,
    threaded: bool,
}

impl StateWriter {
    pub fn spawn() -> Self {
        let shared: Shared = Arc::default();
        let worker = shared.clone();
        let threaded = thread::Builder::new()
            .name("slot-state".into())
            .spawn(move || work(&worker))
            .is_ok();
        StateWriter { shared, threaded }
    }

    pub fn save(&self, root: &Path, state: &SlotState) {
        if !self.threaded {
            return write(root, state);
        }
        let (lock, wake) = &*self.shared;
        let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
        q.next = Some((root.to_path_buf(), state.clone()));
        wake.notify_all();
    }

    pub fn flush(&self) {
        let (lock, wake) = &*self.shared;
        let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
        while q.next.is_some() || q.busy {
            q = wake.wait(q).unwrap_or_else(|e| e.into_inner());
        }
    }
}

impl Drop for StateWriter {
    fn drop(&mut self) {
        self.flush();
        let (lock, wake) = &*self.shared;
        lock.lock().unwrap_or_else(|e| e.into_inner()).closed = true;
        wake.notify_all();
    }
}

fn work(shared: &Shared) {
    let (lock, wake) = &**shared;
    loop {
        let (root, state) = {
            let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if let Some(job) = q.next.take() {
                    q.busy = true;
                    break job;
                }
                if q.closed {
                    return;
                }
                q = wake.wait(q).unwrap_or_else(|e| e.into_inner());
            }
        };
        write(&root, &state);
        lock.lock().unwrap_or_else(|e| e.into_inner()).busy = false;
        wake.notify_all();
    }
}

fn write(root: &Path, state: &SlotState) {
    if let Err(e) = write_slot_state(root, state) {
        eprintln!("slot: slot.state: {e}");
    }
}

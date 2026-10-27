use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use slot_store::atomic_write;

use crate::card_log::CardWrites;

pub struct Shot {
    pub path: PathBuf,
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

enum Job {
    Write(Shot),
    Remove(PathBuf),
}

#[derive(Default)]
struct Queue {
    jobs: VecDeque<Job>,
    removing: HashSet<PathBuf>,
    failures: usize,
    busy: bool,
    closed: bool,
}

type Shared = Arc<(Mutex<Queue>, Condvar)>;

pub struct ShotWriter {
    shared: Shared,
    threaded: bool,
    card_writes: Arc<CardWrites>,
}

impl ShotWriter {
    pub fn spawn(card_writes: Arc<CardWrites>) -> Self {
        let shared: Shared = Arc::default();
        let worker = shared.clone();
        let writes = card_writes.clone();
        let threaded = thread::Builder::new()
            .name("slot-shot".into())
            .spawn(move || work(&worker, &writes))
            .is_ok();
        ShotWriter {
            shared,
            threaded,
            card_writes,
        }
    }

    pub fn write(&self, shot: Shot) {
        self.queue(Job::Write(shot));
    }

    pub fn remove(&self, path: PathBuf) {
        if self.threaded {
            self.shared
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .removing
                .insert(path.clone());
        }
        self.queue(Job::Remove(path));
    }

    pub fn removing(&self) -> HashSet<PathBuf> {
        self.shared
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .removing
            .clone()
    }

    pub fn take_failures(&self) -> usize {
        std::mem::take(
            &mut self
                .shared
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .failures,
        )
    }

    fn queue(&self, job: Job) {
        if !self.threaded {
            if !run(&job, &self.card_writes) {
                self.shared
                    .0
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .failures += 1;
            }
            return;
        }
        let (lock, wake) = &*self.shared;
        lock.lock()
            .unwrap_or_else(|e| e.into_inner())
            .jobs
            .push_back(job);
        wake.notify_all();
    }

    pub fn flush(&self) {
        let (lock, wake) = &*self.shared;
        let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
        while !q.jobs.is_empty() || q.busy {
            q = wake.wait(q).unwrap_or_else(|e| e.into_inner());
        }
    }
}

impl Drop for ShotWriter {
    fn drop(&mut self) {
        self.flush();
        let (lock, wake) = &*self.shared;
        lock.lock().unwrap_or_else(|e| e.into_inner()).closed = true;
        wake.notify_all();
    }
}

fn work(shared: &Shared, card_writes: &CardWrites) {
    let (lock, wake) = &**shared;
    loop {
        let shot = {
            let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if let Some(job) = q.jobs.pop_front() {
                    q.busy = true;
                    break job;
                }
                if q.closed {
                    return;
                }
                q = wake.wait(q).unwrap_or_else(|e| e.into_inner());
            }
        };
        let ok = run(&shot, card_writes);
        let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
        q.busy = false;
        if !ok {
            q.failures += 1;
        }
        if let Job::Remove(path) = &shot {
            q.removing.remove(path);
        }
        drop(q);
        wake.notify_all();
    }
}

fn run(job: &Job, card_writes: &CardWrites) -> bool {
    match job {
        Job::Write(shot) => write(shot, card_writes),
        Job::Remove(path) => {
            let removed = std::fs::remove_file(path);
            if let Err(e) = &removed {
                eprintln!("slot: delete screenshot: {}: {e}", path.display());
            }
            card_writes.wrote("delete screenshot", &removed);
            true
        }
    }
}

fn write(shot: &Shot, card_writes: &CardWrites) -> bool {
    let written = encode(shot).and_then(|png| store(&shot.path, &png));
    if let Err(e) = &written {
        eprintln!("slot: screenshot: {}: {e}", shot.path.display());
    }
    card_writes.wrote("screenshot", &written);
    written.is_ok()
}

fn store(path: &Path, png: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    atomic_write(path, png)
}

fn encode(shot: &Shot) -> std::io::Result<Vec<u8>> {
    let px = (shot.w * shot.h) as usize;
    if shot.rgba.len() < px * 4 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "screenshot is smaller than its size",
        ));
    }
    let rgb: Vec<u8> = shot.rgba[..px * 4]
        .chunks_exact(4)
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, shot.w, shot.h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut w| {
            w.write_image_data(&rgb)?;
            w.finish()
        })
        .map_err(std::io::Error::other)?;
    Ok(out)
}

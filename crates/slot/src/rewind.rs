use std::collections::VecDeque;

/// The whole rewind history, per spec section 5.
pub const REWIND_BYTES: usize = 20 * 1024 * 1024;

/// History is chained from the newest state backwards: `cur` is held whole and every entry
/// is `lz4(state XOR previous_state)`, so a pop is one decompress and one XOR no matter how
/// deep it goes. Anchoring at the newest end is what makes eviction free: the oldest entry
/// is the only one nothing depends on, so dropping it costs history and nothing else.
pub struct Rewind {
    budget: usize,
    /// The state the next `pop` returns, and what the entries behind it are relative to.
    cur: Option<Vec<u8>>,
    deltas: VecDeque<Vec<u8>>,
    bytes: usize,
    scratch: Vec<u8>,
}

impl Rewind {
    pub fn new(budget_bytes: usize) -> Self {
        Rewind {
            budget: budget_bytes,
            cur: None,
            deltas: VecDeque::new(),
            bytes: 0,
            scratch: Vec::new(),
        }
    }

    pub fn push(&mut self, state: &[u8]) {
        let Some(prev) = self.cur.replace(state.to_vec()) else {
            return;
        };
        // A core that re-shaped its state cannot be XORed against what it was before, so
        // everything behind that point is unreachable rather than merely different.
        if prev.len() != state.len() {
            self.deltas.clear();
            self.bytes = 0;
            return;
        }
        self.scratch.clear();
        self.scratch
            .extend(prev.iter().zip(state).map(|(a, b)| a ^ b));
        let entry = lz4_flex::compress(&self.scratch);
        self.bytes += entry.len();
        self.deltas.push_back(entry);
        while self.bytes > self.budget {
            match self.deltas.pop_front() {
                Some(oldest) => self.bytes -= oldest.len(),
                None => break,
            }
        }
    }

    pub fn pop(&mut self) -> Option<Vec<u8>> {
        let cur = self.cur.take()?;
        let Some(entry) = self.deltas.pop_back() else {
            return Some(cur);
        };
        self.bytes -= entry.len();
        let mut prev = vec![0u8; cur.len()];
        match lz4_flex::decompress_into(&entry, &mut prev) {
            Ok(_) => {
                for (p, c) in prev.iter_mut().zip(&cur) {
                    *p ^= c;
                }
                self.cur = Some(prev);
            }
            // A delta that will not decompress ends the history. Handing the core a frame
            // of noise would be worse than refusing to go back any further.
            Err(e) => {
                eprintln!("slot: rewind: {e}");
                self.deltas.clear();
                self.bytes = 0;
            }
        }
        Some(cur)
    }

    /// Compressed history only. `cur` is the cursor, not something the ring is storing.
    pub fn bytes_used(&self) -> usize {
        self.bytes
    }

    /// States `pop` can still hand back.
    pub fn depth(&self) -> usize {
        self.deltas.len() + usize::from(self.cur.is_some())
    }

    /// History held, 0 to 100. Read against the byte budget rather than against `depth`,
    /// which nothing bounds: how many states fit is whatever the deltas compressed to.
    pub fn fill(&self) -> u8 {
        if self.budget == 0 {
            return 0;
        }
        (self.bytes * 100 / self.budget).min(100) as u8
    }
}

/// `Rewind` on a thread of its own.
///
/// The XOR and the LZ4 are 2.6 ms of a snapshot's 9.2 ms on the H700, and neither touches
/// the core — only the bytes it just handed over. Doing them on the emu thread spends that
/// inside a 16.67 ms frame for no reason. `serialize` has to stay where the core is; this
/// is the half that does not.
///
/// Ordering is what makes it safe. The channel is FIFO, so a `pop` sent after a run of
/// `push`es is served after them: history is never read before the writes in front of it
/// have landed. The cost is that the first `pop` of a rewind waits for whatever is still in
/// flight, which is a frame or two of compression and happens once per trigger pull.
pub struct RewindThread {
    tx: std::sync::mpsc::SyncSender<Msg>,
    fill: std::sync::Arc<std::sync::atomic::AtomicU8>,
}

enum Msg {
    Push(Vec<u8>),
    Pop(std::sync::mpsc::SyncSender<Option<Vec<u8>>>),
}

impl RewindThread {
    pub fn spawn(budget_bytes: usize) -> Self {
        // Four deep, and a full queue blocks the sender rather than dropping. A snapshot
        // arrives every other frame and takes about a sixth of that to compress, so four
        // is roughly 130 ms of slack that normal play never touches. Dropping instead was
        // tried and is worse than it sounds: history goes missing with nothing to say so,
        // and rewind quietly coarsens. Blocking only bites when the compressor is
        // persistently behind the core, which is a machine that is not holding 60 fps
        // anyway, and it is self limiting — a stalled emu thread produces fewer snapshots.
        let (tx, rx) = std::sync::mpsc::sync_channel::<Msg>(4);
        let fill = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
        let published = fill.clone();
        std::thread::Builder::new()
            .name("slot-rewind".into())
            .spawn(move || {
                let mut rewind = Rewind::new(budget_bytes);
                while let Ok(msg) = rx.recv() {
                    match msg {
                        Msg::Push(state) => {
                            rewind.push(&state);
                            published.store(rewind.fill(), std::sync::atomic::Ordering::Relaxed);
                        }
                        Msg::Pop(reply) => {
                            let out = rewind.pop();
                            published.store(rewind.fill(), std::sync::atomic::Ordering::Relaxed);
                            // A caller that has gone away is a session that ended mid
                            // rewind, which is not an error worth reporting.
                            let _ = reply.send(out);
                        }
                    }
                }
            })
            .expect("rewind thread");
        RewindThread { tx, fill }
    }

    /// Hands the state over. Returns immediately unless the compressor is four snapshots
    /// behind, which is backpressure rather than a routine cost.
    pub fn push(&self, state: Vec<u8>) {
        let _ = self.tx.send(Msg::Push(state));
    }

    /// Blocks until every push queued ahead of it has been applied, then returns the state.
    pub fn pop(&self) -> Option<Vec<u8>> {
        let (tx, rx) = std::sync::mpsc::sync_channel(0);
        if self.tx.send(Msg::Pop(tx)).is_err() {
            return None;
        }
        rx.recv().ok().flatten()
    }

    /// Last published fill, read without a round trip: the HUD wants it every frame and
    /// does not need it to be this frame's.
    pub fn fill(&self) -> u8 {
        self.fill.load(std::sync::atomic::Ordering::Relaxed)
    }
}

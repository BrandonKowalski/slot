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

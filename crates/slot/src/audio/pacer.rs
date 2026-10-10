use std::time::{Duration, Instant};

const RESYNC_AFTER: u32 = 4;

pub struct Pacer {
    span: Duration,
    next: Option<Instant>,
}

impl Pacer {
    pub fn new(span: Duration) -> Self {
        Pacer { span, next: None }
    }

    pub fn wait(&mut self, now: Instant) -> Duration {
        let from = self
            .next
            .filter(|&due| now < due + self.span * RESYNC_AFTER)
            .unwrap_or(now);
        let due = from + self.span;
        self.next = Some(due);
        due.saturating_duration_since(now)
    }

    pub fn reset(&mut self) {
        self.next = None;
    }
}

use std::time::Duration;

pub struct Silence {
    heard: Duration,
    after: Duration,
}

impl Silence {
    pub fn new(after: Duration) -> Self {
        Silence {
            heard: Duration::ZERO,
            after,
        }
    }

    pub fn hear(&mut self, samples: &[i16], span: Duration) -> bool {
        match samples.iter().all(|&s| s == 0) {
            true => self.heard = self.heard.saturating_add(span),
            false => self.heard = Duration::ZERO,
        }
        self.heard < self.after
    }
}

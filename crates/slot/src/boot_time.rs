use std::sync::OnceLock;
use std::time::Instant;

static START: OnceLock<Instant> = OnceLock::new();

pub fn mark(phase: &str) {
    eprintln!("slot: boot: {phase} at {:.1} ms", ms());
}

pub fn ms() -> f64 {
    START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1e3
}

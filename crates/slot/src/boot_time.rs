use std::sync::OnceLock;
use std::time::Instant;

static START: OnceLock<Instant> = OnceLock::new();

pub fn mark(phase: &str) {
    let start = *START.get_or_init(Instant::now);
    eprintln!(
        "slot: boot: {phase} at {:.1} ms",
        start.elapsed().as_secs_f64() * 1e3
    );
}

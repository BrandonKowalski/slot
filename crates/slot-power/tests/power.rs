use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use slot_power::{Battery, Charge, LedState, LidPolicy, Platform, Power};

/// The panel a real platform would drive, shared so the test can see what reached it.
struct Panel {
    step: Arc<AtomicU8>,
    clock: i64,
}

impl Platform for Panel {
    fn set_backlight(&mut self, step: u8) {
        self.step.store(step, Ordering::Relaxed);
    }

    fn battery(&self) -> Option<Battery> {
        None
    }

    fn charge(&self) -> Charge {
        Charge::Unknown
    }

    /// No LED. The lid tests are about the panel.
    fn set_led(&mut self, _state: LedState) {}

    fn restart(&mut self) -> ! {
        panic!("the panel never powers off")
    }

    fn poweroff(&mut self) -> ! {
        panic!("the panel never powers off")
    }

    fn root(&self) -> &Path {
        Path::new("sdcard")
    }

    fn now(&self) -> i64 {
        self.clock
    }

    fn set_clock(&mut self, secs: i64) {
        self.clock = secs;
    }

    /// No motor. The lid tests are about the panel, and `SimPlatform` is what records a
    /// rumble.
    fn set_rumble(&mut self, _strength: u16) {}
}

fn power() -> (Power, Arc<AtomicU8>) {
    let step = Arc::new(AtomicU8::new(0));
    let panel = Panel {
        step: step.clone(),
        clock: 0,
    };
    (Power::new(Box::new(panel), Duration::from_secs(60)), step)
}

fn lit() -> (Power, Arc<AtomicU8>) {
    power()
}

#[test]
fn the_lid_restores_the_level_it_darkened() {
    let (mut p, step) = lit();
    p.set_backlight(7);
    p.on_close();
    assert_eq!(step.load(Ordering::Relaxed), 0, "the panel stayed lit");
    p.on_open();
    assert_eq!(step.load(Ordering::Relaxed), 7);
}

/// A hall sensor bounces. A second close must not make the dark panel the level to restore.
#[test]
fn closing_twice_does_not_swallow_the_level() {
    let (mut p, step) = lit();
    p.set_backlight(4);
    p.on_close();
    p.on_close();
    p.on_open();
    assert_eq!(step.load(Ordering::Relaxed), 4);
}

/// The keyboard still reaches a dozing host. Brightness pressed with the lid shut belongs
/// to the next wake, not to a panel that is meant to be dark.
#[test]
fn a_level_set_while_shut_waits_for_the_open() {
    let (mut p, step) = lit();
    p.set_backlight(2);
    p.on_close();
    p.set_backlight(9);
    assert_eq!(step.load(Ordering::Relaxed), 0);
    p.on_open();
    assert_eq!(step.load(Ordering::Relaxed), 9);
}

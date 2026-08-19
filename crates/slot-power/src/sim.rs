use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{Battery, Charge, LedState, Motor, Platform};

/// The host stands in for every part of the device the power path touches, so the whole of
/// it runs from the keyboard with no hardware present.
pub struct SimPlatform {
    root: PathBuf,
    /// Posted by the simulated hall sensor or power button, taken by the next `sleep`.
    /// What `set_clock` moved the clock by. The device writes the hardware clock; a
    /// frontend that reset the developer's machine to 2003 would be a bug with a bug report.
    offset: i64,
    motor: Motor,
    /// No gadget to rebind here, so counting the asks is the whole of it.
    relinks: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl SimPlatform {
    pub fn new() -> Self {
        SimPlatform::at(root_from_env())
    }

    pub fn at(root: PathBuf) -> Self {
        SimPlatform {
            root,
            offset: 0,
            motor: Motor::default(),
            relinks: std::sync::Arc::default(),
        }
    }

    pub fn relinks(&self) -> std::sync::Arc<std::sync::atomic::AtomicUsize> {
        self.relinks.clone()
    }

    /// Outlives the move into `Power`, which is what makes the rumble path readable with no
    /// hardware to feel.
    pub fn motor(&self) -> Motor {
        self.motor.clone()
    }
}

impl Default for SimPlatform {
    fn default() -> Self {
        SimPlatform::new()
    }
}

impl Platform for SimPlatform {
    /// macOS has no panel the frontend may drive. Brightness reaches the HUD and stops.
    fn set_backlight(&mut self, _step: u8) {}

    /// Full, and on the cable. Never the laptop's own gauge: that is not this device's, and
    /// reporting it would let the host trip a battery-critical flush that means nothing.
    ///
    /// Fixed rather than absent so the gauge is on screen for captures, and fixed at *this*
    /// reading because it is the only one that arms nothing. `BATTERY_CRITICAL` is 5 and
    /// `BATTERY_LOW` is 20, and both are additionally gated on a charge state that is not
    /// `Charging`, so 100 and `Charging` clears the cutoff twice over. Only the host reads
    /// this; `DevicePlatform` still reads the real gauge out of sysfs on the RG SP.
    fn battery(&self) -> Option<Battery> {
        Some(Battery {
            percent: 100,
            charge: Charge::Charging,
        })
    }

    fn charge(&self) -> Charge {
        Charge::Charging
    }

    /// A laptop has no LED to drive.
    fn set_led(&mut self, _state: LedState) {}

    /// Suspends nothing. The event loop is what feeds the simulated hall sensor, so
    /// blocking here would shut out the very event that ends the wait.
    fn restart(&mut self) -> ! {
        std::process::exit(0)
    }

    fn poweroff(&mut self) -> ! {
        std::process::exit(0)
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn now(&self) -> i64 {
        system_secs() + self.offset
    }

    fn set_clock(&mut self, secs: i64) {
        self.offset = secs - system_secs();
    }

    fn relink_adb(&mut self) -> bool {
        self.relinks
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        true
    }

    /// No motor to drive, so recording the value is the whole of it.
    fn set_rumble(&mut self, strength: u16) {
        self.motor.set(strength);
    }
}

fn system_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn root_from_env() -> PathBuf {
    std::env::var_os("SLOT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("sdcard"))
}

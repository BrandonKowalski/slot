mod device;
mod motor;
mod power;
mod sim;

pub use device::{has_bit, motor_change, rumble_node, DevicePlatform};
pub use motor::Motor;
pub use power::Power;
pub use sim::SimPlatform;

use std::path::Path;
use std::time::Duration;

/// How far down a sleep goes. Which one the lid uses was a bring-up question and hardware
/// has now answered it: `Mem` is suspend-to-RAM and measured under 45 mA on an RG SP,
/// against 141 mA without the platform's Super Standby bit and 400-700 mA awake. `Doze`
/// only darkens and waits, and exists for hosts that cannot suspend at all.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SleepDepth {
    Doze,
    Mem,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum WakeReason {
    LidOpen,
    Power,
    Timeout,
}

/// What the kernel's `status` attribute said, decoded. `Unknown` is a first-class answer
/// rather than an error: on this PMIC `current_now` already reads empty, so a standard
/// `power_supply` attribute being present is no promise that it is populated. Every
/// consumer has to make `Unknown` behave the way the frontend did before it could read
/// the charge state at all.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Charge {
    Unknown,
    Discharging,
    Charging,
    Full,
}

/// One reading of the gauge. The two halves come from different files and, once the fast
/// tick is in, on different cadences, so they are carried together rather than fetched
/// separately: a consumer that asked for each in turn could act on a percent and a charge
/// state that never coexisted.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Battery {
    pub percent: u8,
    pub charge: Charge,
}

/// What the device wants the LED to say, not how to say it. The mapping to colours or to a
/// single brightness belongs to the platform, which is the only thing that has seen the node.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum LedState {
    Off,
    Running,
    Low,
    Charging,
    Charged,
}

pub trait Platform: Send {
    fn set_backlight(&mut self, step: u8);
    /// The gauge and the charge state in one read. `None` where there is no gauge, and
    /// where the gauge will not parse.
    fn battery(&self) -> Option<Battery>;
    /// The charge state alone: the cheap half of `battery`, and the half that changes the
    /// instant a cable moves. `Unknown` where there is no gauge to ask.
    fn charge(&self) -> Charge;
    /// Best effort. A device with no LED node is a device that does not have one, which is
    /// not a failure.
    fn set_led(&mut self, state: LedState);
    fn sleep(&mut self, depth: SleepDepth, timeout: Duration) -> WakeReason;
    fn poweroff(&mut self) -> !;
    /// The same teardown as a power off — busybox init runs its shutdown actions for a
    /// reboot too — so the GPU module is unloaded either way, which is the thing that stops
    /// this hardware hanging with the rails up.
    fn restart(&mut self) -> !;
    /// The content root: the card's mount point on the device, `SLOT_ROOT` on the host.
    fn root(&self) -> &Path;
    /// Seconds since the epoch, from the RTC on device and the system clock on the host.
    fn now(&self) -> i64;
    /// Persist to the hardware clock. The host stores an offset instead, so the setter is
    /// exercisable without touching the machine's clock.
    fn set_clock(&mut self, secs: i64);
    /// 0 is off, `u16::MAX` is full. Strong and weak are one motor here; the caller has
    /// already taken the louder.
    fn set_rumble(&mut self, strength: u16);

    /// Put the debug link back after the cable was pulled, and say whether there was anything
    /// to put back. `false` everywhere there is no USB gadget to rebind, which is every
    /// platform but the device.
    fn relink_adb(&mut self) -> bool {
        false
    }
}

/// Lid close and lid wake are one code path, parameterised.
pub trait LidPolicy {
    fn on_close(&mut self);
    fn on_open(&mut self);
    fn depth(&self) -> SleepDepth;
    fn timeout(&self) -> Duration;
}

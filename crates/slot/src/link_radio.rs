//! Bringing the private link network up and down, by shelling out to `ags-net link`.
//!
//! Not on `Platform` despite that being the seam for talking to the machine: `App` owns its
//! `Power` outright, and a link session needs this from a worker thread. See the plan.
//!
//! Four subcommands rather than two. `host` and `join` are the session itself; `warm` and
//! `cool` are the driver either side of it, and they exist because the radio is off at boot
//! for the standby battery and loading it costs a second that can be paid while the player is
//! still choosing. An older BaseOS has neither and answers with a usage error, which is why
//! nothing here reads their status.

#[cfg(feature = "device")]
use std::sync::mpsc::{channel, Sender};
#[cfg(feature = "device")]
use std::sync::OnceLock;

use crate::link_net::Cancel;

/// Which end of a link session this device is. The host brings the access point up and
/// waits; the joiner associates to it and connects out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkRole {
    Host,
    Join,
}

impl LinkRole {
    /// The subcommand `ags-net link` expects.
    pub fn arg(self) -> &'static str {
        match self {
            LinkRole::Host => "host",
            LinkRole::Join => "join",
        }
    }
}

/// Why the network did not come up.
///
/// Three, not one, because the screen says a different sentence for each and only one of them
/// is about the radio at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RadioFail {
    /// `ags-net link join` exited 3: the station ran for its whole search and no host
    /// answered. The radio is fine; the other player is not there.
    NoHost,
    /// The player backed out, and the child was killed rather than waited out.
    Cancelled,
    /// Anything else — a dead access point, an interface that never appeared, no `ags-net` on
    /// this machine at all.
    Radio(String),
}

/// Work for the radio that nothing waits on, run one at a time and in the order asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioJob {
    /// Load the driver and wait for its interfaces, without associating or hosting.
    Warm,
    /// Unload it, unless `ags-net` finds something still wants it.
    Cool,
    /// End a session: drop the access point or the association, and cool on the way out.
    Down,
}

/// Where `App` sends that work. A trait rather than a function so a test can watch what was
/// asked for, in what order, without a radio or a process anywhere near it.
pub trait RadioJobs: Send {
    fn ask(&mut self, job: RadioJob);
}

/// The real one. Every job goes onto one queue served by one thread, so a `Cool` asked for
/// after a `Warm` cannot overtake it and leave the radio loaded behind a screen that has been
/// left — which is exactly what two threads racing would do on a quick in-and-out.
pub struct RadioQueue;

/// What `App` starts with. The queue's thread is not spawned until the first job is asked
/// for, so a host build that never links never starts one.
pub fn radio_jobs() -> Box<dyn RadioJobs> {
    Box::new(RadioQueue)
}

#[cfg(feature = "device")]
impl RadioJobs for RadioQueue {
    fn ask(&mut self, job: RadioJob) {
        // A send that fails means the worker is gone, which can only happen if it panicked.
        // There is nothing useful to do about it from a frame loop, and nothing waits on it.
        let _ = queue().send(job);
    }
}

/// One worker for the process, spawned on the first job.
#[cfg(feature = "device")]
fn queue() -> &'static Sender<RadioJob> {
    static Q: OnceLock<Sender<RadioJob>> = OnceLock::new();
    Q.get_or_init(|| {
        let (tx, rx) = channel::<RadioJob>();
        std::thread::spawn(move || {
            for job in rx {
                match job {
                    RadioJob::Warm => run("warm"),
                    RadioJob::Cool => run("cool"),
                    RadioJob::Down => down(),
                }
            }
        });
        tx
    })
}

/// A subcommand whose outcome nobody acts on. A BaseOS without `warm` and `cool` exits 2 with
/// a usage message, which is exactly as harmless as it sounds: the radio then behaves the way
/// it did before they existed.
#[cfg(feature = "device")]
fn run(sub: &str) {
    let _ = std::process::Command::new("ags-net")
        .arg("link")
        .arg(sub)
        .status();
}

/// Blocking, and slow enough to matter — about two seconds for a host and up to thirty for a
/// joiner that is waiting out its search. The caller owns a thread the UI is not waiting on,
/// and hands in the flag its player can set: `ags-net` is a child process, so a cancel kills
/// it rather than waiting for a window the player has already given up on.
#[cfg(feature = "device")]
pub fn up(role: LinkRole, cancel: &Cancel) -> Result<(), RadioFail> {
    let mut child = std::process::Command::new("ags-net")
        .arg("link")
        .arg(role.arg())
        .spawn()
        .map_err(|e| {
            RadioFail::Radio(format!("ags-net link {} would not start: {e}", role.arg()))
        })?;
    loop {
        if cancel.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(RadioFail::Cancelled);
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            // 3 is `ags-net link join` saying it searched and found no host. Reported as
            // itself so the screen can say nobody arrived, which is what happened, rather
            // than blaming a radio that worked.
            Ok(Some(status)) if status.code() == Some(3) => return Err(RadioFail::NoHost),
            Ok(Some(status)) => {
                return Err(RadioFail::Radio(format!(
                    "ags-net link {} failed: {status}",
                    role.arg()
                )))
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
            Err(e) => {
                return Err(RadioFail::Radio(format!(
                    "ags-net link {}: {e}",
                    role.arg()
                )))
            }
        }
    }
}

/// Best effort and infallible on purpose: this runs on the failure path of every step above
/// it, and a teardown that can fail is a teardown callers skip.
#[cfg(feature = "device")]
pub fn down() {
    let _ = std::process::Command::new("ags-net")
        .arg("link")
        .arg("down")
        .status();
}

/// No radio to bring up anywhere but the device, and nothing to shell out to. A host build
/// running two copies of slot over loopback is a real way to drive the screen, so this
/// succeeds rather than refusing.
#[cfg(not(feature = "device"))]
pub fn up(_role: LinkRole, _cancel: &Cancel) -> Result<(), RadioFail> {
    Ok(())
}

#[cfg(not(feature = "device"))]
pub fn down() {}

/// The same queue off-device, where every job is a no-op. It keeps one code path in `App`:
/// the ordering the device needs is not something a host build should have to know about.
#[cfg(not(feature = "device"))]
impl RadioJobs for RadioQueue {
    fn ask(&mut self, _job: RadioJob) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A device told to join that hosts instead brings up an access point nobody joins, and
    /// the failure it produces is "nobody arrived" — which reads as the other player's fault.
    #[test]
    fn each_role_asks_for_its_own_subcommand() {
        assert_eq!(LinkRole::Host.arg(), "host");
        assert_eq!(LinkRole::Join.arg(), "join");
    }

    /// The queue is what keeps a cool behind a warm. Off-device it does nothing at all, which
    /// is still the one thing every caller depends on: asking is never an error.
    #[test]
    fn asking_for_a_job_is_never_an_error() {
        let mut jobs = radio_jobs();
        jobs.ask(RadioJob::Warm);
        jobs.ask(RadioJob::Cool);
        jobs.ask(RadioJob::Down);
    }
}

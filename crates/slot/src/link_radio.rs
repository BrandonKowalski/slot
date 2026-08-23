//! Bringing the private link network up and down, by shelling out to `ags-net link`.
//!
//! Not on `Platform` despite that being the seam for talking to the machine: `App` owns its
//! `Power` outright, and a link session needs this from a worker thread. See the plan.

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

/// Blocking, and slow enough to matter — one to five seconds in the measurements, up to
/// `AGS_LINK_WAIT` before `ags-net` gives up. The caller owns a thread the UI is not waiting on.
#[cfg(feature = "device")]
pub fn up(role: LinkRole) -> std::io::Result<()> {
    let status = std::process::Command::new("ags-net")
        .arg("link")
        .arg(role.arg())
        .status()?;
    if status.success() {
        return Ok(());
    }
    Err(std::io::Error::other(format!(
        "ags-net link {} failed: {status}",
        role.arg()
    )))
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
pub fn up(_role: LinkRole) -> std::io::Result<()> {
    Ok(())
}

#[cfg(not(feature = "device"))]
pub fn down() {}

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
}

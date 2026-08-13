//! Build provenance for the about label. Every probe here fails soft and reports `unknown`:
//! the device build runs in a container as root against a bind mount owned by another uid,
//! which is exactly the case git refuses with "detected dubious ownership", and a frontend
//! that would not build because it could not name itself would be a poor trade.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8(out.stdout).ok()?.trim().to_string())
}

fn main() {
    // A new commit changes the hash, so the label has to be rebuilt with it.
    println!("cargo:rerun-if-changed=../../.git/HEAD");

    // The container's git is running as root over a mount owned by someone else. Marking the
    // tree safe is the documented way through it, and it fails harmlessly where the config is
    // already right or git is absent entirely.
    let _ = git(&["config", "--global", "--add", "safe.directory", "/src"]);

    let hash = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let dirty = git(&["status", "--porcelain"]).is_some_and(|s| !s.is_empty());
    // UTC, like everything else this project dates: the card keeps it, the RTC keeps it, and
    // without the flag a container an hour past midnight stamps a different day than the host
    // that started it. No date crate for one string — `date` is on every machine this builds
    // on, and `unknown` stands in where it is not.
    let date = Command::new("date")
        .args(["-u", "+%Y-%m-%d"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());

    println!("cargo:rustc-env=SLOT_GIT_HASH={hash}");
    println!("cargo:rustc-env=SLOT_GIT_DIRTY={}", u8::from(dirty));
    println!("cargo:rustc-env=SLOT_BUILD_DATE={date}");
}

use std::path::{Path, PathBuf};

use slot_retro::{LibretroCore, MockCore, RetroCore};
use slot_store::Core;

use crate::root;

/// Names the dylib outright, for a build that keeps it somewhere the search does not look.
///
/// This wins over `System/selected_core.ini` for which dylib loads — it stays a developer
/// escape hatch, not a second way to pick a core. It is not silent about that: it does not
/// touch which `Core` a cart resolves to (still the ini, still what the state directory
/// under `States/<core>/` is named after), only which file `open_core` opens. Pointing this
/// at gpSP for a cart the ini never mentions runs gpSP with its states filed under
/// `States/mgba/` — correct for a developer who set the override on purpose, a trap for
/// anyone who forgot it was set.
const CORE_ENV: &str = "SLOT_CORE";

/// Which dylib backs a core. The device keeps both in `System/`, so this is a filename
/// rather than a search: whichever the cart asked for is either there or it is not.
pub fn dylib_name(core: Core) -> String {
    format!(
        "{}_libretro.{}",
        core.as_str(),
        std::env::consts::DLL_EXTENSION
    )
}

/// Most specific first: the environment, then next to the binary, which is how the device
/// ships, then the `vendor` directory `scripts/fetch-core.sh` writes into.
fn candidates(core: Core) -> Vec<PathBuf> {
    if let Some(named) = std::env::var_os(CORE_ENV) {
        return vec![PathBuf::from(named)];
    }
    // Spelled from the platform's own convention so the same search finds the device's `.so`.
    let name = dylib_name(core);
    let mut paths = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        paths.push(dir.join(&name));
        paths.push(dir.join("vendor").join(&name));
    }
    paths.push(Path::new("vendor").join(&name));
    paths
}

/// The one place a cart's `Core` becomes a dylib path. Callers that already know which core
/// they want — `session.rs` resolves it once per insert — pass it straight through; nothing
/// downstream of this re-derives it, so there is nowhere left for the dylib choice to
/// disagree with wherever the caller filed that cart's states.
pub fn open_core(root: &Path, core: Core) -> Box<dyn RetroCore> {
    open_core_for(root, &candidates(core))
}

/// The named core if one of these opens, the mock if none of them do. A missing core is not
/// a failure to boot: the shelf, the slot and every gesture are reachable either way.
///
/// The core is told the content root's own folders, never the dylib's: on the device the
/// core lives in `System/` and the user's BIOS does not.
pub fn open_core_for(root: &Path, paths: &[PathBuf]) -> Box<dyn RetroCore> {
    let bios = root::bios_dir(root);
    let saves = root::saves_dir(root);
    for path in paths {
        if !path.exists() {
            continue;
        }
        match LibretroCore::open_with(path, &bios, &saves) {
            Ok(core) => {
                eprintln!("slot: core {}", path.display());
                return Box::new(core);
            }
            Err(e) => eprintln!("slot: {}: {e}", path.display()),
        }
    }
    eprintln!("slot: no core vendored, running the mock");
    Box::new(MockCore::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Both tests below mutate `SLOT_CORE`, which is process-global; the test harness runs
    /// tests in this module on separate threads by default, so without this they can
    /// interleave and read back a value neither of them set.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The half of "the dylib chosen and the state directory used agree" that lives entirely
    /// in this module: `candidates`, and therefore `open_core`, never spells a filename that
    /// does not match the `Core` it was handed. `crates/slot/tests/gpsp.rs` pins the other
    /// half — that a cart resolved to the same `Core` reads its resume state from the
    /// matching `States/<core>/` directory — through the real `Session`.
    #[test]
    fn candidates_search_the_named_cores_own_filename_only() {
        let _g = lock();
        // A developer's shell leaking `SLOT_CORE` into this run would short-circuit the
        // search this test exists to check, so it cannot assume the var is unset.
        std::env::remove_var(CORE_ENV);

        let mgba = candidates(Core::Mgba);
        let gpsp = candidates(Core::Gpsp);
        assert_ne!(mgba, gpsp, "the two cores searched the same paths");
        assert!(!mgba.is_empty());
        assert!(!gpsp.is_empty());
        for path in &mgba {
            let name = path.file_name().unwrap().to_str().unwrap();
            assert!(name.starts_with("mgba_libretro"), "{name} is not mGBA's");
        }
        for path in &gpsp {
            let name = path.file_name().unwrap().to_str().unwrap();
            assert!(name.starts_with("gpsp_libretro"), "{name} is not gpSP's");
        }
    }

    /// The override is a filename, not a `Core`: it must win regardless of which core asked,
    /// which is what makes it a trap when the ini disagrees rather than a second selector.
    #[test]
    fn the_env_override_ignores_which_core_was_asked_for() {
        let _g = lock();
        std::env::set_var(CORE_ENV, "/dev/null/named-core");
        let mgba = candidates(Core::Mgba);
        let gpsp = candidates(Core::Gpsp);
        std::env::remove_var(CORE_ENV);
        assert_eq!(mgba, vec![PathBuf::from("/dev/null/named-core")]);
        assert_eq!(
            mgba, gpsp,
            "the override stopped winning for one of the cores"
        );
    }
}

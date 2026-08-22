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

/// Most specific first: the environment, then the content root's own `System/` — where the
/// device actually keeps a core, and, on the device, also where the binary itself lives, so
/// the next candidate coincides with this one there. Off the device — a host build, or a
/// test with its own tmp root — the binary's directory and `root` are different places, and
/// only searching the root's own `System/` gives a cart's own content root somewhere a test
/// can plant a dylib under. The `vendor` directory `scripts/fetch-core.sh` writes into is
/// last: a host development convenience, not anywhere a shipped device looks.
fn candidates(root: &Path, core: Core) -> Vec<PathBuf> {
    if let Some(named) = std::env::var_os(CORE_ENV) {
        return vec![PathBuf::from(named)];
    }
    // Spelled from the platform's own convention so the same search finds the device's `.so`.
    let name = dylib_name(core);
    let mut paths = vec![root.join("System").join(&name)];
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
/// they want — `session.rs` resolves it once per insert — pass it straight through, which is
/// what keeps this call from disagreeing with the caller's own choice. It says nothing about
/// what the caller does with that `Core` afterward: `App` is the one that has to keep using
/// the same value for every later read and write, and it does that by storing it rather than
/// asking again.
pub fn open_core(root: &Path, core: Core) -> Box<dyn RetroCore> {
    open_core_for(root, core, &candidates(root, core))
}

/// The named core if one of these opens, the mock if none of them do. A missing core is not
/// a failure to boot: the shelf, the slot and every gesture are reachable either way.
///
/// The core is told the content root's own folders, never the dylib's: on the device the
/// core lives in `System/` and the user's BIOS does not.
///
/// `core` is redundant with `paths` in production — `open_core` derived both from the same
/// `Core` — but this function stays the seam that takes `paths` explicitly, because tests
/// plant a dylib somewhere `candidates` would not otherwise look. `apply_core_options` needs
/// `core` too, and only `open_core_for` ever holds a concrete `slot_retro::LibretroCore` to
/// call it on: everything above here deals in `Box<dyn RetroCore>`, which has no `set_option`.
/// That is also why the call sits here rather than at a caller — after `open_with` succeeds,
/// before the `Box<dyn RetroCore>` is handed back and `load` becomes reachable at all.
pub fn open_core_for(root: &Path, core: Core, paths: &[PathBuf]) -> Box<dyn RetroCore> {
    let bios = root::bios_dir(root);
    let saves = root::saves_dir(root);
    for path in paths {
        if !path.exists() {
            continue;
        }
        match LibretroCore::open_with(path, &bios, &saves) {
            Ok(mut opened) => {
                apply_core_options(&mut opened, core);
                eprintln!("slot: core {}", path.display());
                return Box::new(opened);
            }
            Err(e) => eprintln!("slot: {}: {e}", path.display()),
        }
    }
    eprintln!("slot: no core vendored, running the mock");
    Box::new(MockCore::new())
}

/// Options a core needs before `load`, because libretro cores read them during
/// `retro_load_game` rather than continuously.
///
/// `auto` resolves the serial protocol from the ROM, so two devices running the same game
/// agree on a mode without either being told which. Anything more deliberate belongs to a
/// link session, which knows what the other end picked. mGBA gets nothing: it has no
/// `gpsp_serial` option, and handing it one anyway is a landmine the day it grows one.
pub fn apply_core_options(core: &mut LibretroCore, which: Core) {
    if which == Core::Gpsp {
        core.set_option("gpsp_serial", "auto");
    }
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

        let root = Path::new("/root");
        let mgba = candidates(root, Core::Mgba);
        let gpsp = candidates(root, Core::Gpsp);
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

    /// The property `crates/slot/tests/gpsp.rs`'s integration test relies on: a content root
    /// with its own tmp directory is not near `current_exe()` or `./vendor`, so without this
    /// candidate an integration test has nowhere to plant a fake dylib for `open_core` to
    /// find. Root cause of the gap F3 closed — before this candidate existed, a mutation
    /// that fed `open_core` the wrong `Core` had no candidate list a test could observe
    /// disagree.
    #[test]
    fn candidates_search_the_roots_own_system_directory() {
        let _g = lock();
        std::env::remove_var(CORE_ENV);
        let root = Path::new("/some/content/root");
        assert_eq!(
            candidates(root, Core::Gpsp)[0],
            root.join("System").join(dylib_name(Core::Gpsp)),
        );
    }

    /// The override is a filename, not a `Core`: it must win regardless of which core asked,
    /// which is what makes it a trap when the ini disagrees rather than a second selector.
    #[test]
    fn the_env_override_ignores_which_core_was_asked_for() {
        let _g = lock();
        std::env::set_var(CORE_ENV, "/dev/null/named-core");
        let root = Path::new("/root");
        let mgba = candidates(root, Core::Mgba);
        let gpsp = candidates(root, Core::Gpsp);
        std::env::remove_var(CORE_ENV);
        assert_eq!(mgba, vec![PathBuf::from("/dev/null/named-core")]);
        assert_eq!(
            mgba, gpsp,
            "the override stopped winning for one of the cores"
        );
    }
}

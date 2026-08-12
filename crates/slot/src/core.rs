use std::path::{Path, PathBuf};

use slot_retro::{MgbaCore, MockCore, RetroCore};

use crate::root;

/// Names the dylib outright, for a build that keeps it somewhere the search does not look.
const CORE_ENV: &str = "SLOT_CORE";

/// Most specific first: the environment, then next to the binary, which is how the device
/// ships, then the `vendor` directory `scripts/fetch-core.sh` writes into.
fn candidates() -> Vec<PathBuf> {
    if let Some(named) = std::env::var_os(CORE_ENV) {
        return vec![PathBuf::from(named)];
    }
    // Spelled from the platform's own convention so the same search finds the device's `.so`.
    let name = format!("mgba_libretro.{}", std::env::consts::DLL_EXTENSION);
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

pub fn open_core(root: &Path) -> Box<dyn RetroCore> {
    open_core_for(root, &candidates())
}

/// mGBA if one of these opens, the mock if none of them do. A missing core is not a failure
/// to boot: the shelf, the slot and every gesture are reachable either way.
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
        match MgbaCore::open_with(path, &bios, &saves) {
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

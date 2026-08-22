use std::path::{Path, PathBuf};

use slot_store::{atomic_write, read_slot_state, write_slot_state, Core, StateRing};

/// What a save, a load or a flush needs from the emulator. The core runs on a worker thread
/// and nothing above this trait knows that.
pub trait Snapshot {
    fn state(&self) -> Option<Vec<u8>>;
    fn save_ram(&self) -> Option<Vec<u8>>;
    /// The last frame the core produced, PNG encoded. Encoded on the worker, which is where
    /// the frame already is, so a save does not cost the compositor a hitch.
    fn thumb(&self) -> Option<Vec<u8>>;
    fn load(&self, state: Vec<u8>);
}

/// What lid close, the power press edge and the autosave all write. The slot is untouched:
/// none of them is an eject, and the cart has to still be in it on the next boot.
///
/// Takes `core` rather than resolving it here, for the same reason `read_resume` does below:
/// the caller already has to know which core is live to have anything worth flushing, and
/// asking this function to work it out too would be a second, independent read of
/// `selected_core.ini` for the same cart. `App` is that caller — it resolves `core` once, at
/// insert, stores it, and hands the stored value here on every later write, which is what
/// keeps this from ever disagreeing with the dylib actually running.
pub fn flush(
    root: &Path,
    core: Core,
    stem: &str,
    state: &[u8],
    sav: Option<&[u8]>,
) -> std::io::Result<()> {
    StateRing::new(root, core, stem).write_resume(state)?;
    if let Some(sav) = sav {
        write_sav(root, stem, sav)?;
    }
    Ok(())
}

/// Both durable writes land before the slot is recorded empty, so a cut anywhere in here
/// leaves a cart that still resumes rather than a session with nowhere to go back to.
pub fn eject(
    root: &Path,
    core: Core,
    stem: &str,
    state: &[u8],
    sav: Option<&[u8]>,
) -> std::io::Result<()> {
    flush(root, core, stem, state, sav)?;
    let mut slot = read_slot_state(root);
    slot.cart = None;
    write_slot_state(root, &slot)
}

/// The core hands back the whole save ram whether or not the game touched it, so an
/// unchanged one is a rewrite of up to 128 KB of card for nothing.
pub fn write_sav(root: &Path, stem: &str, sav: &[u8]) -> std::io::Result<bool> {
    let path = sav_path(root, stem);
    if std::fs::read(&path).is_ok_and(|old| old == sav) {
        return Ok(false);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    atomic_write(&path, sav)?;
    Ok(true)
}

/// mGBA standalone writes `.sav`, RetroArch's libretro cores write `.srm`. Both are the
/// same battery bytes, so a card carrying either has a real save on it. Only `.sav` is ever
/// written, which makes it the newer of the two whenever both exist.
pub fn read_sav(root: &Path, stem: &str) -> Option<Vec<u8>> {
    std::fs::read(sav_path(root, stem))
        .or_else(|_| std::fs::read(crate::root::saves_dir(root).join(format!("{stem}.srm"))))
        .ok()
}

/// The counterpart to the resume write in `flush`. Without this the cart is seated on the
/// next boot but the game restarts.
///
/// Takes `core` rather than resolving it here: the caller already has to know which core it
/// is about to open, and asking this function to work it out too would be a second,
/// independent read of `selected_core.ini` for the same cart in the same breath as the
/// first. `session.rs` resolves it once per insert and hands that single value to both this
/// and `open_core`, which is what keeps the resume directory and the dylib from disagreeing
/// at that moment. It says nothing about later: `flush` and eject read the core `App` stored
/// from that same resolution rather than asking again, which is what keeps them agreeing too.
pub fn read_resume(root: &Path, core: Core, stem: &str) -> Option<Vec<u8>> {
    StateRing::new(root, core, stem)
        .read_resume()
        .ok()
        .flatten()
}

fn sav_path(root: &Path, stem: &str) -> PathBuf {
    crate::root::saves_dir(root).join(format!("{stem}.sav"))
}

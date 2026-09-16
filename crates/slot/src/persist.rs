use std::path::{Path, PathBuf};

use slot_store::{atomic_write, read_slot_state, write_slot_state, Core, Platform, StateRing};

/// What a save, a load or a flush needs from the emulator. The core runs on a worker thread
/// and nothing above this trait knows that.
pub trait Snapshot {
    fn state(&self) -> Option<Vec<u8>>;
    fn save_ram(&self) -> Option<Vec<u8>>;
    /// The last frame the core produced, PNG encoded. Encoded on the worker, which is where
    /// the frame already is, so a save does not cost the compositor a hitch.
    fn thumb(&self) -> Option<Vec<u8>>;
    fn load(&self, state: Vec<u8>);

    /// Whether `state()` came from a core that actually accepted the resume it was opened
    /// with. Defaults to `true`, which is right for anything that was never handed a resume
    /// to refuse — a stub in a test, or a cart with none on disk yet. The one implementor
    /// that can ever answer `false` is a live core, and only once `unserialize` has failed on
    /// it: see `EmuSnapshot::resume_trusted`, which is where a real refusal is recorded. A
    /// flush path that writes `state()` back without checking this can turn a core's own
    /// default machine into the player's save.
    fn resume_trusted(&self) -> bool {
        true
    }

    /// The `save_ram()` twin of `resume_trusted`, and independent of it: a core can accept
    /// one and refuse the other.
    fn save_ram_trusted(&self) -> bool {
        true
    }
}

/// What lid close, the power press edge and the autosave all write. The slot is untouched:
/// none of them is an eject, and the cart has to still be in it on the next boot.
///
/// Takes `platform` and `core` rather than resolving them here, for the same reason
/// `read_resume` does below: the caller already has to know which platform and core are live to
/// have anything worth flushing, and asking this function to work them out too would be a
/// second, independent derivation for the same cart. `App` is that caller — it resolves both
/// once, at insert, stores them, and hands the stored values here on every later write, which is
/// what keeps this from ever disagreeing with the cart actually seated.
///
/// `state` is `Option` for the same reason `sav` already was: the caller — `App::flush_resume`
/// and `App::flush_eject` — passes `None` for whichever region the live core refused at open,
/// via `Snapshot::resume_trusted`/`save_ram_trusted`. This function trusts whatever it is
/// handed; it is the one place that decides what gets skipped.
pub fn flush(
    root: &Path,
    platform: Platform,
    core: Core,
    stem: &str,
    state: Option<&[u8]>,
    sav: Option<&[u8]>,
) -> std::io::Result<()> {
    if let Some(state) = state {
        StateRing::new(root, platform, core, stem).write_resume(state)?;
    }
    if let Some(sav) = sav {
        write_sav(root, platform, stem, sav)?;
    }
    Ok(())
}

/// Both durable writes land before the slot is recorded empty, so a cut anywhere in here
/// leaves a cart that still resumes rather than a session with nowhere to go back to.
///
/// Clearing the slot still happens even when `state`/`sav` withheld a refused region: the
/// files that refusal left alone are exactly as durable as they were before this cart was
/// seated, so there is nothing an eject would be waiting on.
pub fn eject(
    root: &Path,
    platform: Platform,
    core: Core,
    stem: &str,
    state: Option<&[u8]>,
    sav: Option<&[u8]>,
) -> std::io::Result<()> {
    flush(root, platform, core, stem, state, sav)?;
    let mut slot = read_slot_state(root);
    slot.cart = None;
    write_slot_state(root, &slot)
}

/// The core hands back the whole save ram whether or not the game touched it, so an
/// unchanged one is a rewrite of up to 128 KB of card for nothing.
///
/// Also refuses to shrink an existing save. `load_save_ram` can accept bytes it should have
/// refused: a libretro core that exposes a save-ram region copies `len.min(data.len())` bytes
/// into it and returns `Ok` regardless, so a cart whose two cores disagree on
/// `RETRO_MEMORY_SAVE_RAM`'s size truncates silently rather than failing loudly — the class of
/// bug `resume_trusted`/`save_ram_trusted` cannot see, because as far as the core is concerned
/// it accepted what it was given. This is the backstop for that: whatever produced a shorter
/// save than what is already on the card, refuse it and say so, rather than trust that a
/// smaller battery save is ever a real one.
///
/// The comparison goes through `read_sav`, not a stat of `sav_path` alone: `read_sav` also
/// accepts `Saves/<platform>/<stem>.srm` (RetroArch's name for the same battery bytes, see its
/// own doc comment below), and a card carrying only an `.srm` still has a real save on it.
/// Stat-ing `.sav` directly would find nothing there, wave a smaller write through unguarded,
/// and that new `.sav` would then shadow the larger `.srm` on every read after — this is the
/// exact loss shape the guard above exists to stop, just reached from the one path it could
/// not see.
pub fn write_sav(root: &Path, platform: Platform, stem: &str, sav: &[u8]) -> std::io::Result<bool> {
    let path = sav_path(root, platform, stem);
    if let Some(old) = read_sav(root, platform, stem) {
        if old == sav {
            return Ok(false);
        }
        if sav.len() < old.len() {
            eprintln!(
                "slot: save ram: refusing to shrink {} from {} to {} bytes",
                path.display(),
                old.len(),
                sav.len()
            );
            return Ok(false);
        }
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
pub fn read_sav(root: &Path, platform: Platform, stem: &str) -> Option<Vec<u8>> {
    std::fs::read(sav_path(root, platform, stem))
        .or_else(|_| {
            std::fs::read(
                crate::root::saves_dir(root)
                    .join(platform.dir_name())
                    .join(format!("{stem}.srm")),
            )
        })
        .ok()
}

/// The counterpart to the resume write in `flush`. Without this the cart is seated on the
/// next boot but the game restarts.
///
/// Takes `platform` and `core` rather than resolving them here: the caller already has to know
/// which platform and core it is about to open, and asking this function to work them out too
/// would be a second, independent derivation for the same cart in the same breath as the first.
/// `session.rs` resolves them once per insert and hands those single values to both this and
/// `open_core`, which is what keeps the resume directory and the dylib from disagreeing at that
/// moment. It says nothing about later: `flush` and eject read the platform and core `App`
/// stored from that same resolution rather than asking again, which is what keeps them agreeing
/// too.
pub fn read_resume(root: &Path, platform: Platform, core: Core, stem: &str) -> Option<Vec<u8>> {
    StateRing::new(root, platform, core, stem)
        .read_resume()
        .ok()
        .flatten()
}

fn sav_path(root: &Path, platform: Platform, stem: &str) -> PathBuf {
    crate::root::saves_dir(root)
        .join(platform.dir_name())
        .join(format!("{stem}.sav"))
}

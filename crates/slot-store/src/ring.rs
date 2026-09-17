use std::path::{Path, PathBuf};

use crate::atomic::{atomic_write, sync_dir};
use crate::core::Core;
use crate::platform::Platform;

pub const RING_MAX: usize = 10;

const STATE_EXT: &str = "state";
const THUMB_EXT: &str = "png";
const RESUME: &str = "resume";
/// What a resume the core would not read is renamed to, before the stamp: see
/// `StateRing::retire_resume`.
const REFUSED: &str = "resume-refused";

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StateEntry {
    /// `%Y-%m-%d_%H-%M-%S`, and also the filename. There is no manifest to corrupt.
    pub stamp: String,
    pub state: PathBuf,
    pub thumb: PathBuf,
}

/// The ten deliberate saves for one cart, plus the invisible `resume.state` alongside
/// them. Only `SELECT+R1` pushes; eject, lid, power and autosave all write resume, which
/// is why resume is not an entry.
pub struct StateRing {
    dir: PathBuf,
}

impl StateRing {
    /// States are core private: a serialized machine from one emulator cannot be loaded by
    /// another, so offering them together would only produce a confusing failure. Battery
    /// saves under `Saves/` are raw cartridge bytes and stay shared.
    ///
    /// Platform first, then core: `States/<platform>/<core>/<stem>/`, the shape
    /// `migrate_platforms`' sweep already produces. A `.gb` and a `.gba` cart can share a stem —
    /// two different games, two different carts — so the platform has to separate them before
    /// the core does, or one cart's states would be offered to the other's.
    pub fn new(root: &Path, platform: Platform, core: Core, stem: &str) -> Self {
        StateRing {
            dir: root
                .join("States")
                .join(platform.dir_name())
                .join(core.as_str())
                .join(stem),
        }
    }

    pub fn push(&self, state: &[u8], thumb_png: &[u8], stamp: &str) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        // Thumb first. An entry is listed by its state file, so a cut between the two
        // leaves an unlisted orphan rather than a polaroid with no picture.
        atomic_write(&self.path(stamp, THUMB_EXT), thumb_png)?;
        atomic_write(&self.path(stamp, STATE_EXT), state)?;
        self.evict()
    }

    /// Newest first. A cart that has never been saved lists empty rather than failing:
    /// on the device that directory only exists once something has written to it.
    pub fn list(&self) -> std::io::Result<Vec<StateEntry>> {
        let dir = match std::fs::read_dir(&self.dir) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };

        let mut entries = Vec::new();
        for entry in dir {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some(STATE_EXT) {
                continue;
            }
            let Some(stamp) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if !is_stamp(stamp) {
                continue;
            }
            entries.push(StateEntry {
                thumb: self.path(stamp, THUMB_EXT),
                state: self.path(stamp, STATE_EXT),
                stamp: stamp.to_string(),
            });
        }
        // The stamp is fixed width and most significant first, so lexicographic order is
        // chronological order.
        entries.sort_by(|a, b| b.stamp.cmp(&a.stamp));
        Ok(entries)
    }

    /// Both halves of one entry. A thumbnail that never landed reads as empty rather than as
    /// a failure: the state is the part worth keeping.
    pub fn read(&self, stamp: &str) -> std::io::Result<(Vec<u8>, Vec<u8>)> {
        let state = std::fs::read(self.stamped(stamp, STATE_EXT)?)?;
        let thumb = std::fs::read(self.path(stamp, THUMB_EXT)).unwrap_or_default();
        Ok((state, thumb))
    }

    pub fn remove(&self, stamp: &str) -> std::io::Result<()> {
        std::fs::remove_file(self.stamped(stamp, STATE_EXT)?)?;
        let _ = std::fs::remove_file(self.path(stamp, THUMB_EXT));
        Ok(())
    }

    pub fn write_resume(&self, state: &[u8]) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        atomic_write(&self.path(RESUME, STATE_EXT), state)
    }

    pub fn read_resume(&self) -> std::io::Result<Option<Vec<u8>>> {
        match std::fs::read(self.path(RESUME, STATE_EXT)) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Moves `resume.state` out of the way, because the core that was handed it would not read
    /// it. Answers where it went, or `None` when there was no resume to move — which is what a
    /// cart that has already been retired looks like on every call after the first.
    ///
    /// Renamed rather than deleted. A state one core refuses is still a real session to the
    /// core that wrote it: put that core back and it is worth having again. So this destroys
    /// nothing, which is the same choice `migrate_states` makes when it finds a destination
    /// name already taken, and the same one `write_sav` makes when a save would shrink. What
    /// the move buys is that the next open finds no resume at all — without it the same bytes
    /// are handed to the same core on every boot and refused identically every time, with
    /// nothing the player can do about it but delete the file from a card reader.
    ///
    /// The new name keeps the `.state` extension so it still reads as a save state to whoever
    /// is looking at the card, and `list` still passes over it: its file stem is not a stamp,
    /// which is the same test that already keeps `resume.state` itself out of the ring. So it
    /// can never be offered as an entry, and `evict` — which only ever deletes what `list`
    /// returns — can never delete it either.
    ///
    /// `stamp` is the caller's wall clock, taken as an argument for the same reason `push`
    /// takes one rather than reading a clock of its own.
    pub fn retire_resume(&self, stamp: &str) -> std::io::Result<Option<PathBuf>> {
        let from = self.path(RESUME, STATE_EXT);
        if !from.exists() {
            return Ok(None);
        }
        let to = self.free_refused(stamp);
        std::fs::rename(&from, &to)?;
        // A rename is already atomic, so unlike `atomic_write` there is nothing to write first.
        // The directory entry still has to reach the card, or a power cut here leaves the state
        // back under its old name and the next boot hands it to the core again.
        sync_dir(&to);
        Ok(Some(to))
    }

    /// A `resume-refused-<stamp>.state` nothing is using yet.
    ///
    /// Two retirements of one cart inside the same wall-clock second is not something a player
    /// can produce — the second one needs a whole session in between to write a new resume for
    /// a later core to refuse — but the counter costs three lines, and the alternative is
    /// overwriting a state that is still somebody's, which is the one thing moving rather than
    /// deleting exists to avoid.
    fn free_refused(&self, stamp: &str) -> PathBuf {
        let base = format!("{REFUSED}-{stamp}");
        let first = self.path(&base, STATE_EXT);
        if !first.exists() {
            return first;
        }
        (2u32..)
            .map(|n| self.path(&format!("{base}-{n}"), STATE_EXT))
            .find(|p| !p.exists())
            .unwrap_or(first)
    }

    fn evict(&self) -> std::io::Result<()> {
        for old in self.list()?.into_iter().skip(RING_MAX) {
            std::fs::remove_file(&old.state)?;
            // A push that was cut short can leave a state with no thumb, and refusing to
            // evict it would wedge the ring at eleven.
            let _ = std::fs::remove_file(&old.thumb);
        }
        Ok(())
    }

    fn path(&self, stem: &str, ext: &str) -> PathBuf {
        self.dir.join(format!("{stem}.{ext}"))
    }

    /// The caller names the file, so `resume` and anything else outside the ring is refused
    /// here rather than being deleted or handed back.
    fn stamped(&self, stamp: &str, ext: &str) -> std::io::Result<PathBuf> {
        if !is_stamp(stamp) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("not a state stamp: {stamp}"),
            ));
        }
        Ok(self.path(stamp, ext))
    }
}

/// `resume.state` fails this, and so does anything the user dropped in the directory by
/// hand. Eviction deletes what `list` returns, so being strict here is what stops the
/// ring removing a file it did not write.
fn is_stamp(s: &str) -> bool {
    const SHAPE: &[u8] = b"0000-00-00_00-00-00";
    s.len() == SHAPE.len()
        && s.bytes().zip(SHAPE).all(|(c, shape)| match shape {
            b'0' => c.is_ascii_digit(),
            _ => c == *shape,
        })
}

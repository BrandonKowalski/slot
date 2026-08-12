use std::path::{Path, PathBuf};

use crate::atomic::atomic_write;

pub const RING_MAX: usize = 10;

const STATE_EXT: &str = "state";
const THUMB_EXT: &str = "png";
const RESUME: &str = "resume";

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
    pub fn new(root: &Path, stem: &str) -> Self {
        StateRing {
            dir: root.join("States").join(stem),
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

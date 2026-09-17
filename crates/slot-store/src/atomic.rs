use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

/// Temp, fsync, rename. A reader never observes a half written file, and a power cut
/// during the write leaves the previous contents rather than a truncated one.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = temp_path(path);
    match write_then_rename(&tmp, path, bytes) {
        Ok(()) => {}
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
    }
    sync_dir(path);
    Ok(())
}

/// Flush the directory entry for `path`, having just created, renamed or removed it.
///
/// The name lives in the directory rather than in the file, so without this the bytes can
/// survive a power cut while the name pointing at them does not. Best effort: some filesystems
/// refuse a directory fsync, and failing an otherwise complete write over that would be worse.
///
/// Shared with `StateRing::retire_resume`, which renames a file it did not write: a rename is
/// already atomic, so that path needs nothing from `atomic_write` but this last step.
pub(crate) fn sync_dir(path: &Path) {
    if let Some(dir) = path.parent() {
        if let Ok(d) = File::open(dir) {
            let _ = d.sync_all();
        }
    }
}

fn write_then_rename(tmp: &Path, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut f = File::create(tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(tmp, path)
}

/// The longest one path component may be, on every filesystem a card is ever formatted as:
/// FAT32's long names, exFAT, ext4 and APFS all stop at 255.
///
/// Here because the temp name is longer than the name it stands in for — a leading dot, the
/// process id, a sequence number and `.tmp`, about a dozen characters — so a filename the card
/// accepts can have a temp name the card refuses. A cart named right up to the limit could be
/// listed, inserted and played while every `atomic_write` for its battery save failed with
/// `File name too long`, session after session, with nothing on screen to say so. The state ring
/// was untouched by it, because the file there is named after a timestamp and only the directory
/// carries the stem — so such a cart resumed perfectly and never once saved.
const NAME_MAX: usize = 255;

fn temp_path(path: &Path) -> PathBuf {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    // The process id and the sequence are what make this unique; the name is only there so a
    // temp left behind by a power cut can be recognised. So the name is what gets cut when the
    // two together will not fit, and it is cut on a character boundary — `to_string_lossy` can
    // hand back multi-byte characters, and slicing one in half would panic.
    let tail = format!(".{}.{seq}.tmp", std::process::id());
    let mut room = NAME_MAX.saturating_sub(tail.len() + 1).min(name.len());
    while room > 0 && !name.is_char_boundary(room) {
        room -= 1;
    }
    let tmp = format!(".{}{tail}", &name[..room]);
    match path.parent() {
        Some(dir) => dir.join(tmp),
        None => PathBuf::from(tmp),
    }
}

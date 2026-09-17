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

fn temp_path(path: &Path) -> PathBuf {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = format!(".{name}.{}.{seq}.tmp", std::process::id());
    match path.parent() {
        Some(dir) => dir.join(tmp),
        None => PathBuf::from(tmp),
    }
}

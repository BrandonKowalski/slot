use std::fmt;
use std::path::{Path, PathBuf};

use crate::gba::{header_code, header_title};

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Cart {
    /// Filename stem, which is the key for labels, saves and states. Not a content hash.
    pub stem: String,
    pub rom: PathBuf,
    pub label: Option<PathBuf>,
    pub title: String,
    /// The four character header game code, empty when the rom has none.
    pub code: String,
}

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "io: {e}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e)
    }
}

/// An unmounted card or a card with no library is an empty shelf, not a boot failure.
pub fn scan(root: &Path) -> Result<Vec<Cart>, StoreError> {
    let dir = match std::fs::read_dir(root.join("Games")) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };

    let mut carts = Vec::new();
    for entry in dir {
        let rom = entry?.path();
        if !is_gba(&rom) {
            continue;
        }
        let Some(stem) = rom.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let label = root.join("Labels").join(format!("{stem}.png"));
        carts.push(Cart {
            stem: stem.to_string(),
            title: header_title(&rom).unwrap_or_default(),
            code: header_code(&rom).unwrap_or_default(),
            label: label.is_file().then_some(label),
            rom,
        });
    }
    carts.sort_by(|a, b| a.stem.cmp(&b.stem));
    Ok(carts)
}

fn is_gba(p: &Path) -> bool {
    !is_hidden(p)
        && p.is_file()
        && p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("gba"))
}

/// A leading dot is card metadata rather than content, and every folder on the card is read
/// through this. macOS writes `._<name>` beside each file it copies onto a FAT volume, which
/// carries the extension of the file it shadows, so the extension alone cannot tell them
/// apart. It also sorts first, which is why the sidecar rather than the file is what a picker
/// walking the folder in order tends to land on.
pub fn is_hidden(p: &Path) -> bool {
    p.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with('.'))
}

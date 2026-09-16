use std::fmt;
use std::path::{Path, PathBuf};

use crate::gba::{header_code, header_title};
use crate::platform::Platform;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Cart {
    /// Which console this cart is for, and therefore which folder under `Games/`, `Saves/`,
    /// `States/` and `Labels/` its files live in. Decided by the folder `scan` found the rom
    /// in, never by reading the rom itself.
    pub platform: Platform,
    /// Filename stem, which is the key for labels, saves and states. Not a content hash.
    pub stem: String,
    pub rom: PathBuf,
    pub label: Option<PathBuf>,
    pub title: String,
    /// The four character header game code, empty when the rom has none. A Game Boy cart has
    /// no equivalent field, so this is always empty for `Platform::Gb` and `Platform::Gbc`.
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

/// An unmounted card or a card with no library is an empty shelf, not a boot failure — and so
/// is a platform folder that does not exist, which is the normal state of a card with no
/// Colour carts.
pub fn scan(root: &Path) -> Result<Vec<Cart>, StoreError> {
    let mut carts = Vec::new();
    for platform in Platform::ALL {
        let dir = root.join("Games").join(platform.dir_name());
        let entries = match std::fs::read_dir(&dir) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };
        for entry in entries {
            let rom = entry?.path();
            // The folder decides the platform; the extension decides whether this is a cart at
            // all. A `.gba` under `GB/` is neither, and is passed over in silence.
            if is_hidden(&rom) || !rom.is_file() || !platform.accepts(&rom) {
                continue;
            }
            let Some(stem) = rom.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let label = root
                .join("Labels")
                .join(platform.dir_name())
                .join(format!("{stem}.png"));
            let (title, code) = match platform {
                Platform::Gba => (
                    header_title(&rom).unwrap_or_default(),
                    header_code(&rom).unwrap_or_default(),
                ),
                // A Game Boy cart has no GBA-style four-character game code, and the fields
                // `gba.rs` reads sit below the Game Boy header entirely — 0xA0 and 0xAC are in
                // the cartridge's RST vectors, so they would read arbitrary opcode bytes.
                _ => (crate::gb::title(&rom).unwrap_or_default(), String::new()),
            };
            carts.push(Cart {
                platform,
                stem: stem.to_string(),
                title,
                code,
                label: label.is_file().then_some(label),
                rom,
            });
        }
    }
    carts.sort_by(|a, b| (a.platform as u8, &a.stem).cmp(&(b.platform as u8, &b.stem)));
    Ok(carts)
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

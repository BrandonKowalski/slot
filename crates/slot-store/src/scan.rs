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

/// An unmounted card, or a card with no `Games/GBA/`, is an empty shelf, not a boot failure.
///
/// A folder that exists and cannot be read is not a boot failure either. The only caller is
/// `App::boot`, which does `scan(root).unwrap_or_default()`, so an `Err` out of here is not an
/// error message anywhere, it is an empty shelf. A single directory entry that will not stat
/// costs that one cart and nothing else.
pub fn scan(root: &Path) -> Result<Vec<Cart>, StoreError> {
    let mut carts = Vec::new();
    let dir = root.join("Games").join(crate::CART_DIR);
    let entries = match std::fs::read_dir(&dir) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(carts),
        Err(e) => {
            eprintln!("slot: scan: {}: {e}", dir.display());
            return Ok(carts);
        }
    };
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let rom = entry.path();
        if is_hidden(&rom) || !rom.is_file() || !is_gba(&rom) {
            continue;
        }
        let Some(stem) = rom.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let label = root
            .join("Labels")
            .join(crate::CART_DIR)
            .join(format!("{stem}.png"));
        carts.push(Cart {
            stem: stem.to_string(),
            title: header_title(&rom).unwrap_or_default(),
            code: header_code(&rom).unwrap_or_default(),
            label: label.is_file().then_some(label),
            rom,
        });
    }
    carts.sort_by_key(|c| sort_key(&c.stem));
    Ok(carts)
}

fn is_gba(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gba"))
}

/// Where a title files on the shelf: digits first, then A to Z, and case ignored.
///
/// Plain byte order put `apple` after `Zebra`, because every lowercase letter sorts above every
/// uppercase one, so a card's row depended on how its files happened to be capitalised.
///
/// The group runs ahead of the text rather than being folded into it, so that one digit-led title
/// cannot land between two letters however it is spelled, and anything led by neither, a bracket
/// or a quote, files after both rather than silently first.
pub fn sort_key(stem: &str) -> (u8, String) {
    (group_of(stem), stem.to_uppercase())
}

fn group_of(stem: &str) -> u8 {
    match stem.chars().find(|c| !c.is_whitespace()) {
        Some(c) if c.is_ascii_digit() => 0,
        Some(c) if c.is_alphabetic() => 1,
        _ => 2,
    }
}

/// The letter a title is filed under, for skipping a row a letter at a time. Every digit-led
/// title shares one bucket, and so does everything led by neither a digit nor a letter: a row of
/// thirty carts has few enough of either that giving each its own stop would be a stop that moves
/// by one, which is what the shoulder buttons are already for.
pub fn initial(stem: &str) -> char {
    match group_of(stem) {
        1 => stem
            .chars()
            .find(|c| !c.is_whitespace())
            .and_then(|c| c.to_uppercase().next())
            .unwrap_or('#'),
        _ => '#',
    }
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

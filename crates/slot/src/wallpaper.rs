use std::path::{Path, PathBuf};

/// One picture behind the shelf, from `Wallpapers`. PNG only, as the labels are: the card
/// holds user supplied art and one decoder is one thing that can go wrong.
///
/// `seed` is the wall clock at boot. There is no settings screen to choose a picture on and
/// no reason to prefer one, so the card gets a different one each time it is turned on.
pub fn pick(root: &Path, seed: u64) -> Option<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(root.join("Wallpapers"))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| !slot_store::is_hidden(p))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")))
        .collect();
    if files.is_empty() {
        return None;
    }
    // Sorted first, so the same seed on the same card is the same picture. A directory hands
    // its entries back in whatever order it likes.
    files.sort();
    let i = (seed % files.len() as u64) as usize;
    Some(files.swap_remove(i))
}

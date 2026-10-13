use std::path::{Path, PathBuf};

use slot_store::Platform;

pub const BEZELS: &str = "System/bezels";
pub const GAME_BEZELS: &str = "bezels";
pub const GAME_BEZELS_640: &str = "bezels 640";

const BUNDLED: &[(&str, &str, &str, &[u8])] = &[
    (
        GAME_BEZELS,
        "GB",
        "bezel_gb.png",
        include_bytes!("../../../bezels/GB/bezel_gb.png"),
    ),
    (
        GAME_BEZELS,
        "GBC",
        "bezel_gbc.png",
        include_bytes!("../../../bezels/GBC/bezel_gbc.png"),
    ),
    (
        GAME_BEZELS_640,
        "GB",
        "bezel_gb_640.png",
        include_bytes!("../../../bezels 640/GB/bezel_gb_640.png"),
    ),
    (
        GAME_BEZELS_640,
        "GBC",
        "bezel_gbc_640.png",
        include_bytes!("../../../bezels 640/GBC/bezel_gbc_640.png"),
    ),
];

/// The folder a panel reads. A 640-wide display, such as the RG35XXSP, uses
/// the frames drawn for that panel; every other width uses the canvas-sized set.
pub fn game_dir(width: u32) -> &'static str {
    if width == 640 {
        GAME_BEZELS_640
    } else {
        GAME_BEZELS
    }
}

/// Copy the shipped frames into their folders when those names are not already
/// on the card, so a picture the user replaced stays put.
pub fn install(root: &Path) {
    for (folder, dir, name, bytes) in BUNDLED {
        let path = root.join(folder).join(dir).join(name);
        if path.exists() {
            continue;
        }
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&path, bytes) {
            eprintln!("slot: bezel: {e}");
        }
    }
}

pub fn list(root: &Path, platform: Platform, width: u32) -> Vec<String> {
    if platform == Platform::Gba {
        return Vec::new();
    }
    let mut files = pngs(&root.join(game_dir(width)).join(platform.dir_name()));
    files.sort();
    files
}

pub fn path(root: &Path, platform: Platform, name: &str, width: u32) -> PathBuf {
    root.join(game_dir(width))
        .join(platform.dir_name())
        .join(name)
}

fn pngs(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| !slot_store::is_hidden(p))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")))
        .filter_map(|p| Some(p.file_name()?.to_str()?.to_owned()))
        .collect()
}

fn dims(path: &Path) -> Option<(u32, u32)> {
    if !path.extension()?.eq_ignore_ascii_case("png") {
        return None;
    }
    let (w, h) = path.file_stem()?.to_str()?.split_once('x')?;
    let (w, h) = (w.parse::<u32>().ok()?, h.parse::<u32>().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

pub fn pick(root: &Path, panel: (u32, u32)) -> Option<PathBuf> {
    let mut fits: Vec<((u32, u32), PathBuf)> = std::fs::read_dir(root.join(BEZELS))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| !slot_store::is_hidden(p))
        .filter_map(|p| Some((dims(&p)?, p)))
        .filter(|((w, h), _)| {
            u64::from(*w) * u64::from(panel.1) == u64::from(*h) * u64::from(panel.0)
        })
        .collect();
    fits.sort_by_key(|((w, _), p)| (w.abs_diff(panel.0), p.clone()));
    fits.into_iter().next().map(|(_, p)| p)
}

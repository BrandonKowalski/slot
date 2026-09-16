use std::path::Path;

/// Which console a cart is for, and therefore which folder every one of its files lives in.
///
/// Three variants, one per card directory. There is deliberately no variant meaning "loose at
/// the root": nothing stays loose, and a file's platform is a property of *where it is*, which
/// is what lets the scan answer it without opening the file at all.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Platform {
    #[default]
    Gba,
    Gb,
    Gbc,
}

/// Which shelf a cart appears on. Two, not three: a Game Boy and a Game Boy Color cartridge
/// are dimensionally identical — 65.5 × 57 × 7.5 mm both — so one silhouette serves both and a
/// third shelf would redraw the same art under a different name. Storage and display are
/// allowed to differ, and here they do.
///
/// Named `ShelfKind` rather than `Shelf` because `slot_ui::Shelf` is the carousel widget, and
/// `slot::app` holds one of those per grouping while importing from both crates. This is the
/// grouping; that is the thing being grouped.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShelfKind {
    Gba,
    GameBoy,
}

impl Platform {
    /// Every variant, once, in the order shelves are switched through.
    pub const ALL: [Platform; 3] = [Platform::Gba, Platform::Gb, Platform::Gbc];

    /// The card directory this platform's files live under, in `Games/`, `Saves/`, `States/`
    /// and `Labels/` alike. Every platform has one — see the type's own comment.
    pub fn dir_name(self) -> &'static str {
        match self {
            Platform::Gba => "GBA",
            Platform::Gb => "GB",
            Platform::Gbc => "GBC",
        }
    }

    pub fn shelf(self) -> ShelfKind {
        match self {
            Platform::Gba => ShelfKind::Gba,
            Platform::Gb | Platform::Gbc => ShelfKind::GameBoy,
        }
    }

    /// The ROM extensions this folder holds. A `.gba` sitting in `GB/` is not a Game Boy cart
    /// and is not scanned as one: the folder says where a cart's files go, but it cannot make
    /// a GBA ROM into a Game Boy game.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Platform::Gba => &["gba"],
            Platform::Gb | Platform::Gbc => &["gb", "gbc"],
        }
    }

    pub fn accepts(self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| {
                self.extensions()
                    .iter()
                    .any(|k| ext.eq_ignore_ascii_case(k))
            })
    }
}

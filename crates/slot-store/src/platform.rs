use std::path::Path;

/// Which console a cart is for, and therefore which folder every one of its files lives in —
/// and, since there is one shelf per platform, which shelf of the carousel it stands on.
///
/// Three variants, one per card directory. There is deliberately no variant meaning "loose at
/// the root": nothing stays loose, and a file's platform is a property of *where it is*, which
/// is what lets the scan answer it without opening the file at all.
///
/// There is no separate grouping type. A Game Boy and a Game Boy Color cartridge are the same
/// object dimensionally, and they were grouped onto one shelf for exactly that reason; the
/// shelves are one per platform now, which leaves nothing for a second type to say. The name
/// collision with `slot_ui::Shelf` — the carousel widget — is unchanged: `slot::app` holds one
/// of those per `Platform`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Platform {
    #[default]
    Gba,
    Gb,
    Gbc,
}

impl Platform {
    /// Every variant, once, in the order the shelves are switched through.
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

    /// The ROM extensions this folder holds. A `.gba` sitting in `GB/` is not a Game Boy cart
    /// and is not scanned as one: the folder says where a cart's files go, but it cannot make
    /// a GBA ROM into a Game Boy game.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Platform::Gba => &["gba"],
            Platform::Gb | Platform::Gbc => &["gb", "gbc"],
        }
    }

    /// The picture this console draws, in pixels. The GBA's is the whole frame buffer the
    /// device is built around; a Game Boy's is smaller and `video_refresh` centres it inside
    /// that same buffer, so this is also what says how much of the buffer is the picture and
    /// how much is the margin around it.
    ///
    /// A Game Boy Color draws the same 160x144 as a Game Boy — the colour is in the pixels,
    /// not in how many of them there are.
    ///
    /// Spelled out here rather than taken from `slot_retro`: this crate is the card's own view
    /// of what a platform is, and it does not know a libretro core exists.
    pub fn picture(self) -> (u32, u32) {
        match self {
            Platform::Gba => (240, 160),
            Platform::Gb | Platform::Gbc => (160, 144),
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

use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Platform {
    #[default]
    Gba,
    Gb,
    Gbc,
    Ps1,
    Snes,
    Psp,
}

impl Platform {
    pub const ALL: [Platform; 6] = [
        Platform::Gba,
        Platform::Gb,
        Platform::Gbc,
        Platform::Ps1,
        Platform::Snes,
        Platform::Psp,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Platform::Gba => "Game Boy Advance",
            Platform::Gb => "Game Boy",
            Platform::Gbc => "Game Boy Color",
            Platform::Ps1 => "PlayStation",
            Platform::Snes => "Super Nintendo",
            Platform::Psp => "PlayStation Portable",
        }
    }

    pub fn dir_name(self) -> &'static str {
        match self {
            Platform::Gba => "GBA",
            Platform::Gb => "GB",
            Platform::Gbc => "GBC",
            Platform::Ps1 => "PS1",
            Platform::Snes => "SNES",
            Platform::Psp => "PSP",
        }
    }

    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Platform::Gba => &["gba"],
            Platform::Gb | Platform::Gbc => &["gb", "gbc"],
            Platform::Ps1 => &["cue", "chd", "m3u", "pbp", "iso"],
            Platform::Snes => &["sfc", "smc", "swc", "fig"],
            Platform::Psp => &["iso", "cso", "pbp"],
        }
    }

    pub fn picture(self) -> (u32, u32) {
        match self {
            Platform::Gba => (240, 160),
            Platform::Gb | Platform::Gbc => (160, 144),
            Platform::Ps1 | Platform::Snes | Platform::Psp => (240, 160),
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

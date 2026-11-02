//! The dumps a libretro No-Intro dat lists.
// Each example reads only the fields it needs.
#![allow(dead_code)]

pub struct Dump {
    /// The dump's name, without extension: "Tetris (World) (Rev 1)".
    pub name: String,
    /// The ROM file's name: "Tetris (World) (Rev 1).gb".
    pub file: String,
    pub size: u64,
    pub crc: u32,
}

pub fn dumps(path: &std::path::Path) -> Vec<Dump> {
    let text = std::fs::read_to_string(path).expect("dat");
    let quoted = |s: &str| {
        s.split_once('"')
            .and_then(|(_, r)| r.split_once('"'))
            .map(|(q, _)| q.to_owned())
    };
    let mut name = String::new();
    let mut out = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with("name \"") {
            name = quoted(line).unwrap_or_default();
        } else if let Some(rom) = line.strip_prefix("rom (") {
            let words: Vec<&str> = rom.split_whitespace().collect();
            let after = |key: &str| {
                words
                    .iter()
                    .position(|w| *w == key)
                    .and_then(|i| words.get(i + 1))
            };
            let crc = after("crc").and_then(|c| u32::from_str_radix(c, 16).ok());
            let size = after("size").and_then(|s| s.parse().ok());
            if let (Some(crc), Some(size), Some(file)) = (crc, size, quoted(rom)) {
                out.push(Dump {
                    name: name.clone(),
                    file,
                    size,
                    crc,
                });
            }
        }
    }
    out
}

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// GBA cartridge header: a 12 byte ASCII game title, NUL padded on the right.
const TITLE_OFF: u64 = 0xa0;
const TITLE_LEN: usize = 12;

/// The four character game code, immediately after the title. The fourth character is the
/// region, so callers keying on the game itself want the first three.
const CODE_OFF: u64 = 0xac;
const CODE_LEN: usize = 4;

/// Seeks rather than reading the file, because a commercial rom is up to 32 MB and the
/// shelf reads this for every cart at boot.
pub fn header_title(rom: &Path) -> Option<String> {
    field(rom, TITLE_OFF, &mut [0u8; TITLE_LEN])
}

pub fn header_code(rom: &Path) -> Option<String> {
    field(rom, CODE_OFF, &mut [0u8; CODE_LEN])
}

fn field(rom: &Path, off: u64, buf: &mut [u8]) -> Option<String> {
    let mut f = File::open(rom).ok()?;
    f.seek(SeekFrom::Start(off)).ok()?;
    f.read_exact(buf).ok()?;
    text_from_bytes(buf)
}

fn text_from_bytes(buf: &[u8]) -> Option<String> {
    let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
    let text = std::str::from_utf8(&buf[..end]).ok()?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::text_from_bytes;

    #[test]
    fn padding_is_trimmed_but_interior_spaces_are_kept() {
        let full = text_from_bytes(b"POKEMON EMER");
        assert_eq!(full.as_deref(), Some("POKEMON EMER"));
        assert_eq!(
            text_from_bytes(b"ADVANCEWARS\0").as_deref(),
            Some("ADVANCEWARS")
        );
        assert_eq!(text_from_bytes(b"KIRBY      \0").as_deref(), Some("KIRBY"));
        assert_eq!(text_from_bytes(&[0u8; 12]), None);
    }
}

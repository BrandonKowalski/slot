use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const TITLE_OFF: u64 = 0xa0;
const TITLE_LEN: usize = 12;

const CODE_OFF: u64 = 0xac;
const CODE_LEN: usize = 4;

pub fn header_title(rom: &Path) -> Option<String> {
    field(rom, TITLE_OFF, &mut [0u8; TITLE_LEN])
}

pub fn header_code(rom: &Path) -> Option<String> {
    field(rom, CODE_OFF, &mut [0u8; CODE_LEN])
}

pub fn header_title_code(rom: &Path) -> Option<(String, String)> {
    let mut f = File::open(rom).ok()?;
    f.seek(SeekFrom::Start(TITLE_OFF)).ok()?;
    let mut head = [0u8; (CODE_OFF - TITLE_OFF) as usize + CODE_LEN];
    f.read_exact(&mut head).ok()?;
    let (title, code) = head.split_at(TITLE_LEN);
    Some((
        text_from_bytes(title).unwrap_or_default(),
        text_from_bytes(code).unwrap_or_default(),
    ))
}

pub fn header_clean(rom: &Path) -> bool {
    let Ok(mut f) = File::open(rom) else {
        return false;
    };
    let Ok(meta) = f.metadata() else {
        return false;
    };
    let mut head = [0u8; 0xB3];
    f.read_exact(&mut head).is_ok()
        && head[3] == 0xEA
        && head[0xB2] == 0x96
        && meta.len() <= 16 * 1024 * 1024
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

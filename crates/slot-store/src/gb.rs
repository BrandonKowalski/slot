use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Game Boy cartridge header. The title runs from 0x134 and was **11** bytes on later carts,
/// which shortened it to make room for a four-character manufacturer code at 0x13F and the CGB
/// flag at 0x143. Reading sixteen from 0x134, as the old field allowed, swallows both.
const TITLE_OFF: u64 = 0x134;
const TITLE_LEN: usize = 11;

/// 0x00 is a plain Game Boy cart, 0x80 is Colour-enhanced but still runs on original hardware,
/// and 0xC0 is Colour-only. Three values, not two.
const CGB_OFF: u64 = 0x143;

/// The header title, or `None` when the field is empty — which is not a malformed ROM. Ours is:
/// `Tetris Chromatic.gbc` fills none of it. The shelf names a cart from its filename anyway.
pub fn title(rom: &Path) -> Option<String> {
    let mut buf = [0u8; TITLE_LEN];
    read_at(rom, TITLE_OFF, &mut buf)?;
    let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
    let text = std::str::from_utf8(&buf[..end]).ok()?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

pub fn cgb_flag(rom: &Path) -> Option<u8> {
    let mut buf = [0u8; 1];
    read_at(rom, CGB_OFF, &mut buf)?;
    Some(buf[0])
}

/// Whether this cart wears a clear shell. Both Colour values count: a Colour-enhanced cart
/// shipped in the same plastic as a Colour-only one, so the flag's three values collapse onto
/// two finishes here and stay three in `cgb_flag` for anything that needs the distinction.
pub fn is_colour(rom: &Path) -> bool {
    matches!(cgb_flag(rom), Some(0x80) | Some(0xc0))
}

fn read_at(rom: &Path, off: u64, buf: &mut [u8]) -> Option<()> {
    let mut f = File::open(rom).ok()?;
    f.seek(SeekFrom::Start(off)).ok()?;
    f.read_exact(buf).ok()
}

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Game Boy cartridge header. The title runs from 0x134 and was **11** bytes on later carts,
/// which shortened it to make room for a four-character manufacturer code at 0x13F and the CGB
/// flag at 0x143. Reading sixteen from 0x134, as the old field allowed, swallows both.
const TITLE_OFF: u64 = 0x134;
const TITLE_LEN: usize = 11;

/// 0x00 is a plain Game Boy cart, 0x80 is Colour-enhanced but still runs on original hardware,
/// and 0xC0 is Colour-only. Three values, not two, and `class` is where they stay three: they
/// are three different cartridges, in three different plastics, and two different shells.
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

/// Which cartridge Nintendo actually manufactured for this rom. The three classes are the ones
/// its own typology names, and the CGB flag is what picks between them — not the file
/// extension, which is a dumping convention: a `.gb` file is routinely Colour-exclusive and a
/// `.gbc` file is routinely DMG-compatible.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Class {
    /// Flag 0x00, "CGB Incompatible" in the programming manual: a grey pak with the
    /// power-switch notch, which runs on every Game Boy there is.
    Original,
    /// Flag 0x80, "CGB Compatible": a **black** pak. It uses Colour functions and still runs on
    /// original hardware, so it was moulded in the same notched shell as a grey one. It was
    /// never clear plastic, which is the error this type exists to stop repeating.
    DualMode,
    /// Flag 0xc0, "CGB Exclusive": the clear pak, and the only one of the three without the
    /// notch — which is what stops it turning an original Game Boy on at all.
    ColourOnly,
}

/// The manual lists three values and no others, so anything else — including a rom too short to
/// have a header — is an original pak. That is the safe way to be wrong: a mislabelled cart is
/// drawn as the commonest object rather than as a cart that never existed.
pub fn class(rom: &Path) -> Class {
    match cgb_flag(rom) {
        Some(0xc0) => Class::ColourOnly,
        Some(0x80) => Class::DualMode,
        _ => Class::Original,
    }
}

fn read_at(rom: &Path, off: u64, buf: &mut [u8]) -> Option<()> {
    let mut f = File::open(rom).ok()?;
    f.seek(SeekFrom::Start(off)).ok()?;
    f.read_exact(buf).ok()
}

//! An artwork pack: every label, ready made, in one file a user copies to `Labels/Artwork.pack`
//! so carts are labelled without a network. The file is
//!
//! ```text
//! b"SLOTART1", entry count (u32)
//! per entry, sorted by CRC32: the ROM's CRC32, image offset, image length (u32 each)
//! the images: JPEG, at the label size of the platform each was made for
//! ```
//!
//! with every integer little-endian. A lookup reads the table and one image, never the whole file.
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

pub const FILE: &str = "Artwork.pack";
const MAGIC: &[u8; 8] = b"SLOTART1";
const HEAD: u64 = 12;
const ENTRY: u64 = 12;
const MAX_IMAGE: u32 = 4 << 20;

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_owned())
}

/// Writes a pack of (CRC32, JPEG) entries. A checksum listed twice keeps its first image.
pub fn write(out: &mut impl Write, mut entries: Vec<(u32, Vec<u8>)>) -> io::Result<()> {
    entries.sort_by_key(|(crc, _)| *crc);
    entries.dedup_by_key(|(crc, _)| *crc);
    let too_big = || io::Error::new(io::ErrorKind::InvalidInput, "artwork pack over 4 GB");
    let count = u32::try_from(entries.len()).map_err(|_| too_big())?;
    out.write_all(MAGIC)?;
    out.write_all(&count.to_le_bytes())?;
    let mut offset = HEAD + ENTRY * u64::from(count);
    for (crc, image) in &entries {
        let len = u32::try_from(image.len()).map_err(|_| too_big())?;
        let at = u32::try_from(offset).map_err(|_| too_big())?;
        for field in [*crc, at, len] {
            out.write_all(&field.to_le_bytes())?;
        }
        offset += u64::from(len);
    }
    u32::try_from(offset).map_err(|_| too_big())?;
    for (_, image) in &entries {
        out.write_all(image)?;
    }
    Ok(())
}

/// The pack's image for a ROM. No pack is `Ok(None)`; a damaged one is an error.
pub(crate) fn find(path: &Path, crc: u32) -> io::Result<Option<Vec<u8>>> {
    let mut file = match File::open(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        file => file?,
    };
    let size = file.metadata()?.len();
    let mut head = [0u8; HEAD as usize];
    file.read_exact(&mut head)
        .map_err(|_| invalid("artwork pack too short"))?;
    if &head[..8] != MAGIC {
        return Err(invalid("not an artwork pack"));
    }
    let count = u64::from(u32::from_le_bytes(head[8..12].try_into().unwrap()));
    if HEAD + ENTRY * count > size {
        return Err(invalid("artwork pack table runs past the end"));
    }
    let mut table = vec![0u8; (ENTRY * count) as usize];
    file.read_exact(&mut table)?;
    let field =
        |entry: &[u8], i: usize| u32::from_le_bytes(entry[i * 4..i * 4 + 4].try_into().unwrap());
    let entries: Vec<&[u8]> = table.chunks_exact(ENTRY as usize).collect();
    let Ok(at) = entries.binary_search_by_key(&crc, |entry| field(entry, 0)) else {
        return Ok(None);
    };
    let (offset, len) = (field(entries[at], 1), field(entries[at], 2));
    if len > MAX_IMAGE || u64::from(offset) + u64::from(len) > size {
        return Err(invalid("artwork pack entry runs past the end"));
    }
    let mut image = vec![0u8; len as usize];
    file.seek(SeekFrom::Start(offset.into()))?;
    file.read_exact(&mut image)?;
    Ok(Some(image))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_pack_finds_each_image_by_checksum_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        let mut out = Vec::new();
        write(
            &mut out,
            vec![
                (30, b"third".to_vec()),
                (10, b"first".to_vec()),
                (20, b"second".to_vec()),
                (10, b"duplicate".to_vec()),
            ],
        )
        .unwrap();
        std::fs::write(&path, &out).unwrap();
        for (crc, image) in [(10, &b"first"[..]), (20, b"second"), (30, b"third")] {
            assert_eq!(find(&path, crc).unwrap().unwrap(), image);
        }
        assert_eq!(find(&path, 15).unwrap(), None);
        assert_eq!(find(&dir.path().join("missing.pack"), 10).unwrap(), None);
    }

    #[test]
    fn a_damaged_pack_is_an_error_not_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        let mut good = Vec::new();
        write(&mut good, vec![(10, vec![7; 100])]).unwrap();
        for damaged in [
            b"not a pack at all".to_vec(),
            good[..6].to_vec(),
            good[..20].to_vec(),
            good[..good.len() - 1].to_vec(),
        ] {
            std::fs::write(&path, damaged).unwrap();
            assert!(find(&path, 10).is_err());
        }
    }
}

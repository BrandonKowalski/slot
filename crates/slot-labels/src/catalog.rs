//! LaunchBox's cart fronts for every No-Intro dump it has one for, by the dump's CRC32. The
//! checksum names one exact dump, so the device needs no title search and no agreement about
//! spelling or region to find it. `task labels:index` rebuilds the table.
use std::io::Write;
use std::path::Path;

use crate::source::IMAGE;

pub(crate) const INDEX: &str = include_str!("../data/launchbox.tsv");

struct Crc(crc32fast::Hasher);
impl Write for Crc {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.update(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(crate) fn crc32(rom: &Path) -> std::io::Result<u32> {
    let mut crc = Crc(crc32fast::Hasher::new());
    std::io::copy(&mut std::fs::File::open(rom)?, &mut crc)?;
    Ok(crc.0.finalize())
}

/// Image URLs for a dump, best first; empty when the index does not know it.
pub(crate) fn lookup(index: &str, crc: u32) -> Vec<String> {
    let key = format!("{crc:08X}\t");
    index
        .lines()
        .find_map(|line| line.strip_prefix(&key))
        .map(|files| files.split('\t').map(|f| format!("{IMAGE}{f}")).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checksum_is_the_standard_crc32() {
        let dir = tempfile::tempdir().unwrap();
        let rom = dir.path().join("rom.gb");
        std::fs::write(&rom, b"123456789").unwrap();
        assert_eq!(crc32(&rom).unwrap(), 0xCBF4_3926);
    }

    #[test]
    fn a_dump_finds_its_images_in_order_and_others_find_none() {
        let index = "# comment\nCBF43926\ta.png\tb.jpg\nCBF43927\tc.png\n";
        assert_eq!(
            lookup(index, 0xCBF4_3926),
            [
                "https://images.launchbox-app.com/a.png",
                "https://images.launchbox-app.com/b.jpg"
            ]
        );
        assert!(lookup(index, 0x1234).is_empty());
    }

    #[test]
    fn the_bundled_index_is_well_formed() {
        let mut last = None;
        for line in INDEX.lines().filter(|l| !l.starts_with('#')) {
            let mut fields = line.split('\t');
            let crc = u32::from_str_radix(fields.next().unwrap(), 16).unwrap();
            assert!(last < Some(crc), "sorted and unique at {line}");
            last = Some(crc);
            let files: Vec<_> = fields.collect();
            assert!((1..=3).contains(&files.len()), "{line}");
            for f in files {
                assert!(
                    f.chars()
                        .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)),
                    "{line}"
                );
            }
        }
        assert!(last.is_some());
    }
}

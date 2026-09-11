//! `header_clean`: whether gpSP takes a cart's header at its word. A file-backed test file,
//! the same reason `scan.rs` is one — the byte at 3, the byte at 0xB2 and the file's own
//! length are all facts about something on disk, not about a slice in memory.

use std::fs::File;
use std::path::PathBuf;

use slot_store::header_clean;
use tempfile::TempDir;

fn clean_header() -> Vec<u8> {
    let mut rom = vec![0u8; 0x100];
    rom[3] = 0xEA;
    rom[0xB2] = 0x96;
    rom
}

fn write_rom(d: &TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let path = d.path().join(name);
    std::fs::write(&path, bytes).expect("write rom");
    path
}

#[test]
fn a_standard_header_within_16mb_is_clean() {
    let d = tempfile::tempdir().expect("tempdir");
    let rom = write_rom(&d, "Clean.gba", &clean_header());
    assert!(header_clean(&rom));
}

#[test]
fn a_wrong_entry_branch_opcode_is_not_clean() {
    let d = tempfile::tempdir().expect("tempdir");
    let mut bytes = clean_header();
    bytes[3] = 0x00;
    let rom = write_rom(&d, "BadOpcode.gba", &bytes);
    assert!(!header_clean(&rom));
}

#[test]
fn a_wrong_fixed_byte_is_not_clean() {
    let d = tempfile::tempdir().expect("tempdir");
    let mut bytes = clean_header();
    bytes[0xB2] = 0x00;
    let rom = write_rom(&d, "BadFixed.gba", &bytes);
    assert!(!header_clean(&rom));
}

/// `set_len` past 16 MiB is sparse and cheap — this proves an expanded ROM is refused without
/// the test itself having to write 16 MB to disk.
#[test]
fn a_clean_header_over_16mb_is_not_clean() {
    let d = tempfile::tempdir().expect("tempdir");
    let rom = write_rom(&d, "Big.gba", &clean_header());
    let f = File::options().write(true).open(&rom).expect("open");
    f.set_len(16 * 1024 * 1024 + 1).expect("set_len");
    assert!(!header_clean(&rom));
}

#[test]
fn a_rom_shorter_than_the_header_is_not_clean() {
    let d = tempfile::tempdir().expect("tempdir");
    let rom = write_rom(&d, "Short.gba", &[0u8; 0x50]);
    assert!(!header_clean(&rom));
}

#[test]
fn a_missing_file_is_not_clean() {
    let d = tempfile::tempdir().expect("tempdir");
    assert!(!header_clean(&d.path().join("Missing.gba")));
}

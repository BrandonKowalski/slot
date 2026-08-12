use std::path::{Path, PathBuf};

use slot_retro::{ButtonMask, MgbaCore, RetroCore, Rumble};

/// `RETRO_RUMBLE_STRONG` and `RETRO_RUMBLE_WEAK`, spelled out here because the test stands
/// in for the core and the core only ever passes the raw numbers.
const STRONG: u32 = 0;
const WEAK: u32 = 1;

fn test_core() -> Option<MgbaCore> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/mgba_libretro.dylib");
    if !p.exists() {
        return None;
    }
    Some(MgbaCore::open(&p).expect("vendored core is present but would not open"))
}

/// A header and nothing else. The core sniffs the fixed byte at 0xb2 and the branch at the
/// entry point; what the cart then executes does not matter to the rumble interface.
fn test_rom() -> PathBuf {
    let mut rom = vec![0u8; 0x8000];
    rom[0..4].copy_from_slice(&0xea00002eu32.to_le_bytes());
    rom[0xa0..0xac].copy_from_slice(b"SLOT TEST\0\0\0");
    rom[0xac..0xb0].copy_from_slice(b"SLTE");
    rom[0xb0..0xb2].copy_from_slice(b"00");
    rom[0xb2] = 0x96;
    let sum = rom[0xa0..0xbd].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    rom[0xbd] = 0u8.wrapping_sub(sum).wrapping_sub(0x19);
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("rumble.gba");
    std::fs::write(&path, &rom).expect("write test rom");
    path
}

/// Without the interface the core never rumbles, and the failure is silent: no error, just
/// a motor that never moves. mGBA asks for it on the first frame rather than at init, so
/// there has to be a cart running before the question has been put.
#[test]
fn the_core_is_offered_a_rumble_interface() {
    let Some(mut c) = test_core() else { return };
    c.load(&test_rom()).expect("load");
    c.run_frame(ButtonMask::default());
    assert!(
        c.asked_for_rumble(),
        "mgba never asked, or the callback refused"
    );
}

/// Two effects, one motor. Stopping the strong one must not stop the weak one.
#[test]
fn the_motor_takes_the_louder_of_the_two_effects() {
    let r = Rumble::default();
    assert!(r.set(0, STRONG, 40_000));
    assert!(r.set(0, WEAK, 10_000));
    assert_eq!(r.strength(), 40_000);
    r.set(0, STRONG, 0);
    assert_eq!(
        r.strength(),
        10_000,
        "the weak motor stopped with the strong"
    );
    r.set(0, WEAK, 0);
    assert_eq!(r.strength(), 0);
}

/// There is one pad and it is soldered in. A core asking for a second one is asking for
/// hardware that is not there, and answering with this device's motor would be a lie.
#[test]
fn a_second_port_is_refused_and_moves_nothing() {
    let r = Rumble::default();
    assert!(!r.set(1, STRONG, u16::MAX));
    assert_eq!(r.strength(), 0);
}

/// libretro may grow a third effect. Taking one this device cannot tell apart from rumble
/// would run the motor for something that is not rumble.
#[test]
fn an_unknown_effect_is_refused() {
    let r = Rumble::default();
    assert!(!r.set(0, 7, u16::MAX));
    assert_eq!(r.strength(), 0);
}

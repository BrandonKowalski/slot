use slot_retro::{ButtonMask, MgbaCore, RetroCore, GBA_H, GBA_W};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// libretro cores keep their state in dylib globals, so two live cores over one dylib is
/// not a supported configuration and the tests must not overlap.
static CORE_LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    CORE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn dylib() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/mgba_libretro.dylib")
}

fn test_core() -> Option<MgbaCore> {
    let p = dylib();
    if !p.exists() {
        return None;
    }
    Some(MgbaCore::open(&p).expect("vendored core is present but would not open"))
}

/// The one real `gba_bios.bin` in the tree. Without it mGBA falls back to its own HLE bios,
/// which has no boot animation to play whatever the core is told about skipping it.
fn core_with_bios() -> Option<MgbaCore> {
    let p = dylib();
    let bios = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdcard/BIOS");
    if !p.exists() || !bios.join("gba_bios.bin").exists() {
        return None;
    }
    Some(
        MgbaCore::open_with(&p, &bios, &bios).expect("vendored core is present but would not open"),
    )
}

/// Sets mode 3 and writes a frame counter into the first pixel once per vblank, so
/// consecutive frames differ and a savestate has both registers and VRAM worth restoring.
/// The counter is vblank paced rather than free running because a free running one is
/// sensitive to single cycle drift across a savestate.
fn test_rom() -> PathBuf {
    const CODE: [u32; 15] = [
        0xe3a00404, // mov  r0, #0x04000000
        0xe3a01c04, // mov  r1, #0x400
        0xe3811003, // orr  r1, r1, #3
        0xe5801000, // str  r1, [r0]          DISPCNT: mode 3, BG2 on
        0xe3a02406, // mov  r2, #0x06000000
        0xe3a03000, // mov  r3, #0
        0xe1d040b6, // vb:  ldrh r4, [r0, #6] VCOUNT
        0xe35400a0, //      cmp  r4, #160
        0x1afffffc, //      bne  vb
        0xe2833001, //      add  r3, r3, #1
        0xe1c230b0, //      strh r3, [r2]
        0xe1d040b6, // dr:  ldrh r4, [r0, #6]
        0xe35400a0, //      cmp  r4, #160
        0x0afffffc, //      beq  dr
        0xeafffff6, //      b    vb
    ];
    let mut rom = vec![0u8; 0x8000];
    rom[0..4].copy_from_slice(&0xea00002eu32.to_le_bytes()); // b 0xc0
    rom[0xa0..0xac].copy_from_slice(b"SLOT TEST\0\0\0");
    rom[0xac..0xb0].copy_from_slice(b"SLTE");
    rom[0xb0..0xb2].copy_from_slice(b"00");
    rom[0xb2] = 0x96; // fixed header byte, cores sniff it to identify a GBA rom
    let sum = rom[0xa0..0xbd].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    rom[0xbd] = 0u8.wrapping_sub(sum).wrapping_sub(0x19);
    for (i, w) in CODE.iter().enumerate() {
        let o = 0xc0 + i * 4;
        rom[o..o + 4].copy_from_slice(&w.to_le_bytes());
    }
    let p = Path::new(env!("CARGO_TARGET_TMPDIR")).join("slot-test.gba");
    std::fs::write(&p, rom).expect("write test rom");
    p
}

/// The same rom carrying a cart header logo the bios will accept. Everything above the
/// header is what `test_rom` builds; the logo is 156 bytes of Nintendo's, so it is lifted
/// off a cart on the card rather than checked in, and there is nothing to run without one.
///
/// A bios that does not recognise the logo skips itself and boots the cart anyway, which is
/// what every other test in this file relies on.
fn logo_rom() -> Option<PathBuf> {
    let games = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdcard/Games");
    let logo = std::fs::read_dir(games).ok()?.find_map(|e| {
        let p = e.ok()?.path();
        let rom = (p.extension()? == "gba").then(|| std::fs::read(&p).ok())??;
        (rom.get(4..8)? == [0x24, 0xff, 0xae, 0x51]).then(|| rom[4..0xa0].to_vec())
    })?;
    let mut rom = std::fs::read(test_rom()).ok()?;
    rom[4..0xa0].copy_from_slice(&logo);
    let p = Path::new(env!("CARGO_TARGET_TMPDIR")).join("slot-test-logo.gba");
    std::fs::write(&p, rom).ok()?;
    Some(p)
}

#[test]
fn mgba_reports_gba_geometry_and_round_trips_state() {
    let _g = lock();
    let Some(mut c) = test_core() else { return };
    let rom = test_rom();
    c.load(&rom).unwrap();
    for _ in 0..60 {
        c.run_frame(ButtonMask::default());
    }
    let info = c.av_info();
    assert!((info.fps - 59.7275).abs() < 0.01, "fps {}", info.fps);
    let s = c.serialize().unwrap();
    assert!(s.len() > 100_000, "state suspiciously small: {}", s.len());
    for _ in 0..60 {
        c.run_frame(ButtonMask::default());
    }
    let diverged = c.video_xrgb8888().to_vec();
    c.unserialize(&s).unwrap();
    c.run_frame(ButtonMask::default());
    let restored = c.video_xrgb8888().to_vec();
    assert_ne!(diverged, restored, "the rom paints the same frame forever");
    drop(c);

    let mut fresh = test_core().expect("dylib was there a moment ago");
    fresh.load(&rom).unwrap();
    for _ in 0..61 {
        fresh.run_frame(ButtonMask::default());
    }
    assert_eq!(restored, fresh.video_xrgb8888());
}

#[test]
fn audio_arrives_at_roughly_the_reported_sample_rate() {
    let _g = lock();
    let Some(mut c) = test_core() else { return };
    c.load(&test_rom()).unwrap();
    let info = c.av_info();
    let mut got = 0usize;
    for _ in 0..60 {
        c.run_frame(ButtonMask::default());
        let a = c.take_audio();
        assert_eq!(a.len() % 2, 0, "audio must be interleaved stereo");
        got += a.len() / 2;
    }
    let want = (60.0 / info.fps * info.sample_rate) as usize;
    assert!(
        got.abs_diff(want) * 10 < want,
        "{got} stereo frames over 60 video frames, expected about {want}"
    );
}

/// A clean start has to show the Nintendo logo and the chime, so mGBA must not be left on
/// its own default for `mgba_skip_bios`.
///
/// The rom fills the screen with black the moment it runs and the boot animation is a white
/// one, so half a second in the screen itself says which of the two is up.
#[test]
fn the_bios_intro_plays_when_a_bios_is_present() {
    let _g = lock();
    let (Some(mut c), Some(rom)) = (core_with_bios(), logo_rom()) else {
        return;
    };
    c.load(&rom).unwrap();
    for _ in 0..30 {
        c.run_frame(ButtonMask::default());
    }
    let lit = c
        .video_xrgb8888()
        .chunks(4)
        .filter(|p| p[0] > 0x40 && p[1] > 0x40 && p[2] > 0x40)
        .count();
    let all = (GBA_W * GBA_H) as usize;
    assert!(
        lit * 2 > all,
        "{lit} of {all} pixels are lit: the rom is already painting and the intro was skipped"
    );
}

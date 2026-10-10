use slot_retro::{ButtonMask, LibretroCore, RetroCore, GBA_H, GBA_W};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static CORE_LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    CORE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn dylib() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/mgba_libretro.dylib")
}

fn test_core() -> Option<LibretroCore> {
    let p = dylib();
    if !p.exists() {
        return None;
    }
    Some(LibretroCore::open(&p).expect("vendored core is present but would not open"))
}

fn core_with_bios() -> Option<LibretroCore> {
    let p = dylib();
    let bios = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdcard/BIOS");
    if !p.exists() || !bios.join("gba_bios.bin").exists() {
        return None;
    }
    Some(
        LibretroCore::open_with(&p, &bios, &bios)
            .expect("vendored core is present but would not open"),
    )
}

fn test_rom() -> PathBuf {
    const CODE: [u32; 15] = [
        0xe3a00404, 0xe3a01c04, 0xe3811003, 0xe5801000, 0xe3a02406, 0xe3a03000, 0xe1d040b6,
        0xe35400a0, 0x1afffffc, 0xe2833001, 0xe1c230b0, 0xe1d040b6, 0xe35400a0, 0x0afffffc,
        0xeafffff6,
    ];
    let mut rom = vec![0u8; 0x8000];
    rom[0..4].copy_from_slice(&0xea00002eu32.to_le_bytes());
    rom[0xa0..0xac].copy_from_slice(b"SLOT TEST\0\0\0");
    rom[0xac..0xb0].copy_from_slice(b"SLTE");
    rom[0xb0..0xb2].copy_from_slice(b"00");
    rom[0xb2] = 0x96;
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

const RTC_SRAM: usize = 0x2000;
const RTC_BLOCK: usize = 48;

fn rtc_rom() -> PathBuf {
    let mut rom = vec![0u8; 0x8000];
    rom[0x100..0x104].copy_from_slice(&[0x00, 0xc3, 0x50, 0x01]);
    rom[0x104..0x108].copy_from_slice(&[0xce, 0xed, 0x66, 0x66]);
    rom[0x134..0x13d].copy_from_slice(b"SLOT RTC\0");
    rom[0x147] = 0x10;
    rom[0x148] = 0x00;
    rom[0x149] = 0x02;
    rom[0x14d] = rom[0x134..0x14d]
        .iter()
        .fold(0u8, |a, b| a.wrapping_sub(*b).wrapping_sub(1));
    rom[0x150..0x152].copy_from_slice(&[0x18, 0xfe]);
    let p = Path::new(env!("CARGO_TARGET_TMPDIR")).join("slot-rtc.gbc");
    std::fs::write(&p, rom).expect("write rtc rom");
    p
}

fn rtc_sav(days: u32, unix: u64) -> Vec<u8> {
    let mut sav = vec![0x5a; RTC_SRAM];
    let regs = [12u32, 34, 5, days, 0];
    for r in regs.iter().chain(regs.iter()) {
        sav.extend_from_slice(&r.to_le_bytes());
    }
    sav.extend_from_slice(&unix.to_le_bytes());
    assert_eq!(sav.len(), RTC_SRAM + RTC_BLOCK);
    sav
}

fn booted_with(sav: &[u8]) -> Option<LibretroCore> {
    let mut c = test_core()?;
    c.load(&rtc_rom()).unwrap();
    c.load_save_ram(sav).unwrap();
    for _ in 0..5 {
        c.run_frame(ButtonMask::default());
    }
    Some(c)
}

#[test]
fn a_cartridge_clock_rides_along_in_the_save_ram() {
    let _g = lock();
    let sav = rtc_sav(100, 1_700_000_000);
    let Some(c) = booted_with(&sav) else { return };
    assert_eq!(c.save_ram().as_deref(), Some(&sav[..]));
}

#[test]
fn the_cartridge_clock_in_a_save_reaches_the_game() {
    let _g = lock();
    let Some(mut a) = booted_with(&rtc_sav(100, 1_700_000_000)) else {
        return;
    };
    let state = a.serialize().unwrap();
    drop(a);
    let mut b = booted_with(&rtc_sav(200, 1_700_000_000)).unwrap();
    assert_ne!(
        state,
        b.serialize().unwrap(),
        "the save's clock never reached the core"
    );
}

//! mGBA's link mode: two GBAs from one game, joined by mGBA's own lockstep link cable, stepped
//! together on one thread, with only the local player's GBA shown. Real core only: every test
//! skips when `vendor/mgba_libretro.dylib` is absent, and fails with a rebuild hint when the
//! vendored core predates link mode.

mod common;

use std::path::{Path, PathBuf};

use slot_retro::{ButtonMask, LibretroCore, RetroCore, GBA_H, GBA_W};

fn vendored() -> Option<PathBuf> {
    let dylib = common::vendored_core();
    if dylib.is_none() {
        eprintln!("no vendored mGBA core on this host, skipping");
    }
    dylib
}

fn rom(name: &str, bytes: Vec<u8>) -> PathBuf {
    let p = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&p, bytes).expect("write rom");
    p
}

/// `common::gba_rom` with its vblank counter swapped for KEYINPUT. Every vblank the rom reads
/// the buttons and writes them into the first pixel, so the picture says which buttons that
/// GBA was holding. Two instructions change, and the branches around them stay put.
fn keys_rom() -> Vec<u8> {
    let mut rom = common::gba_rom();
    let mut set = |index: usize, word: u32| {
        let o = 0xc0 + index * 4;
        rom[o..o + 4].copy_from_slice(&word.to_le_bytes());
    };
    set(5, 0xe2805c01); // add  r5, r0, #0x100   (was mov r3, #0)
    set(9, 0xe1d533b0); // ldrh r3, [r5, #0x30]  KEYINPUT (was add r3, r3, #1)
    rom
}

/// `common::gba_rom` wired to the link port. At start it clears RCNT, which puts the port in
/// serial mode rather than GPIO. Every vblank it writes SIOCNT for multiplayer mode and paints
/// what SIOCNT reads back into the first pixel. mGBA fills in SIOCNT's multiplayer id from the
/// cable on each write, so the picture says which player a GBA is on a cable, if it is on one
/// at all.
fn sio_rom() -> Vec<u8> {
    let mut rom = common::gba_rom();
    let mut set = |index: usize, word: u32| {
        let o = 0xc0 + index * 4;
        rom[o..o + 4].copy_from_slice(&word.to_le_bytes());
    };
    set(5, 0xea000008); // b     setup                  (was mov r3, #0)
    set(9, 0xea000009); // b     poke                   (was add r3, r3, #1)
    set(15, 0xe2805c01); // setup: add  r5, r0, #0x100
    set(16, 0xe3a06000); //        mov  r6, #0
    set(17, 0xe1c563b4); //        strh r6, [r5, #0x34]   RCNT = 0: serial, not GPIO
    set(18, 0xe3a06a02); //        mov  r6, #0x2000       SIOCNT: multiplayer mode
    set(19, 0xeafffff1); //        b    vb
    set(20, 0xe1c562b8); // poke:  strh r6, [r5, #0x28]   write SIOCNT
    set(21, 0xe1d532b8); //        ldrh r3, [r5, #0x28]   read it back
    set(22, 0xeafffff2); //        b    strh r3, [r2]
    rom
}

/// `common::gba_rom` as a multiplayer game that starts a transfer every frame. At start it clears
/// RCNT, for serial mode, and sets SIOCNT's multiplayer mode on its own: player 0's GBA waits on
/// player 1's to take a new mode, so a start bit in the same write would wait twice. Every vblank
/// it writes SIOCNT again with the start bit, which starts a transfer on player 0's GBA and does
/// nothing on player 1's, then paints the buttons into the first pixel as `keys_rom` does. Player
/// 0's GBA waits on player 1's when a transfer starts and again when it ends, 63,427 cycles later.
/// Branches are to `0xc0 + index * 4 + 8 + offset * 4`.
fn transfer_rom() -> Vec<u8> {
    let mut rom = common::gba_rom();
    let mut set = |index: usize, word: u32| {
        let o = 0xc0 + index * 4;
        rom[o..o + 4].copy_from_slice(&word.to_le_bytes());
    };
    set(5, 0xea000008); // 0x0d4:        b    setup (0x0fc)          (was mov r3, #0)
    set(9, 0xea00000b); // 0x0e4:        b    poke (0x118)           (was add r3, r3, #1)
    set(15, 0xe2805c01); // 0x0fc: setup: add  r5, r0, #0x100
    set(16, 0xe3a06000); // 0x100:        mov  r6, #0
    set(17, 0xe1c563b4); // 0x104:        strh r6, [r5, #0x34]   RCNT = 0: serial, not GPIO
    set(18, 0xe3a06a02); // 0x108:        mov  r6, #0x2000
    set(19, 0xe1c562b8); // 0x10c:        strh r6, [r5, #0x28]   SIOCNT: multiplayer mode
    set(20, 0xe3866080); // 0x110:        orr  r6, r6, #0x80     r6: multiplayer mode and start
    set(21, 0xeaffffef); // 0x114:        b    vb (0x0d8)
    set(22, 0xe1c562b8); // 0x118: poke:  strh r6, [r5, #0x28]   SIOCNT: start a transfer
    set(23, 0xe1d533b0); // 0x11c:        ldrh r3, [r5, #0x30]   KEYINPUT
    set(24, 0xeafffff0); // 0x120:        b    strh r3, [r2] (0x0e8)
    rom
}

/// `keys_rom` that starts a long DMA before its first vblank, as Mario Kart: Super Circuit's boot
/// does: 0x10000 32-bit words from the cartridge into EWRAM. A DMA holds the CPU until it is done,
/// almost three frames here, so the GBA reaches its first frame ends with its CPU blocked. Branches
/// are to `0xc0 + index * 4 + 8 + offset * 4`.
fn dma_rom() -> Vec<u8> {
    let mut rom = common::gba_rom();
    let mut set = |index: usize, word: u32| {
        let o = 0xc0 + index * 4;
        rom[o..o + 4].copy_from_slice(&word.to_le_bytes());
    };
    set(5, 0xea000008); // 0x0d4:        b    setup (0x0fc)          (was mov r3, #0)
    set(9, 0xe1d533b0); // 0x0e4:        ldrh r3, [r5, #0x30]   KEYINPUT (was add r3, r3, #1)
    set(15, 0xe2805c01); // 0x0fc: setup: add  r5, r0, #0x100
    set(16, 0xe3a06408); // 0x100:        mov  r6, #0x08000000
    set(17, 0xe58060d4); // 0x104:        str  r6, [r0, #0xd4]   DMA3SAD: the cartridge
    set(18, 0xe3a06402); // 0x108:        mov  r6, #0x02000000
    set(19, 0xe58060d8); // 0x10c:        str  r6, [r0, #0xd8]   DMA3DAD: EWRAM
    set(20, 0xe3a06484); // 0x110:        mov  r6, #0x84000000
    set(21, 0xe58060dc); // 0x114:        str  r6, [r0, #0xdc]   DMA3CNT: 0x10000 words, 32-bit, now
    set(22, 0xeaffffee); // 0x118:        b    vb (0x0d8)
    rom
}

/// `common::gba_rom` as a multiplayer game that talks over the cable every frame and paints what it
/// last heard. At start it clears RCNT, for serial mode, and sets SIOCNT's multiplayer mode. Every
/// vblank it paints SIOCNT into the second pixel, and SIOMULTI0 and SIOMULTI1, what the last
/// transfer carried from player 0 and from player 1, into the third and fourth. Then it writes
/// SIOCNT's mode again, which has mGBA fill in the GBA's multiplayer id from the cable, puts
/// 0x1000 plus that id times 0x10 into SIOMLT_SEND (0x1000 on player 0's GBA, 0x1010 on player
/// 1's) and starts a transfer. Like a game, it fills SIOMLT_SEND before every transfer: mGBA does
/// not keep that register in a savestate. Branches are to `0xc0 + index * 4 + 8 + offset * 4`.
fn multiplayer_rom() -> Vec<u8> {
    let mut rom = common::gba_rom();
    let mut set = |index: usize, word: u32| {
        let o = 0xc0 + index * 4;
        rom[o..o + 4].copy_from_slice(&word.to_le_bytes());
    };
    set(5, 0xea000008); // 0x0d4:        b    setup (0x0fc)          (was mov r3, #0)
    set(9, 0xea00000a); // 0x0e4:        b    poke (0x114)           (was add r3, r3, #1)
    set(15, 0xe2805c01); // 0x0fc: setup: add  r5, r0, #0x100
    set(16, 0xe3a06000); // 0x100:        mov  r6, #0
    set(17, 0xe1c563b4); // 0x104:        strh r6, [r5, #0x34]   RCNT = 0: serial, not GPIO
    set(18, 0xe3a06a02); // 0x108:        mov  r6, #0x2000
    set(19, 0xe1c562b8); // 0x10c:        strh r6, [r5, #0x28]   SIOCNT: multiplayer mode
    set(20, 0xeafffff0); // 0x110:        b    vb (0x0d8)
    set(21, 0xe1d532b8); // 0x114: poke:  ldrh r3, [r5, #0x28]   SIOCNT
    set(22, 0xe1c230b2); // 0x118:        strh r3, [r2, #2]      into the second pixel
    set(23, 0xe1d532b0); // 0x11c:        ldrh r3, [r5, #0x20]   SIOMULTI0
    set(24, 0xe1c230b4); // 0x120:        strh r3, [r2, #4]      into the third
    set(25, 0xe1d532b2); // 0x124:        ldrh r3, [r5, #0x22]   SIOMULTI1
    set(26, 0xe1c230b6); // 0x128:        strh r3, [r2, #6]      into the fourth
    set(27, 0xe1c562b8); // 0x12c:        strh r6, [r5, #0x28]   SIOCNT: the mode, and the id
    set(28, 0xe1d532b8); // 0x130:        ldrh r3, [r5, #0x28]
    set(29, 0xe2033030); // 0x134:        and  r3, r3, #0x30     the id, times 0x10
    set(30, 0xe3833a01); // 0x138:        orr  r3, r3, #0x1000
    set(31, 0xe1c532ba); // 0x13c:        strh r3, [r5, #0x2a]   SIOMLT_SEND
    set(32, 0xe3863080); // 0x140:        orr  r3, r6, #0x80
    set(33, 0xe1c532b8); // 0x144:        strh r3, [r5, #0x28]   SIOCNT: start a transfer
    set(34, 0xeaffffe7); // 0x148:        b    dr (0x0ec)
    rom
}

/// `transfer_rom` as a game that fills SIOMLT_SEND once and then leaves it alone, which is what
/// makes it a probe for everything a savestate does not carry. Three changes: it transfers at
/// VCOUNT 140 rather than at the frame end, it writes SIOMLT_SEND only while A is held, and it
/// paints SIOMULTI1, the last word it heard from player 1, instead of the buttons. A GBA that has
/// run with A held holds 0x2080 in a register mGBA never saves, and its transfers have left RCNT's
/// SC bit set; a freshly loaded one holds neither. Restoring the same link state into both has to
/// give the same machine. Branches are to `0xc0 + index * 4 + 8 + offset * 4`.
fn probe_rom() -> Vec<u8> {
    let mut rom = transfer_rom();
    let mut set = |index: usize, word: u32| {
        let o = 0xc0 + index * 4;
        rom[o..o + 4].copy_from_slice(&word.to_le_bytes());
    };
    set(7, 0xe354008c); // 0x0dc:        cmp  r4, #140          transfer at VCOUNT 140 (was 160)
    set(12, 0xe354008c); // 0x0f0: dr:   cmp  r4, #140          (was 160)
    set(22, 0xe1d543b0); // 0x118: poke: ldrh r4, [r5, #0x30]   KEYINPUT
    set(23, 0xe3140001); // 0x11c:       tst  r4, #1            A held? (KEYINPUT is active low)
    set(24, 0x01c562ba); // 0x120:       strheq r6, [r5, #0x2a] SIOMLT_SEND, only while A is held
    set(25, 0xe1d532b2); // 0x124:       ldrh r3, [r5, #0x22]   SIOMULTI1
    set(26, 0xe1c562b8); // 0x128:       strh r6, [r5, #0x28]   SIOCNT: start a transfer
    set(27, 0xeaffffed); // 0x12c:       b    strh r3, [r2] (0x0e8)
    rom
}

/// `dma_rom` that starts its long DMA when it reads A rather than before its first vblank, so a
/// test can choose which frame end the two GBAs reach with player 0's CPU blocked. The boot DMA is
/// the one the join now plays alone, before the cable goes in; this one lands mid-session, where
/// the cable is already in and the next hard sync is a long way off. Branches are to
/// `0xc0 + index * 4 + 8 + offset * 4`.
fn dma_a_rom() -> Vec<u8> {
    let mut rom = dma_rom();
    let mut set = |index: usize, word: u32| {
        let o = 0xc0 + index * 4;
        rom[o..o + 4].copy_from_slice(&word.to_le_bytes());
    };
    set(10, 0xea00000b); // 0x0e8:        b    test (0x11c)      (was strh r3, [r2])
    set(21, 0xeaffffef); // 0x114:        b    vb (0x0d8)        setup now only loads the registers
    set(23, 0xe3130001); // 0x11c: test:  tst  r3, #1            A held? (KEYINPUT is active low)
    set(24, 0x058060dc); // 0x120:        streq r6, [r0, #0xdc]  DMA3CNT, only on the frame A is read
    set(25, 0xe1c230b0); // 0x124:        strh r3, [r2]          paint the buttons
    set(26, 0xeaffffef); // 0x128:        b    dr (0x0ec)
    rom
}

/// `common::gba_rom` as a game sitting in the serial port's normal 8-bit mode rather than in
/// multiplayer mode. At start it clears RCNT, for serial rather than GPIO, and writes SIOCNT for
/// normal 8-bit with an external clock, which is the mode a game holds while it waits to be
/// clocked. Every vblank it writes SIODATA8 and paints the buttons into the first pixel, as
/// `keys_rom` does. Nothing transfers, and that is the point: a normal-mode slave waits for a clock
/// that never comes, so the cable has to take up the mode the restored registers hold with no
/// transfer to carry it. It paints the buttons rather than SIOCNT because SIOCNT on a cable is
/// player-dependent by design - `sio_rom`'s test turns on the two GBAs reading different values -
/// and these starts compare the two devices' pictures. Branches are to
/// `0xc0 + index * 4 + 8 + offset * 4`.
fn normal_rom() -> Vec<u8> {
    let mut rom = common::gba_rom();
    let mut set = |index: usize, word: u32| {
        let o = 0xc0 + index * 4;
        rom[o..o + 4].copy_from_slice(&word.to_le_bytes());
    };
    set(5, 0xea000008); // 0x0d4:        b    setup (0x0fc)          (was mov r3, #0)
    set(9, 0xea00000b); // 0x0e4:        b    poke (0x118)           (was add r3, r3, #1)
    set(15, 0xe2805c01); // 0x0fc: setup: add  r5, r0, #0x100
    set(16, 0xe3a06000); // 0x100:        mov  r6, #0
    set(17, 0xe1c563b4); // 0x104:        strh r6, [r5, #0x34]   RCNT = 0: serial, not GPIO
    set(18, 0xe1c562b8); // 0x108:        strh r6, [r5, #0x28]   SIOCNT: normal 8-bit, external clock
    set(19, 0xe3a06001); // 0x10c:        mov  r6, #1
    set(20, 0xeafffff0); // 0x110:        b    vb (0x0d8)
    set(22, 0xe1c562ba); // 0x118: poke:  strh r6, [r5, #0x2a]   SIODATA8
    set(23, 0xe1d533b0); // 0x11c:        ldrh r3, [r5, #0x30]   KEYINPUT
    set(24, 0xeafffff0); // 0x120:        b    strh r3, [r2] (0x0e8)
    rom
}

/// The 15-bit colour a rom wrote into pixel `x` of the first row, read back from the picture. A
/// register painted this way loses its top bit.
fn painted(picture: &[u8], x: usize) -> u16 {
    let pixel = &picture[x * 4..x * 4 + 4];
    u16::from(pixel[2] >> 3) | u16::from(pixel[1] >> 3) << 5 | u16::from(pixel[0] >> 3) << 10
}

/// A player can open a game's link menu before the two SPs connect, so a session can start from
/// states already in multiplayer mode: the game chose that mode before there was a cable, and does
/// not write it again. The cable has to take up the mode each GBA's registers hold as it goes in,
/// as if the game had just written them. From the first transfer after the restore, each GBA has
/// to hear the other's value, and SIOCNT has to show every GBA on the cable ready. Before, the
/// fresh cable only learned a GBA's mode when its game wrote a new one, so player 0's GBA never
/// saw player 1's as ready. A link state of a pair that was transferring restores the same way.
/// The first frame is left out: what it paints was heard before the restore.
#[test]
fn a_pair_restored_in_multiplayer_mode_talks_from_the_first_transfer() {
    let _g = common::core_lock();
    let Some(dylib) = vendored() else { return };
    let rom = rom("mgba-link-multiplayer.gba", multiplayer_rom());

    let alone = single_state(&dylib, &rom, 20);
    let mut pair = link_core(&dylib, 0);
    pair.load(&rom).expect("link mode refused the rom");
    for _ in 0..30 {
        pair.run_frame_linked(ButtonMask::default(), ButtonMask::default());
    }
    let transferring = pair.serialize().expect("no link state");
    drop(pair);

    let mut deaf = Vec::new();
    for (what, container) in [
        ("two one-GBA states", slk1([&alone, &alone])),
        ("a link state of a pair that was transferring", transferring),
    ] {
        let pictures = linked_pictures(&dylib, &rom, Some(&container), 10, |_| {
            ButtonMask::default()
        });
        for (player, frames) in pictures.iter().enumerate() {
            let heard = frames.iter().enumerate().skip(1).find_map(|(frame, picture)| {
                let (siocnt, from_0, from_1) =
                    (painted(picture, 1), painted(picture, 2), painted(picture, 3));
                (siocnt & 0x0008 == 0 || from_0 != 0x1000 || from_1 != 0x1010).then(|| {
                    format!(
                        "{what}: player {player}'s GBA, first on frame {frame}: SIOCNT {siocnt:04x}, \
                         SIOMULTI0 {from_0:04x}, SIOMULTI1 {from_1:04x}"
                    )
                })
            });
            deaf.extend(heard);
        }
    }
    assert!(
        deaf.is_empty(),
        "a pair restored in multiplayer mode was not ready or did not hear each other: {deaf:#?}"
    );
}

fn single_core(dylib: &Path) -> LibretroCore {
    LibretroCore::open(dylib).expect("vendored core is present but would not open")
}

/// The core reads options during `retro_load_game`, so they are set here, before any `load`.
fn link_core(dylib: &Path, player: u8) -> LibretroCore {
    let mut core = single_core(dylib);
    core.set_option("mgba_link", "on");
    core.set_option("mgba_link_player", &player.to_string());
    core
}

/// The link state: `SLK1`, then for each player a little-endian u32 length and that GBA's
/// state, player 0 first.
fn slk1(states: [&[u8]; 2]) -> Vec<u8> {
    let mut out = b"SLK1".to_vec();
    for state in states {
        out.extend_from_slice(&(state.len() as u32).to_le_bytes());
        out.extend_from_slice(state);
    }
    out
}

fn split_slk1(container: &[u8]) -> [Vec<u8>; 2] {
    assert_eq!(
        &container[..4],
        b"SLK1",
        "not a link state. A vendored core built before link mode ignores the option: run `task core`"
    );
    let mut rest = &container[4..];
    let mut take = || {
        let len = u32::from_le_bytes(rest[..4].try_into().unwrap()) as usize;
        let state = rest[4..4 + len].to_vec();
        rest = &rest[4 + len..];
        state
    };
    let states = [take(), take()];
    assert!(
        rest.is_empty(),
        "{} bytes after player 1's state",
        rest.len()
    );
    states
}

/// The picture after `frames` frames of `keys` on a lone GBA.
fn single_picture(dylib: &Path, rom: &Path, keys: u16, frames: usize) -> Vec<u8> {
    let mut core = single_core(dylib);
    core.load(rom).expect("load");
    for _ in 0..frames {
        core.run_frame(ButtonMask(keys));
    }
    core.video_xrgb8888().to_vec()
}

/// Player 0 holds A on port 0 and player 1 holds B on port 1. Each device must show its own
/// player's GBA, and that GBA must be holding its own player's buttons.
#[test]
fn link_mode_shows_the_local_players_gba_holding_its_own_port() {
    let _g = common::core_lock();
    let Some(dylib) = vendored() else { return };
    let rom = rom("mgba-link-keys.gba", keys_rom());

    let a = single_picture(&dylib, &rom, ButtonMask::A, 10);
    let b = single_picture(&dylib, &rom, ButtonMask::B, 10);
    assert_ne!(a, b, "the rom paints the same picture whatever is held");

    for (player, want) in [a, b].iter().enumerate() {
        let mut core = link_core(&dylib, player as u8);
        core.load(&rom).expect("link mode refused the rom");
        for _ in 0..10 {
            core.run_frame_linked(ButtonMask(ButtonMask::A), ButtonMask(ButtonMask::B));
        }
        assert!(
            core.video_xrgb8888() == want.as_slice(),
            "mgba_link_player={player} did not show player {player}'s GBA holding port {player}'s \
             buttons. A vendored core built before link mode ignores the option: run `task core`"
        );
    }
}

#[test]
fn link_mode_saves_both_gbas_in_one_link_state() {
    let _g = common::core_lock();
    let Some(dylib) = vendored() else { return };
    let rom = rom("mgba-link-state.gba", common::gba_rom());

    let mut core = link_core(&dylib, 0);
    core.load(&rom).expect("link mode refused the rom");
    for _ in 0..30 {
        core.run_frame_linked(ButtonMask::default(), ButtonMask::default());
    }
    let state = core.serialize().expect("no link state");
    for (player, gba) in split_slk1(&state).iter().enumerate() {
        assert!(
            gba.len() > 100_000,
            "player {player}'s GBA state is {} bytes",
            gba.len()
        );
    }
}

/// A guard for single-player: an explicit `off` is today's core, one GBA and its own state.
#[test]
fn link_mode_off_keeps_the_single_gba_state() {
    let _g = common::core_lock();
    let Some(dylib) = vendored() else { return };
    let rom = rom("mgba-link-off.gba", common::gba_rom());

    let mut core = single_core(&dylib);
    core.set_option("mgba_link", "off");
    core.load(&rom).expect("load");
    core.run_frame(ButtonMask::default());
    let state = core.serialize().expect("no state");
    assert_ne!(&state[..4], b"SLK1");
    assert!(state.len() > 100_000, "state is {} bytes", state.len());
}

/// How a link session starts: two ordinary one-GBA states, each player's own, restored into link
/// mode. Each GBA has to carry on exactly as it would have alone.
#[test]
fn a_link_state_of_two_single_gba_states_restores_each_player_where_they_were() {
    let _g = common::core_lock();
    let Some(dylib) = vendored() else { return };
    let rom = rom("mgba-link-restore.gba", common::gba_rom());

    // Two moments of the counter rom, far enough apart to paint different pictures.
    let mut single = single_core(&dylib);
    single.load(&rom).expect("load");
    let mut states = Vec::new();
    for frames in [10, 40] {
        for _ in 0..frames {
            single.run_frame(ButtonMask::default());
        }
        states.push(single.serialize().expect("no state"));
    }
    drop(single);

    // Where each moment gets to alone, 5 frames on.
    let mut alone = Vec::new();
    for state in &states {
        let mut core = single_core(&dylib);
        core.load(&rom).expect("load");
        core.unserialize(state)
            .expect("the single core refused its own state");
        for _ in 0..5 {
            core.run_frame(ButtonMask::default());
        }
        alone.push(core.video_xrgb8888().to_vec());
    }
    assert_ne!(alone[0], alone[1], "the two moments paint the same picture");

    let container = slk1([&states[0], &states[1]]);
    for (player, want) in alone.iter().enumerate() {
        let mut core = link_core(&dylib, player as u8);
        core.load(&rom).expect("link mode refused the rom");
        core.unserialize(&container)
            .expect("link mode refused the link state");
        for _ in 0..5 {
            core.run_frame_linked(ButtonMask::default(), ButtonMask::default());
        }
        assert!(
            core.video_xrgb8888() == want.as_slice(),
            "player {player}'s GBA did not carry on from player {player}'s state"
        );
    }
}

/// Plan 2 restores what came over the network, so anything but a whole link state is refused, and
/// the core still takes its own link state afterwards.
#[test]
fn link_mode_refuses_anything_but_a_whole_link_state() {
    let _g = common::core_lock();
    let Some(dylib) = vendored() else { return };
    let rom = rom("mgba-link-malformed.gba", common::gba_rom());

    let mut core = link_core(&dylib, 0);
    core.load(&rom).expect("link mode refused the rom");
    core.run_frame_linked(ButtonMask::default(), ButtonMask::default());
    let good = core.serialize().expect("no link state");
    let [player_0, _] = split_slk1(&good);

    let mut wrong_magic = good.clone();
    wrong_magic[3] = b'2';
    let mut long_length = good.clone();
    long_length[4..8].copy_from_slice(&(good.len() as u32).to_le_bytes());
    let mut trailing = good.clone();
    trailing.push(0);
    let empty = slk1([&[], &[]]);
    let short = slk1([&player_0, &player_0[..100]]);

    for (what, bytes) in [
        ("a one-GBA state", player_0.as_slice()),
        ("a different magic", wrong_magic.as_slice()),
        ("a length past the end", long_length.as_slice()),
        ("bytes after player 1", trailing.as_slice()),
        ("the magic alone", b"SLK1".as_slice()),
        ("player 1 missing", &good[..8 + player_0.len()]),
        ("two empty states", empty.as_slice()),
        ("a player 1 state too short to be one", short.as_slice()),
    ] {
        assert!(core.unserialize(bytes).is_err(), "link mode took {what}");
    }
    core.unserialize(&good)
        .expect("link mode refused its own link state");
}

/// A restore the core refuses part way leaves both GBAs where they were. Player 0's state here
/// is sound, but player 1's claims a savestate version from the future, which the core refuses
/// only once it is already loading. Stopping there would leave player 0 restored and player 1
/// not, so both have to go back.
#[test]
fn a_link_state_the_core_refuses_leaves_both_gbas_where_they_were() {
    let _g = common::core_lock();
    let Some(dylib) = vendored() else { return };
    let rom = rom("mgba-link-rollback.gba", common::gba_rom());

    let mut single = single_core(&dylib);
    single.load(&rom).expect("load");
    for _ in 0..10 {
        single.run_frame(ButtonMask::default());
    }
    let early = single.serialize().expect("no state");
    drop(single);
    let mut refused = early.clone();
    refused[..4].copy_from_slice(&u32::MAX.to_le_bytes());

    // Where the pair is before the refused restore, 60 frames in, and where it goes 5 frames on.
    let mut control = link_core(&dylib, 0);
    control.load(&rom).expect("link mode refused the rom");
    for _ in 0..60 {
        control.run_frame_linked(ButtonMask::default(), ButtonMask::default());
    }
    let here = control.serialize().expect("no link state");
    drop(control);
    let mut control = link_core(&dylib, 0);
    control.load(&rom).expect("link mode refused the rom");
    control
        .unserialize(&here)
        .expect("link mode refused its own link state");
    for _ in 0..5 {
        control.run_frame_linked(ButtonMask::default(), ButtonMask::default());
    }
    let want = control.video_xrgb8888().to_vec();
    drop(control);

    let mut core = link_core(&dylib, 0);
    core.load(&rom).expect("link mode refused the rom");
    core.unserialize(&here)
        .expect("link mode refused its own link state");
    assert!(
        core.unserialize(&slk1([&early, &refused])).is_err(),
        "link mode took a state the core refuses"
    );
    for _ in 0..5 {
        core.run_frame_linked(ButtonMask::default(), ButtonMask::default());
    }
    assert!(
        core.video_xrgb8888() == want.as_slice(),
        "a refused restore left player 0's GBA restored instead of where it was"
    );
}

/// FNV-1a 64: the hash the lockstep's checksums will use.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Different buttons on each port, changing often, so a port read by the wrong GBA, or a frame
/// late, would change the machines.
fn script(frame: usize) -> (ButtonMask, ButtonMask) {
    let p0 = if frame.is_multiple_of(3) {
        ButtonMask::A
    } else {
        ButtonMask::RIGHT
    };
    let p1 = if frame % 5 < 2 {
        ButtonMask::B | ButtonMask::L
    } else {
        0
    };
    (ButtonMask(p0), ButtonMask(p1))
}

/// Each SP runs both GBAs and shows its own player's. The two SPs stay in lockstep only if both
/// compute the same pair of machines whichever player they show. So after the same start and
/// the same buttons, the link state must hash the same for `mgba_link_player` 0 and 1, and the
/// same again on a second run.
#[test]
fn both_players_devices_compute_the_same_machines() {
    let _g = common::core_lock();
    let Some(dylib) = vendored() else { return };
    let rom = rom("mgba-link-lockstep.gba", keys_rom());

    let mut single = single_core(&dylib);
    single.load(&rom).expect("load");
    let mut starts = Vec::new();
    for frames in [20, 45] {
        for _ in 0..frames {
            single.run_frame(ButtonMask::default());
        }
        starts.push(single.serialize().expect("no state"));
    }
    drop(single);
    let container = slk1([&starts[0], &starts[1]]);

    let mut hashes = Vec::new();
    for player in [0u8, 1, 0] {
        let mut core = link_core(&dylib, player);
        core.load(&rom).expect("link mode refused the rom");
        core.unserialize(&container)
            .expect("link mode refused the link state");
        for frame in 0..600 {
            let (p0, p1) = script(frame);
            core.run_frame_linked(p0, p1);
        }
        hashes.push(fnv1a(&core.serialize().expect("no link state")));
    }
    assert_eq!(
        hashes[0], hashes[2],
        "the same device computed two different machines"
    );
    assert_eq!(
        hashes[0], hashes[1],
        "player 0's and player 1's devices computed different machines"
    );
}

/// The same buttons for both players, pressed on frame 10 and released on 40, then B on 50 to 53
/// and for frame 70 alone, then RIGHT for three frames in every seven from 84 to 119. Every change
/// is an edge a GBA reading its buttons a frame late would paint differently.
fn shared_script(frame: usize) -> ButtonMask {
    let mut keys = 0;
    if (10..40).contains(&frame) {
        keys |= ButtonMask::A;
    }
    if (50..53).contains(&frame) || frame == 70 {
        keys |= ButtonMask::B;
    }
    if (80..120).contains(&frame) && frame % 7 < 3 {
        keys |= ButtonMask::RIGHT;
    }
    ButtonMask(keys)
}

/// `dma_a_rom`'s buttons: A on frame 3 alone, which is the frame that starts the long DMA, then
/// RIGHT for frames 60 to 62 and again for frame 70, then B for frames 80 to 83. A is pressed on an
/// odd frame on purpose. A DMA started on frame 3, 5, 7, 9 or 11 leaves the two GBAs at a frame end
/// with player 0's CPU blocked while the cable's next hard sync is still more than a frame away,
/// which is the one case the sync's early exit is there for. Every later change is an edge a GBA a
/// frame behind would paint differently.
fn dma_script(frame: usize) -> ButtonMask {
    let mut keys = 0;
    if frame == 3 {
        keys |= ButtonMask::A;
    }
    if (60..63).contains(&frame) || frame == 70 {
        keys |= ButtonMask::RIGHT;
    }
    if (80..84).contains(&frame) {
        keys |= ButtonMask::B;
    }
    ButtonMask(keys)
}

/// Every frame's picture from each player's device, running `rom` as a linked pair from
/// `container` (or from a load, when there is none) with `buttons(frame)` on both ports. Player 0
/// first.
fn linked_pictures(
    dylib: &Path,
    rom: &Path,
    container: Option<&[u8]>,
    frames: usize,
    buttons: fn(usize) -> ButtonMask,
) -> [Vec<Vec<u8>>; 2] {
    [0u8, 1].map(|player| {
        let mut core = link_core(dylib, player);
        core.load(rom).expect("link mode refused the rom");
        if let Some(container) = container {
            core.unserialize(container)
                .expect("link mode refused the link state");
        }
        (0..frames)
            .map(|frame| {
                let keys = buttons(frame);
                core.run_frame_linked(keys, keys);
                core.video_xrgb8888().to_vec()
            })
            .collect()
    })
}

/// The frames on which player 0's device and player 1's device showed different pictures.
fn differing_frames(pictures: &[Vec<Vec<u8>>; 2]) -> Vec<usize> {
    (0..pictures[0].len())
        .filter(|&frame| pictures[0][frame] != pictures[1][frame])
        .collect()
}

/// A lone GBA's state, `frames` frames into `rom`. A state taken between `run_frame` calls is
/// always at the end of a frame.
fn single_state(dylib: &Path, rom: &Path, frames: usize) -> Vec<u8> {
    let mut single = single_core(dylib);
    single.load(rom).expect("load");
    for _ in 0..frames {
        single.run_frame(ButtonMask::default());
    }
    single.serialize().expect("no state")
}

/// Two identical GBAs given the same buttons have to read each change on the same frame, or two
/// games that wait on each other's input start a frame apart. Restoring one one-GBA state into
/// both slots puts the two GBAs' frames in phase, ending at the same emulated moment, and every
/// link session starts from two such states. The cable keeps player 1 a little behind player 0,
/// so player 0 finishes each frame while player 1 is still short of its own. Had player 0 run on
/// into its next frame there, it would read that frame's buttons before they were set. Mario
/// Kart: Super Circuit's two GBAs did that, entered the link lobby a frame apart and stalled.
#[test]
fn two_identical_gbas_read_the_same_buttons_on_the_same_frame() {
    let _g = common::core_lock();
    let Some(dylib) = vendored() else { return };
    let rom = rom("mgba-link-same-buttons.gba", keys_rom());

    let state = single_state(&dylib, &rom, 20);
    let pictures = linked_pictures(
        &dylib,
        &rom,
        Some(&slk1([&state, &state])),
        120,
        shared_script,
    );
    let differing = differing_frames(&pictures);
    assert!(
        differing.is_empty(),
        "two identical GBAs given the same buttons painted different buttons, first on frame {:?} \
         (all: {differing:?})",
        differing.first()
    );
}

/// Every way we know a linked pair can start, by name, with the rom it runs, the link state it
/// restores, if any, and the buttons both ports hold on each frame. Five starts of `transfer_rom`:
/// a fresh load, and a restore of each pairing of a state taken at a frame end, 20 frames in, with
/// a state taken straight after a load, which stands at VCOUNT 126, 41,000 cycles short of its
/// first vblank. A fresh load of `dma_rom`, whose GBAs are blocked by a DMA at their first frame
/// ends, which the join plays alone. A fresh load of `dma_a_rom`, which blocks them at a frame end
/// mid-session instead, with the cable already in. And a restore of two `normal_rom` states, which
/// hold the serial port's normal 8-bit mode rather than multiplayer mode.
type Start = (
    &'static str,
    PathBuf,
    Option<Vec<u8>>,
    fn(usize) -> ButtonMask,
);

fn every_start(dylib: &Path) -> Vec<Start> {
    let transfers = rom("mgba-link-starts.gba", transfer_rom());
    let blocked = rom("mgba-link-starts-dma.gba", dma_rom());
    let blocked_late = rom("mgba-link-starts-dma-a.gba", dma_a_rom());
    let normal = rom("mgba-link-starts-normal.gba", normal_rom());
    let end = single_state(dylib, &transfers, 20);
    let reset = single_state(dylib, &transfers, 0);
    let normal_end = single_state(dylib, &normal, 20);
    vec![
        ("a fresh load", transfers.clone(), None, shared_script),
        (
            "[end, end]",
            transfers.clone(),
            Some(slk1([&end, &end])),
            shared_script,
        ),
        (
            "[end, reset]",
            transfers.clone(),
            Some(slk1([&end, &reset])),
            shared_script,
        ),
        (
            "[reset, end]",
            transfers.clone(),
            Some(slk1([&reset, &end])),
            shared_script,
        ),
        (
            "[reset, reset]",
            transfers,
            Some(slk1([&reset, &reset])),
            shared_script,
        ),
        (
            "a fresh load blocked by a DMA",
            blocked,
            None,
            shared_script,
        ),
        (
            "a DMA started mid-session, on frame 3",
            blocked_late,
            None,
            dma_script,
        ),
        (
            "[end, end] in normal serial mode",
            normal,
            Some(slk1([&normal_end, &normal_end])),
            shared_script,
        ),
    ]
}

/// When a GBA's next frame ends on the cable's shared clock, read from its half of a link state.
/// The core state gives the GBA's own clock, masterCycles at 0x0c plus the CPU's cycles at 0x68,
/// and how far its video is from the next vblank: the video event's countdown at 0x1f4, VCOUNT at
/// 0x406, and DISPSTAT's hblank bit at 0x404, which says whether that countdown ends a line's
/// hdraw or its hblank. The lockstep driver's cycleOffset turns the GBA's clock into the shared
/// one. mGBA appends the driver's state as extdata after the 0x61000-byte core state: headers of
/// {u32 tag, i32 size, i64 offset} ending at tag 0, where tag 0x41 is a u32 driver id followed by
/// the driver's state, with cycleOffset 0x34 into it. The arithmetic wraps, as mGBA's clocks do.
fn next_frame_end(gba: &[u8]) -> u32 {
    let u32_at = |at: usize| u32::from_le_bytes(gba[at..at + 4].try_into().unwrap());
    let u16_at = |at: usize| u16::from_le_bytes(gba[at..at + 2].try_into().unwrap());
    let mut header = 0x61000;
    let driver = loop {
        let tag = u32_at(header);
        assert_ne!(tag, 0, "a GBA in a link state has no lockstep driver state");
        if tag == 0x41 {
            break u32_at(header + 8) as usize + 4;
        }
        header += 16;
    };
    let clock = u32_at(0x0c)
        .wrapping_add(u32_at(0x68))
        .wrapping_sub(u32_at(driver + 0x34));
    let lines = (159 + 228 - u32::from(u16_at(0x406))) % 228;
    let hblank = if u16_at(0x404) & 2 == 0 { 224 } else { 0 };
    clock
        .wrapping_add(u32_at(0x1f4))
        .wrapping_add(hblank + lines * 1232)
}

/// Two GBAs given the same buttons have to read each change on the same frame however a session
/// starts, or two games that wait on each other's input start a frame apart. A link state may pair
/// any two GBA states, so the cable goes in only once both GBAs stand at a frame end. Before it
/// waited for that, [end, reset] ended player 1's frames about 240,000 cycles before player 0's:
/// while player 0 slept waiting on player 1, at a transfer or a hard sync, player 1 ran on into
/// its next frame still holding the old buttons. And a GBA that reaches the frame end where the
/// cable syncs with its CPU blocked by a DMA runs on through that sync to its next vblank unless
/// the sync asks it to stop, which leaves player 1 a whole frame behind for the rest of the
/// session. Mario Kart: Super Circuit's boot reaches its first frame end that way, but the join
/// now plays that first frame alone, so `dma_a_rom` starts its DMA mid-session instead, with the
/// cable already in and the next hard sync more than a frame away. Every frame is compared, the
/// first too: a GBA restored mid-frame has finished that frame before the cable goes in.
#[test]
fn every_start_gives_both_gbas_the_same_buttons_on_the_same_frame() {
    let _g = common::core_lock();
    let Some(dylib) = vendored() else { return };

    let mut late = Vec::new();
    for (what, rom, container, buttons) in every_start(&dylib) {
        let pictures = linked_pictures(&dylib, &rom, container.as_deref(), 120, buttons);
        let differing = differing_frames(&pictures);
        if !differing.is_empty() {
            late.push(format!("{what}: frames {differing:?}"));
        }
    }
    assert!(
        late.is_empty(),
        "two GBAs given the same buttons painted different buttons: {late:#?}"
    );
}

/// The pictures cannot catch every wrong start. A pair whose player 1 ends each frame after player
/// 0's reads its buttons on time, but [reset, end] ran at under a third of the speed of the other
/// starts. So each start also has to leave the two GBAs' frames ending together, read off the link
/// state after 120 frames: each GBA's next frame end on the cable's shared clock has to be within
/// 256 cycles of the other's. Two GBAs whose frames end together can stand a few cycles apart,
/// since each frame ends on whichever instruction or event crosses its vblank, but only a few:
/// every start here has measured 0. The threshold is well inside a scanline's 1,232 cycles on
/// purpose. The join calls a GBA standing anywhere in the 1,008 cycles between its vblank and that
/// line's hblank "at a frame end" and joins it as it is, so a link state from before the join
/// waited for a frame end, or a run-on regression, could put the two GBAs that far apart while
/// still reading their buttons on time. Out of phase they stood about 240,000 cycles apart, and a
/// GBA a frame behind stands 280,896 cycles behind.
#[test]
fn every_start_joins_the_two_gbas_with_their_frames_ending_together() {
    let _g = common::core_lock();
    let Some(dylib) = vendored() else { return };

    let mut apart = Vec::new();
    for (what, rom, container, buttons) in every_start(&dylib) {
        let mut core = link_core(&dylib, 0);
        core.load(&rom).expect("link mode refused the rom");
        if let Some(container) = &container {
            core.unserialize(container)
                .expect("link mode refused the link state");
        }
        for frame in 0..120 {
            let keys = buttons(frame);
            core.run_frame_linked(keys, keys);
        }
        let [player_0, player_1] = split_slk1(&core.serialize().expect("no link state"));
        let gap = next_frame_end(&player_1).wrapping_sub(next_frame_end(&player_0)) as i32;
        if gap.unsigned_abs() >= 256 {
            apart.push(format!(
                "{what}: player 1's next frame ends {gap} cycles after player 0's"
            ));
        }
    }
    assert!(
        apart.is_empty(),
        "the two GBAs' frames do not end together: {apart:#?}"
    );
}

/// A restore has to be a function of the link state alone. Plan 2 resyncs a desynced pair, and a
/// player can reconnect or join late, so one SP restores the shared link state into cores that have
/// been running while the other restores it into freshly loaded ones. If the two come out
/// different, the devices have stopped computing the same machines from the restore on.
///
/// A GBA savestate does not carry everything a GBA holds. RCNT's SC, SD, SI and SO bits are put
/// back through `GBASIOWriteRCNT`, which keeps them from whatever the core held before. SIOMLT_SEND
/// is not a register mGBA saves at all, so a restored GBA sends whatever its core last had there.
/// `haltPending` and the idle-loop counters are not saved either. `probe_rom` shows the first two:
/// run with A held it fills SIOMLT_SEND with 0x2080 and its transfers set SC, and it never writes
/// SIOMLT_SEND again unless A is held, so a restore that carried the old value over sends it on the
/// cable and paints it. Both devices are run, and the same link state has to give one machine at
/// the restore and one machine ten frames on, whichever cores it landed in.
#[test]
fn a_restore_does_not_depend_on_what_the_cores_ran_before_it() {
    let _g = common::core_lock();
    let Some(dylib) = vendored() else { return };
    let rom = rom("mgba-link-probe.gba", probe_rom());

    // Taken with nothing held, so the state itself carries no SIOMLT_SEND worth sending.
    let alone = single_state(&dylib, &rom, 20);
    let container = slk1([&alone, &alone]);

    let mut seen = Vec::new();
    for player in [0u8, 1] {
        for ran_first in [false, true] {
            let mut core = link_core(&dylib, player);
            core.load(&rom).expect("link mode refused the rom");
            if ran_first {
                let a = ButtonMask(ButtonMask::A);
                for _ in 0..30 {
                    core.run_frame_linked(a, a);
                }
            }
            core.unserialize(&container)
                .expect("link mode refused the link state");
            let at_restore = fnv1a(&core.serialize().expect("no link state"));
            for _ in 0..10 {
                core.run_frame_linked(ButtonMask::default(), ButtonMask::default());
            }
            let ten_frames_on = fnv1a(&core.serialize().expect("no link state"));
            let cores = if ran_first {
                "cores that ran 30 linked frames first"
            } else {
                "freshly loaded cores"
            };
            seen.push((
                format!("player {player}'s device, {cores}"),
                at_restore,
                ten_frames_on,
            ));
        }
    }

    let (_, want_at_restore, want_ten_on) = &seen[0];
    let differing: Vec<String> = seen
        .iter()
        .filter(|(_, at_restore, ten_on)| at_restore != want_at_restore || ten_on != want_ten_on)
        .map(|(what, at_restore, ten_on)| {
            format!("{what}: {at_restore:016x} at the restore, {ten_on:016x} ten frames on")
        })
        .collect();
    assert!(
        differing.is_empty(),
        "the same link state computed different machines depending on what the cores ran before \
         it. Wanted {want_at_restore:016x} then {want_ten_on:016x}, as {} gave: {differing:#?}",
        seen[0].0
    );
}

/// Mario Kart: Super Circuit's scripted walk from the title screen into a linked two-player race.
/// It is DOWN and A at the title, then A for 3 frames every 30 from frame 4300, which carries both
/// players through the menus into the race. The spike that proved link mode on the SP ran exactly
/// this script.
fn race_script(frame: usize) -> ButtonMask {
    let mut keys = 0;
    if (1500..=1506).contains(&frame) {
        keys |= ButtonMask::DOWN;
    }
    if (1560..=1566).contains(&frame) || (frame >= 4300 && (frame - 4300) % 30 <= 3) {
        keys |= ButtonMask::A;
    }
    ButtonMask(keys)
}

/// A picture a person can open, to see where the scripted walk got to.
fn write_ppm(path: &Path, xrgb: &[u8]) {
    let mut out = format!("P6\n{GBA_W} {GBA_H}\n255\n").into_bytes();
    for pixel in xrgb.chunks(4) {
        out.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
    }
    std::fs::write(path, out).expect("write picture");
}

/// Needs a Mario Kart: Super Circuit ROM, which is not in the tree. With `SLOT_MKSC_ROM` set to
/// it, `cargo test --release -p slot --test mgba_link mario_kart -- --ignored` runs the Mario Kart
/// tests.
/// 25,000 frames covers the menus, the link handshake and minutes of racing. Each device's last
/// picture lands in the test's temp directory as a PPM.
#[test]
#[ignore]
fn mario_kart_super_circuit_is_the_same_race_on_both_devices() {
    let _g = common::core_lock();
    let dylib = common::vendored_core().expect("no vendored mGBA core: run `task core`");
    let rom = PathBuf::from(
        std::env::var_os("SLOT_MKSC_ROM")
            .expect("set SLOT_MKSC_ROM to a Mario Kart: Super Circuit ROM"),
    );

    let mut hashes = Vec::new();
    for player in [0u8, 1] {
        let mut core = link_core(&dylib, player);
        core.load(&rom).expect("link mode refused Mario Kart");
        let started = std::time::Instant::now();
        for frame in 1..=25_000 {
            let keys = race_script(frame);
            core.run_frame_linked(keys, keys);
        }
        let secs = started.elapsed().as_secs_f64();
        eprintln!(
            "mgba_link_player={player}: 25000 frame pairs in {secs:.1} s, {:.0} pairs/s",
            25_000.0 / secs
        );
        let picture =
            Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("mksc-player{player}.ppm"));
        write_ppm(&picture, core.video_xrgb8888());
        eprintln!("last picture: {}", picture.display());
        hashes.push(fnv1a(&core.serialize().expect("no link state")));
    }
    assert_eq!(
        hashes[0], hashes[1],
        "player 0's and player 1's devices computed different races"
    );
}

/// The Mario Kart walk, started the way every link session starts: both players' GBAs restored
/// from a one-GBA state taken at the title screen (frame 1400), then linked from there. It has to
/// race exactly as a reset boot does. At frame 25,000, each device shows the very picture a reset
/// boot's shows, mid-race, and both devices compute the same machines. Before the cable synced at
/// player 0's frame end, player 0 read the lobby's A press a frame late here, and the pair stalled
/// at character select behind a "WAIT" box. Player 1's picture only matches since a reset boot
/// stopped leaving player 1 a frame behind: its boot reaches its first frame end inside a DMA,
/// and the cable's sync there used to carry player 0 through a second frame. The states are not
/// compared: a one-GBA state's clock is not a linked boot's, so neither GBA's state can match the
/// boot's byte for byte. It needs the ROM too, and runs the way the test above says.
#[test]
#[ignore]
fn mario_kart_super_circuit_races_linked_after_a_restore() {
    let _g = common::core_lock();
    let dylib = common::vendored_core().expect("no vendored mGBA core: run `task core`");
    let rom = PathBuf::from(
        std::env::var_os("SLOT_MKSC_ROM")
            .expect("set SLOT_MKSC_ROM to a Mario Kart: Super Circuit ROM"),
    );

    let raced: Vec<Vec<u8>> = [0u8, 1]
        .into_iter()
        .map(|player| {
            let mut boot = link_core(&dylib, player);
            boot.load(&rom).expect("link mode refused Mario Kart");
            for frame in 1..=25_000 {
                let keys = race_script(frame);
                boot.run_frame_linked(keys, keys);
            }
            boot.video_xrgb8888().to_vec()
        })
        .collect();

    let mut single = single_core(&dylib);
    single.load(&rom).expect("mGBA refused Mario Kart");
    for frame in 1..=1400 {
        single.run_frame(race_script(frame));
    }
    let title = single.serialize().expect("no state");
    drop(single);
    let container = slk1([&title, &title]);

    let mut hashes = Vec::new();
    let mut pictures = Vec::new();
    for player in [0u8, 1] {
        let mut core = link_core(&dylib, player);
        core.load(&rom).expect("link mode refused Mario Kart");
        core.unserialize(&container)
            .expect("link mode refused the title-screen link state");
        let started = std::time::Instant::now();
        for frame in 1401..=25_000 {
            let keys = race_script(frame);
            core.run_frame_linked(keys, keys);
        }
        let secs = started.elapsed().as_secs_f64();
        eprintln!(
            "restored, mgba_link_player={player}: 23600 frame pairs in {secs:.1} s, {:.0} pairs/s",
            23_600.0 / secs
        );
        let picture = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("mksc-restored-player{player}.ppm"));
        write_ppm(&picture, core.video_xrgb8888());
        eprintln!("last picture: {}", picture.display());
        pictures.push(core.video_xrgb8888().to_vec());
        hashes.push(fnv1a(&core.serialize().expect("no link state")));
    }
    assert_eq!(
        hashes[0], hashes[1],
        "player 0's and player 1's devices computed different machines after a restore"
    );
    for (player, (restored, booted)) in pictures.iter().zip(&raced).enumerate() {
        assert!(
            restored == booted,
            "restored at the title screen, player {player}'s device is not showing the race a \
             reset boot shows at frame 25,000"
        );
    }
}

/// Every link session starts by restoring two one-GBA states, so the cable has to be plugged in
/// after a restore as well as after a fresh load. mGBA sets SIOCNT's multiplayer id from the cable
/// each time the game writes it, so player 1's GBA paints something different from player 0's, and
/// from a lone GBA's, only while a cable joins it to player 0's. The test asserts "different" rather
/// than exact bits because mGBA ORs SIOCNT's old bits back in: a restored state keeps the slave bit
/// it had while it ran alone.
#[test]
fn the_cable_is_plugged_in_after_a_load_and_after_a_restore() {
    let _g = common::core_lock();
    let Some(dylib) = vendored() else { return };
    let rom = rom("mgba-link-sio.gba", sio_rom());

    let alone = single_picture(&dylib, &rom, 0, 30);

    let pictures = |container: Option<&[u8]>| -> Vec<Vec<u8>> {
        (0..2u8)
            .map(|player| {
                let mut core = link_core(&dylib, player);
                core.load(&rom).expect("link mode refused the rom");
                if let Some(container) = container {
                    core.unserialize(container)
                        .expect("link mode refused the link state");
                }
                for _ in 0..30 {
                    core.run_frame_linked(ButtonMask::default(), ButtonMask::default());
                }
                core.video_xrgb8888().to_vec()
            })
            .collect()
    };

    let loaded = pictures(None);
    assert!(
        loaded[1] != loaded[0],
        "after a load, player 1's GBA read the same SIOCNT as player 0's: no cable"
    );
    assert!(
        loaded[1] != alone,
        "after a load, player 1's GBA read what a lone GBA reads: no cable"
    );

    let mut single = single_core(&dylib);
    single.load(&rom).expect("load");
    for _ in 0..30 {
        single.run_frame(ButtonMask::default());
    }
    let state = single.serialize().expect("no state");
    drop(single);

    let restored = pictures(Some(&slk1([&state, &state])));
    assert!(
        restored[1] != restored[0],
        "after a restore, player 1's GBA read the same SIOCNT as player 0's: no cable"
    );
    assert!(
        restored[1] != alone,
        "after a restore, player 1's GBA read what a lone GBA reads: no cable"
    );
}

/// The session's own state swap, which is how two devices come to simulate the same machine: a
/// link-mode core serializes its pair, and another link-mode core loads it. On hardware this came
/// back as `unserialize refused` with the whole 1,057,876 bytes across, so the question is whether
/// the container is at fault or the core that received it was never in link mode.
#[test]
fn a_link_states_travels_between_two_link_mode_cores() {
    let _g = common::core_lock();
    let Some(dylib) = vendored() else { return };
    let rom = rom("mgba-link-multiplayer.gba", multiplayer_rom());

    let mut host = link_core(&dylib, 0);
    host.load(&rom).expect("link mode refused the rom");
    let state = host.serialize().expect("the host would not serialize");
    assert!(
        state.starts_with(b"SLK1"),
        "the host produced something that is not a link state"
    );
    // One libretro core to a process, so the host goes before the joiner arrives.
    drop(host);

    let mut joiner = link_core(&dylib, 1);
    joiner.load(&rom).expect("link mode refused the rom");
    joiner
        .unserialize(&state)
        .expect("a link-mode core refused a link state from its own build");
}

/// And the failure the device saw, reproduced deliberately: a core that is *not* in link mode
/// cannot take a link state, because it is expecting one GBA's worth and this is two.
#[test]
fn a_single_core_refuses_a_link_state() {
    let _g = common::core_lock();
    let Some(dylib) = vendored() else { return };
    let rom = rom("mgba-link-multiplayer.gba", multiplayer_rom());

    let mut host = link_core(&dylib, 0);
    host.load(&rom).expect("link mode refused the rom");
    let state = host.serialize().expect("the host would not serialize");
    drop(host);

    let mut single = single_core(&dylib);
    single.load(&rom).expect("load");
    assert!(
        single.unserialize(&state).is_err(),
        "a single GBA took a state holding two"
    );
}

/// The swap against a real commercial cart with a real save, which is where it failed on
/// hardware. `a_link_states_travels_between_two_link_mode_cores` proves the same thing on a
/// synthetic rom; this proves the cart and its save are not what made the difference. The BIOS
/// was, and `apply_link_options` pins it now.
#[test]
fn the_card_cart_link_state_travels_between_two_link_mode_cores() {
    let _g = common::core_lock();
    let Some(dylib) = vendored() else { return };
    let card = common::repo_root().join("sdcard/Games/GBA/Advance Wars.gba");
    let sav = common::repo_root().join("sdcard/Saves/GBA/Advance Wars.sav");
    if !card.exists() {
        eprintln!("no card cart on this machine, skipping");
        return;
    }
    let save = std::fs::read(&sav).ok();

    let mut host = link_core(&dylib, 0);
    host.load(&card).expect("link mode refused the cart");
    if let Some(s) = &save {
        host.load_save_ram(s)
            .expect("the host refused its own save");
    }
    let state = host.serialize().expect("the host would not serialize");
    eprintln!("host state {} bytes", state.len());
    drop(host);

    let mut joiner = link_core(&dylib, 1);
    joiner.load(&card).expect("link mode refused the cart");
    if let Some(s) = &save {
        joiner
            .load_save_ram(s)
            .expect("the joiner refused its own save");
    }
    joiner
        .unserialize(&state)
        .expect("the joiner refused the host's link state");
}

/// Each device skips both the picture and the sound of the console it never shows, and the two
/// devices skip *different* consoles. That is only safe if skipping cannot change the machine, so
/// this asks the machine. Identical inputs from both sides have to leave byte-identical state, or
/// two SPs would drift apart the moment a race started and no test below this one would notice.
///
/// It guards the audio skip as much as the video one: the mixing deliberately still runs, because
/// `GBAAudioSerialize` carries `chA.samples` and `chB.samples`, and only the write into an output
/// ring nobody drains is dropped. If that line ever moved to cover the mixing, this fails.
#[test]
fn skipping_the_peers_picture_and_sound_does_not_change_the_machine() {
    let _g = common::core_lock();
    let Some(dylib) = vendored() else { return };
    let rom = rom("mgba-link-multiplayer.gba", multiplayer_rom());
    let script = [
        ButtonMask(ButtonMask::A),
        ButtonMask(ButtonMask::B),
        ButtonMask::default(),
        ButtonMask(ButtonMask::A | ButtonMask::B),
    ];

    let run = |player: u8| {
        let mut core = link_core(&dylib, player);
        core.load(&rom).expect("link mode refused the rom");
        for i in 0..240 {
            core.run_frame_linked(script[i % script.len()], script[(i + 1) % script.len()]);
        }
        core.serialize().expect("no link state")
    };

    let as_host = run(0);
    let as_joiner = run(1);
    assert_eq!(
        as_host.len(),
        as_joiner.len(),
        "the two ends produced link states of different sizes"
    );
    assert_eq!(
        as_host, as_joiner,
        "the two ends ran the same inputs to different machines: skipping the peer's picture is \
         not state-safe, and two devices would drift apart in a race"
    );
}

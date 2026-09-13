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

/// The mirror of the test above, and a problem the frame-end sync does not reach, so it is
/// ignored: it fails. When player 1's frames end before player 0's, player 1 finishes first. Then
/// player 0 sleeps waiting on player 1, at the end of a transfer or at the cable's periodic hard
/// sync, and the cable needs player 1 to reach that moment. So player 1 runs on past the end of
/// its frame, still holding that frame's buttons, and reads them where it should read the next
/// frame's. Player 1's GBA here starts from a state taken straight after a load, 41,888 cycles
/// short of its first vblank, so its frames end 239,008 cycles before player 0's. Player 0's
/// transfer ends 63,427 cycles into its frame, about 21,500 after player 1's frame ended. A reset
/// boot of Mario Kart: Super Circuit runs its GBAs out of phase the same way. Found 2026-09-15: the
/// pictures differ from frame 10, and on the same frames before the frame-end sync was added.
/// Without transfers, the hard sync alone makes some reads late too. A session started from two
/// states taken at a frame's end runs in phase, and never gets here. The first frame is left out:
/// player 1's GBA has painted nothing by the end of it.
#[test]
#[ignore]
fn player_1_reads_the_same_buttons_on_the_same_frame_while_player_0_waits_on_it() {
    let _g = common::core_lock();
    let Some(dylib) = vendored() else { return };
    let rom = rom("mgba-link-transfers.gba", transfer_rom());

    let frame_end = single_state(&dylib, &rom, 20);
    let loaded = single_state(&dylib, &rom, 0);
    let pictures = linked_pictures(
        &dylib,
        &rom,
        Some(&slk1([&frame_end, &loaded])),
        120,
        shared_script,
    );
    let differing: Vec<usize> = differing_frames(&pictures)
        .into_iter()
        .filter(|&frame| frame > 0)
        .collect();
    assert!(
        differing.is_empty(),
        "player 1's GBA read the buttons on a different frame from player 0's, first on frame {:?} \
         (all: {differing:?})",
        differing.first()
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
/// it, `cargo test --release -p slot --test mgba_link mario_kart -- --ignored` runs both Mario
/// Kart tests and leaves out the ignored test that fails on purpose.
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
/// race exactly as a reset boot does. At frame 25,000, player 0's device shows the very picture
/// a reset boot's shows, mid-race, and both devices compute the same machines. Before the cable
/// synced at player 0's frame end, player 0 read the lobby's A press a frame late here, and the
/// pair stalled at character select behind a "WAIT" box. Only player 0's picture is compared. A
/// reset boot ends player 1's frames 204,248 cycles before player 0's, while a restore of two
/// frame-end states ends them together, so player 1 draws what it has been sent at a different
/// moment. And a one-GBA state's clock is not a linked boot's, so neither GBA's state can match
/// the boot's byte for byte. It needs the ROM too, and runs the way the test above says.
#[test]
#[ignore]
fn mario_kart_super_circuit_races_linked_after_a_restore() {
    let _g = common::core_lock();
    let dylib = common::vendored_core().expect("no vendored mGBA core: run `task core`");
    let rom = PathBuf::from(
        std::env::var_os("SLOT_MKSC_ROM")
            .expect("set SLOT_MKSC_ROM to a Mario Kart: Super Circuit ROM"),
    );

    let mut boot = link_core(&dylib, 0);
    boot.load(&rom).expect("link mode refused Mario Kart");
    for frame in 1..=25_000 {
        let keys = race_script(frame);
        boot.run_frame_linked(keys, keys);
    }
    let raced = boot.video_xrgb8888().to_vec();
    drop(boot);

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
    assert!(
        pictures[0] == raced,
        "restored at the title screen, player 0's device is not showing the race a reset boot \
         shows at frame 25,000"
    );
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

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
        "{} bytes after player 2's state",
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

/// Player 1 holds A on port 0 and player 2 holds B on port 1. Each device must show its own
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
/// refused before either GBA is touched.
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
        ("bytes after player 2", trailing.as_slice()),
        ("the magic alone", b"SLK1".as_slice()),
        ("player 2 missing", &good[..8 + player_0.len()]),
        ("two empty states", empty.as_slice()),
        ("a player 2 state too short to be one", short.as_slice()),
    ] {
        assert!(core.unserialize(bytes).is_err(), "link mode took {what}");
    }
    core.unserialize(&good)
        .expect("link mode refused its own link state");
}

/// A restore the core refuses part way leaves both GBAs where they were. Player 1's state here
/// is sound, but player 2's claims a savestate version from the future, which the core refuses
/// only once it is already loading. Stopping there would leave player 1 restored and player 2
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
        "a refused restore left player 1's GBA restored instead of where it was"
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
    let p1 = if frame.is_multiple_of(3) {
        ButtonMask::A
    } else {
        ButtonMask::RIGHT
    };
    let p2 = if frame % 5 < 2 {
        ButtonMask::B | ButtonMask::L
    } else {
        0
    };
    (ButtonMask(p1), ButtonMask(p2))
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
            let (p1, p2) = script(frame);
            core.run_frame_linked(p1, p2);
        }
        hashes.push(fnv1a(&core.serialize().expect("no link state")));
    }
    assert_eq!(
        hashes[0], hashes[2],
        "the same device computed two different machines"
    );
    assert_eq!(
        hashes[0], hashes[1],
        "player 1's and player 2's devices computed different machines"
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

/// Needs a Mario Kart: Super Circuit ROM, which is not in the tree:
/// `SLOT_MKSC_ROM=/path/to/mksc.gba cargo test --release -p slot --test mgba_link -- --ignored`.
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
        "player 1's and player 2's devices computed different races"
    );
}

//! mGBA's link mode: two GBAs from one game, joined by mGBA's own lockstep link cable, stepped
//! together on one thread, with only the local player's GBA shown. Real core only: every test
//! skips when `vendor/mgba_libretro.dylib` is absent, and fails with a rebuild hint when the
//! vendored core predates link mode.

mod common;

use std::path::{Path, PathBuf};

use slot_retro::{ButtonMask, LibretroCore, RetroCore};

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

    for (what, bytes) in [
        ("a one-GBA state", player_0.as_slice()),
        ("a different magic", wrong_magic.as_slice()),
        ("a length past the end", long_length.as_slice()),
        ("bytes after player 2", trailing.as_slice()),
        ("the magic alone", b"SLK1".as_slice()),
        ("player 2 missing", &good[..8 + player_0.len()]),
    ] {
        assert!(core.unserialize(bytes).is_err(), "link mode took {what}");
    }
    core.unserialize(&good)
        .expect("link mode refused its own link state");
}

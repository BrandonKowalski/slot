//! Every option `apply_core_options` sets, checked against what the core itself declares.
//!
//! This is the test that could not be written before. `LibretroCore::set_option` writes into
//! the frontend's own map and `option` reads back out of it, so a misspelt key round trips
//! perfectly and reaches the core never — which meant nothing in the tree could tell a correct
//! option key from a typo, and three wrong assumptions about core options got through review on
//! the strength of that. `mgba_sgb_borders` had no test at all, deliberately, because the test
//! anyone would have written would have passed with the key misspelt; the `mgba_frameskip` test
//! had the same hole and nobody had noticed; and gpSP was said to have no colour correction
//! option when it has one.
//!
//! `LibretroCore::declared_options` now records what the core sends at `SET_VARIABLES`, so this
//! asks the core rather than the frontend. Nothing here names an option key: it reads back
//! whatever `apply_core_options` set and crosses it against the declaration, which is the only
//! shape of this test a typo cannot survive — one that spelled the keys out would spell them
//! wrong in exactly the same way the code does.
//!
//! Skipped on a machine that has not fetched a core, the same way every other test here that
//! needs a real dylib is.

mod common;

use slot::link_kind::{serial_option, LinkKind};
use slot_retro::LibretroCore;
use slot_store::Core;

fn dylib_for(core: Core) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor")
        .join(slot::core::dylib_name(core))
}

/// Every `gpsp_serial` the product can actually load a cart with, taken from `serial_option`
/// itself rather than from a list copied out of it — a fifth mode added there has to be a mode
/// gpSP declares, and the only way this test notices is by asking the same function.
///
/// The codes and titles are the ones that reach each arm: a Pokémon header, Advance Wars 1 and
/// 2, and a cart that is none of them.
fn every_serial_mode() -> Vec<&'static str> {
    let carts = [
        ("BPEE", "POKEMON EMER"),
        ("AWRE", "ADVANCEWARS"),
        ("AW2E", "ADVANCE WARS2"),
        ("AMFE", "METROID4"),
    ];
    let mut modes = Vec::new();
    for (code, title) in carts {
        for auto in [LinkKind::Cable, LinkKind::Wireless] {
            for chosen in [LinkKind::Cable, LinkKind::Wireless] {
                let mode = serial_option(chosen, auto, code, title);
                if !modes.contains(&mode) {
                    modes.push(mode);
                }
            }
        }
    }
    modes
}

/// Assert that every option now set on `core` is one the core declared, at a value it declared
/// for that key. `what` says which combination produced it, so a failure names the call rather
/// than only the key.
fn every_option_is_one_the_core_has(core: &LibretroCore, what: &str) {
    let declared = core.declared_options();
    assert!(
        !declared.is_empty(),
        "{what}: the core declared no options at all, so this test would pass on anything"
    );
    let set = core.options();
    assert!(!set.is_empty(), "{what}: no options were set");
    for (key, value) in set {
        let Some(values) = declared.get(&key) else {
            let mut known: Vec<_> = declared.keys().cloned().collect();
            known.sort();
            panic!("{what}: the core declares no option {key:?}. It declares: {known:?}");
        };
        assert!(
            values.contains(&value),
            "{what}: the core declares {key:?} as {values:?}, and slot set it to {value:?}"
        );
    }
}

/// Every combination of the three things `apply_core_options` branches on, against both cores.
///
/// Run as one test rather than one per core because `core_lock` serialises them anyway — a
/// libretro core keeps its machine in dylib globals, so two live at once is not a state any
/// test here may reach.
#[test]
fn every_option_slot_sets_is_one_the_core_declares() {
    let _g = common::core_lock();
    let mut ran = 0;
    for which in Core::ALL {
        let path = dylib_for(which);
        if !path.exists() {
            eprintln!("no {} dylib on this host, skipping", which.as_str());
            continue;
        }
        for serial in every_serial_mode() {
            for bios in [false, true] {
                for colour in [false, true] {
                    let mut core = LibretroCore::open(&path).expect("open core");
                    slot::core::apply_core_options(&mut core, which, serial, bios, colour);
                    every_option_is_one_the_core_has(
                        &core,
                        &format!(
                            "{} serial={serial} bios={bios} colour={colour}",
                            which.as_str()
                        ),
                    );
                    ran += 1;
                }
            }
        }
    }
    if ran == 0 {
        eprintln!("no cores on this host, nothing was checked");
    }
}

/// The half that the test above cannot see: a key that is simply never set is not a key the
/// core does not have. Both cores declare an option for colour correction — the thing that was
/// asserted false about gpSP for a while — and both are told about it, so the quick menu's row
/// is not silently a mGBA-only row.
///
/// Named here rather than derived, deliberately, and it is not the hole the module comment
/// describes: the assertion is that the core *declares* these, which is the core's own word.
/// A typo in this test fails it instead of passing it.
#[test]
fn both_cores_declare_a_colour_correction_option() {
    let _g = common::core_lock();
    for (which, key) in [
        (Core::Mgba, "mgba_color_correction"),
        (Core::Gpsp, "gpsp_color_correction"),
    ] {
        let path = dylib_for(which);
        if !path.exists() {
            eprintln!("no {} dylib on this host, skipping", which.as_str());
            continue;
        }
        let core = LibretroCore::open(&path).expect("open core");
        assert!(
            core.declared_options().contains_key(key),
            "{} declares no {key}",
            which.as_str()
        );
    }
}

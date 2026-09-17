use std::path::{Path, PathBuf};

use slot_retro::{LibretroCore, MockCore, RetroCore};
use slot_store::Core;

use crate::root;

/// Names the dylib outright, for a build that keeps it somewhere the search does not look.
///
/// This wins over `System/selected_core.ini` for which dylib loads — it stays a developer
/// escape hatch, not a second way to pick a core. It is not silent about that: it does not
/// touch which `Core` a cart resolves to (still the ini, still what the core half of
/// `States/<platform>/<core>/` is named after), only which file `open_core` opens. Pointing
/// this at gpSP for a cart the ini never mentions runs gpSP with its states filed under that
/// platform's `mgba` directory — correct for a developer who set the override on purpose, a
/// trap for anyone who forgot it was set.
const CORE_ENV: &str = "SLOT_CORE";

/// Which dylib backs a core. The device keeps both in `System/`, so this is a filename
/// rather than a search: whichever the cart asked for is either there or it is not.
pub fn dylib_name(core: Core) -> String {
    format!(
        "{}_libretro.{}",
        core.as_str(),
        std::env::consts::DLL_EXTENSION
    )
}

/// Most specific first: the environment, then the content root's own `System/` — where the
/// device actually keeps a core, and, on the device, also where the binary itself lives, so
/// the next candidate coincides with this one there. Off the device — a host build, or a
/// test with its own tmp root — the binary's directory and `root` are different places, and
/// only searching the root's own `System/` gives a cart's own content root somewhere a test
/// can plant a dylib under. The `vendor` directory `scripts/fetch-core.sh` writes into is
/// last: a host development convenience, not anywhere a shipped device looks.
fn candidates(root: &Path, core: Core) -> Vec<PathBuf> {
    if let Some(named) = std::env::var_os(CORE_ENV) {
        return vec![PathBuf::from(named)];
    }
    // Spelled from the platform's own convention so the same search finds the device's `.so`.
    let name = dylib_name(core);
    let mut paths = vec![root.join("System").join(&name)];
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        paths.push(dir.join(&name));
        paths.push(dir.join("vendor").join(&name));
    }
    paths.push(Path::new("vendor").join(&name));
    paths
}

/// What `open_core` opened: the core to run, and whether it is the one that was asked for.
///
/// The second half is not a line for the log. A card whose dylib is missing runs the mock, and
/// the mock refuses every state it did not write itself — so a refusal from it says the
/// emulator is absent, not that the state on the card is bad. Anything that acts on a refusal
/// has to be able to tell those two apart, and this is the only moment either is knowable:
/// `Box<dyn RetroCore>` is the same type whichever opened. `App::set_named_core` is where the
/// answer goes, and `App::retire_refused_resume` is what it protects.
pub struct Opened {
    pub core: Box<dyn RetroCore>,
    /// `true` when one of the candidate dylibs really opened, `false` when every one of them
    /// was missing or would not load and `MockCore` is standing in for it.
    pub named: bool,
}

/// The one place a cart's `Core` becomes a dylib path. Callers that already know which core
/// they want — `session.rs` resolves it once per insert — pass it straight through, which is
/// what keeps this call from disagreeing with the caller's own choice. It says nothing about
/// what the caller does with that `Core` afterward: `App` is the one that has to keep using
/// the same value for every later read and write, and it does that by storing it rather than
/// asking again.
///
/// `serial` is the `gpsp_serial` a gpSP core loads with, and `colour` the quick menu's Colour
/// Correction. See `apply_core_options`.
///
/// The `named` half of what comes back is what lets a caller tell "the real core refused this
/// state" from "there was no real core to ask": the mock refuses everything, so without it a
/// missing dylib looks exactly like a state the core rejected — and retiring a save on that
/// evidence would lose it to a file that simply was not there.
pub fn open_core(root: &Path, core: Core, serial: &str, colour: bool) -> Opened {
    let paths = candidates(root, core);
    match open_named(root, core, serial, colour, &paths) {
        Some(core) => Opened { core, named: true },
        None => {
            report_missing(core, &paths);
            Opened {
                core: Box::new(MockCore::new()),
                named: false,
            }
        }
    }
}

/// The named core if one of these opens, the mock if none of them do. A missing core is not
/// a failure to boot: the shelf, the slot and every gesture are reachable either way.
///
/// The core is told the content root's own folders, never the dylib's: on the device the
/// core lives in `System/` and the user's BIOS does not.
///
/// `core` is redundant with `paths` in production — `open_core` derived both from the same
/// `Core` — but this function stays the seam that takes `paths` explicitly, because tests
/// plant a dylib somewhere `candidates` would not otherwise look. `apply_core_options` needs
/// `core` too, and only `open_core_for` ever holds a concrete `slot_retro::LibretroCore` to
/// call it on: everything above here deals in `Box<dyn RetroCore>`, which has no `set_option`.
/// That is also why the call sits here rather than at a caller — after `open_with` succeeds,
/// before the `Box<dyn RetroCore>` is handed back and `load` becomes reachable at all. `serial`
/// and `colour` go the same way, for the same reason.
pub fn open_core_for(
    root: &Path,
    core: Core,
    serial: &str,
    colour: bool,
    paths: &[PathBuf],
) -> Box<dyn RetroCore> {
    open_named(root, core, serial, colour, paths).unwrap_or_else(|| {
        report_missing(core, paths);
        Box::new(MockCore::new())
    })
}

/// The search itself, with no fallback of its own: `None` means every candidate was missing or
/// would not load. Split out so `open_core` can say which of the two happened without deriving
/// it a second time — a stat of the candidate paths from outside would be a second opinion, and
/// free to disagree with this one about a dylib that exists but will not open.
fn open_named(
    root: &Path,
    core: Core,
    serial: &str,
    colour: bool,
    paths: &[PathBuf],
) -> Option<Box<dyn RetroCore>> {
    let bios = root::bios_dir(root);
    let saves = root::saves_dir(root);
    for path in paths {
        if !path.exists() {
            continue;
        }
        match LibretroCore::open_with(path, &bios, &saves) {
            Ok(mut opened) => {
                apply_core_options(&mut opened, core, serial, root::has_real_bios(root), colour);
                eprintln!("slot: core {}", path.display());
                return Some(Box::new(opened));
            }
            Err(e) => eprintln!("slot: {}: {e}", path.display()),
        }
    }
    None
}

/// Name the core and every path that was tried. The mock renders a rainbow test pattern and a
/// sine tone, which on screen reads as "this core is broken" rather than "this core is
/// missing" — the one time that happened it cost an afternoon, so the log says which file was
/// wanted and where it was looked for.
fn report_missing(core: Core, paths: &[PathBuf]) {
    eprintln!(
        "slot: no {} core found, running the mock test pattern instead. Looked in: {}",
        core.as_str(),
        paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
}

/// Options a core needs before `load`, because libretro cores read them during
/// `retro_load_game` rather than continuously.
///
/// `serial` is gpSP's link mode, and loading a game is the only time gpSP reads it: a reset
/// does not, and a save state does not carry it. `auto` resolves the protocol from the ROM, so
/// two devices running the same game agree on a mode without either being told which, and it
/// is what every cart loads with until its link screen is switched to the other hardware (see
/// `link_kind::serial_option`). mGBA gets none of gpSP's options: it has no `gpsp_serial`, and
/// handing it one anyway is a landmine the day it grows one. It does get a frameskip option,
/// below, but under its own prefix — neither core reads the other's — and one of its own that
/// gpSP has no equivalent for, `mgba_sgb_borders`, because mGBA is the core that runs a Game Boy
/// cart and a Super Game Boy border is the one thing it can draw that will not fit the panel.
///
/// `bios` is whether the card carries a real BIOS (`root::has_real_bios`), and it buys the
/// player the boot logo and chime. gpSP defaults to `game`, which drops straight into the
/// cart; `bios` runs the BIOS first. It is only set when the file is really there, because
/// booting through gpSP's built-in replacement instead spends those seconds on a blank screen
/// — a pause that reads as a hang rather than as the hardware starting up.
///
/// `gpsp_bios` is deliberately left alone. Its default, `auto`, already loads
/// `<system>/gba_bios.bin` and uses it whenever the image passes gpSP's own first-byte test,
/// which is the same test `has_real_bios` applies — so naming `official` would select the
/// identical image. All it would change is the failure path, where `official` puts a warning
/// on screen through the core's OSD ("Could not load BIOS image file", "BIOS image seems
/// incorrect") before falling back to exactly the built-in BIOS `auto` falls back to silently.
/// That is a core-drawn message over slot's own chrome, bought for no change in behaviour.
///
/// Setting this on every load, rather than only on a fresh start, is safe because a resume
/// does not survive to be seen: `emu::Worker::run` unserializes the resume state after `load`
/// and before it publishes a single frame, so the restored machine replaces the BIOS's before
/// anything reaches the screen. That covers a reload for a link too — `session::reload_for_link`
/// flushes and resumes through the same path.
///
/// `colour` is the quick menu's Colour Correction, and unlike everything else here it is set on
/// both cores, because both of them have the option — which is worth stating outright, since it
/// was assumed for a while that only mGBA did. Each spells it its own way; see below.
pub fn apply_core_options(
    core: &mut LibretroCore,
    which: Core,
    serial: &str,
    bios: bool,
    colour: bool,
) {
    // Auto frameskip, on whichever core this is, for the whole session. Nothing is skipped by
    // merely turning it on: both cores only skip a frame when the frontend says its audio
    // buffer is about to run dry, and `RetroCore::set_frame_skip` is the one thing that ever
    // says so — once per frame of a fast forward present, for every frame but the one that
    // will be shown. At normal speed the answer is always no and every frame draws.
    //
    // Set here with the rest, rather than switched on and off around a fast forward, because
    // a libretro core reads its options during `retro_load_game`; changing this one later
    // would mean re-entering `set_option`, whose doc comment spells out the aliasing that
    // invites. The option is the standing arrangement; the callback is the per-frame lever.
    //
    // The key is the core's own name with `_frameskip` after it, which is how mGBA and gpSP
    // both spell it. TGB Dual has no frameskip option at all: its whole option list is the six
    // at `libretro/libretro.cpp:19-28`, all of them about the two screens and which one is
    // heard. Setting a key a core does not have would be ignored rather than harmful, but it
    // would also be a quiet lie in a function whose whole job is to say what each core was
    // told, so it is not set. The consequence is that a Game Boy cart's fast forward has no
    // core frameskip behind it and is capped by emulation speed alone, which for a core that
    // costs 2 ms of a 16.7 ms frame is not the binding constraint.
    if matches!(which, Core::Mgba | Core::Gpsp) {
        core.set_option(&format!("{}_frameskip", which.as_str()), "auto");
    }
    if which == Core::Mgba {
        // An SGB border makes the picture 256x224, and 256 is wider than the 240 this whole path
        // is built on — `video_refresh` would crop it. The core declares this one as `ON|OFF`
        // and its default is `ON`, so this is not merely belt and braces; and it is set
        // explicitly rather than counted on staying that way, so a default that changes under us
        // cannot turn a working screen into a cropped one either.
        //
        // `mgba_gb_model` is deliberately left alone. Its default is `Autodetect`, which is each
        // cart's own header read honestly, and naming a model here would override what the cart
        // says about itself. Nothing sets `mgba_use_bios` or `mgba_skip_bios`.
        core.set_option("mgba_sgb_borders", "OFF");
        // The next two put back what a Game Boy Advance SP showed when an original monochrome
        // Game Boy cartridge was pushed into it. The SP has no monochrome mode to fall back on:
        // a Game Boy cart runs through the Game Boy Color compatibility the AGB inherits, and
        // that boot ROM colourises the cart before handing it control. It colourises from a
        // table built into the ROM and keyed on the cartridge's own header — the licensee code
        // has to be Nintendo's `01`, and a checksum of the sixteen title bytes then selects one
        // of ninety-four palette assignments, with the fourth title letter breaking the ties.
        // A cart that misses, for either reason, is given the table's entry zero. So grayscale,
        // which is what mGBA does when left alone, is the one thing no hardware here ever put on
        // a screen: a Game Boy's own panel was green, and every machine that could still take
        // its cartridges afterwards coloured it. Two options rather than one because the
        // hardware behaviour has two halves, and setting either alone reproduces half of it.
        //
        // `mgba_gb_colors_preset` is the table. mGBA declares it as the bare digits `0|1|2|3`
        // with no labels in the list this frontend is old enough to be handed, so a digit is all
        // there is to set: 0 is off, 1 the Game Boy Color's presets, 2 the Super Game Boy's, 3
        // both. `1` is the SP exactly — for these cartridges an SP *is* a Game Boy Color, and it
        // is emphatically not a Super Game Boy, so `3` would hand a cart a palette no SP could
        // have shown. mGBA keys its own copy of the table on a CRC32 of the header instead of on
        // the licensee and title checksum, which reaches the same palette for a cart it has an
        // entry for by a different route, and can miss one the hardware would have matched.
        //
        // `mgba_gb_colors` is the other half: what a cart the table has no entry for is given.
        // The hardware's answer to that is not a neutral grey but one specific triple — the
        // background on the boot ROM's palette 29, white through green and blue to black, and
        // both object palettes on its 4, white through salmon and dark red to black — and it is
        // what every unlicensed, third-party and homebrew monochrome cart came up in. mGBA
        // exposes that triple under the name of a boot button combination, `GBC Dark Green →A`,
        // because the boot ROM reuses one entry for the default and for that combination. The
        // name is the coincidence; the colours are the hardware's default, which is why this is
        // that value rather than one of the grey or green ramps that sit above it in the list.
        //
        // Neither of these is the Colour Correction row below. That one is about how the SP's
        // screen answered a colour it was given; these decide which colours a cartridge with no
        // colours of its own is given in the first place. Both are Game Boy only: a Game Boy
        // Color or Game Boy Advance cart carries its own palettes and renders identically either
        // way, which `render_gb_palette.rs` holds to pixel for pixel.
        core.set_option("mgba_gb_colors_preset", "1");
        core.set_option("mgba_gb_colors", "GBC Dark Green →A");
        // mGBA declares this one as `OFF|GBA|GBC|Auto`, read off the vendored dylib rather than
        // guessed at, because nothing in this tree can tell a correct option value from a typo:
        // `SET_VARIABLES` is answered `true` and the declared list thrown away, so a misspelt
        // value is accepted in silence and simply never takes.
        //
        // `Auto` rather than `GBA` or `GBC`, because mGBA is the core that runs all three
        // consoles here and `Auto` is the only value that picks the right tint for each of them.
        // Naming one would give a Game Boy cart the GBA's correction, which is a tint of the
        // wrong console rather than a stronger or weaker version of the right one.
        core.set_option("mgba_color_correction", if colour { "Auto" } else { "OFF" });
    }
    if which == Core::Gpsp {
        core.set_option("gpsp_serial", serial);
        if bios {
            core.set_option("gpsp_boot_mode", "bios");
        }
        // gpSP has its own, declared `disabled|enabled` — a different key and a different pair of
        // words from mGBA's, which is why this is spelled out here rather than shared. gpSP only
        // ever runs GBA carts, so there is no console for an `Auto` to choose between and it
        // offers none: on or off is the whole option.
        //
        // Setting it here is what keeps the row from being a lie on a gpSP cart. The quick menu
        // is a device-wide screen shown on the shelf with nothing seated, so it cannot know which
        // core the next cart will use; a row that only reached mGBA would do nothing, silently,
        // for every cart whose `selected_core.ini` line says gpsp.
        core.set_option(
            "gpsp_color_correction",
            if colour { "enabled" } else { "disabled" },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Both tests below mutate `SLOT_CORE`, which is process-global; the test harness runs
    /// tests in this module on separate threads by default, so without this they can
    /// interleave and read back a value neither of them set.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The half of "the dylib chosen and the state directory used agree" that lives entirely
    /// in this module: `candidates`, and therefore `open_core`, never spells a filename that
    /// does not match the `Core` it was handed. `crates/slot/tests/gpsp.rs` pins the other
    /// half — that a cart resolved to the same `Core` reads its resume state from the
    /// matching `States/<platform>/<core>/` directory — through the real `Session`.
    #[test]
    fn candidates_search_the_named_cores_own_filename_only() {
        let _g = lock();
        // A developer's shell leaking `SLOT_CORE` into this run would short-circuit the
        // search this test exists to check, so it cannot assume the var is unset.
        std::env::remove_var(CORE_ENV);

        let root = Path::new("/root");
        let mgba = candidates(root, Core::Mgba);
        let gpsp = candidates(root, Core::Gpsp);
        assert_ne!(mgba, gpsp, "the two cores searched the same paths");
        assert!(!mgba.is_empty());
        assert!(!gpsp.is_empty());
        for path in &mgba {
            let name = path.file_name().unwrap().to_str().unwrap();
            assert!(name.starts_with("mgba_libretro"), "{name} is not mGBA's");
        }
        for path in &gpsp {
            let name = path.file_name().unwrap().to_str().unwrap();
            assert!(name.starts_with("gpsp_libretro"), "{name} is not gpSP's");
        }
    }

    /// The property `crates/slot/tests/gpsp.rs`'s integration test relies on: a content root
    /// with its own tmp directory is not near `current_exe()` or `./vendor`, so without this
    /// candidate an integration test has nowhere to plant a fake dylib for `open_core` to
    /// find. Root cause of the gap F3 closed — before this candidate existed, a mutation
    /// that fed `open_core` the wrong `Core` had no candidate list a test could observe
    /// disagree.
    #[test]
    fn candidates_search_the_roots_own_system_directory() {
        let _g = lock();
        std::env::remove_var(CORE_ENV);
        let root = Path::new("/some/content/root");
        assert_eq!(
            candidates(root, Core::Gpsp)[0],
            root.join("System").join(dylib_name(Core::Gpsp)),
        );
    }

    /// The override is a filename, not a `Core`: it must win regardless of which core asked,
    /// which is what makes it a trap when the ini disagrees rather than a second selector.
    #[test]
    fn the_env_override_ignores_which_core_was_asked_for() {
        let _g = lock();
        std::env::set_var(CORE_ENV, "/dev/null/named-core");
        let root = Path::new("/root");
        let mgba = candidates(root, Core::Mgba);
        let gpsp = candidates(root, Core::Gpsp);
        std::env::remove_var(CORE_ENV);
        assert_eq!(mgba, vec![PathBuf::from("/dev/null/named-core")]);
        assert_eq!(
            mgba, gpsp,
            "the override stopped winning for one of the cores"
        );
    }
}

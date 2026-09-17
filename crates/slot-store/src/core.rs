use std::collections::HashMap;
use std::path::Path;

use crate::Platform;

pub const SELECTED_CORE_FILE: &str = "System/selected_core.ini";

/// Which emulator runs a cart. mGBA is the GBA default; gpSP exists for the serial hardware
/// mGBA's libretro build does not carry; TGB Dual is the Game Boy and Game Boy Color default,
/// because it runs two Game Boys in one process with a cable between them and is the only core
/// measured to carry a linked Colour pair inside an H700 frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Core {
    #[default]
    Mgba,
    Gpsp,
    TgbDual,
}

impl Core {
    /// The core picker's sockets, in the order they are drawn.
    ///
    /// **This is not every variant, and that is the design rather than an oversight.** The
    /// picker is drawn as a two-socket GBA cartridge PCB, and the board art is traced from real
    /// hardware, so a third socket would be a liberty taken with the drawing to express a choice
    /// nobody makes: which core runs a Game Boy cart is decided by its platform, with
    /// `selected_core.ini` as the escape hatch for anyone who wants the other one. A variant left
    /// out of here therefore never reaches `index()` either, which is what keeps that safe: the
    /// picker refuses to open for anything but a GBA cart (`app.rs`, `open_core_picker`).
    pub const ALL: [Core; 2] = [Core::Mgba, Core::Gpsp];

    pub fn as_str(&self) -> &'static str {
        match self {
            Core::Mgba => "mgba",
            Core::Gpsp => "gpsp",
            Core::TgbDual => "tgbdual",
        }
    }

    /// Position in `ALL`, which is the order the picker's rows and their faces are in. Only
    /// meaningful for a variant `ALL` holds; see the note there for why nothing else reaches it.
    pub fn index(self) -> usize {
        self as usize
    }

    /// What the picker calls it. Not `as_str`: that is the ini's spelling, meant to be typed
    /// by hand into a text editor on a computer, and this is the player's, meant to be read
    /// off a panel. The two are free to differ, and already do.
    pub fn text(self) -> &'static str {
        match self {
            Core::Mgba => "mGBA",
            Core::Gpsp => "gpSP",
            Core::TgbDual => "TGB Dual",
        }
    }

    pub fn parse(s: &str) -> Option<Core> {
        match s.trim().to_ascii_lowercase().as_str() {
            "mgba" => Some(Core::Mgba),
            "gpsp" => Some(Core::Gpsp),
            "tgbdual" => Some(Core::TgbDual),
            _ => None,
        }
    }

    /// Whether this core runs that platform's carts at all.
    ///
    /// gpSP is a GBA emulator and nothing else: handed a Game Boy ROM it refuses it or paints
    /// garbage, and either way the cart's states end up filed under a core that never ran it.
    /// TGB Dual is the mirror image, a Game Boy emulator with no GBA in it. mGBA is the only one
    /// that runs both, which is why it stays the fallback everywhere.
    pub fn runs(self, platform: Platform) -> bool {
        match self {
            Core::Mgba => true,
            Core::Gpsp => platform == Platform::Gba,
            Core::TgbDual => matches!(platform, Platform::Gb | Platform::Gbc),
        }
    }

    /// The core a cart of this platform gets when nothing on the card says otherwise.
    ///
    /// Game Boy and Game Boy Color default to TGB Dual for one reason, and it is worth stating
    /// because it is a trade: TGB Dual is the only core measured to hold 60 fps for a *linked*
    /// Colour pair on an H700 (2.0 ms of a 16.743 ms frame, against SameBoy's 22.4 ms, which
    /// does not fit at all). mGBA is the more accurate Game Boy and costs little more in single
    /// player. The default is TGB Dual anyway, for both single and linked play, because a cart
    /// that changed core when a link began would change which `States/<platform>/<core>/`
    /// directory its saves live in, and strand them.
    pub fn default_for(platform: Platform) -> Core {
        match platform {
            Platform::Gba => Core::Mgba,
            Platform::Gb | Platform::Gbc => Core::TgbDual,
        }
    }
}

/// `<rom stem> = <core>`, one per line — `crate::ini`'s shape, and every rule about hand-edited
/// files that goes with it lives there. This is only the value type on top: a name we do not
/// know is dropped rather than raised, because it is a card written for a newer build, or a
/// typo, and either way the default is the safe reading.
pub fn read_selected_cores(root: &Path) -> HashMap<String, Core> {
    crate::ini::read(root, SELECTED_CORE_FILE)
        .into_iter()
        .filter_map(|(stem, name)| Core::parse(&name).map(|core| (stem, core)))
        .collect()
}

/// The core one cart wants, or the default for a cart the file does not name, which is also
/// what a cart whose line nobody can parse gets.
///
/// Platform-blind, and therefore GBA-only in practice: it cannot tell a line naming gpSP for a
/// Game Boy cart from one naming it for a GBA cart. `core_for_platform` is the one to call when
/// the platform is known, which is everywhere that matters.
pub fn core_for(root: &Path, stem: &str) -> Core {
    crate::ini::value(root, SELECTED_CORE_FILE, stem)
        .as_deref()
        .and_then(Core::parse)
        .unwrap_or_default()
}

/// The core one cart runs on: its own line in `selected_core.ini` if that line names a core
/// which runs this platform, and this platform's default otherwise.
///
/// The platform outranks the file, and it has to. `selected_core.ini` is hand-edited on a card,
/// so nothing stops a line reading `Tetris = gpsp`, and gpSP does not run Game Boy games: it
/// would refuse the ROM or paint garbage, with the cart's states filed under a core that never
/// ran it. Dropping a line that names a core this platform cannot use is the same reading
/// `read_selected_cores` already gives an unparseable one, for the same reason: the default is
/// the safe answer, and a card written for a different build should not be able to break a cart.
///
/// What is new here, and is the whole point of reading the file for a Game Boy cart at all, is
/// that there are now two cores that do run one. A player who wants mGBA's accuracy over TGB
/// Dual's link can write `Tetris = mgba` and get it. That is the escape hatch the Game Boy
/// support design asked for, and it is why this is not simply a hardcoded per-platform match.
pub fn core_for_platform(root: &Path, stem: &str, platform: Platform) -> Core {
    crate::ini::value(root, SELECTED_CORE_FILE, stem)
        .as_deref()
        .and_then(Core::parse)
        .filter(|core| core.runs(platform))
        .unwrap_or_else(|| Core::default_for(platform))
}

/// Set one cart's core, leaving the rest of the file exactly as it was.
pub fn write_selected_core(root: &Path, stem: &str, core: Core) -> std::io::Result<()> {
    crate::ini::write(root, SELECTED_CORE_FILE, stem, core.as_str())
}

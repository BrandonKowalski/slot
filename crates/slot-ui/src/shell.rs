use std::path::Path;

use slot_store::{Cart, ShelfKind};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Finish {
    Solid,
    /// Clear plastic: the shell colour lightens and desaturates toward the rim, the way light
    /// catches the edge of a translucent case. The gen 3 Pokemon releases wear it — they were
    /// shipped in coloured clear shells, and drawing them solid was the table's one wrong note.
    Translucent,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Shell {
    pub colour: [u8; 3],
    pub finish: Finish,
}

pub const DEFAULT_SHELL: Shell = Shell {
    colour: [0x35, 0x35, 0x3a],
    finish: Finish::Solid,
};

const fn shell(colour: [u8; 3], finish: Finish) -> Shell {
    Shell { colour, finish }
}

/// Keyed on the region free game code prefix, so one row covers every region a title
/// shipped in. Every code here was read off a real header rather than recalled: a wrong one
/// paints some other game in the wrong shell, which is worse than defaulting to grey.
const EXACT: &[(&str, Shell)] = &[
    ("AXV", shell([0xc2, 0x33, 0x2e], Finish::Translucent)), // Pokemon Ruby
    ("AXP", shell([0x2f, 0x5c, 0xc0], Finish::Translucent)), // Pokemon Sapphire
    ("BPE", shell([0x24, 0x9c, 0x60], Finish::Translucent)), // Pokemon Emerald
    ("BPR", shell([0xd8, 0x52, 0x24], Finish::Translucent)), // Pokemon FireRed
    ("BPG", shell([0x63, 0xb0, 0x44], Finish::Translucent)), // Pokemon LeafGreen
];

/// Keyed on the first letter alone. `M` is the Game Boy Advance Video family, thirty odd
/// releases that would otherwise be thirty hand transcribed rows.
const FAMILY: &[(u8, Shell)] = &[(b'M', shell([0xc6, 0xc6, 0xc9], Finish::Solid))];

/// The plain Game Boy Game Pak. The reference photograph the outline was drawn from is a grey
/// pak, and `slot-card-backups/cart-refs/PROVENANCE.md` names it grey; the brief for this work
/// called it black, which is the plastic a Colour-compatible pak shipped in rather than an
/// original one. Drawn as the object is, warm and light enough that the moulded ribs and the
/// recess walls have somewhere to go.
pub const DMG_SHELL: Shell = shell([0x9a, 0x97, 0x8f], Finish::Solid);

/// A Colour pak's smoke coloured clear plastic. Cooler than the grey pak beside it, because
/// they are otherwise close enough in value that only the lit rim would tell them apart.
pub const GB_CLEAR_SHELL: Shell = shell([0x7c, 0x7a, 0x8a], Finish::Translucent);

/// What plastic this cart shipped in. Which question to ask depends on the platform: a GBA cart
/// is looked up by the game code in its header, and a Game Boy pak has no such field at all, so
/// the CGB flag answers instead.
pub fn shell_for(cart: &Cart) -> Shell {
    match cart.platform.shelf() {
        ShelfKind::Gba => gba_shell_for(&cart.code),
        ShelfKind::GameBoy => gb_shell_for(&cart.rom),
    }
}

/// `code` is the four character game code; only the first three are matched.
pub fn gba_shell_for(code: &str) -> Shell {
    lookup(code, EXACT, FAMILY)
}

/// Both Colour values wear the clear shell: `0xC0` is Colour only and `0x80` is Colour enhanced
/// but still runs on original hardware, and `slot_store::gb::is_colour` is where that collapse
/// from three flag values onto two finishes already lives. A rom that cannot be read is a plain
/// pak rather than a failure — the shelf still has a cart to draw.
fn gb_shell_for(rom: &Path) -> Shell {
    if slot_store::gb::is_colour(rom) {
        GB_CLEAR_SHELL
    } else {
        DMG_SHELL
    }
}

pub fn table_keys() -> Vec<&'static str> {
    EXACT.iter().map(|(k, _)| *k).collect()
}

/// Probed against a fixture where the exact row, the family letter and the default all
/// disagree. The shipping table has no code that two rules both claim, so the order cannot
/// be observed through it, and the order is the whole escape hatch: an explicit row is how
/// a wrongly coloured family member gets fixed.
pub fn lookup_order_is_exact_then_family_then_default() -> bool {
    const A: Shell = shell([1, 1, 1], Finish::Solid);
    const B: Shell = shell([2, 2, 2], Finish::Solid);
    let exact = [("MSK", A)];
    let family = [(b'M', B)];
    lookup("MSKE", &exact, &family) == A
        && lookup("MPOE", &exact, &family) == B
        && lookup("ZZZZ", &exact, &family) == DEFAULT_SHELL
}

fn lookup(code: &str, exact: &[(&str, Shell)], family: &[(u8, Shell)]) -> Shell {
    let prefix: String = code.chars().take(3).collect();
    if let Some((_, s)) = exact.iter().find(|(k, _)| *k == prefix) {
        return *s;
    }
    if let Some(first) = code.as_bytes().first() {
        if let Some((_, s)) = family.iter().find(|(k, _)| k == first) {
            return *s;
        }
    }
    DEFAULT_SHELL
}

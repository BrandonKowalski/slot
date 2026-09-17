use std::path::Path;

use slot_store::gb::Class;
use slot_store::{Cart, Platform};

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

/// The plain Game Boy Game Pak, CGB flag 0x00. The reference photograph the outline was drawn
/// from is a grey pak, and `slot-card-backups/cart-refs/PROVENANCE.md` names it grey. Drawn as
/// the object is, warm and light enough that the moulded ribs and the recess walls have
/// somewhere to go.
pub const DMG_SHELL: Shell = shell([0x9a, 0x97, 0x8f], Finish::Solid);

/// A Colour-enhanced pak, CGB flag 0x80: the **black** cartridge. Not the clear one — a 0x80
/// cart runs on original hardware and was moulded in the same notched shell as a grey pak, in
/// black plastic. Charcoal rather than ink, because the moulding is drawn by darkening the
/// shell and a shell already at zero has nothing left to give.
pub const DUAL_MODE_SHELL: Shell = shell([0x33, 0x30, 0x31], Finish::Solid);

/// A Colour-only pak, CGB flag 0xc0: smoke coloured clear plastic. Cooler than the grey pak
/// beside it, because they are otherwise close enough in value that only the lit rim would
/// tell them apart.
pub const GB_CLEAR_SHELL: Shell = shell([0x7c, 0x7a, 0x8a], Finish::Translucent);

/// What plastic this cart shipped in. Which question to ask depends on the platform: a GBA cart
/// is looked up by the game code in its header, and a Game Boy pak has no such field at all, so
/// the CGB flag answers instead.
///
/// `Gb` and `Gbc` are one arm on purpose, and it is not the shelf being ignored. The shelves
/// are one per folder; the plastic is one per CGB flag, and the two groupings do not line up.
/// A `.gb` file is routinely Colour-exclusive and a `.gbc` file is routinely DMG-compatible, so
/// asking the folder would paint a misfiled cart as something it is not. `gb_shell_for` asks
/// the rom instead.
pub fn shell_for(cart: &Cart) -> Shell {
    match cart.platform {
        Platform::Gba => gba_shell_for(&cart.code),
        Platform::Gb | Platform::Gbc => gb_shell_for(&cart.rom),
    }
}

/// `code` is the four character game code; only the first three are matched.
pub fn gba_shell_for(code: &str) -> Shell {
    lookup(code, EXACT, FAMILY)
}

/// Three flag values, three plastics: grey, black, clear. They used to be two, with 0x80 drawn
/// in the clear shell on the reasoning that a Colour-enhanced cart shipped in the same plastic
/// as a Colour-only one. It did not — the 0x80 cart is the black one, and Nintendo's own
/// typology has always named all three. A rom that cannot be read falls out as a plain pak
/// rather than as a failure, so the shelf still has a cart to draw.
fn gb_shell_for(rom: &Path) -> Shell {
    match slot_store::gb::class(rom) {
        Class::Original => DMG_SHELL,
        Class::DualMode => DUAL_MODE_SHELL,
        Class::ColourOnly => GB_CLEAR_SHELL,
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

//! Which link hardware a cart uses, so the link screen draws the thing the game expects: the
//! cable or the Wireless Adapter. Mirrors the rule gpSP's `gpsp_serial=auto` applies, because
//! that is the link the core actually runs — until the player switches it, and then
//! `serial_option` names the mode gpSP has to be loaded with instead.

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Cable,
    Wireless,
}

impl LinkKind {
    /// The one SELECT switches to.
    pub fn other(self) -> LinkKind {
        match self {
            LinkKind::Cable => LinkKind::Wireless,
            LinkKind::Wireless => LinkKind::Cable,
        }
    }
}

/// gpSP's `FLAGS_RFU` entries, from `gba_over.h` in `vendor/gpsp-src.tar.gz`.
const WIRELESS: [&str; 43] = [
    "B2WE", "B3AE", "B4UE", "B4UP", "B85A", "B85P", "BDGE", "BDGP", "BG3E", "BKRJ", "BMGD", "BMGE",
    "BMGF", "BMGI", "BMGJ", "BMGP", "BMGS", "BMGU", "BPED", "BPEE", "BPEF", "BPEI", "BPEJ", "BPES",
    "BPGD", "BPGE", "BPGF", "BPGI", "BPGJ", "BPGS", "BPRD", "BPRE", "BPRF", "BPRI", "BPRJ", "BPRS",
    "BR5E", "BR6E", "BRBE", "BRKE", "BTME", "BTMJ", "BTMP",
];

/// `code` and `title` are the header's, as `Cart` has them; `clean` is
/// `slot_store::header_clean` for the same ROM.
pub fn link_kind(code: &str, title: &str, clean: bool) -> LinkKind {
    if pokemon(code, title) {
        // gpSP treats a Pokémon ROM as a hack, and links it by cable, unless its header is
        // standard, it is 16 MB or smaller, its code is one gpSP knows and its title is exactly
        // the retail one. Of the retail games only FireRed, LeafGreen and Emerald get the adapter.
        let retail = clean
            && WIRELESS.contains(&code)
            && ["POKEMON FIRE", "POKEMON LEAF", "POKEMON EMER"].contains(&title);
        return if retail {
            LinkKind::Wireless
        } else {
            LinkKind::Cable
        };
    }
    if WIRELESS.contains(&code) {
        LinkKind::Wireless
    } else {
        LinkKind::Cable
    }
}

/// The `gpsp_serial` a cart loads with to link over `chosen`, where `auto` is what gpSP picks
/// for it on its own (`link_kind`'s answer). `code` and `title` are the header's.
///
/// The mode gpSP would pick is left to gpSP, as `auto`, which is how every cart loaded before
/// there was a choice: a cart nobody switched never changes. The adapter is one mode for every
/// game. The cable is not — gpSP speaks each family's own protocol and has no generic one — so
/// a switch to the cable names the family's, and any other game stays on `auto`, which is
/// still gpSP's own pick.
pub fn serial_option(chosen: LinkKind, auto: LinkKind, code: &str, title: &str) -> &'static str {
    if chosen == auto {
        return "auto";
    }
    match chosen {
        LinkKind::Wireless => "rfu",
        LinkKind::Cable if pokemon(code, title) => "mul_poke",
        LinkKind::Cable if code.starts_with("AWR") => "mul_aw1",
        LinkKind::Cable if code.starts_with("AW2") => "mul_aw2",
        LinkKind::Cable => "auto",
    }
}

/// Whether gpSP can actually carry this cart's link.
///
/// gpSP does not emulate the link cable. `serial.c` has no generic multiplayer path at all: it
/// speaks the Wireless Adapter and three named cable protocols — Pokémon Gen3, Advance Wars 1
/// and Advance Wars 2 — and a cart it recognises none of is left on `SERIAL_MODE_AUTO`, which
/// `netpacket_receive` has no case for. The session still comes up, which is what makes this
/// worth asking before the radio does: `netpacket_connected` tests against
/// `maxpl[SERIAL_MODE_AUTO] - 1U`, and that entry is 0, so the subtraction underflows and every
/// peer is accepted. Two devices join, and then every packet is dropped in silence — a link
/// that looks made from both panels and does nothing in either game.
///
/// True for the three sets gpSP has a protocol for, which are exactly the ones `serial_option`
/// can name: the adapter list, the Pokémon family, and Advance Wars 1 and 2. `code` and `title`
/// are the header's, as `Cart` has them.
///
/// Whether the header is clean does not enter into it, unlike `link_kind`, which is why this
/// does not ask for it. gpSP reaches a protocol for each of these either way: an adapter cart
/// keeps its `FLAGS_RFU` however its header reads, and a Pokémon ROM gets the adapter or the
/// cable when gpSP takes it for retail and `mul_poke` when it takes it for a hack. The cart is
/// carried in both cases, so a clean header can only change which mode it is carried in.
pub fn link_carried(code: &str, title: &str) -> bool {
    WIRELESS.contains(&code)
        || pokemon(code, title)
        || code.starts_with("AWR")
        || code.starts_with("AW2")
}

/// The Pokémon family, by title or by any of its codes. gpSP's own test, and the one both its
/// automatic pick and its cable protocol hang off.
fn pokemon(code: &str, title: &str) -> bool {
    title.starts_with("POKEMON")
        || ["AXV", "AXP", "BPE", "BPR", "BPG"]
            .iter()
            .any(|p| code.starts_with(p))
}

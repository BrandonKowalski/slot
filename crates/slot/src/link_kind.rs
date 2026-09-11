//! Which link hardware a cart uses, so the link screen draws the thing the game expects: the
//! cable or the Wireless Adapter. Mirrors the rule gpSP's `gpsp_serial=auto` applies, because
//! that is the link the core actually runs.

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Cable,
    Wireless,
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
    let pokemon = title.starts_with("POKEMON")
        || ["AXV", "AXP", "BPE", "BPR", "BPG"]
            .iter()
            .any(|p| code.starts_with(p));
    if pokemon {
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

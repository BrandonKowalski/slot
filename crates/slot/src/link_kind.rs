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

/// `code` is the four-character header code and `title` the header title, as `Cart` has them.
pub fn link_kind(code: &str, title: &str) -> LinkKind {
    // Ruby, Sapphire, and Advance Wars use the cable.
    if code.starts_with("AXV") || code.starts_with("AXP") || code.starts_with("AW") {
        return LinkKind::Cable;
    }
    let pokemon =
        title.starts_with("POKEMON") || ["BPE", "BPR", "BPG"].iter().any(|p| code.starts_with(p));
    if pokemon || WIRELESS.contains(&code) {
        LinkKind::Wireless
    } else {
        LinkKind::Cable
    }
}

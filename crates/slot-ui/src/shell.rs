#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Finish {
    Solid,
    /// Currently unused: every cart in the table is solid, including the gen 3 Pokemon
    /// releases that were thought to be translucent. Kept because the rendering path is
    /// written and a clear shell would only need a table row, not new code.
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
    ("AXV", shell([0xc2, 0x33, 0x2e], Finish::Solid)), // Pokemon Ruby
    ("AXP", shell([0x2f, 0x5c, 0xc0], Finish::Solid)), // Pokemon Sapphire
    ("BPE", shell([0x24, 0x9c, 0x60], Finish::Solid)), // Pokemon Emerald
    ("BPR", shell([0xd8, 0x52, 0x24], Finish::Solid)), // Pokemon FireRed
    ("BPG", shell([0x63, 0xb0, 0x44], Finish::Solid)), // Pokemon LeafGreen
];

/// Keyed on the first letter alone. `M` is the Game Boy Advance Video family, thirty odd
/// releases that would otherwise be thirty hand transcribed rows.
const FAMILY: &[(u8, Shell)] = &[(b'M', shell([0xc6, 0xc6, 0xc9], Finish::Solid))];

/// `code` is the four character game code; only the first three are matched.
pub fn shell_for(code: &str) -> Shell {
    lookup(code, EXACT, FAMILY)
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

use slot::link_kind::{link_kind, LinkKind};

#[test]
fn the_wireless_adapter_games_are_wireless() {
    for code in ["BMGE", "BTME", "BR5E", "BRBE", "BDGE", "B4UE", "B85A"] {
        assert_eq!(link_kind(code, "", true), LinkKind::Wireless, "{code}");
    }
    // A Pokémon code with no title is family by code and not retail, so it links by cable —
    // gpSP's own rule, not merely "every code in WIRELESS".
    for code in ["BPEE", "BPRE", "BPGE"] {
        assert_eq!(link_kind(code, "", true), LinkKind::Cable, "{code}");
    }
    for (code, title) in [
        ("BPRE", "POKEMON FIRE"),
        ("BPGE", "POKEMON LEAF"),
        ("BPEE", "POKEMON EMER"),
    ] {
        assert_eq!(link_kind(code, title, true), LinkKind::Wireless, "{code}");
    }
}

#[test]
fn ruby_sapphire_and_advance_wars_use_the_cable() {
    for code in ["AXVE", "AXPE", "AWRE", "AW2E"] {
        assert_eq!(
            link_kind(code, "POKEMON RUBY", true),
            LinkKind::Cable,
            "{code}"
        );
    }
}

/// gpSP's own rule: a Pokémon ROM is a hack, forced to the cable, unless its header is
/// standard, it is 16 MB or smaller, its code is one gpSP knows and its title is exactly the
/// retail one.
#[test]
fn a_pokemon_hack_links_by_cable() {
    for (code, title, clean) in [
        ("BPEE", "POKEMON EMER", false), // nonstandard header, or the ROM is expanded
        ("BPRE", "PKMN RADICAL", true),  // altered title, family by code
        ("ZZZZ", "POKEMON EMER", true),  // a code gpSP does not know
        ("", "POKEMON", true),
    ] {
        assert_eq!(
            link_kind(code, title, clean),
            LinkKind::Cable,
            "{code} {title} {clean}"
        );
    }
}

#[test]
fn everything_else_is_the_cable() {
    assert_eq!(link_kind("SLTE", "SLOT TEST", true), LinkKind::Cable);
    assert_eq!(link_kind("", "", true), LinkKind::Cable);
}

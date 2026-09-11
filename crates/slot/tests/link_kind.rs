use slot::link_kind::{link_kind, serial_option, LinkKind};

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

/// A cart nobody switched loads exactly as it always has, Pokémon and Advance Wars included:
/// gpSP's own pick is the one both devices can agree on without being told.
#[test]
fn the_mode_gpsp_would_pick_is_left_to_gpsp() {
    for (kind, code, title) in [
        (LinkKind::Cable, "SLTE", "SLOT TEST"),
        (LinkKind::Wireless, "BMGE", "MARIOGOLFADV"),
        (LinkKind::Wireless, "BPEE", "POKEMON EMER"),
        (LinkKind::Cable, "AXVE", "POKEMON RUBY"),
        (LinkKind::Cable, "AWRE", "ADVANCEWARS"),
        (LinkKind::Cable, "AW2E", "ADVANCEWARS2"),
    ] {
        assert_eq!(serial_option(kind, kind, code, title), "auto", "{code}");
    }
}

#[test]
fn switched_to_the_adapter_is_rfu() {
    for (code, title) in [
        ("SLTE", "SLOT TEST"),
        ("AXVE", "POKEMON RUBY"),
        ("AWRE", "ADVANCEWARS"),
        ("", ""),
    ] {
        assert_eq!(
            serial_option(LinkKind::Wireless, LinkKind::Cable, code, title),
            "rfu",
            "{code}"
        );
    }
}

/// The same family `link_kind` recognises: by title, or by any of the five codes whatever the
/// title says.
#[test]
fn a_pokemon_cart_switched_to_the_cable_uses_the_pokemon_protocol() {
    for (code, title) in [
        ("BPEE", "POKEMON EMER"),
        ("ZZZZ", "POKEMON"),
        ("AXVE", "PKMN HACK"),
        ("AXPE", "PKMN HACK"),
        ("BPEE", "PKMN HACK"),
        ("BPRE", "PKMN HACK"),
        ("BPGE", "PKMN HACK"),
    ] {
        assert_eq!(
            serial_option(LinkKind::Cable, LinkKind::Wireless, code, title),
            "mul_poke",
            "{code} {title}"
        );
    }
}

#[test]
fn advance_wars_switched_to_the_cable_uses_its_own_protocol() {
    assert_eq!(
        serial_option(LinkKind::Cable, LinkKind::Wireless, "AWRE", "ADVANCEWARS"),
        "mul_aw1"
    );
    assert_eq!(
        serial_option(LinkKind::Cable, LinkKind::Wireless, "AW2E", "ADVANCEWARS2"),
        "mul_aw2"
    );
}

/// gpSP has no cable mode of its own to ask for, so every other cart switched to the cable is
/// left on `auto`.
#[test]
fn any_other_cart_switched_to_the_cable_stays_on_auto() {
    for (code, title) in [("BMGE", "MARIOGOLFADV"), ("AWXE", "ADVANCEWARS"), ("", "")] {
        assert_eq!(
            serial_option(LinkKind::Cable, LinkKind::Wireless, code, title),
            "auto",
            "{code}"
        );
    }
}

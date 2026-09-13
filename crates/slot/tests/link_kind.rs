use slot::link_kind::{link_carried, link_kind, serial_option, LinkKind};

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

/// The three sets gpSP has a protocol for, which are exactly the ones `serial_option` can name.
#[test]
fn gpsp_carries_the_adapter_list_the_pokemon_family_and_advance_wars() {
    for (code, title) in [
        ("BMGE", "MARIOGOLFADV"), // the adapter list
        ("BTME", "MARIOKARTADV"),
        ("B2WE", "WARIOWARE"),
        ("BPEE", "POKEMON EMER"), // the Pokémon family, by code
        ("AXVE", "POKEMON RUBY"),
        ("AXPE", "POKEMON SAPP"),
        ("ZZZZ", "POKEMON FIRE"), // and by title, whatever the code says
        ("AWRE", "ADVANCEWARS"),  // Advance Wars 1, both its codes
        ("AWRP", "ADVANCEWARS"),
        ("AW2E", "ADVANCEWARS2"), // and 2
        ("AW2P", "ADVANCEWARS2"),
    ] {
        assert!(link_carried(code, title), "{code} {title}");
    }
}

/// Apotris is the one on the card: a real cable game, absent from gpSP's `gba_over.h`, so gpSP
/// leaves it on `SERIAL_MODE_AUTO` — takes the session, then drops every packet. Mario & Luigi
/// and Super Mario Advance 4 are the other shape of it: gpSP knows them and gives them
/// `SERIAL_MODE_GBP`, the GBA Player, which its netpacket hooks have no case for either.
#[test]
fn gpsp_carries_nothing_else() {
    for (code, title) in [
        ("2ATE", "APOTRIS"),
        ("SLTE", "SLOT TEST"),
        ("AMTE", "METROIDFUSION"),
        ("A88E", "MARIO&LUIGIRPG"),
        ("AX4E", "SUPER MARIOD"),
        ("", ""),
    ] {
        assert!(!link_carried(code, title), "{code} {title}");
    }
}

/// A hack is still carried: gpSP hands a Pokémon ROM it will not take for retail to `mul_poke`
/// rather than leaving it on `auto`. Whether the header is clean decides which mode carries it,
/// never whether one does, which is why `link_carried` does not ask.
#[test]
fn a_pokemon_hack_is_carried_the_same_as_the_retail_game() {
    for (code, title) in [
        ("BPEE", "PKMN RADICAL"), // family by code, title altered
        ("ZZZZ", "POKEMON"),      // family by title, code gpSP does not know
        ("BPPE", "POKEMON PINB"), // the GBA Player entry gpSP overrides to mul_poke
    ] {
        assert!(link_carried(code, title), "{code} {title}");
    }
}

/// The predicate and the modes agree, and this is the pair that must not drift: a carried cart is
/// exactly one gpSP either picks the adapter for on its own or has a cable protocol to name for.
///
/// Switching to the adapter is deliberately not part of that test. `serial_option` answers `rfu`
/// for any cart at all, because the adapter is one mode for every game, so "names something other
/// than `auto`" is true even of a cart gpSP cannot link — gpSP would run the adapter emulation for
/// it and the game would never speak to it. The cable side is the one that distinguishes.
#[test]
fn the_carried_carts_are_exactly_the_ones_with_a_mode_of_their_own() {
    for (code, title) in [
        ("BMGE", "MARIOGOLFADV"),
        ("BPEE", "POKEMON EMER"),
        ("AXVE", "POKEMON RUBY"),
        ("AWRE", "ADVANCEWARS"),
        ("AW2E", "ADVANCEWARS2"),
        ("2ATE", "APOTRIS"),
        ("SLTE", "SLOT TEST"),
        ("A88E", "MARIO&LUIGIRPG"),
        ("", ""),
    ] {
        let own = link_kind(code, title, true) == LinkKind::Wireless
            || serial_option(LinkKind::Cable, LinkKind::Wireless, code, title) != "auto";
        assert_eq!(link_carried(code, title), own, "{code} {title}");
    }
}

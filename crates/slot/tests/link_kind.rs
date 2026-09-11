use slot::link_kind::{link_kind, LinkKind};

#[test]
fn the_wireless_adapter_games_are_wireless() {
    for code in [
        "BPEE", "BPRE", "BPGE", "BMGE", "BTME", "BR5E", "BRBE", "BDGE", "B4UE", "B85A",
    ] {
        assert_eq!(link_kind(code, ""), LinkKind::Wireless, "{code}");
    }
}

#[test]
fn ruby_sapphire_and_advance_wars_use_the_cable() {
    for code in ["AXVE", "AXPE", "AWRE", "AW2E"] {
        assert_eq!(link_kind(code, "POKEMON RUBY"), LinkKind::Cable, "{code}");
    }
}

/// gpSP's own rule: a Pokémon title is the adapter unless it is Ruby or Sapphire.
#[test]
fn an_unlisted_pokemon_title_is_wireless() {
    assert_eq!(link_kind("ZZZZ", "POKEMON EMER"), LinkKind::Wireless);
    assert_eq!(link_kind("", "POKEMON"), LinkKind::Wireless);
}

#[test]
fn everything_else_is_the_cable() {
    assert_eq!(link_kind("SLTE", "SLOT TEST"), LinkKind::Cable);
    assert_eq!(link_kind("", ""), LinkKind::Cable);
}

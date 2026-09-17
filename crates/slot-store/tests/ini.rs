//! The `<stem> = <value>` files under `System/`, at the layer that knows nothing about what a
//! value means. `selected_core.ini` had all of this to itself; `video_mode.ini` is the second
//! file to want it and the cart shells will be the third, so it is checked here once rather
//! than a second and third time through whatever type happens to be sitting on top of it.

use slot_store::ini;
use tempfile::tempdir;

const FILE: &str = "System/example.ini";

fn root_with(text: Option<&str>) -> tempfile::TempDir {
    let d = tempdir().unwrap();
    std::fs::create_dir(d.path().join("System")).unwrap();
    if let Some(text) = text {
        std::fs::write(d.path().join(FILE), text).unwrap();
    }
    d
}

#[test]
fn an_absent_file_is_an_empty_map_rather_than_an_error() {
    let d = root_with(None);
    assert!(ini::read(d.path(), FILE).is_empty());
    assert_eq!(ini::value(d.path(), FILE, "Emerald"), None);
}

/// Every one of these is a typo somebody will make in a text editor on a card, and not one of
/// them may cost them the file. The string layer keeps an empty value rather than dropping it:
/// what an empty value means is the business of whatever type sits on top.
#[test]
fn malformed_lines_are_skipped_rather_than_fatal() {
    let d = root_with(Some(concat!(
        "\n",
        "# a comment\n",
        "; another comment\n",
        "[section]\n",
        "no equals sign here\n",
        "  Spaced Out   =   stretch  \n",
        "= orphaned\n",
        "Trailing =\n",
    )));
    let map = ini::read(d.path(), FILE);
    assert_eq!(map.get("Spaced Out").map(String::as_str), Some("stretch"));
    assert_eq!(map.get("Trailing").map(String::as_str), Some(""));
    assert_eq!(map.len(), 2, "a comment or an orphan was stored");
}

#[test]
fn a_later_duplicate_wins() {
    let d = root_with(Some("Emerald = one\nEmerald = two\n"));
    assert_eq!(
        ini::value(d.path(), FILE, "Emerald").as_deref(),
        Some("two")
    );
}

#[test]
fn writing_creates_the_file_when_it_is_absent() {
    let d = root_with(None);
    ini::write(d.path(), FILE, "Emerald", "stretch").unwrap();
    assert_eq!(
        ini::value(d.path(), FILE, "Emerald").as_deref(),
        Some("stretch")
    );
}

/// The file is meant to be opened in a text editor on a computer. A rebuild from the map would
/// quietly drop every comment, blank line and unparsed line in it — including the note somebody
/// wrote to themselves above a cart.
#[test]
fn writing_replaces_one_line_and_leaves_the_rest_of_the_file_alone() {
    let d = root_with(Some(concat!(
        "# my notes\n",
        "\n",
        "Emerald = actual\n",
        "Metroid Fusion = stretch\n",
    )));
    ini::write(d.path(), FILE, "Emerald", "stretch").unwrap();

    let text = std::fs::read_to_string(d.path().join(FILE)).unwrap();
    assert!(
        text.contains("# my notes"),
        "a hand-written comment was destroyed"
    );
    assert!(text.contains("\n\n"), "a blank line was closed up");
    assert!(
        text.contains("Metroid Fusion = stretch"),
        "another key's entry was lost"
    );
    assert_eq!(text.matches("Emerald").count(), 1, "the old line was left");
}

#[test]
fn writing_appends_a_key_the_file_has_never_seen() {
    let d = root_with(Some("Emerald = actual\n"));
    ini::write(d.path(), FILE, "Drill Dozer", "stretch").unwrap();
    assert_eq!(
        ini::value(d.path(), FILE, "Emerald").as_deref(),
        Some("actual")
    );
    assert_eq!(
        ini::value(d.path(), FILE, "Drill Dozer").as_deref(),
        Some("stretch")
    );
}

/// Every one of these is a filename somebody can really put in `Games/`, and not one of them
/// survives a trip through the parser: the spaced ones come back trimmed, the `=` one comes
/// back cut at the `=`, and the last three come back as comments or a section header.
///
/// Writing them anyway was the bug. The line could never be found again, so the preference
/// never persisted; the write appended rather than replaced, so the card's file grew by a line
/// on every press; and the `=` one let a cart called `Cheats` claim the line belonging to a
/// cart called `Cheats = On`. The file is left exactly as it was instead, and the caller — which
/// already logs a failed write — is told why.
const UNSAYABLE: [&str; 7] = [
    " Tetris",
    "Tetris ",
    "Cheats = On",
    "#1 Racer",
    ";Notes",
    "[Section]",
    "",
];

#[test]
fn a_key_the_file_cannot_say_is_refused_rather_than_written() {
    for key in UNSAYABLE {
        let d = root_with(Some("Emerald = actual\n"));
        let e = ini::write(d.path(), FILE, key, "stretch")
            .expect_err(&format!("{key:?} was written anyway"));
        assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput, "{key:?}");
        assert_eq!(
            std::fs::read_to_string(d.path().join(FILE)).unwrap(),
            "Emerald = actual\n",
            "{key:?} changed the file"
        );
    }
}

/// The shape of the harm, pinned on its own because the refusal above is only the mechanism.
/// Three presses of L and R on a cart with a space in front of its name used to leave three
/// lines on the card, none of which any read would ever find.
#[test]
fn a_refused_key_cannot_grow_the_file_a_line_at_a_time() {
    let d = root_with(None);
    for value in ["stretch", "actual", "stretch"] {
        let _ = ini::write(d.path(), FILE, " Tetris", value);
    }
    let text = std::fs::read_to_string(d.path().join(FILE)).unwrap_or_default();
    assert!(
        text.lines().count() <= 1,
        "the file grew a line per write: {text:?}"
    );
}

/// One cart's write must never reach another cart's line, and the `=` refusal above is what
/// makes that reachable to state. A cart called `Cheats = On` used to be written down as
/// `Cheats = On = stretch`, which `read` attributes to a *different* cart, `Cheats` — so the
/// next time that cart was given a mode it replaced the line, then dropped the leftover as a
/// duplicate of itself, and one cart's preference deleted another's.
///
/// Neither line can be written now, so the collision cannot be manufactured from inside slot.
/// A person can still type the ambiguous line by hand; what they get is the reading `read` has
/// always taken of it, which is the one thing a write has to agree with.
#[test]
fn neither_cart_can_write_the_line_the_other_would_claim() {
    let d = root_with(None);
    ini::write(d.path(), FILE, "Cheats = On", "stretch")
        .expect_err("the ambiguous line was written");
    ini::write(d.path(), FILE, "Cheats", "actual").unwrap();
    assert_eq!(
        std::fs::read_to_string(d.path().join(FILE)).unwrap(),
        "Cheats = actual\n"
    );
}

/// A write disturbs the line `read` attributes to this key and nothing else on the card. The
/// two used to decide that separately — `read` by one set of rules and `write` by splitting on
/// `=` with none of them — and two copies of a rule is how two functions meant to agree start
/// to differ.
#[test]
fn a_write_leaves_every_line_that_is_not_this_keys_alone() {
    let d = root_with(Some(concat!(
        "# Emerald = what I had before\n",
        "; Emerald = and before that\n",
        "[Emerald]\n",
        "= Emerald\n",
        "Emerald = actual\n",
        "Emeralds = actual\n",
    )));
    ini::write(d.path(), FILE, "Emerald", "stretch").unwrap();
    let text = std::fs::read_to_string(d.path().join(FILE)).unwrap();
    for kept in [
        "# Emerald = what I had before",
        "; Emerald = and before that",
        "[Emerald]",
        "= Emerald",
        "Emeralds = actual",
    ] {
        assert!(text.contains(kept), "{kept:?} was disturbed: {text:?}");
    }
    assert!(text.contains("Emerald = stretch"));
    assert!(!text.contains("Emerald = actual"));
}

/// The line `write` produces and the line `read` understands are decided by one function, so a
/// key that is accepted is a key that comes back. Checked across the punctuation a rom filename
/// really carries rather than on one example.
#[test]
fn every_key_a_write_accepts_reads_back_as_itself() {
    for key in [
        "Emerald",
        "Pokemon - Emerald Version (USA, Europe)",
        "Mario & Luigi",
        "Rhythm Tengoku (J) [!]",
        "F-Zero: Maximum Velocity",
        "Yoshi's Island",
        "50%",
        "Dr. Mario",
    ] {
        let d = root_with(None);
        ini::write(d.path(), FILE, key, "stretch").expect(key);
        assert_eq!(
            ini::value(d.path(), FILE, key).as_deref(),
            Some("stretch"),
            "{key:?} did not come back"
        );
        // Twice, because the second write is the one that has to find the first one's line.
        ini::write(d.path(), FILE, key, "actual").expect(key);
        assert_eq!(ini::value(d.path(), FILE, key).as_deref(), Some("actual"));
        let text = std::fs::read_to_string(d.path().join(FILE)).unwrap();
        assert_eq!(text.lines().count(), 1, "{key:?} left {text:?}");
    }
}

/// A key written twice by hand collapses to one line on the next write, so the file goes on
/// saying one thing per key — the same reading `read` already takes.
#[test]
fn writing_collapses_a_duplicate_the_file_already_had() {
    let d = root_with(Some("Emerald = actual\nEmerald = stretch\n"));
    ini::write(d.path(), FILE, "Emerald", "actual").unwrap();
    let text = std::fs::read_to_string(d.path().join(FILE)).unwrap();
    assert_eq!(text.matches("Emerald").count(), 1);
    assert_eq!(
        ini::value(d.path(), FILE, "Emerald").as_deref(),
        Some("actual")
    );
}

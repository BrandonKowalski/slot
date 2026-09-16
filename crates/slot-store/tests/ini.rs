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

use slot_store::Theme;

/// The card is edited on a desktop by hand. Every way that can go wrong has to leave a device
/// that still boots and a slot that is still visible.
#[test]
fn a_broken_line_leaves_that_colour_alone_and_the_rest_applies() {
    let t = Theme::parse(
        "housing #102030\n\
         recess  not-a-colour\n\
         openin  #ffffff\n\
         edge\n\
         opening #010203\n",
    );
    let d = Theme::default();
    assert_eq!(t.housing, [0x10, 0x20, 0x30], "a good line was dropped");
    assert_eq!(
        t.opening,
        [0x01, 0x02, 0x03],
        "a line after a bad one was lost"
    );
    assert_eq!(t.recess, d.recess, "a malformed value was taken anyway");
    assert_eq!(t.edge, d.edge, "a line with no value was taken anyway");
}

/// `#` opens a comment only at the start of a line, because it is also how a colour is
/// written. Reading it both ways silently drops every colour in the file.
#[test]
fn a_leading_hash_is_a_comment_and_a_value_hash_is_not() {
    let t = Theme::parse("# housing #ffffff\nhousing #102030\n");
    assert_eq!(t.housing, [0x10, 0x20, 0x30]);
}

#[test]
fn a_colour_reads_with_or_without_its_hash() {
    assert_eq!(
        Theme::parse("edge #0a0b0c").edge,
        Theme::parse("edge 0a0b0c").edge
    );
}

/// Trailing junk means the line was meant as something else. Taking the first two words of it
/// turns a typo into a colour nobody chose.
#[test]
fn a_line_with_more_than_a_name_and_a_value_is_ignored() {
    assert_eq!(
        Theme::parse("housing #102030 #405060").housing,
        Theme::default().housing
    );
}

/// A card with no theme is the common case, not an error.
#[test]
fn a_missing_file_is_the_default_theme() {
    let d = tempfile::tempdir().unwrap();
    assert_eq!(Theme::read(d.path()), Theme::default());
}

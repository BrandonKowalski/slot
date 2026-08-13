use slot_ui::code39;

/// Nine elements per character, and the run starts and ends with the sentinel.
#[test]
fn a_hash_encodes_to_nine_elements_per_character_between_sentinels() {
    let run = code39("*9E11A10*").expect("hex and sentinels are in the alphabet");
    assert_eq!(run.len(), 9 * 9, "nine elements per character");
}

/// Every Code 39 character is three wide elements out of nine — that is the whole of the
/// symbology's redundancy, and a pattern that misses it cannot be read by anything.
#[test]
fn every_character_is_three_wide_elements_of_nine() {
    let run = code39("*0123456789ABCDEF*").unwrap();
    for (n, c) in run.chunks_exact(9).enumerate() {
        assert_eq!(c.iter().filter(|w| **w).count(), 3, "character {n}");
    }
}

/// Two wide bars and one wide space, for every character this alphabet has — the sentinel
/// included. Code 39 has 44 symbols and two shapes: 5-choose-2 bars times 4-choose-1 spaces
/// gives 40, and no wide bars times 3-of-4 wide spaces gives 4. The four are `$ / + %`, which
/// leaves the digits, the letters and `*` in the forty. Getting this backwards encodes
/// happily and scans as nothing.
#[test]
fn the_wide_elements_fall_where_the_symbology_says() {
    let run = code39("*0F*").unwrap();
    let bars = |c: &[bool]| (0..9).step_by(2).filter(|i| c[*i]).count();
    let spaces = |c: &[bool]| (1..9).step_by(2).filter(|i| c[*i]).count();
    for (n, c) in run.chunks_exact(9).enumerate() {
        assert_eq!((bars(c), spaces(c)), (2, 1), "character {n}");
    }
}

/// The whole alphabet, so a bare URL encodes: letters, digits, and the symbols a host and a
/// path need.
#[test]
fn the_alphabet_covers_a_bare_url() {
    assert!(code39("*GITHUB.COM/BRANDONKOWALSKI/SLOT*").is_some());
    assert!(code39("*SLOT.KOWALSKI.IO*").is_some());
    assert!(
        code39("*A B-C.D$E/F+G%H*").is_some(),
        "the symbols are all in"
    );
}

/// What it still cannot do, which is why no scheme and no case-sensitive path can go in one
/// of these: Code 39 has no lower case and no colon at all.
#[test]
fn lower_case_and_a_colon_are_refused_rather_than_dropped() {
    assert!(code39("*https*").is_none(), "there is no lower case");
    assert!(code39("*HTTPS://X*").is_none(), "there is no colon");
}

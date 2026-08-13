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

#[test]
fn a_character_outside_the_alphabet_is_refused_rather_than_dropped() {
    assert!(
        code39("*9E11A10 DIRTY*").is_none(),
        "the space is not encodable"
    );
    assert!(code39("*ghijk*").is_none());
}

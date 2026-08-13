//! Code 39, so the label's barcode is a barcode rather than a picture of one. The whole
//! alphabet: digits, letters, and the handful of symbols a bare URL needs.
//!
//! Every symbol is nine elements — five bars and four spaces — of which exactly three are
//! wide. Code 39 has 44 symbols and two shapes for them: two wide bars with one wide space
//! (5-choose-2 times 4-choose-1, forty of them) and no wide bars with three wide spaces
//! (4-choose-3, four of them). The four are `$ / + %`, which leaves the digits, the letters
//! and the `*` sentinel in the forty — the sentinel is shaped like a data character, which is
//! the one thing about this symbology that reads as though it should not be true.
//!
//! The table is derived rather than transcribed. Each group of ten runs the same sequence of
//! bar pairs and differs only in which space is wide, which the seventeen rows checked against
//! a reader confirm exactly. The four no-wide-bar symbols were settled the same way: render
//! each candidate, ask `zbarimg` what it says, keep the one that answers `/`.

/// Element widths in draw units. Code 39 wants the wide element between two and three times
/// the narrow one; 2.5 sits in the middle of what readers accept.
pub const CODE39_NARROW: f32 = 1.0;
pub const CODE39_WIDE: f32 = 2.5;

/// Held at compile time rather than by a test: both sides are constants, so a runtime
/// assertion could only ever restate what the compiler already knows. A ratio outside 2:1 to
/// 3:1 is refused by readers, and the failure is a barcode that simply does not scan.
const _: () = assert!(CODE39_WIDE >= CODE39_NARROW * 2.0);
const _: () = assert!(CODE39_WIDE <= CODE39_NARROW * 3.0);

/// Nine elements per character: bar, space, bar, space, bar, space, bar, space, bar.
fn pattern(c: char) -> Option<[bool; 9]> {
    let bits: [u8; 9] = match c {
        '0' => [0, 0, 0, 1, 1, 0, 1, 0, 0],
        '1' => [1, 0, 0, 1, 0, 0, 0, 0, 1],
        '2' => [0, 0, 1, 1, 0, 0, 0, 0, 1],
        '3' => [1, 0, 1, 1, 0, 0, 0, 0, 0],
        '4' => [0, 0, 0, 1, 1, 0, 0, 0, 1],
        '5' => [1, 0, 0, 1, 1, 0, 0, 0, 0],
        '6' => [0, 0, 1, 1, 1, 0, 0, 0, 0],
        '7' => [0, 0, 0, 1, 0, 0, 1, 0, 1],
        '8' => [1, 0, 0, 1, 0, 0, 1, 0, 0],
        '9' => [0, 0, 1, 1, 0, 0, 1, 0, 0],
        'A' => [1, 0, 0, 0, 0, 1, 0, 0, 1],
        'B' => [0, 0, 1, 0, 0, 1, 0, 0, 1],
        'C' => [1, 0, 1, 0, 0, 1, 0, 0, 0],
        'D' => [0, 0, 0, 0, 1, 1, 0, 0, 1],
        'E' => [1, 0, 0, 0, 1, 1, 0, 0, 0],
        'F' => [0, 0, 1, 0, 1, 1, 0, 0, 0],
        'G' => [0, 0, 0, 0, 0, 1, 1, 0, 1],
        'H' => [1, 0, 0, 0, 0, 1, 1, 0, 0],
        'I' => [0, 0, 1, 0, 0, 1, 1, 0, 0],
        'J' => [0, 0, 0, 0, 1, 1, 1, 0, 0],
        'K' => [1, 0, 0, 0, 0, 0, 0, 1, 1],
        'L' => [0, 0, 1, 0, 0, 0, 0, 1, 1],
        'M' => [1, 0, 1, 0, 0, 0, 0, 1, 0],
        'N' => [0, 0, 0, 0, 1, 0, 0, 1, 1],
        'O' => [1, 0, 0, 0, 1, 0, 0, 1, 0],
        'P' => [0, 0, 1, 0, 1, 0, 0, 1, 0],
        'Q' => [0, 0, 0, 0, 0, 0, 1, 1, 1],
        'R' => [1, 0, 0, 0, 0, 0, 1, 1, 0],
        'S' => [0, 0, 1, 0, 0, 0, 1, 1, 0],
        'T' => [0, 0, 0, 0, 1, 0, 1, 1, 0],
        'U' => [1, 1, 0, 0, 0, 0, 0, 0, 1],
        'V' => [0, 1, 1, 0, 0, 0, 0, 0, 1],
        'W' => [1, 1, 1, 0, 0, 0, 0, 0, 0],
        'X' => [0, 1, 0, 0, 1, 0, 0, 0, 1],
        'Y' => [1, 1, 0, 0, 1, 0, 0, 0, 0],
        'Z' => [0, 1, 1, 0, 1, 0, 0, 0, 0],
        '-' => [0, 1, 0, 0, 0, 0, 1, 0, 1],
        '.' => [1, 1, 0, 0, 0, 0, 1, 0, 0],
        ' ' => [0, 1, 1, 0, 0, 0, 1, 0, 0],
        '$' => [0, 1, 0, 1, 0, 1, 0, 0, 0],
        '/' => [0, 1, 0, 1, 0, 0, 0, 1, 0],
        '+' => [0, 1, 0, 0, 0, 1, 0, 1, 0],
        '%' => [0, 0, 0, 1, 0, 1, 0, 1, 0],
        '*' => [0, 1, 0, 0, 1, 0, 1, 0, 0],
        _ => return None,
    };
    Some(bits.map(|b| b == 1))
}

/// The element run for `text`, `true` where the element is wide. `None` if anything in it is
/// outside the alphabet, which is better than a barcode that silently reads back short.
pub fn code39(text: &str) -> Option<Vec<bool>> {
    let mut out = Vec::with_capacity(text.len() * 9);
    for c in text.chars() {
        out.extend_from_slice(&pattern(c)?);
    }
    Some(out)
}

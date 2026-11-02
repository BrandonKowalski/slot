use unicode_normalization::UnicodeNormalization;

use super::api::Game;

const STEM_BYTES: usize = 200;

pub fn card_stem(game: &Game) -> String {
    let source = [&game.fs_name_no_ext, &game.name]
        .into_iter()
        .find(|s| !s.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| format!("rom-{}", game.id));
    let mut stem: String = source
        .nfc()
        .filter(|c| !c.is_control() && !"\\/:*?\"<>|".contains(*c))
        .collect();
    if stem.len() > STEM_BYTES {
        let mut cut = STEM_BYTES;
        while !stem.is_char_boundary(cut) {
            cut -= 1;
        }
        stem.truncate(cut);
    }
    let stem = stem.trim_matches(|c: char| c == '.' || c.is_whitespace());
    match stem.is_empty() {
        true => format!("rom-{}", game.id),
        false => stem.to_string(),
    }
}

pub fn same_game(a: &str, b: &str) -> bool {
    let a = normalise(a);
    !a.is_empty() && a == normalise(b)
}

pub fn normalise(name: &str) -> String {
    let lower = name.to_lowercase();
    let stem = [".gba", ".gbc", ".gb", ".zip"]
        .iter()
        .find_map(|ext| lower.strip_suffix(ext))
        .unwrap_or(&lower);
    let mut out = String::new();
    let mut depth = 0i32;
    for c in fold_accents(stem).chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            '&' if depth == 0 => out.push_str("and"),
            c if depth == 0 && c.is_ascii_alphanumeric() => out.push(c),
            _ => {}
        }
    }
    out
}

pub fn fold(text: &str) -> String {
    let words: Vec<String> = fold_accents(&text.to_lowercase())
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    words.join(" ")
}

pub fn matches(query: &str, haystack: &str) -> bool {
    let hay = fold(haystack);
    fold(query).split(' ').all(|term| hay.contains(term))
}

fn fold_accents(text: &str) -> String {
    text.nfd()
        .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
        .collect()
}

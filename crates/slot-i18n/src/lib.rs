use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    OnceLock,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    English,
    Japanese,
}
impl Language {
    pub const ALL: [Self; 2] = [Self::English, Self::Japanese];
    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Japanese => "ja",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Japanese => "日本語",
        }
    }
    pub fn parse(code: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|l| l.code() == code)
            .unwrap_or_default()
    }
    pub fn step(self, right: bool) -> Self {
        let at = Self::ALL.iter().position(|l| *l == self).unwrap_or(0);
        Self::ALL[(at + if right { 1 } else { Self::ALL.len() - 1 }) % Self::ALL.len()]
    }
}
static LANGUAGE: AtomicUsize = AtomicUsize::new(0);
/// Set before starting UI workers; wait for existing workers before changing it.
pub fn set_language(language: Language) {
    LANGUAGE.store(language as usize, Ordering::Relaxed);
}
pub fn language() -> Language {
    Language::ALL
        .get(LANGUAGE.load(Ordering::Relaxed))
        .copied()
        .unwrap_or_default()
}
fn japanese() -> &'static HashMap<&'static str, &'static str> {
    static CATALOG: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    CATALOG.get_or_init(|| parse_catalog(include_str!("../locales/ja.tsv")))
}
fn parse_catalog(text: &str) -> HashMap<&str, &str> {
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split_once('\t'))
        .filter(|(_, value)| !value.trim().is_empty())
        .collect()
}

pub fn translate(language: Language, english: &str) -> &str {
    match language {
        Language::English => english,
        Language::Japanese => japanese().get(english).copied().unwrap_or(english),
    }
}
pub fn tr(english: &str) -> &str {
    translate(language(), english)
}
/// Named placeholders can be reordered by translators. Values are never translated.
pub fn format(english: &str, args: &[(&str, &str)]) -> String {
    let text = tr(english);
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = &rest[start + 1..start + end];
        out.push_str(
            args.iter()
                .find(|(key, _)| *key == name)
                .map_or(&rest[start..start + end + 1], |(_, value)| *value),
        );
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn blank_translations_are_missing() {
        let catalog = super::parse_catalog("Back\t戻る\nNew\t\nSpace\t   \n");
        for english in ["New", "Space", "Absent"] {
            assert_eq!(catalog.get(english).copied().unwrap_or(english), english);
        }
        assert_eq!(catalog.get("Back"), Some(&"戻る"));
    }
}

use slot_i18n::{translate, Language};
use std::collections::{BTreeSet, HashMap};

fn catalog(text: &str) -> HashMap<&str, &str> {
    let mut out = HashMap::new();
    for line in text
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let (key, value) = line.split_once('\t').expect("tab-separated entry");
        assert!(!key.is_empty());
        assert!(out.insert(key, value).is_none(), "duplicate key: {key}");
    }
    out
}
fn placeholders(text: &str) -> BTreeSet<&str> {
    text.split('{')
        .skip(1)
        .map(|p| p.split_once('}').expect("closed placeholder").0)
        .collect()
}
#[test]
fn catalog_keys_and_placeholders_match_english() {
    let en = catalog(include_str!("../locales/en.tsv"));
    let ja = catalog(include_str!("../locales/ja.tsv"));
    for (key, translated) in ja {
        assert!(en.contains_key(key), "unknown key: {key}");
        assert_eq!(placeholders(key), placeholders(translated), "{key}");
    }
}
#[test]
fn missing_translations_and_unknown_languages_fall_back_to_english() {
    assert_eq!(
        translate(Language::Japanese, "A newly added message"),
        "A newly added message"
    );
    assert_eq!(translate(Language::English, "Language"), "Language");
    assert_eq!(translate(Language::Japanese, "Language"), "言語");
    assert_eq!(Language::parse("unknown"), Language::English);
    assert_eq!(Language::parse(""), Language::English);
    for lang in Language::ALL {
        assert_eq!(Language::parse(lang.code()), lang);
        assert_eq!(lang.step(true).step(false), lang);
    }
}
// Only this test changes the active language in this test process.
#[test]
fn parameters_can_move_and_are_never_retranslated_or_reexpanded() {
    slot_i18n::set_language(Language::Japanese);
    assert_eq!(
        slot_i18n::format("{index} of {total}", &[("index", "2"), ("total", "5")]),
        "5枚中2枚目"
    );
    assert_eq!(
        slot_i18n::format("Same as {button}", &[("button", "{count} Language")]),
        "{count} Languageと重複"
    );
    slot_i18n::set_language(Language::English);
}

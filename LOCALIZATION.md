# Localization

Open the shelf menu and choose **General → Language**.
Changes apply immediately and are saved as `language=en` or `language=ja` in
`Config/slot.state`. Existing installations and unknown language codes use English.

Japanese ROM names render in either language, on cartridge labels, boards and
shelf titles. These names are never looked up as translation keys. The bundled
Japanese font also works offline on devices without system fonts.

## Translating

`crates/slot-i18n/locales/en.tsv` is the English reference catalog. Each UTF-8 line
contains an English source string, one tab, and its translation. Lines starting
with `#` are comments. `ja.tsv` contains the Japanese translations. Missing,
empty, or whitespace-only translations fall back to the original English string.
The English source text is the key; changing it requires updating the catalogs.
Technical identifiers and physical button legends retain their original names.

To add a language:

1. Copy `en.tsv` to `<language-code>.tsv` and translate the second column. Preserve
   named parameters such as `{count}`; they may move within the sentence.
2. Add a `Language` variant, stable code and native name in `slot-i18n/src/lib.rs`,
   include it in `Language::ALL`, and register its embedded catalog in `translate`
   using a `OnceLock` like the Japanese catalog. The menu cycles `Language::ALL`;
   it does not need a new row or value per language.
3. Check date conventions in `slot-ui/src/clock.rs`, and add any necessary glyph
   coverage to `text::glyph_font`. The current renderer supports Latin and
   Japanese; scripts requiring shaping or bidirectional layout need additional
   renderer support.
4. Add the catalog to the catalog validation tests, run the tests below, and
   check the UI at its smallest supported size (640 × 480).

Use `slot_i18n::tr("English text")` for static UI text and
`slot_i18n::format("Delete {count} screenshots?", &[("count", &count.to_string())])`
for messages with parameters. Do not construct an English sentence before looking
it up: each complete template belongs in the catalog so translators can reorder
its parts. Parameter values are inserted once and are never translated.

UI-specific helpers (`menu_face`, `quick_value_face`, hints and word labels) look
up their text before measuring it. Low-level text layout, cartridge labels and
`title_line_face` deliberately draw literal text. Keep user-provided ROM names,
paths, configuration keys and libretro values out of translation lookups.

The active locale is process-wide. Boot initializes it before UI workers start.
The frontend waits for the existing UI worker before changing locale, rebuilds
localized faces and reuses their GPU textures. When introducing another cached
UI string, include it in the language refresh path in `Frontend::sync_language`.
Locale-changing tests must own their process or serialize access to the locale.

## Validation and previews

```sh
cargo test -p slot-i18n -p slot-store -p slot-ui
cargo test -p slot --no-default-features --test quick_menu --test clock --test in_game_menu
cargo run -p slot-ui --example localization -- /tmp/slot-ja.png ja
cargo run -p slot-ui --example localization -- /tmp/slot-en.png en
```

The preview uses the actual text renderer without a GPU. It includes paired menu
labels and values, Japanese ROM names, confirmation messages, link status and a
localized date. Test the full UI on hardware as well, particularly menus, clock
editing, screenshots, save states and repeated language switching.

## Japanese font

`slot-ui/assets/NotoSansJP.ttf` is a static weight-500 instance of Noto Sans JP,
licensed under the SIL Open Font License (the adjacent `.OFL.txt`, also copied to
`licenses/NotoSansJP-OFL.txt` for device distributions). It retains
the source font's complete character repertoire rather than subsetting to UI
strings, so ROM names can use characters absent from the translations. It adds
approximately 5.5 MiB to the binary. Font loading is lazy and shared, and the
original Latin face is used whenever it has the requested glyph.

Source: <https://github.com/google/fonts/tree/main/ofl/notosansjp>

The downloaded `NotoSansJP[wght].ttf` had SHA-256
`c2f3b4d463500a2ddcd3849cded1fceeb9fd6d1c32e6cbecd568453ba50fc68f`.
The bundled instance was produced with FontTools 4.63.0:

```sh
fonttools varLib.instancer 'NotoSansJP[wght].ttf' wght=500 --output NotoSansJP.ttf
```

The font is used per glyph for both measurement and rasterization. Text is NFC
normalized so decomposed kana render consistently. Japanese text can wrap at
character boundaries without requiring spaces. This is basic character wrapping,
not full Japanese typographic line-breaking rules.

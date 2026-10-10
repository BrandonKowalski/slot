mod common;

use slot::wallpaper::pick;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn write_png(dir: &Path, name: &str) {
    let f = std::fs::File::create(dir.join(name)).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), 2, 2);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    e.write_header()
        .unwrap()
        .write_image_data(&[0u8; 12])
        .unwrap();
}

fn papers(names: &[&str]) -> (tempfile::TempDir, PathBuf) {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    let dir = d.path().join("Wallpapers");
    std::fs::create_dir_all(&dir).unwrap();
    for n in names {
        write_png(&dir, n);
    }
    (d, dir)
}

fn name(p: PathBuf) -> String {
    p.file_name().unwrap().to_string_lossy().into_owned()
}

#[test]
fn a_card_with_no_wallpapers_picks_nothing() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    assert!(pick(d.path(), 0).is_none());
    std::fs::create_dir_all(d.path().join("Wallpapers")).unwrap();
    assert!(pick(d.path(), 7).is_none(), "an empty folder picked a file");
}

#[test]
fn every_wallpaper_shows_once_before_any_repeats() {
    let (d, _) = papers(&["a.png", "b.png", "c.png", "d.png", "e.png"]);
    for round in 0..4 {
        let shown: Vec<_> = (0..5)
            .map(|i| name(pick(d.path(), (round * 5 + i) * 60).unwrap()))
            .collect();
        assert_eq!(
            shown.iter().collect::<HashSet<_>>().len(),
            5,
            "round {round} showed {shown:?}"
        );
    }
}

#[test]
fn a_new_round_never_opens_on_the_last_wallpaper() {
    let (d, _) = papers(&["a.png", "b.png", "c.png"]);
    let mut last = name(pick(d.path(), 0).unwrap());
    for seed in 1..60 {
        let next = name(pick(d.path(), seed * 60).unwrap());
        assert_ne!(next, last, "seed {seed} repeated {last}");
        last = next;
    }
}

#[test]
fn two_wallpapers_alternate() {
    let (d, _) = papers(&["psyduck.png", "aussyduck.png"]);
    let shown: Vec<_> = (0..6)
        .map(|s| name(pick(d.path(), s * 60).unwrap()))
        .collect();
    for pair in shown.windows(2) {
        assert_ne!(pair[0], pair[1], "two wallpapers gave {shown:?}");
    }
}

#[test]
fn an_added_wallpaper_joins_the_current_round() {
    let (d, dir) = papers(&["a.png", "b.png", "c.png", "d.png"]);
    let first = name(pick(d.path(), 0).unwrap());
    write_png(&dir, "new.png");
    let rest: Vec<_> = (1..5)
        .map(|s| name(pick(d.path(), s * 60).unwrap()))
        .collect();
    assert!(
        rest.contains(&"new.png".to_string()),
        "the added wallpaper missed the round: {first} then {rest:?}"
    );
    let round: HashSet<_> = std::iter::once(&first).chain(&rest).collect();
    assert_eq!(round.len(), 5, "the round repeated: {first} then {rest:?}");
}

#[test]
fn a_removed_wallpaper_is_never_picked() {
    let (d, dir) = papers(&["a.png", "b.png", "c.png"]);
    let first = name(pick(d.path(), 0).unwrap());
    let gone = ["a.png", "b.png", "c.png"]
        .into_iter()
        .find(|n| *n != first)
        .unwrap();
    std::fs::remove_file(dir.join(gone)).unwrap();
    for seed in 1..20 {
        let p = pick(d.path(), seed * 60).unwrap();
        assert!(p.exists(), "seed {seed} picked {p:?}");
        assert_ne!(name(p), gone, "seed {seed} picked the removed file");
    }
}

#[test]
fn the_shown_wallpaper_being_removed_moves_on() {
    let (d, dir) = papers(&["a.png", "b.png"]);
    let first = name(pick(d.path(), 0).unwrap());
    std::fs::remove_file(dir.join(&first)).unwrap();
    let next = name(pick(d.path(), 60).unwrap());
    assert_ne!(next, first);
    assert_eq!(name(pick(d.path(), 120).unwrap()), next, "one file left");
}

#[test]
fn a_damaged_deck_starts_a_fresh_one() {
    let (d, _) = papers(&["a.png", "b.png"]);
    std::fs::create_dir_all(d.path().join("Config")).unwrap();
    std::fs::write(d.path().join("Config/wallpaper.deck"), b"\xff\xfe junk").unwrap();
    assert!(pick(d.path(), 0).is_some());
}

#[test]
fn a_sidecar_is_never_the_wallpaper() {
    let (d, dir) = papers(&["psyduck.png"]);
    write_png(&dir, "._psyduck.png");
    for seed in 0..8 {
        let picked = pick(d.path(), seed).expect("a wallpaper was present");
        assert_eq!(
            picked.file_name().unwrap(),
            "psyduck.png",
            "seed {seed} picked the sidecar"
        );
    }
}

#[test]
fn only_pngs_are_picked() {
    let (d, dir) = papers(&[]);
    std::fs::write(dir.join("notes.txt"), b"hello").unwrap();
    std::fs::write(dir.join("art.jpg"), b"not a png either").unwrap();
    assert!(pick(d.path(), 0).is_none(), "a non png was picked");
    write_png(&dir, "real.PNG");
    assert!(
        pick(d.path(), 0).is_some(),
        "an upper case extension was skipped"
    );
}

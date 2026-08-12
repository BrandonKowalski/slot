mod common;

use slot::wallpaper::pick;
use std::path::Path;

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

#[test]
fn a_card_with_no_wallpapers_picks_nothing() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    assert!(pick(d.path(), 0).is_none());
    std::fs::create_dir_all(d.path().join("Wallpapers")).unwrap();
    assert!(pick(d.path(), 7).is_none(), "an empty folder picked a file");
}

/// The seed is the wall clock at boot, so a card that has been off overnight comes back with
/// a different picture. Two seeds that land on the same file is fine; never moving is not.
#[test]
fn the_seed_chooses_between_the_files_present() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    let dir = d.path().join("Wallpapers");
    std::fs::create_dir_all(&dir).unwrap();
    for n in ["a.png", "b.png", "c.png"] {
        write_png(&dir, n);
    }
    let picked: Vec<_> = (0..3).map(|s| pick(d.path(), s).unwrap()).collect();
    assert_eq!(
        picked
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        3,
        "three seeds over three files gave {picked:?}"
    );
    assert_eq!(
        pick(d.path(), 4).unwrap(),
        pick(d.path(), 1).unwrap(),
        "the same seed on the same card gave a different picture"
    );
}

/// The card is loaded from a Mac, which leaves a sidecar beside every file it copies. They
/// carry the extension of the file they shadow and decode as nothing.
#[test]
fn a_sidecar_is_never_the_wallpaper() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    let dir = d.path().join("Wallpapers");
    std::fs::create_dir_all(&dir).unwrap();
    write_png(&dir, "psyduck.png");
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

/// The folder is the user's and they will put other things in it.
#[test]
fn only_pngs_are_picked() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    let dir = d.path().join("Wallpapers");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("notes.txt"), b"hello").unwrap();
    std::fs::write(dir.join("art.jpg"), b"not a png either").unwrap();
    assert!(pick(d.path(), 0).is_none(), "a non png was picked");
    write_png(&dir, "real.PNG");
    assert!(
        pick(d.path(), 0).is_some(),
        "an upper case extension was skipped"
    );
}

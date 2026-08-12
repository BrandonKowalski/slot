mod common;

use tempfile::tempdir;

/// `task dist` and `task sdcard` both build the layout by running the binary with
/// `--init-root`, so `root::ensure` is the only implementation of it. This is what keeps a
/// card the app cannot read from being assembled in the first place.
#[test]
fn ensure_creates_the_six_folders_and_nothing_else() {
    let d = tempdir().unwrap();
    let out = d.path().join("dist");
    slot::root::ensure(&out);
    let mut got: Vec<String> = std::fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    got.sort();
    let mut want: Vec<String> = slot::root::DIRS.iter().map(|s| s.to_string()).collect();
    want.sort();
    assert_eq!(got, want);
}

/// `task sdcard` points at a directory the caller already keeps roms in.
#[test]
fn ensure_leaves_existing_content_alone() {
    let d = tempdir().unwrap();
    let out = d.path().join("dist");
    std::fs::create_dir_all(out.join("Games")).unwrap();
    std::fs::write(out.join("Games/Emerald.gba"), b"rom").unwrap();
    slot::root::ensure(&out);
    assert_eq!(
        std::fs::read(out.join("Games/Emerald.gba")).unwrap(),
        b"rom"
    );
}

#[test]
fn a_booted_app_root_has_the_same_six_folders() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    for name in slot::root::DIRS {
        assert!(d.path().join(name).is_dir(), "app root is missing {name}");
    }
}

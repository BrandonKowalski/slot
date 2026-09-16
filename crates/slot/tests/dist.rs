mod common;

use tempfile::tempdir;

/// `task dist` and `task sdcard` both build the layout by running the binary with
/// `--init-root`, so `root::ensure` is the only implementation of it. This is what keeps a
/// card the app cannot read from being assembled in the first place.
#[test]
fn ensure_creates_every_folder_dirs_names_and_nothing_else() {
    let d = tempdir().unwrap();
    let out = d.path().join("dist");
    slot::root::ensure(&out);

    // Every entry `DIRS` names is created, `Games/GBA` and its siblings included: `join`
    // and `is_dir` both follow a `/` the same as any other path.
    for name in slot::root::DIRS {
        assert!(out.join(name).is_dir(), "{name} was not created");
    }

    // And nothing extra sits beside them. `DIRS` now names some entries one level under a
    // top level folder rather than only at the top level, so what a plain, non-recursive
    // `read_dir` of `out` sees is each entry's first path segment, not `DIRS` itself.
    let mut got: Vec<String> = std::fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    got.sort();
    let mut want: Vec<String> = slot::root::DIRS
        .iter()
        .map(|s| s.split('/').next().unwrap().to_string())
        .collect();
    want.sort();
    want.dedup();
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

/// Named after `DIRS` rather than after a count, as the test above now is. The array grew from
/// seven entries to ten and then to thirteen inside one plan, and the `six` both these names
/// used to carry was never right at any of the three — a count in a test name is a second copy
/// of a fact the assertion below already reads straight off `DIRS`, and nothing fails when the
/// copy goes stale.
#[test]
fn a_booted_app_root_has_the_same_folders() {
    let d = common::tmp_root_with_carts(&["Emerald"]);
    for name in slot::root::DIRS {
        assert!(d.path().join(name).is_dir(), "app root is missing {name}");
    }
}

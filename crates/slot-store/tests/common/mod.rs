use tempfile::TempDir;

pub fn tmp_root() -> TempDir {
    let d = tempfile::tempdir().expect("tempdir");
    for sub in ["Games", "Labels", "Saves", "States", "System"] {
        std::fs::create_dir(d.path().join(sub)).expect("create content dir");
    }
    d
}

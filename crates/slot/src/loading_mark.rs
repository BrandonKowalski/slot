use std::path::{Path, PathBuf};

pub const RUN_VAR: &str = "SLOT_RUN";
const MARK: &str = "slot-loading";

#[derive(Clone, Debug)]
pub struct LoadingMark(PathBuf);

impl LoadingMark {
    pub fn in_dir(dir: &Path) -> Self {
        LoadingMark(dir.join(MARK))
    }

    pub fn from_env() -> Option<Self> {
        std::env::var_os(RUN_VAR).map(|dir| Self::in_dir(Path::new(&dir)))
    }

    pub fn set(&self, stem: &str) {
        if let Err(e) = std::fs::write(&self.0, stem) {
            eprintln!("slot: loading mark: {}: {e}", self.0.display());
        }
    }

    pub fn clear(&self) {
        let _ = std::fs::remove_file(&self.0);
    }

    pub fn left(&self) -> Option<String> {
        std::fs::read_to_string(&self.0).ok()
    }
}

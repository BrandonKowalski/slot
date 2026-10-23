use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::platform::Platform;
use crate::ring::is_stamp;
use crate::stamp::format_stamp;

pub const SCREENSHOTS_DIR: &str = "Screenshots";

const SHOT_EXT: &str = "png";

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ShotEntry {
    pub stamp: String,
    pub path: PathBuf,
}

pub fn shot_stamp(millis: i64) -> String {
    format!(
        "{}-{:03}",
        format_stamp(millis.div_euclid(1000)),
        millis.rem_euclid(1000)
    )
}

fn is_shot_stamp(s: &str) -> bool {
    let (Some(secs), Some(ms)) = (s.get(..19), s.get(19..)) else {
        return false;
    };
    is_stamp(secs)
        && ms.len() == 4
        && ms.starts_with('-')
        && ms[1..].bytes().all(|c| c.is_ascii_digit())
}

fn shot_stem(path: &Path) -> Option<&str> {
    if path.extension().and_then(|e| e.to_str()) != Some(SHOT_EXT) {
        return None;
    }
    path.file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| is_shot_stamp(s))
}

#[derive(Clone)]
pub struct ShotDir {
    dir: PathBuf,
}

impl ShotDir {
    pub fn new(root: &Path, platform: Platform, stem: &str) -> Self {
        ShotDir {
            dir: root
                .join(SCREENSHOTS_DIR)
                .join(platform.dir_name())
                .join(stem),
        }
    }

    pub fn list(&self) -> std::io::Result<Vec<ShotEntry>> {
        let dir = match std::fs::read_dir(&self.dir) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };

        let mut entries = Vec::new();
        for entry in dir {
            let path = entry?.path();
            let Some(stamp) = shot_stem(&path) else {
                continue;
            };
            entries.push(ShotEntry {
                stamp: stamp.to_string(),
                path: self.path(stamp),
            });
        }
        entries.sort_by(|a, b| b.stamp.cmp(&a.stamp));
        Ok(entries)
    }

    pub fn any(&self, skip: &HashSet<PathBuf>) -> bool {
        let Ok(dir) = std::fs::read_dir(&self.dir) else {
            return false;
        };
        dir.flatten()
            .map(|e| e.path())
            .any(|p| shot_stem(&p).is_some() && !skip.contains(&p))
    }

    pub fn path(&self, stamp: &str) -> PathBuf {
        self.dir.join(format!("{stamp}.{SHOT_EXT}"))
    }

    pub fn remove(&self, stamp: &str) -> std::io::Result<()> {
        if !is_shot_stamp(stamp) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("not a screenshot stamp: {stamp}"),
            ));
        }
        std::fs::remove_file(self.path(stamp))
    }
}

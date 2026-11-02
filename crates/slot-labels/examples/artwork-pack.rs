//! Builds the optional `Artwork.pack`: for every dump in the LaunchBox index the device carries,
//! the label it would download, made ahead of time. Each dump's cart fronts are tried in the
//! index's order and the first that crops is kept, exactly as the device chooses online.
//!
//!     artwork-pack <launchbox.tsv> <cache> <Artwork.pack> GB.dat GBC.dat GBA.dat
//!
//! The dats say which platform's label each dump gets. Downloads are kept in the cache by file
//! name, so a later build fetches only images it has not seen.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

#[path = "shared/dat.rs"]
mod dat;

use slot_labels::build;
use slot_store::Platform;

const IMAGES: &str = "https://images.launchbox-app.com/";
const AGENT: &str = concat!("slot/", env!("CARGO_PKG_VERSION"), " artwork-pack");
const THREADS: usize = 4;

/// An image's bytes, from the cache or LaunchBox; `None` when LaunchBox no longer has it.
fn image(cache: &Path, file: &str) -> Result<Option<Vec<u8>>, String> {
    let path = cache.join(file);
    if let Ok(bytes) = std::fs::read(&path) {
        return Ok(Some(bytes));
    }
    let mut last = String::new();
    for attempt in 0..4u32 {
        match ureq::get(format!("{IMAGES}{file}"))
            .header("User-Agent", AGENT)
            .call()
        {
            Ok(mut response) => {
                let bytes = response
                    .body_mut()
                    .with_config()
                    .limit(32 << 20)
                    .read_to_vec()
                    .map_err(|e| e.to_string())?;
                let part = path.with_extension("part");
                std::fs::write(&part, &bytes).map_err(|e| e.to_string())?;
                std::fs::rename(&part, &path).map_err(|e| e.to_string())?;
                return Ok(Some(bytes));
            }
            Err(ureq::Error::StatusCode(404 | 410)) => return Ok(None),
            Err(e) => last = e.to_string(),
        }
        std::thread::sleep(Duration::from_secs(2 << attempt));
    }
    Err(last)
}

fn main() {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [index, cache, out, gb, gbc, gba] = args.as_slice() else {
        eprintln!(
            "usage: artwork-pack <launchbox.tsv> <cache> <Artwork.pack> GB.dat GBC.dat GBA.dat"
        );
        std::process::exit(2);
    };
    std::fs::create_dir_all(cache).expect("cache");
    let mut platforms = HashMap::new();
    for (platform, dat) in [
        (Platform::Gb, gb),
        (Platform::Gbc, gbc),
        (Platform::Gba, gba),
    ] {
        for dump in dat::dumps(dat) {
            platforms.entry(dump.crc).or_insert(platform);
        }
    }
    let text = std::fs::read_to_string(index).expect("launchbox.tsv");
    let jobs: Vec<(u32, Platform, Vec<&str>)> = text
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let crc = u32::from_str_radix(fields.next()?, 16).ok()?;
            Some((crc, *platforms.get(&crc)?, fields.collect()))
        })
        .collect();
    eprintln!("{} dumps", jobs.len());

    let next = AtomicUsize::new(0);
    let entries = Mutex::new(Vec::new());
    let (none, failed) = (AtomicUsize::new(0), AtomicUsize::new(0));
    // A few at a time: LaunchBox serves these to its own app, not to bulk downloads.
    std::thread::scope(|s| {
        for _ in 0..THREADS {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some((crc, platform, files)) = jobs.get(i) else {
                    break;
                };
                let mut label = None;
                for file in files {
                    match image(cache, file) {
                        Ok(Some(bytes)) => {
                            if let Ok(jpeg) = build::cart_label_jpeg(&bytes, *platform) {
                                label = Some(jpeg);
                                break;
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            eprintln!("{file}: {e}");
                            failed.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
                match label {
                    Some(jpeg) => entries.lock().unwrap().push((*crc, jpeg)),
                    None => {
                        none.fetch_add(1, Ordering::Relaxed);
                    }
                }
                if (i + 1).is_multiple_of(250) {
                    eprintln!("{} / {}", i + 1, jobs.len());
                }
            });
        }
    });
    let entries = entries.into_inner().unwrap();
    let count = entries.len();
    let mut file = std::io::BufWriter::new(std::fs::File::create(out).expect("create pack"));
    build::write_pack(&mut file, entries).expect("write pack");
    std::io::Write::flush(&mut file).expect("write pack");
    eprintln!(
        "{}: {count} labels; {} dumps with no usable cart, {} downloads failed (run again to retry)",
        out.display(),
        none.into_inner(),
        failed.into_inner()
    );
}

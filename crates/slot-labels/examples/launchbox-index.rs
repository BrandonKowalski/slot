//! Builds `data/launchbox.tsv`: for each No-Intro dump, the LaunchBox cart fronts the device
//! would choose for it, keyed by the dump's CRC32. Games are matched by title exactly as the
//! device's own search matches them, then by LaunchBox's alternate names, and a title that names
//! more than one game is left out rather than guessed.
//!
//!     launchbox-index Metadata.xml GB.dat GBC.dat GBA.dat > data/launchbox.tsv
//!
//! Metadata.xml is in LaunchBox's https://gamesdb.launchbox-app.com/Metadata.zip, and the dats are
//! libretro-database's metadat/no-intro files. `task labels:index` fetches both.
use std::collections::{BTreeMap, HashMap};

#[path = "shared/dat.rs"]
mod dat;

use quick_xml::escape::resolve_xml_entity;
use quick_xml::events::Event;
use quick_xml::Reader;
use slot_labels::build;
use slot_store::Platform;

type Images = Vec<(String, String, String)>;

#[derive(Default)]
struct Metadata {
    games: HashMap<String, (String, String)>,
    alternates: Vec<(String, String)>,
    images: HashMap<String, Images>,
}

/// One pass over the 500 MB file, keeping only the three platforms' games and their cart fronts.
fn metadata(path: &str, platforms: &[&str]) -> Metadata {
    let mut reader = Reader::from_file(path).expect("Metadata.xml");
    let mut buf = Vec::new();
    let mut fields: HashMap<String, String> = HashMap::new();
    let mut field = None::<String>;
    let mut out = Metadata::default();
    loop {
        match reader
            .read_event_into(&mut buf)
            .expect("well-formed Metadata.xml")
        {
            Event::Start(e) => {
                field = Some(String::from_utf8_lossy(e.name().as_ref()).into_owned());
            }
            Event::Text(t) => {
                if let Some(f) = &field {
                    fields
                        .entry(f.clone())
                        .or_default()
                        .push_str(&t.decode().unwrap());
                }
            }
            Event::GeneralRef(r) => {
                if let Some(f) = &field {
                    let text = match r.resolve_char_ref().unwrap() {
                        Some(c) => c.to_string(),
                        None => resolve_xml_entity(&r.decode().unwrap())
                            .unwrap_or_default()
                            .to_owned(),
                    };
                    fields.entry(f.clone()).or_default().push_str(&text);
                }
            }
            Event::End(e) => {
                field = None;
                let mut take = |k: &str| fields.remove(k).unwrap_or_default();
                match e.name().as_ref() {
                    b"Game" => {
                        let platform = take("Platform");
                        if platforms.contains(&platform.as_str()) {
                            out.games
                                .insert(take("DatabaseID"), (take("Name"), platform));
                        }
                    }
                    b"GameAlternateName" => {
                        out.alternates
                            .push((take("DatabaseID"), take("AlternateName")));
                    }
                    b"GameImage" => {
                        let kind = take("Type");
                        if kind == build::PHOTO || kind == build::RENDER {
                            let id = take("DatabaseID");
                            let image = (kind, take("Region"), take("FileName"));
                            out.images.entry(id).or_default().push(image);
                        }
                    }
                    _ => continue,
                }
                fields.clear();
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [metadata_xml, gb, gbc, gba] = args.as_slice() else {
        eprintln!("usage: launchbox-index Metadata.xml GB.dat GBC.dat GBA.dat");
        std::process::exit(2);
    };
    let consoles = [
        (Platform::Gb, gb),
        (Platform::Gbc, gbc),
        (Platform::Gba, gba),
    ];
    let names: Vec<&str> = consoles
        .iter()
        .map(|(p, _)| build::platforms(*p)[0])
        .collect();
    let meta = metadata(metadata_xml, &names);

    // Exact names first: an alternate name only decides a title no game is primarily called.
    let mut by_name: HashMap<(&str, String), Vec<&str>> = HashMap::new();
    let mut by_alternate: HashMap<(&str, String), Vec<&str>> = HashMap::new();
    for (id, (name, platform)) in &meta.games {
        by_name
            .entry((platform, build::normalize(name)))
            .or_default()
            .push(id);
    }
    for (id, name) in &meta.alternates {
        if let Some((_, platform)) = meta.games.get(id) {
            let ids = by_alternate
                .entry((platform, build::normalize(name)))
                .or_default();
            if !ids.contains(&id.as_str()) {
                ids.push(id);
            }
        }
    }
    // The device's hand-picked games are website IDs, which are not the metadata's DatabaseIDs.
    let game = |platform: Platform, title: &str| -> Option<String> {
        for table in [&by_name, &by_alternate] {
            for name in build::platforms(platform) {
                match table.get(&(*name, title.to_owned())).map(Vec::as_slice) {
                    Some([id]) => return Some(id.to_string()),
                    Some([_, _, ..]) => return None,
                    _ => {}
                }
            }
        }
        None
    };

    let plain = |f: &str| {
        !f.is_empty()
            && f.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    };
    let mut index = BTreeMap::new();
    let mut counts = Vec::new();
    for (platform, dat) in consoles {
        let (mut all, mut found) = (0, 0);
        for dat::Dump { name, crc, .. } in dat::dumps(dat.as_ref()) {
            all += 1;
            let title = build::title(&name);
            let Some(id) = Some(&title)
                .filter(|t| !t.is_empty())
                .and_then(|t| game(platform, t))
            else {
                continue;
            };
            let images: Vec<(&str, &str, &str)> = meta
                .images
                .get(&id)
                .into_iter()
                .flatten()
                .filter(|(_, _, file)| plain(file))
                .map(|(k, r, f)| (k.as_str(), r.as_str(), f.as_str()))
                .collect();
            let files = build::rank(&name, "", &images);
            if !files.is_empty() && !index.contains_key(&crc) {
                index.insert(crc, files);
                found += 1;
            }
        }
        counts.push(format!("{platform:?} {found}/{all}"));
    }
    eprintln!("cart fronts for {}", counts.join(", "));

    println!(
        "# LaunchBox cart fronts by No-Intro CRC32, best first. Built by `task labels:index`;"
    );
    println!("# do not edit by hand. Images: https://images.launchbox-app.com/<file>");
    for (crc, files) in index {
        println!("{crc:08X}\t{}", files.join("\t"));
    }
}

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use slot_store::{Cart, Platform};

use super::api::{content_url, Game};
use super::http::{ok, Http, Req, JSON_LIMIT, ROM_TIMEOUT};
use super::label::{crop, find_box, Rgba};
use super::name::card_stem;
use super::{RommFail, Server};

pub const ROM_LIMIT: u64 = 256 * 1024 * 1024;
const PHOTO_LIMIT: u64 = 32 * 1024 * 1024;
const SPARE: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Downloading { got: u64, total: u64 },
    Labelling,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub platform: Platform,
    pub stem: String,
    pub labelled: bool,
}

pub fn on_card(root: &Path, platform: Platform, stem: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(root.join("Games").join(platform.dir_name())) else {
        return false;
    };
    entries.flatten().any(|e| {
        let path = e.path();
        platform.accepts(&path)
            && path
                .file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case(stem))
    })
}

pub struct Order<'a> {
    pub server: &'a Server,
    pub root: &'a Path,
    pub platform: Platform,
    pub game: &'a Game,
}

pub fn install(
    http: &dyn Http,
    order: Order,
    cancelled: &dyn Fn() -> bool,
    step: &mut dyn FnMut(Step),
    fallback_label: &dyn Fn(&Cart) -> bool,
) -> Result<Installed, RommFail> {
    let Order {
        server,
        root,
        platform,
        game,
    } = order;
    let stem = card_stem(game);
    if on_card(root, platform, &stem) {
        return Err(RommFail::Exists);
    }
    let free = crate::update::free_space(root);
    if game.size > 0 && free > 0 && free < game.size + SPARE {
        return Err(RommFail::Space);
    }
    let dir = root.join("Games").join(platform.dir_name());
    std::fs::create_dir_all(&dir).map_err(|_| RommFail::Write)?;
    let download = dir.join(format!(".{stem}.download"));
    let unpacked = dir.join(format!(".{stem}.rom"));
    let result = fetch_rom(http, server, game, &download, cancelled, step)
        .and_then(|()| unpack(&download, &unpacked, platform, &game.fs_name))
        .and_then(|(from, ext)| {
            let path = dir.join(format!("{stem}.{ext}"));
            if on_card(root, platform, &stem) {
                return Err(RommFail::Exists);
            }
            std::fs::rename(&from, &path).map_err(|_| RommFail::Write)
        });
    let _ = std::fs::remove_file(&download);
    let _ = std::fs::remove_file(&unpacked);
    result?;
    step(Step::Labelling);
    let cart = slot_store::scan_platform(root, platform)
        .unwrap_or_default()
        .into_iter()
        .find(|c| c.stem == stem);
    let labelled = cart.is_some_and(|cart| {
        romm_label(http, server, root, &cart, game).is_some() || fallback_label(&cart)
    });
    Ok(Installed {
        platform,
        stem,
        labelled,
    })
}

fn fetch_rom(
    http: &dyn Http,
    server: &Server,
    game: &Game,
    to: &Path,
    cancelled: &dyn Fn() -> bool,
    step: &mut dyn FnMut(Step),
) -> Result<(), RommFail> {
    let mut req = Req::get(server, content_url(server, game));
    req.timeout = ROM_TIMEOUT;
    let reply = ok(http.call(&req)?)?;
    let total = reply.length.filter(|n| *n > 0).unwrap_or(game.size);
    if total > ROM_LIMIT {
        return Err(RommFail::Bad);
    }
    let mut body = reply.body.take(ROM_LIMIT + 1);
    let mut file = File::create(to).map_err(|_| RommFail::Write)?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut got = 0u64;
    step(Step::Downloading { got, total });
    loop {
        if cancelled() {
            return Err(RommFail::Cancelled);
        }
        let n = body.read(&mut buf).map_err(|_| RommFail::Offline)?;
        if n == 0 {
            break;
        }
        got += n as u64;
        if got > ROM_LIMIT {
            return Err(RommFail::Bad);
        }
        file.write_all(&buf[..n]).map_err(write_fail)?;
        step(Step::Downloading { got, total });
    }
    if got == 0 {
        return Err(RommFail::Bad);
    }
    file.sync_all().map_err(write_fail)
}

fn write_fail(e: std::io::Error) -> RommFail {
    match e.raw_os_error() == Some(libc::ENOSPC) {
        true => RommFail::Space,
        false => RommFail::Write,
    }
}

fn extension_of(name: &str) -> Option<String> {
    let (_, ext) = name.rsplit_once('.')?;
    Some(ext.to_ascii_lowercase())
}

fn allowed(platform: Platform, name: &str) -> Option<String> {
    extension_of(name).filter(|e| platform.extensions().contains(&e.as_str()))
}

fn unpack(
    download: &Path,
    unpacked: &Path,
    platform: Platform,
    fs_name: &str,
) -> Result<(PathBuf, String), RommFail> {
    let mut magic = [0u8; 6];
    let n = File::open(download)
        .and_then(|mut f| f.read(&mut magic))
        .map_err(|_| RommFail::Write)?;
    let magic = &magic[..n];
    if magic.starts_with(b"7z\xBC\xAF\x27\x1C") || magic.starts_with(b"Rar!") {
        return Err(RommFail::Archive);
    }
    if !magic.starts_with(b"PK\x03\x04") {
        let ext =
            allowed(platform, fs_name).unwrap_or_else(|| platform.extensions()[0].to_string());
        return Ok((download.to_path_buf(), ext));
    }
    let file = File::open(download).map_err(|_| RommFail::Write)?;
    let mut zip = zip::ZipArchive::new(file).map_err(|_| RommFail::Archive)?;
    let mut best: Option<(usize, u64, String)> = None;
    for i in 0..zip.len() {
        let Ok(entry) = zip.by_index_raw(i) else {
            continue;
        };
        let Some(ext) = entry
            .enclosed_name()
            .and_then(|p| allowed(platform, &p.to_string_lossy()))
        else {
            continue;
        };
        if entry.is_file() && best.as_ref().is_none_or(|b| entry.size() > b.1) {
            best = Some((i, entry.size(), ext));
        }
    }
    let (index, size, ext) = best.ok_or(RommFail::Archive)?;
    if size > ROM_LIMIT {
        return Err(RommFail::Bad);
    }
    let mut entry = zip.by_index(index).map_err(|_| RommFail::Archive)?;
    let mut out = File::create(unpacked).map_err(|_| RommFail::Write)?;
    std::io::copy(&mut (&mut entry).take(ROM_LIMIT), &mut out).map_err(|e| match e.kind() {
        std::io::ErrorKind::InvalidData | std::io::ErrorKind::Unsupported => RommFail::Archive,
        _ => write_fail(e),
    })?;
    out.sync_all().map_err(write_fail)?;
    Ok((unpacked.to_path_buf(), ext))
}

pub fn romm_label(
    http: &dyn Http,
    server: &Server,
    root: &Path,
    cart: &Cart,
    game: &Game,
) -> Option<()> {
    let url = server.resource(game.cart_photo.as_deref()?);
    let photo = ok(http.call(&Req::get(server, url)).ok()?)
        .ok()?
        .bytes(PHOTO_LIMIT.min(JSON_LIMIT))
        .ok()?;
    let png = label_from_photo(&photo, cart.platform)?;
    let path = crate::art::make::label_path(root, cart);
    crate::art::make::write_replacing(&path, &png).ok()?;
    let mut labelled = cart.clone();
    labelled.label = Some(path);
    let _ = crate::label_cache::label_art(root, &labelled);
    Some(())
}

pub fn label_from_photo(photo: &[u8], platform: Platform) -> Option<Vec<u8>> {
    let png = crate::art::make::as_png(photo)?;
    let (px, w, h) = slot_ui::decode_png_bytes(&png)?;
    let img = Rgba {
        px: &px,
        w: w as usize,
        h: h as usize,
    };
    if img.w == 0 || img.h == 0 {
        return None;
    }
    let (b, _) = find_box(img, platform);
    let (cut, cw, ch) = crop(img, b);
    let mut encoded = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut encoded, cw as u32, ch as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().ok()?.write_image_data(&cut).ok()?;
    }
    crate::art::make::real_label(&encoded, platform)
}

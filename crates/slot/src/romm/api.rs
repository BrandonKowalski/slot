use std::time::Duration;

use serde_json::{json, Value};
use slot_store::Platform;

use super::http::{ok, Http, Req};
use super::{RommFail, Server};

pub const SCOPES: [&str; 2] = ["platforms.read", "roms.read"];
const PAGE: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shelf {
    pub id: u64,
    pub platform: Platform,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Game {
    pub id: u64,
    pub name: String,
    pub fs_name: String,
    pub fs_name_no_ext: String,
    pub fs_name_no_tags: String,
    pub size: u64,
    pub crcs: Vec<u32>,
    pub cart_photo: Option<String>,
    pub missing: bool,
}

impl Game {
    pub fn title(&self) -> &str {
        [&self.name, &self.fs_name_no_tags, &self.fs_name_no_ext]
            .into_iter()
            .find(|s| !s.trim().is_empty())
            .map_or("?", |s| s.as_str())
    }
}

pub fn platform_of(slug: &str) -> Option<Platform> {
    match slug.to_ascii_lowercase().as_str() {
        "gba" | "game-boy-advance" | "gameboy-advance" => Some(Platform::Gba),
        "gb" | "game-boy" | "gameboy" => Some(Platform::Gb),
        "gbc" | "game-boy-color" | "gameboy-color" => Some(Platform::Gbc),
        _ => None,
    }
}

pub fn shelves(http: &dyn Http, server: &Server) -> Result<Vec<Shelf>, RommFail> {
    let reply = ok(http.call(&Req::get(server, server.url("/api/platforms")))?)?;
    Ok(parse_shelves(&reply.json()?))
}

pub fn parse_shelves(v: &Value) -> Vec<Shelf> {
    let Some(items) = v.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|p| {
            let platform = ["slug", "fs_slug"]
                .iter()
                .find_map(|k| p[k].as_str().and_then(platform_of))?;
            Some(Shelf {
                id: p["id"].as_u64()?,
                platform,
                name: p["name"].as_str().unwrap_or(platform.name()).to_string(),
            })
        })
        .collect()
}

pub fn games(
    http: &dyn Http,
    server: &Server,
    shelf: &Shelf,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<Game>, RommFail> {
    let mut out = Vec::new();
    loop {
        if cancelled() {
            return Err(RommFail::Offline);
        }
        let url = server.url(&format!(
            "/api/roms?platform_ids={}&with_files=true&limit={PAGE}&offset={}",
            shelf.id,
            out.len()
        ));
        let page = ok(http.call(&Req::get(server, url))?)?.json()?;
        let items = page["items"].as_array().cloned().unwrap_or_default();
        if items.is_empty() {
            break;
        }
        out.extend(items.iter().filter_map(parse_game));
        let total = page["total"].as_u64().unwrap_or(0) as usize;
        if total == 0 || out.len() >= total {
            break;
        }
    }
    Ok(out)
}

pub fn parse_game(v: &Value) -> Option<Game> {
    let text = |k: &str| v[k].as_str().unwrap_or("").to_string();
    let ss = &v["ss_metadata"];
    let cart_photo = ["physical_path", "physical_url"]
        .iter()
        .find_map(|k| ss[k].as_str().filter(|s| !s.is_empty()))
        .map(str::to_string);
    let mut crcs: Vec<u32> = std::iter::once(&v["crc_hash"])
        .chain(
            v["files"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|f| &f["crc_hash"]),
        )
        .filter_map(|c| c.as_str().and_then(parse_crc))
        .collect();
    crcs.dedup();
    Some(Game {
        id: v["id"].as_u64()?,
        name: text("name"),
        fs_name: text("fs_name"),
        fs_name_no_ext: text("fs_name_no_ext"),
        fs_name_no_tags: text("fs_name_no_tags"),
        size: v["fs_size_bytes"].as_u64().unwrap_or(0),
        crcs,
        cart_photo,
        missing: v["missing_from_fs"].as_bool().unwrap_or(false),
    })
}

fn parse_crc(s: &str) -> Option<u32> {
    let t = s.trim();
    match t.is_empty() || t.len() > 8 {
        true => None,
        false => u32::from_str_radix(t, 16).ok(),
    }
}

pub fn content_url(server: &Server, game: &Game) -> String {
    let file = match game.fs_name.is_empty() {
        true => format!("rom-{}", game.id),
        false => escape_path(&game.fs_name),
    };
    server.url(&format!("/api/roms/{}/content/{file}", game.id))
}

fn escape_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn exchange_code(http: &dyn Http, server: &Server, code: &str) -> Result<String, RommFail> {
    let clean: String = code
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let body = json!({ "code": clean }).to_string();
    let reply = http.call(&Req::post(
        server,
        server.url("/api/client-tokens/exchange"),
        body,
    ))?;
    let reply = match reply.status {
        404 => return Err(RommFail::Expired),
        403 => return Err(RommFail::Denied),
        _ => ok(reply)?,
    };
    reply.json()?["raw_token"]
        .as_str()
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .ok_or(RommFail::Bad)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pairing {
    pub device_code: String,
    pub user_code: String,
    pub link: String,
    pub interval: Duration,
    pub expires_in: Duration,
}

pub fn start_pairing(
    http: &dyn Http,
    server: &Server,
    device_id: &str,
) -> Result<Pairing, RommFail> {
    let body = json!({
        "client_device_identifier": device_id,
        "name": "slot",
        "client": "slot",
        "platform": "Anbernic",
        "client_version": crate::build_info::Build::current().version,
        "requested_scopes": SCOPES,
    })
    .to_string();
    let anonymous = Server {
        login: None,
        ..server.clone()
    };
    let reply = http.call(&Req::post(
        &anonymous,
        server.url("/api/auth/device/init"),
        body,
    ))?;
    let reply = match reply.status {
        404 | 405 => return Err(RommFail::TooOld),
        _ => ok(reply)?,
    };
    parse_pairing(server, &reply.json()?).ok_or(RommFail::Bad)
}

pub fn parse_pairing(server: &Server, v: &Value) -> Option<Pairing> {
    let path = v["verification_path_complete"].as_str()?;
    let safe = path.starts_with('/')
        && !path.starts_with("//")
        && !path.contains(['\\', '\r', '\n', '\t', ' ', '@']);
    if !safe {
        return None;
    }
    let secs = |k: &str, lo: u64, hi: u64, or: u64| {
        Duration::from_secs(v[k].as_u64().unwrap_or(or).clamp(lo, hi))
    };
    Some(Pairing {
        device_code: v["device_code"].as_str()?.to_string(),
        user_code: v["user_code"].as_str()?.to_string(),
        link: server.url(path),
        interval: secs("interval", 2, 30, 5),
        expires_in: secs("expires_in", 30, 1800, 600),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Poll {
    Token(String),
    Waiting,
    SlowDown,
}

pub fn poll_pairing(http: &dyn Http, server: &Server, pairing: &Pairing) -> Result<Poll, RommFail> {
    let body = json!({ "device_code": pairing.device_code }).to_string();
    let anonymous = Server {
        login: None,
        ..server.clone()
    };
    let reply = match http.call(&Req::post(
        &anonymous,
        server.url("/api/auth/device/token"),
        body,
    )) {
        Ok(r) => r,
        Err(RommFail::Offline) => return Ok(Poll::Waiting),
        Err(e) => return Err(e),
    };
    let status = reply.status;
    if status == 429 {
        return Ok(Poll::SlowDown);
    }
    let v = reply.json().unwrap_or(Value::Null);
    if status == 200 {
        return v["access_token"]
            .as_str()
            .filter(|t| !t.is_empty())
            .map(|t| Poll::Token(t.to_string()))
            .ok_or(RommFail::Bad);
    }
    match v["detail"].as_str() {
        Some("authorization_pending") => Ok(Poll::Waiting),
        Some("slow_down") => Ok(Poll::SlowDown),
        Some("access_denied") => Err(RommFail::Denied),
        Some("expired_token") => Err(RommFail::Expired),
        _ => Err(RommFail::Http(status)),
    }
}

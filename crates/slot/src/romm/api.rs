use std::time::Duration;

use serde_json::{json, Value};
use slot_store::Platform;

use super::http::{ok, Http, Req};
use super::{RommFail, Server};

pub const SCOPES: [&str; 2] = ["platforms.read", "roms.read"];
pub const REMOTE_SCOPES: [&str; 3] = ["roms.read", "devices.read", "devices.write"];
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
    pub file_ids: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Grant {
    pub token: String,
    pub device: Option<String>,
    pub scopes: Vec<String>,
}

impl Grant {
    pub fn remote(&self) -> bool {
        self.device.is_some()
            && REMOTE_SCOPES
                .iter()
                .all(|s| self.scopes.iter().any(|g| g == s))
    }
}

fn parse_grant(v: &Value, key: &str) -> Option<Grant> {
    let token = v[key].as_str().filter(|t| !t.is_empty())?.to_string();
    let device = match &v["device_id"] {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    };
    let scopes = v["scopes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| s.as_str().map(str::to_string))
        .collect();
    Some(Grant {
        token,
        device,
        scopes,
    })
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
        file_ids: Vec::new(),
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
    let ids: Vec<String> = game.file_ids.iter().map(u64::to_string).collect();
    let query = match ids.is_empty() {
        true => String::new(),
        false => format!("?file_ids={}", ids.join(",")),
    };
    server.url(&format!("/api/roms/{}/content/{file}{query}", game.id))
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

pub fn exchange_code(http: &dyn Http, server: &Server, code: &str) -> Result<Grant, RommFail> {
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
    parse_grant(&reply.json()?, "raw_token").ok_or(RommFail::Bad)
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
        "requested_scopes": ["platforms.read", "roms.read", "devices.read", "devices.write"],
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
    Token(Grant),
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
        return parse_grant(&v, "access_token")
            .map(Poll::Token)
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

fn device_url(server: &Server, device: &str, tail: &str) -> String {
    server.url(&format!("/api/devices/{}{tail}", escape_path(device)))
}

pub fn set_remote(
    http: &dyn Http,
    server: &Server,
    device: &str,
    on: bool,
) -> Result<(), RommFail> {
    let body = json!({
        "capabilities": { "remote_install": on },
        "client_version": crate::build_info::Build::current().version,
    })
    .to_string();
    let req = Req::put(server, device_url(server, device, ""), body);
    ok(http.call(&req)?).map(|_| ())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claimed {
    pub id: String,
    pub rom_id: u64,
    pub file_ids: Vec<u64>,
}

pub fn claim(http: &dyn Http, server: &Server, device: &str) -> Result<Vec<Claimed>, RommFail> {
    let req = Req::post(
        server,
        device_url(server, device, "/installs/claim"),
        "{}".into(),
    );
    let v = ok(http.call(&req)?)?.json()?;
    Ok(v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            Some(Claimed {
                id: r["id"].as_str()?.to_string(),
                rom_id: r["rom_id"].as_u64()?,
                file_ids: r["file_ids"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_u64)
                    .collect(),
            })
        })
        .collect())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Done,
    AlreadyInstalled,
    Failed(String),
}

pub fn report(
    http: &dyn Http,
    server: &Server,
    device: &str,
    request: &str,
    outcome: &Outcome,
) -> Result<(), RommFail> {
    let (status, reason) = match outcome {
        Outcome::Done => ("done", None),
        Outcome::AlreadyInstalled => ("already_installed", None),
        Outcome::Failed(why) => ("failed", Some(why.chars().take(500).collect::<String>())),
    };
    let body = json!({ "status": status, "reason": reason }).to_string();
    let tail = format!("/installs/{}", escape_path(request));
    let reply = http.call(&Req::put(server, device_url(server, device, &tail), body))?;
    match reply.status {
        404 => Ok(()),
        _ => ok(reply).map(|_| ()),
    }
}

pub fn rom(
    http: &dyn Http,
    server: &Server,
    id: u64,
) -> Result<(Option<Platform>, Game), RommFail> {
    let v = ok(http.call(&Req::get(server, server.url(&format!("/api/roms/{id}"))))?)?.json()?;
    let platform = ["platform_slug", "platform_fs_slug"]
        .iter()
        .find_map(|k| v[k].as_str().and_then(platform_of));
    Ok((platform, parse_game(&v).ok_or(RommFail::Bad)?))
}

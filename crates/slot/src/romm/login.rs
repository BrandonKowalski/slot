use std::path::Path;

use super::api::REMOTE_SCOPES;
use super::tls::{parse_pin, pin_hex, Pin};
use super::{Login, Server};

pub const LOGIN_FILE: &str = "Cache/romm/login.txt";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Saved {
    pub url: Option<String>,
    pub login: Option<Login>,
    pub device: Option<String>,
    pub pin: Option<Pin>,
    pub romm_device: Option<String>,
    pub scopes: Vec<String>,
    pub remote: bool,
}

impl Saved {
    pub fn can_remote(&self) -> bool {
        matches!(self.login, Some(Login::Token(_)))
            && self.romm_device.is_some()
            && REMOTE_SCOPES
                .iter()
                .all(|s| self.scopes.iter().any(|g| g == s))
    }

    pub fn remote_on(&self) -> bool {
        self.remote && self.can_remote()
    }

    pub fn signed_out(&self) -> Saved {
        Saved {
            login: None,
            romm_device: None,
            scopes: Vec::new(),
            remote: false,
            ..self.clone()
        }
    }

    pub fn server(&self) -> Option<Server> {
        let mut server = Server::new(self.url.as_deref()?, self.login.clone())?;
        server.pin = self.pin;
        Some(server)
    }
}

pub fn load(root: &Path) -> Saved {
    let map = slot_store::ini::read(root, LOGIN_FILE);
    let get = |k: &str| map.get(k).filter(|v| !v.is_empty()).cloned();
    let login = match (
        get("token"),
        get("user"),
        get("pass").and_then(|p| unhex(&p)),
    ) {
        (Some(t), _, _) => Some(Login::Token(t)),
        (None, Some(user), Some(pass)) => Some(Login::Password { user, pass }),
        _ => None,
    };
    Saved {
        url: get("url"),
        login,
        device: get("device"),
        pin: get("pin").and_then(|p| parse_pin(&p)),
        romm_device: get("romm_device"),
        scopes: get("scopes")
            .map(|s| s.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default(),
        remote: get("remote").as_deref() == Some("on"),
    }
}

pub fn save(root: &Path, saved: &Saved) -> std::io::Result<()> {
    let (token, user, pass) = match &saved.login {
        Some(Login::Token(t)) => (Some(t.clone()), None, None),
        Some(Login::Password { user, pass }) => (None, Some(user.clone()), Some(hex(pass))),
        None => (None, None, None),
    };
    let mut text = String::new();
    for (key, value) in [
        ("url", saved.url.clone()),
        ("device", saved.device.clone()),
        ("pin", saved.pin.as_ref().map(pin_hex)),
        ("romm_device", saved.romm_device.clone()),
        (
            "scopes",
            (!saved.scopes.is_empty()).then(|| saved.scopes.join(" ")),
        ),
        ("remote", saved.remote.then(|| "on".to_string())),
        ("token", token),
        ("user", user),
        ("pass", pass),
    ] {
        text = slot_store::ini::set(&text, key, value.as_deref())?;
    }
    let path = root.join(LOGIN_FILE);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    slot_store::atomic_write(&path, text.as_bytes())
}

pub fn forget(root: &Path) -> std::io::Result<()> {
    save(root, &load(root).signed_out())
}

pub fn new_device_id(seed: u64) -> String {
    let mut x = seed | 1;
    let mut out = String::from("slot-");
    for _ in 0..4 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.push_str(&format!("{:08x}", x as u32));
    }
    out
}

fn hex(text: &str) -> String {
    text.bytes().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<String> {
    let bytes: Option<Vec<u8>> = (0..text.len())
        .step_by(2)
        .map(|i| {
            text.get(i..i + 2)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        })
        .collect();
    String::from_utf8(bytes?).ok()
}

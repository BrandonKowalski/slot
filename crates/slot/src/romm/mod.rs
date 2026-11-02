pub mod api;
pub mod http;
pub mod install;
pub mod jobs;
pub mod label;
pub mod login;
pub mod name;
pub mod screen;
pub mod tls;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RommFail {
    Offline,
    Clock,
    Refused,
    Scopes,
    NotFound,
    TooOld,
    TooMany,
    Expired,
    Denied,
    Bad,
    Http(u16),
    Exists,
    Space,
    Archive,
    Write,
    Cancelled,
    Address,
    Certificate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Login {
    Password { user: String, pass: String },
    Token(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Server {
    pub base: String,
    pub login: Option<Login>,
    pub pin: Option<tls::Pin>,
}

impl Server {
    pub fn new(base: &str, login: Option<Login>) -> Option<Server> {
        Some(Server {
            base: normalize_url(base)?,
            login,
            pin: None,
        })
    }

    pub fn pin_for(&self, url: &str) -> Option<tls::Pin> {
        let pin = self.pin?;
        let same = host_of(url).is_some() && host_of(url) == host_of(&self.base);
        (same && url.starts_with("https://")).then_some(pin)
    }

    pub fn secure(&self) -> bool {
        self.base.starts_with("https://")
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    pub fn resource(&self, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") {
            path.to_string()
        } else if path.starts_with('/') {
            self.url(path)
        } else {
            self.url(&format!("/assets/romm/resources/{path}"))
        }
    }

    pub fn auth_for(&self, url: &str) -> Option<String> {
        let login = self.login.as_ref()?;
        if host_of(url)? != host_of(&self.base)? {
            return None;
        }
        if self.secure() && !url.starts_with("https://") {
            return None;
        }
        Some(match login {
            Login::Token(t) => format!("Bearer {t}"),
            Login::Password { user, pass } => {
                format!("Basic {}", base64(format!("{user}:{pass}").as_bytes()))
            }
        })
    }
}

pub fn normalize_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() || trimmed.contains(char::is_whitespace) {
        return None;
    }
    let full = match trimmed.contains("://") {
        true => trimmed.to_string(),
        false => format!("https://{trimmed}"),
    };
    let lower = full.to_ascii_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return None;
    }
    let scheme_len = full.find("://")? + 3;
    let rest = &full[scheme_len..];
    if rest.is_empty() || rest.starts_with('/') || rest.contains('@') {
        return None;
    }
    Some(format!("{}{rest}", &lower[..scheme_len]))
}

fn host_of(url: &str) -> Option<String> {
    let rest = &url[url.find("://")? + 3..];
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    Some(rest[..end].to_ascii_lowercase())
}

pub fn base64(bytes: &[u8]) -> String {
    const SET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        for i in 0..4 {
            match i <= chunk.len() {
                true => out.push(SET[(n >> (18 - 6 * i) & 63) as usize] as char),
                false => out.push('='),
            }
        }
    }
    out
}

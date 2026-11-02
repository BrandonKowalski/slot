use std::io::Read;
use std::time::Duration;

use std::sync::{Arc, Mutex};

use super::tls::{pinned_agent, Pin};
use super::{RommFail, Server};

pub const JSON_LIMIT: u64 = 64 * 1024 * 1024;
pub const JSON_TIMEOUT: Duration = Duration::from_secs(60);
pub const ROM_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Req {
    pub method: &'static str,
    pub url: String,
    pub auth: Option<String>,
    pub json: Option<String>,
    pub timeout: Duration,
    pub pin: Option<Pin>,
}

impl Req {
    pub fn get(server: &Server, url: String) -> Req {
        Req {
            method: "GET",
            auth: server.auth_for(&url),
            pin: server.pin_for(&url),
            url,
            json: None,
            timeout: JSON_TIMEOUT,
        }
    }

    pub fn put(server: &Server, url: String, json: String) -> Req {
        Req {
            method: "PUT",
            ..Req::post(server, url, json)
        }
    }

    pub fn post(server: &Server, url: String, json: String) -> Req {
        Req {
            method: "POST",
            auth: server.auth_for(&url),
            pin: server.pin_for(&url),
            url,
            json: Some(json),
            timeout: JSON_TIMEOUT,
        }
    }
}

pub struct Reply {
    pub status: u16,
    pub length: Option<u64>,
    pub body: Box<dyn Read + Send>,
}

impl Reply {
    pub fn bytes(self, limit: u64) -> Result<Vec<u8>, RommFail> {
        let mut out = Vec::new();
        self.body
            .take(limit + 1)
            .read_to_end(&mut out)
            .map_err(|_| RommFail::Offline)?;
        match out.len() as u64 > limit {
            true => Err(RommFail::Bad),
            false => Ok(out),
        }
    }

    pub fn json(self) -> Result<serde_json::Value, RommFail> {
        let bytes = self.bytes(JSON_LIMIT)?;
        serde_json::from_slice(&bytes).map_err(|_| RommFail::Bad)
    }
}

pub trait Http: Send + Sync {
    fn call(&self, req: &Req) -> Result<Reply, RommFail>;

    fn certificate(&self, _url: &str) -> Result<Pin, RommFail> {
        Err(RommFail::Offline)
    }
}

fn fail_of(e: &ureq::Error) -> RommFail {
    let text = format!("{e:?}");
    match crate::update::request_fail(e) {
        crate::update::UpdateFail::Clock => RommFail::Clock,
        _ if text.contains("InvalidCertificate") || text.contains("UnknownIssuer") => {
            RommFail::Certificate
        }
        _ => RommFail::Offline,
    }
}

fn config(timeout: Duration, https: bool) -> ureq::config::Config {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .https_only(https)
        .max_redirects(10)
        .user_agent(format!(
            "slot/{}",
            crate::build_info::Build::current().version
        ))
        .build()
}

pub fn ok(reply: Reply) -> Result<Reply, RommFail> {
    match reply.status {
        200..=299 => Ok(reply),
        401 => Err(RommFail::Refused),
        403 => Err(RommFail::Scopes),
        404 => Err(RommFail::NotFound),
        429 => Err(RommFail::TooMany),
        s => Err(RommFail::Http(s)),
    }
}

pub struct Ureq;

impl Http for Ureq {
    fn call(&self, req: &Req) -> Result<Reply, RommFail> {
        let config = config(req.timeout, req.url.starts_with("https://"));
        let agent: ureq::Agent = match req.pin {
            Some(pin) => pinned_agent(config, Some(pin), Arc::default()),
            None => config.into(),
        };
        let sent = match (&req.json, req.method) {
            (Some(json), "POST" | "PUT") => {
                let mut r = match req.method {
                    "PUT" => agent.put(&req.url),
                    _ => agent.post(&req.url),
                }
                .content_type("application/json");
                if let Some(a) = &req.auth {
                    r = r.header("Authorization", a);
                }
                r.send(json.as_str())
            }
            _ => {
                let mut r = agent.get(&req.url);
                if let Some(a) = &req.auth {
                    r = r.header("Authorization", a);
                }
                r.call()
            }
        };
        let response = sent.map_err(|e| fail_of(&e))?;
        let status = response.status().as_u16();
        let length = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok());
        let body = response
            .into_body()
            .into_with_config()
            .limit(u64::MAX)
            .reader();
        Ok(Reply {
            status,
            length,
            body: Box::new(body),
        })
    }

    fn certificate(&self, url: &str) -> Result<Pin, RommFail> {
        let seen: Arc<Mutex<Option<Pin>>> = Arc::default();
        let agent = pinned_agent(config(JSON_TIMEOUT, true), None, seen.clone());
        let tried = agent.get(url).call();
        let got = *seen.lock().map_err(|_| RommFail::Bad)?;
        match (got, tried) {
            (Some(pin), _) => Ok(pin),
            (None, Err(e)) => Err(fail_of(&e)),
            (None, Ok(_)) => Err(RommFail::Bad),
        }
    }
}

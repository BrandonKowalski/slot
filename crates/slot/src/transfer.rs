//! A short-lived, authenticated LAN transfer server. Uploads stream to a temporary file
//! on the card and only become visible when complete. No shell or arbitrary paths.
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const MAX_FILE: u64 = 128 * 1024 * 1024;
pub const FOLDERS: [&str; 7] = [
    "Games/GBA",
    "Games/GB",
    "Games/GBC",
    "Labels/GBA",
    "Labels/GB",
    "Labels/GBC",
    "BIOS",
];
const PAGE: &str = include_str!("transfer.html");

#[derive(Clone, Default)]
pub struct Status {
    pub message: String,
    pub uploaded: u64,
}

struct Shared {
    stopped: bool,
    status: Status,
}

pub struct Server {
    pub address: SocketAddr,
    pub pin: String,
    shared: Arc<Mutex<Shared>>,
}

impl Server {
    pub fn start(root: &Path, ip: Ipv4Addr, port: u16) -> io::Result<Self> {
        let root = root.canonicalize()?;
        let listener = TcpListener::bind((ip, port))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let mut random = [0u8; 4];
        File::open("/dev/urandom")?.read_exact(&mut random)?;
        let pin = format!("{:08}", u32::from_ne_bytes(random) % 100_000_000);
        let shared = Arc::new(Mutex::new(Shared {
            stopped: false,
            status: Status {
                message: "Ready for your browser".into(),
                uploaded: 0,
            },
        }));
        let worker_shared = shared.clone();
        let worker_pin = pin.clone();
        thread::Builder::new()
            .name("slot-transfer".into())
            .spawn(move || {
                let mut failed = 0u8;
                let mut reset = Instant::now();
                while !stopped(&worker_shared) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
                            let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                            let _ = serve(
                                &mut stream,
                                &root,
                                &worker_pin,
                                address,
                                &worker_shared,
                                &mut failed,
                                &mut reset,
                            );
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(40))
                        }
                        Err(_) => break,
                    }
                }
            })?;
        Ok(Self {
            address,
            pin,
            shared,
        })
    }
    pub fn status(&self) -> Status {
        self.shared.lock().unwrap().status.clone()
    }
    pub fn stop(&self) {
        self.shared.lock().unwrap().stopped = true;
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}
fn stopped(shared: &Arc<Mutex<Shared>>) -> bool {
    shared.lock().unwrap().stopped
}

fn response(stream: &mut TcpStream, code: u16, mime: &str, body: &[u8]) -> io::Result<()> {
    headers(stream, code, mime, body.len() as u64)?;
    stream.write_all(body)
}
fn headers(stream: &mut TcpStream, code: u16, mime: &str, len: u64) -> io::Result<()> {
    let label = match code {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        413 => "Content Too Large",
        429 => "Too Many Requests",
        _ => "Error",
    };
    write!(stream, "HTTP/1.1 {code} {label}\r\nContent-Type: {mime}\r\nContent-Length: {len}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; img-src 'self' data:; frame-ancestors 'none'; base-uri 'none'; form-action 'self'\r\n\r\n")
}
fn error(stream: &mut TcpStream, code: u16, message: &str) -> io::Result<()> {
    response(
        stream,
        code,
        "application/json",
        serde_json::json!({"error":message}).to_string().as_bytes(),
    )
}

fn serve(
    stream: &mut TcpStream,
    root: &Path,
    pin: &str,
    address: SocketAddr,
    shared: &Arc<Mutex<Shared>>,
    failed: &mut u8,
    reset: &mut Instant,
) -> io::Result<()> {
    let mut bytes = Vec::with_capacity(4096);
    let deadline = Instant::now() + Duration::from_secs(5);
    let end = loop {
        if stopped(shared) || Instant::now() >= deadline {
            return Ok(());
        }
        if let Some(at) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
            break at + 4;
        }
        if bytes.len() > 16_384 {
            return error(stream, 400, "Request headers are too large");
        }
        let mut buf = [0; 4096];
        match stream.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(n) => bytes.extend_from_slice(&buf[..n]),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                continue
            }
            Err(e) => return Err(e),
        }
    };
    let mut fields = [httparse::EMPTY_HEADER; 48];
    let mut request = httparse::Request::new(&mut fields);
    if !matches!(
        request.parse(&bytes[..end]),
        Ok(httparse::Status::Complete(_))
    ) {
        return error(stream, 400, "Invalid request");
    }
    let values = |key: &str| {
        request
            .headers
            .iter()
            .filter(|h| h.name.eq_ignore_ascii_case(key))
            .map(|h| h.value)
            .collect::<Vec<_>>()
    };
    let host = values("host");
    let expected = address.to_string();
    if host.len() != 1 || host[0] != expected.as_bytes() {
        return error(stream, 403, "Open the address shown on Slot");
    }
    let origins = values("origin");
    if origins.len() > 1
        || origins
            .first()
            .is_some_and(|s| *s != format!("http://{expected}").as_bytes())
    {
        return error(stream, 403, "Use the Slot transfer page");
    }
    if !values("transfer-encoding").is_empty() {
        return error(stream, 400, "A file size is required");
    }
    let lengths = values("content-length");
    if lengths.len() > 1 {
        return error(stream, 400, "Invalid file size");
    }
    let length = if let Some(value) = lengths.first() {
        match std::str::from_utf8(value)
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
        {
            Some(n) => n,
            None => return error(stream, 400, "Invalid file size"),
        }
    } else {
        0
    };
    let method = request.method.unwrap_or("");
    let path = request.path.unwrap_or("");
    if method == "GET" && path == "/" {
        return response(stream, 200, "text/html; charset=utf-8", PAGE.as_bytes());
    }
    if reset.elapsed() >= Duration::from_secs(30) {
        *failed = 0;
        *reset = Instant::now();
    }
    if *failed >= 8 {
        return error(
            stream,
            429,
            "Too many incorrect codes. Wait 30 seconds and try again.",
        );
    }
    let auth = values("x-slot-pin");
    if auth.len() != 1 || auth[0] != pin.as_bytes() {
        *failed += 1;
        return error(
            stream,
            401,
            "Enter the eight-digit code shown on your handheld",
        );
    }
    let Some((route, params)) = path.split_once('?') else {
        return error(stream, 404, "Not found");
    };
    let Some(params) = query(params) else {
        return error(stream, 400, "Invalid filename");
    };
    let dir = params
        .iter()
        .find(|(k, _)| k == "dir")
        .map(|(_, v)| v.as_str())
        .unwrap_or("");
    let Some(folder) = folder(root, dir) else {
        return error(stream, 403, "Choose a game, label or BIOS folder");
    };
    if route == "/api/files" && method == "GET" {
        let Ok(entries) = fs::read_dir(folder) else {
            return error(stream, 500, "Cannot read this folder");
        };
        let mut files = Vec::new();
        for entry in entries.flatten() {
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if !filename(&name) {
                continue;
            }
            let Ok(meta) = fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            files.push(serde_json::json!({"name":name,"size":meta.len()}));
        }
        files.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        return response(
            stream,
            200,
            "application/json",
            serde_json::json!({"files":files,"maxFileSize":MAX_FILE})
                .to_string()
                .as_bytes(),
        );
    }
    if route != "/api/file" {
        return error(stream, 404, "Not found");
    }
    let name = params
        .iter()
        .find(|(k, _)| k == "name")
        .map(|(_, v)| v.as_str())
        .unwrap_or("");
    if !filename(name) {
        return error(
            stream,
            400,
            "Use a filename without folders or special path characters",
        );
    }
    let target = folder.join(name);
    if let Ok(meta) = fs::symlink_metadata(&target) {
        if !meta.is_file() {
            return error(stream, 403, "This is not a regular file");
        }
    }
    match method {
        "GET" => {
            let Ok(mut file) = File::open(&target) else {
                return error(stream, 404, "File not found");
            };
            let len = file.metadata()?.len();
            headers(stream, 200, "application/octet-stream", len)?;
            let mut buf = [0; 65536];
            let mut left = len;
            while left > 0 && !stopped(shared) {
                let n = file.read(&mut buf[..left.min(65536) as usize])?;
                if n == 0 {
                    break;
                }
                stream.write_all(&buf[..n])?;
                left -= n as u64;
            }
            Ok(())
        }
        "PUT" => {
            if length == 0 {
                return error(stream, 400, "The file is empty");
            }
            if length > MAX_FILE {
                return error(stream, 413, "Files must be 128 MB or smaller");
            }
            if !extension(dir, name) {
                return error(
                    stream,
                    400,
                    "This file type does not belong in the selected folder",
                );
            }
            let replace = params.iter().any(|(k, v)| k == "replace" && v == "1");
            if target.exists() && !replace {
                return error(stream, 409, "A file with this name already exists");
            }
            let temp = folder.join(format!(".slot-upload-{pin}.part"));
            let Ok(mut file) = OpenOptions::new().write(true).create_new(true).open(&temp) else {
                return error(
                    stream,
                    500,
                    "Cannot create upload; check free space on the SD card",
                );
            };
            let cleanup = Temporary(temp.clone());
            shared.lock().unwrap().status.message = format!("Receiving {name}");
            let outcome = receive(stream, &mut file, &bytes[end..], length, shared)
                .and_then(|_| file.sync_all());
            drop(file);
            if let Err(e) = outcome {
                shared.lock().unwrap().status.message =
                    "Upload interrupted; original file preserved".into();
                let _ = error(
                    stream,
                    500,
                    "Upload interrupted or SD card full; original file preserved",
                );
                return Err(e);
            }
            // Serialize stop and commit: no upload can be published after Stop returns.
            let mut state = shared.lock().unwrap();
            if state.stopped {
                return Ok(());
            }
            if fs::symlink_metadata(&target).is_ok_and(|m| !m.is_file())
                || (target.exists() && !replace)
            {
                return error(stream, 409, "Destination changed; please try again");
            }
            if fs::rename(&temp, &target).is_err() {
                return error(stream, 500, "Could not save the file on the SD card");
            }
            state.status.uploaded += 1;
            state.status.message = format!("Saved {name}");
            drop(state);
            // Never hold the UI's status lock while waiting for the card to sync.
            let _ = File::open(&folder).and_then(|f| f.sync_all());
            drop(cleanup);
            response(stream, 201, "application/json", b"{\"ok\":true}")
        }
        _ => error(stream, 400, "Unsupported request"),
    }
}

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn receive(
    stream: &mut TcpStream,
    file: &mut File,
    first: &[u8],
    length: u64,
    shared: &Arc<Mutex<Shared>>,
) -> io::Result<()> {
    if first.len() as u64 > length {
        return Err(io::ErrorKind::InvalidData.into());
    }
    file.write_all(first)?;
    let mut left = length - first.len() as u64;
    let mut active = Instant::now();
    let mut buf = [0; 65536];
    while left > 0 {
        if stopped(shared) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if active.elapsed() > Duration::from_secs(30) {
            return Err(io::ErrorKind::TimedOut.into());
        }
        match stream.read(&mut buf[..left.min(65536) as usize]) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => {
                file.write_all(&buf[..n])?;
                left -= n as u64;
                active = Instant::now();
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn folder(root: &Path, name: &str) -> Option<PathBuf> {
    if !FOLDERS.contains(&name) {
        return None;
    }
    let mut path = root.to_path_buf();
    for part in name.split('/') {
        path.push(part);
        if !fs::symlink_metadata(&path).ok()?.is_dir() {
            return None;
        }
    }
    if !path.canonicalize().ok()?.starts_with(root) {
        return None;
    }
    Some(path)
}
fn filename(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 240
        && !name.starts_with('.')
        && !name.ends_with(['.', ' '])
        && !name
            .chars()
            .any(|c| c.is_control() || "/\\:*?\"<>|".contains(c))
}
fn extension(dir: &str, name: &str) -> bool {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match dir {
        "Games/GBA" => ext == "gba",
        "Games/GB" => ext == "gb",
        "Games/GBC" => ext == "gbc",
        "BIOS" => ext == "bin",
        _ => ext == "png",
    }
}
fn query(text: &str) -> Option<Vec<(String, String)>> {
    let mut result = Vec::new();
    for pair in text.split('&') {
        let (k, v) = pair.split_once('=')?;
        let key = decode(k)?;
        if result.iter().any(|(k, _)| k == &key) {
            return None;
        }
        result.push((key, decode(v)?));
    }
    Some(result)
}
fn decode(text: &str) -> Option<String> {
    let mut bytes = text.bytes();
    let mut result = Vec::new();
    while let Some(b) = bytes.next() {
        result.push(match b {
            b'%' => {
                ((bytes.next()? as char).to_digit(16)? * 16
                    + (bytes.next()? as char).to_digit(16)?) as u8
            }
            b'+' => b' ',
            _ => b,
        });
    }
    String::from_utf8(result).ok()
}

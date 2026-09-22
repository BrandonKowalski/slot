use slot::transfer::{Server, FOLDERS, MAX_FILE};
use std::fs;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, Shutdown, TcpStream};
use std::time::{Duration, Instant};

fn setup() -> (tempfile::TempDir, Server) {
    let root = tempfile::tempdir().unwrap();
    for folder in FOLDERS {
        fs::create_dir_all(root.path().join(folder)).unwrap();
    }
    let server = Server::start(root.path(), Ipv4Addr::LOCALHOST, 0).unwrap();
    (root, server)
}
fn request(
    server: &Server,
    method: &str,
    path: &str,
    body: &[u8],
    extra: &str,
    authenticated: bool,
) -> (u16, Vec<u8>) {
    let mut stream = TcpStream::connect(server.address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(4)))
        .unwrap();
    let pin = if authenticated {
        server.pin.as_str()
    } else {
        "incorrect"
    };
    write!(stream, "{method} {path} HTTP/1.1\r\nHost: {}\r\nX-Slot-Pin: {pin}\r\nContent-Length: {}\r\n{extra}\r\n", server.address, body.len()).unwrap();
    let _ = stream.write_all(body);
    read_response(stream)
}
fn read_response(mut stream: TcpStream) -> (u16, Vec<u8>) {
    let mut output = Vec::new();
    let _ = stream.read_to_end(&mut output);
    let end = output
        .windows(4)
        .position(|s| s == b"\r\n\r\n")
        .expect("HTTP response")
        + 4;
    let header = std::str::from_utf8(&output[..end]).unwrap();
    let code = header.split_whitespace().nth(1).unwrap().parse().unwrap();
    (code, output[end..].to_vec())
}
#[test]
fn page_login_upload_list_download_and_explicit_replace() {
    let (root, server) = setup();
    assert_eq!(request(&server, "GET", "/", b"", "", false).0, 200);
    assert_eq!(
        request(&server, "GET", "/api/files?dir=Games%2FGBA", b"", "", false).0,
        401
    );
    let path = "/api/file?dir=Games%2FGBA&name=Test%20game.gba";
    assert_eq!(request(&server, "PUT", path, b"old", "", true).0, 201);
    assert_eq!(request(&server, "PUT", path, b"new", "", true).0, 409);
    assert_eq!(
        fs::read(root.path().join("Games/GBA/Test game.gba")).unwrap(),
        b"old"
    );
    assert_eq!(
        request(
            &server,
            "PUT",
            &format!("{path}&replace=1"),
            b"new",
            "",
            true
        )
        .0,
        201
    );
    assert_eq!(
        request(&server, "GET", path, b"", "", true),
        (200, b"new".to_vec())
    );
    let (code, body) = request(&server, "GET", "/api/files?dir=Games%2FGBA", b"", "", true);
    assert_eq!(code, 200);
    let list: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(list["files"][0]["name"], "Test game.gba");
    assert_eq!(list["files"][0]["size"], 3);
    assert_eq!(server.status().uploaded, 2);
}
#[test]
fn traversal_wrong_types_and_symlinks_cannot_escape_the_card_folders() {
    let (root, server) = setup();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("secret.gba"), b"private").unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("secret.gba"),
        root.path().join("Games/GBA/link.gba"),
    )
    .unwrap();
    for path in [
        "/api/file?dir=Games%2FGBA&name=..%2F..%2FSystem%2Fslot",
        "/api/file?dir=System&name=wifi.conf",
        "/api/file?dir=Games%2FGBA&name=link.gba",
        "/api/file?dir=Games%2FGBA&name=.hidden.gba",
        "/api/file?dir=Games%2FGBA&name=test%00.gba",
    ] {
        assert!(
            request(&server, "PUT", path, b"bad", "", true).0 >= 400,
            "{path}"
        );
    }
    assert_eq!(
        request(
            &server,
            "GET",
            "/api/file?dir=Games%2FGBA&name=link.gba",
            b"",
            "",
            true
        )
        .0,
        403
    );
    assert_eq!(
        request(
            &server,
            "PUT",
            "/api/file?dir=Games%2FGBA&name=test.png",
            b"bad",
            "",
            true
        )
        .0,
        400
    );
    fs::remove_dir(root.path().join("BIOS")).unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("BIOS")).unwrap();
    assert_eq!(
        request(&server, "GET", "/api/files?dir=BIOS", b"", "", true).0,
        403
    );
    assert_eq!(
        fs::read(outside.path().join("secret.gba")).unwrap(),
        b"private"
    );
}
#[test]
fn foreign_origins_and_repeated_wrong_codes_are_rejected() {
    let (_root, server) = setup();
    let path = "/api/files?dir=Games%2FGBA";
    assert_eq!(
        request(
            &server,
            "GET",
            path,
            b"",
            "Origin: http://unrelated.example\r\n",
            true
        )
        .0,
        403
    );
    for _ in 0..8 {
        assert_eq!(request(&server, "GET", path, b"", "", false).0, 401);
    }
    assert_eq!(request(&server, "GET", path, b"", "", false).0, 429);
}
fn partial(server: &Server, length: u64) -> TcpStream {
    let mut stream = TcpStream::connect(server.address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(4)))
        .unwrap();
    write!(stream, "PUT /api/file?dir=Games%2FGBA&name=existing.gba&replace=1 HTTP/1.1\r\nHost: {}\r\nX-Slot-Pin: {}\r\nContent-Length: {length}\r\n\r\nabc", server.address, server.pin).unwrap();
    stream
}
fn wait_for(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(20));
    }
}
#[test]
fn interrupted_upload_keeps_original_and_removes_temporary_file() {
    let (root, server) = setup();
    let target = root.path().join("Games/GBA/existing.gba");
    fs::write(&target, b"original").unwrap();
    let stream = partial(&server, 100);
    wait_for(|| server.status().message.starts_with("Receiving"));
    stream.shutdown(Shutdown::Both).unwrap();
    drop(stream);
    wait_for(|| server.status().message.contains("interrupted"));
    wait_for(|| fs::read_dir(root.path().join("Games/GBA")).unwrap().count() == 1);
    assert_eq!(fs::read(target).unwrap(), b"original");
}
#[test]
fn stopping_transfer_cancels_an_inflight_upload_and_closes_the_port() {
    let (root, server) = setup();
    let target = root.path().join("Games/GBA/existing.gba");
    fs::write(&target, b"original").unwrap();
    let _stream = partial(&server, 100);
    wait_for(|| server.status().message.starts_with("Receiving"));
    server.stop();
    wait_for(|| TcpStream::connect(server.address).is_err());
    assert_eq!(fs::read(target).unwrap(), b"original");
    assert_eq!(
        fs::read_dir(root.path().join("Games/GBA")).unwrap().count(),
        1
    );
}
#[test]
fn oversized_upload_is_rejected_before_the_body_is_read() {
    let (root, server) = setup();
    assert_eq!(read_response(partial(&server, MAX_FILE + 1)).0, 413);
    assert_eq!(
        fs::read_dir(root.path().join("Games/GBA")).unwrap().count(),
        0
    );
}
#[test]
fn fragmented_binary_upload_is_exact_and_rebinded_host_is_rejected() {
    let (root, server) = setup();
    let mut stream = TcpStream::connect(server.address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(4)))
        .unwrap();
    let body: Vec<u8> = (0..200_000).map(|i| (i % 256) as u8).collect();
    let header = format!("PUT /api/file?dir=Games%2FGBA&name=binary.gba HTTP/1.1\r\nHost: {}\r\nX-Slot-Pin: {}\r\nContent-Length: {}\r\n\r\n",server.address,server.pin,body.len());
    for chunk in header.as_bytes().chunks(7) {
        stream.write_all(chunk).unwrap();
    }
    for chunk in body.chunks(1023) {
        stream.write_all(chunk).unwrap();
    }
    assert_eq!(read_response(stream).0, 201);
    assert_eq!(
        fs::read(root.path().join("Games/GBA/binary.gba")).unwrap(),
        body
    );
    let mut stream = TcpStream::connect(server.address).unwrap();
    write!(stream, "GET / HTTP/1.1\r\nHost: attacker.example\r\n\r\n").unwrap();
    assert_eq!(read_response(stream).0, 403);
}

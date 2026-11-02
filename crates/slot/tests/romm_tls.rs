use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use slot::romm::http::{Http, Req, Ureq};
use slot::romm::tls::{fingerprint, parse_pin, pin_hex, pin_text};
use slot::romm::{RommFail, Server};

const CERT: &[u8] = include_bytes!("fixtures/romm_self_signed.der");
const KEY: &[u8] = include_bytes!("fixtures/romm_self_signed.key.der");

fn serve(requests: usize) -> String {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(CERT.to_vec())],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(KEY.to_vec())),
        )
        .unwrap();
    let config = Arc::new(config);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().take(requests) {
            let Ok(mut tcp) = stream else { continue };
            let Ok(mut conn) = rustls::ServerConnection::new(config.clone()) else {
                continue;
            };
            let mut tls = rustls::Stream::new(&mut conn, &mut tcp);
            let mut buf = [0u8; 4096];
            if tls.read(&mut buf).is_err() {
                continue;
            }
            let body = b"[]";
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = tls.write_all(head.as_bytes());
            let _ = tls.write_all(body);
            let _ = tls.flush();
            tls.conn.send_close_notify();
            let _ = tls.flush();
        }
    });
    format!("https://127.0.0.1:{port}")
}

fn server(base: &str, pin: Option<[u8; 32]>) -> Server {
    let mut s = Server::new(base, None).unwrap();
    s.pin = pin;
    s
}

#[test]
fn pins_print_and_parse_back() {
    let pin = fingerprint(CERT);
    assert_eq!(parse_pin(&pin_hex(&pin)), Some(pin));
    assert_eq!(parse_pin(&pin_text(&pin)), Some(pin));
    assert_eq!(pin_text(&pin).len(), 32 * 3 - 1);
    assert_eq!(parse_pin("abc"), None);
}

#[test]
fn a_self_signed_server_is_refused_until_its_certificate_is_pinned() {
    let base = serve(4);
    let url = format!("{base}/api/platforms");
    let plain = server(&base, None);
    assert_eq!(
        Ureq.call(&Req::get(&plain, url.clone())).err(),
        Some(RommFail::Certificate)
    );
    let seen = Ureq.certificate(&format!("{base}/")).unwrap();
    assert_eq!(seen, fingerprint(CERT));
    let pinned = server(&base, Some(seen));
    let reply = Ureq.call(&Req::get(&pinned, url.clone())).unwrap();
    assert_eq!(reply.status, 200);
    assert_eq!(reply.bytes(64).unwrap(), b"[]");
    let wrong = server(&base, Some([7; 32]));
    assert_eq!(
        Ureq.call(&Req::get(&wrong, url)).err(),
        Some(RommFail::Certificate)
    );
}

#[test]
fn the_pin_only_goes_to_the_romm_host() {
    let s = server("https://romm.home", Some([1; 32]));
    assert_eq!(
        Req::get(&s, "https://romm.home/api".into()).pin,
        Some([1; 32])
    );
    assert_eq!(
        Req::get(&s, "https://screenscraper.fr/cart.png".into()).pin,
        None
    );
    assert_eq!(Req::get(&s, "http://romm.home/api".into()).pin, None);
}

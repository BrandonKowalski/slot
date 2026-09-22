//! Desktop test harness: serves only an explicitly supplied fixture card on loopback.
fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).expect("fixture card directory"));
    let server = slot::transfer::Server::start(&root, std::net::Ipv4Addr::LOCALHOST, 8080).unwrap();
    println!("http://{}/  code {}", server.address, server.pin);
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

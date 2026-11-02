mod common;

use std::io::Cursor;
use std::sync::{Arc, Mutex, MutexGuard};

use common::{app_booting_at, tmp_root_with_carts, CLOCK_IS_SET};
use slot::app::{App, KeyboardFor, Phase};
use slot::romm::http::{Http, Reply, Req};
use slot::romm::jobs::romm_jobs;
use slot::romm::login::{load, LOGIN_FILE};
use slot::romm::screen::{View, LOADING_MESSAGE, NO_MATCH_MESSAGE};
use slot::romm::{Login, RommFail};
use slot::wifi::{Cmd, Fake, Wifi};
use slot_input::{Action, Btn};
use slot_store::{write_slot_state, Platform, SlotState};
use slot_ui::QuickRow;
use tempfile::TempDir;

static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

const BASE: &str = "https://romm.home";

#[derive(Default)]
struct Server {
    seen: Mutex<Vec<Req>>,
    password_ok: bool,
    self_signed: bool,
    queue: Mutex<Vec<(String, u64)>>,
    reports: Mutex<Vec<(String, String)>>,
    capability: Mutex<Vec<bool>>,
}

const PIN: [u8; 32] = [0xAB; 32];

fn games_page(platform: u64) -> String {
    let games: Vec<String> = match platform {
        1 => vec![
            game(10, "Emerald", "Emerald"),
            game(11, "Metroid Fusion", "Metroid Fusion (USA)"),
            game(12, "Advance Wars", "Advance Wars (USA)"),
        ],
        2 => (0..40)
            .map(|i| {
                game(
                    100 + i,
                    &format!("Color Game {i:02}"),
                    &format!("Color Game {i:02}"),
                )
            })
            .collect(),
        _ => Vec::new(),
    };
    format!(
        "{{\"items\":[{}],\"total\":{}}}",
        games.join(","),
        games.len()
    )
}

fn game(id: u64, name: &str, file: &str) -> String {
    format!(
        "{{\"id\":{id},\"name\":\"{name}\",\"fs_name\":\"{file}.gba\",\"fs_name_no_ext\":\"{file}\",\"fs_name_no_tags\":\"{name}\",\"fs_size_bytes\":4}}"
    )
}

impl Http for Server {
    fn call(&self, req: &Req) -> Result<Reply, RommFail> {
        self.seen.lock().unwrap().push(req.clone());
        if self.self_signed && req.pin != Some(PIN) {
            return Err(RommFail::Certificate);
        }
        let path = req.url.strip_prefix(BASE).unwrap_or(&req.url).to_string();
        let authed = match req.auth.as_deref() {
            Some("Bearer rmm_paired") => true,
            Some(a) if a.starts_with("Basic ") => self.password_ok,
            _ => false,
        };
        let (status, body): (u16, Vec<u8>) = match path.as_str() {
            "/api/client-tokens/exchange" => (200, br#"{"raw_token":"rmm_paired"}"#.to_vec()),
            "/api/auth/device/init" => (
                201,
                br#"{"device_code":"dc","user_code":"K7QF-2MXP","verification_path_complete":"/pair/device?user_code=K7QF-2MXP","expires_in":600,"interval":2}"#.to_vec(),
            ),
            "/api/auth/device/token" => (
                200,
                br#"{"access_token":"rmm_paired","device_id":"dev-1","scopes":["platforms.read","roms.read","devices.read","devices.write"]}"#.to_vec(),
            ),
            _ if !authed => (401, b"{}".to_vec()),
            "/api/devices/dev-1" if req.method == "PUT" => {
                let v: serde_json::Value = serde_json::from_str(req.json.as_deref().unwrap()).unwrap();
                let on = v["capabilities"]["remote_install"].as_bool().unwrap();
                self.capability.lock().unwrap().push(on);
                (200, b"{}".to_vec())
            }
            "/api/devices/dev-1/installs/claim" => {
                let items: Vec<String> = self
                    .queue
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|(id, rom)| format!("{{\"id\":\"{id}\",\"rom_id\":{rom},\"file_ids\":[]}}"))
                    .collect();
                (200, format!("[{}]", items.join(",")).into_bytes())
            }
            p if p.starts_with("/api/devices/dev-1/installs/") => {
                let id = p.rsplit('/').next().unwrap().to_string();
                let v: serde_json::Value = serde_json::from_str(req.json.as_deref().unwrap()).unwrap();
                self.queue.lock().unwrap().retain(|(q, _)| *q != id);
                let status = v["status"].as_str().unwrap().to_string();
                self.reports.lock().unwrap().push((id, status));
                (200, b"{}".to_vec())
            }
            "/api/roms/12" => (
                200,
                br#"{"id":12,"name":"Advance Wars","fs_name":"Advance Wars (USA).gba","fs_name_no_ext":"Advance Wars (USA)","platform_slug":"gba"}"#.to_vec(),
            ),
            p if p.starts_with("/api/roms/12/content/") => (200, b"WARS".to_vec()),
            "/api/platforms" => (
                200,
                br#"[{"id":1,"slug":"gba","name":"Game Boy Advance"},{"id":2,"slug":"gbc","name":"Game Boy Color"}]"#.to_vec(),
            ),
            p if p.starts_with("/api/roms?platform_ids=1&") => (200, games_page(1).into_bytes()),
            p if p.starts_with("/api/roms?platform_ids=2&") => (200, games_page(2).into_bytes()),
            p if p.starts_with("/api/roms/11/content/") => (200, b"ROM!".to_vec()),
            _ => (404, b"{}".to_vec()),
        };
        Ok(Reply {
            status,
            length: Some(body.len() as u64),
            body: Box::new(Cursor::new(body)),
        })
    }

    fn certificate(&self, url: &str) -> Result<[u8; 32], RommFail> {
        assert_eq!(url, format!("{BASE}/"));
        Ok(PIN)
    }
}

fn booted(server: Server) -> (TempDir, App, Arc<Server>) {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    write_slot_state(
        d.path(),
        &SlotState {
            clock_set: true,
            ..SlotState::default()
        },
    )
    .unwrap();
    std::fs::write(d.path().join("Config/wifi.conf"), b"network={}\n").unwrap();
    let (mut a, _clock) = app_booting_at(d.path(), CLOCK_IS_SET);
    a.set_wifi(Wifi::inline(Box::new(Fake::demo())));
    settle(&mut a);
    a.wifi_send(Cmd::Join {
        ssid: "CoffeeShop Guest".into(),
        password: String::new(),
        hidden: false,
    });
    settle(&mut a);
    assert!(a.wifi_up());
    let server = Arc::new(server);
    let http: Arc<dyn Http> = server.clone();
    a.set_romm_jobs(romm_jobs(http.clone(), Arc::new(|_, _| false)));
    a.set_romm_remote_jobs(romm_jobs(http, Arc::new(|_, _| false)));
    (d, a, server)
}

fn settle(a: &mut App) {
    for _ in 0..4 {
        let t = a.now();
        a.tick_ms(t + 1_000);
    }
}

fn wait(a: &mut App, done: impl Fn(&App) -> bool) {
    for _ in 0..200 {
        if done(a) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        let t = a.now();
        a.tick_ms(t + 50);
    }
    panic!("timed out in {:?}", a.phase());
}

fn press(a: &mut App, btn: Btn) {
    a.apply(Action::GbaDown(btn));
    a.apply(Action::GbaUp(btn));
}

fn labels(a: &App) -> Vec<String> {
    a.network_screen()
        .expect("no screen")
        .lines
        .into_iter()
        .map(|l| l.label)
        .collect()
}

fn selected(a: &App) -> String {
    let s = a.network_screen().unwrap();
    s.lines[s.selected - s.first].label.clone()
}

fn select(a: &mut App, label: &str) {
    for _ in 0..60 {
        if selected(a) == label {
            return;
        }
        press(a, Btn::Down);
    }
    panic!("{label} is not on the list: {:?}", labels(a));
}

fn key_at(a: &App, want: &str) -> Option<(usize, usize)> {
    let kb = a.keyboard().expect("no keyboard");
    kb.rows().iter().enumerate().find_map(|(r, row)| {
        row.iter()
            .position(|cap| kb.label(*cap) == want)
            .map(|c| (r, c))
    })
}

fn go_press(a: &mut App, (r, c): (usize, usize)) {
    for _ in 0..12 {
        if a.keyboard().unwrap().row() == r {
            break;
        }
        press(a, Btn::Down);
    }
    for _ in 0..16 {
        if a.keyboard().unwrap().col() == c {
            break;
        }
        press(a, Btn::Right);
    }
    press(a, Btn::A);
}

fn flip_symbols(a: &mut App) {
    let kb = a.keyboard().unwrap();
    let at = kb
        .rows()
        .iter()
        .enumerate()
        .find_map(|(r, row)| {
            row.iter()
                .position(|cap| *cap == slot_ui::Cap::Special(slot_ui::Special::Symbols))
                .map(|c| (r, c))
        })
        .expect("no symbols key");
    go_press(a, at);
}

fn type_text(a: &mut App, text: &str) {
    for ch in text.chars() {
        let want = ch.to_string();
        let at = match key_at(a, &want) {
            Some(at) => at,
            None => {
                flip_symbols(a);
                key_at(a, &want).unwrap_or_else(|| panic!("no key for {ch}"))
            }
        };
        go_press(a, at);
        if a.keyboard().unwrap().symbols() {
            flip_symbols(a);
        }
    }
    assert!(
        a.keyboard().unwrap().text().ends_with(text),
        "{:?}",
        a.keyboard().unwrap().text()
    );
}

fn open_romm(a: &mut App) {
    a.apply(Action::QuickMenu);
    for _ in 0..12 {
        if a.quick_menu() == Some(QuickRow::Network) {
            break;
        }
        press(a, Btn::Down);
    }
    press(a, Btn::A);
    for _ in 0..8 {
        if a.quick_menu() == Some(QuickRow::Romm) {
            break;
        }
        press(a, Btn::Down);
    }
    assert_eq!(
        a.quick_menu(),
        Some(QuickRow::Romm),
        "{:?}",
        a.hidden_rows()
    );
    press(a, Btn::A);
    assert!(matches!(a.phase(), Phase::Romm { .. }), "{:?}", a.phase());
}

fn set_server(a: &mut App) {
    select(a, "Server");
    press(a, Btn::A);
    assert_eq!(a.keyboard_for(), Some(&KeyboardFor::RommServer));
    type_text(a, "romm.home");
    press(a, Btn::Start);
    assert!(a.keyboard().is_none());
}

fn sign_in_with_code(a: &mut App) {
    set_server(a);
    select(a, "Sign In with Pair Code");
    press(a, Btn::A);
    assert_eq!(a.keyboard_for(), Some(&KeyboardFor::RommCode));
    type_text(a, "mynx-dxcg");
    press(a, Btn::Start);
    wait(a, |a| a.romm().library.is_some());
}

#[test]
fn romm_sits_with_the_network_tools() {
    let _g = serial();
    let (_d, mut a, _s) = booted(Server::default());
    open_romm(&mut a);
    assert_eq!(
        labels(&a),
        [
            "Server",
            "Sign In with QR Code",
            "Sign In with Pair Code",
            "Sign In with Password"
        ]
    );
    press(&mut a, Btn::B);
    assert_eq!(a.quick_menu(), Some(QuickRow::Romm));
}

#[test]
fn a_pair_code_signs_in_and_loads_the_library() {
    let _g = serial();
    let (d, mut a, s) = booted(Server::default());
    open_romm(&mut a);
    sign_in_with_code(&mut a);
    let exchange = s
        .seen
        .lock()
        .unwrap()
        .iter()
        .find(|r| r.url.ends_with("/exchange"))
        .cloned()
        .unwrap();
    assert_eq!(exchange.json.as_deref(), Some(r#"{"code":"MYNXDXCG"}"#));
    let saved = load(d.path());
    assert_eq!(saved.url.as_deref(), Some(BASE));
    assert_eq!(saved.login, Some(Login::Token("rmm_paired".into())));
    assert!(d
        .path()
        .join(LOGIN_FILE)
        .starts_with(d.path().join("Cache")));
    assert_eq!(
        labels(&a),
        [
            "Game Boy Advance",
            "Game Boy Color",
            "Game Boy",
            "Search",
            "Refresh Library",
            "Remote Install",
            "Server",
            "Certificate",
            "Sign Out"
        ]
    );
    let lines = a.network_screen().unwrap().lines;
    assert_eq!(lines[0].value.as_deref(), Some("3"));
    assert_eq!(lines[1].value.as_deref(), Some("40"));
}

#[test]
fn a_saved_login_loads_the_library_on_open() {
    let _g = serial();
    let (d, mut a, _s) = booted(Server::default());
    open_romm(&mut a);
    sign_in_with_code(&mut a);
    drop(a);
    let (mut a, _clock) = app_booting_at(d.path(), CLOCK_IS_SET);
    a.set_wifi(Wifi::inline(Box::new(Fake::demo())));
    settle(&mut a);
    a.wifi_send(Cmd::Join {
        ssid: "CoffeeShop Guest".into(),
        password: String::new(),
        hidden: false,
    });
    settle(&mut a);
    let http: Arc<dyn Http> = Arc::new(Server::default());
    a.set_romm_jobs(romm_jobs(http, Arc::new(|_, _| false)));
    open_romm(&mut a);
    assert_eq!(a.network_screen().unwrap().message, Some(LOADING_MESSAGE));
    wait(&mut a, |a| a.romm().library.is_some());
    assert_eq!(labels(&a)[0], "Game Boy Advance");
}

#[test]
fn a_wrong_password_is_not_kept() {
    let _g = serial();
    let (d, mut a, _s) = booted(Server::default());
    open_romm(&mut a);
    set_server(&mut a);
    select(&mut a, "Sign In with Password");
    press(&mut a, Btn::A);
    type_text(&mut a, "me");
    press(&mut a, Btn::Start);
    assert_eq!(a.keyboard_for(), Some(&KeyboardFor::RommPassword));
    type_text(&mut a, "nope");
    press(&mut a, Btn::Start);
    wait(&mut a, |a| a.romm().busy.is_none());
    assert_eq!(a.update_text().unwrap().0, "RomM refused the login");
    assert_eq!(load(d.path()).login, None);
    press(&mut a, Btn::B);
    assert_eq!(labels(&a)[0], "Server");
}

#[test]
fn a_right_password_is_kept() {
    let _g = serial();
    let (d, mut a, _s) = booted(Server {
        password_ok: true,
        ..Server::default()
    });
    open_romm(&mut a);
    set_server(&mut a);
    select(&mut a, "Sign In with Password");
    press(&mut a, Btn::A);
    type_text(&mut a, "me");
    press(&mut a, Btn::Start);
    type_text(&mut a, "pw");
    press(&mut a, Btn::Start);
    wait(&mut a, |a| a.romm().library.is_some());
    assert_eq!(
        load(d.path()).login,
        Some(Login::Password {
            user: "me".into(),
            pass: "pw".into()
        })
    );
}

#[test]
fn the_qr_screen_shows_the_code_until_the_phone_approves() {
    let _g = serial();
    let (d, mut a, _s) = booted(Server::default());
    open_romm(&mut a);
    set_server(&mut a);
    select(&mut a, "Sign In with QR Code");
    press(&mut a, Btn::A);
    assert!(matches!(
        a.phase(),
        Phase::Romm {
            view: View::Pairing
        }
    ));
    wait(&mut a, |a| a.romm().pairing.is_some());
    let lines = a.network_screen().unwrap().lines;
    assert!(
        lines
            .iter()
            .any(|l| l.value.as_deref() == Some("K7QF-2MXP")),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.label == "romm.home/pair/device"),
        "{lines:?}"
    );
    wait(&mut a, |a| a.romm().library.is_some());
    assert!(matches!(
        a.phase(),
        Phase::Romm {
            view: View::Home { .. }
        }
    ));
    let saved = load(d.path());
    assert_eq!(saved.login, Some(Login::Token("rmm_paired".into())));
    assert!(saved.device.unwrap().starts_with("slot-"));
}

#[test]
fn long_lists_hand_over_only_the_rows_on_screen() {
    let _g = serial();
    let (_d, mut a, _s) = booted(Server::default());
    open_romm(&mut a);
    sign_in_with_code(&mut a);
    select(&mut a, "Game Boy Color");
    press(&mut a, Btn::A);
    let s = a.network_screen().unwrap();
    assert_eq!((s.lines.len(), s.first, s.total), (6, 0, Some(40)));
    for _ in 0..20 {
        press(&mut a, Btn::Down);
    }
    let s = a.network_screen().unwrap();
    assert_eq!(s.selected, 20);
    assert_eq!(selected(&a), "Color Game 20");
    assert!(s.first <= 20 && 20 < s.first + s.lines.len());
    press(&mut a, Btn::R1);
    assert!(
        matches!(
            a.phase(),
            Phase::Romm {
                view: View::Games {
                    platform: Platform::Gb,
                    row: 0
                }
            }
        ),
        "{:?}",
        a.phase()
    );
    assert_eq!(a.network_screen().unwrap().message, Some("No games here"));
}

#[test]
fn search_narrows_every_shelf() {
    let _g = serial();
    let (_d, mut a, _s) = booted(Server::default());
    open_romm(&mut a);
    sign_in_with_code(&mut a);
    select(&mut a, "Search");
    press(&mut a, Btn::A);
    type_text(&mut a, "metroid");
    press(&mut a, Btn::Start);
    let lines = a.network_screen().unwrap().lines;
    assert_eq!(lines[0].value.as_deref(), Some("1 of 3"));
    assert_eq!(lines[1].value.as_deref(), Some("0 of 40"));
    select(&mut a, "Game Boy Color");
    press(&mut a, Btn::A);
    assert_eq!(a.network_screen().unwrap().message, Some(NO_MATCH_MESSAGE));
}

#[test]
fn games_already_on_the_card_say_so() {
    let _g = serial();
    let (_d, mut a, _s) = booted(Server::default());
    open_romm(&mut a);
    sign_in_with_code(&mut a);
    press(&mut a, Btn::A);
    let lines = a.network_screen().unwrap().lines;
    let value = |title: &str| {
        lines
            .iter()
            .find(|l| l.label == title)
            .and_then(|l| l.value.clone())
    };
    assert_eq!(value("Emerald").as_deref(), Some("On Card"));
    assert_eq!(value("Advance Wars").as_deref(), Some("1 KB"));
    assert_eq!(a.network_action(), Some("Download"));
}

#[test]
fn a_download_lands_on_the_card_and_opens_the_studio() {
    let _g = serial();
    let (d, mut a, _s) = booted(Server::default());
    open_romm(&mut a);
    sign_in_with_code(&mut a);
    press(&mut a, Btn::A);
    select(&mut a, "Metroid Fusion");
    press(&mut a, Btn::A);
    wait(&mut a, |a| a.studio().is_some());
    let rom = d.path().join("Games/GBA/Metroid Fusion (USA).gba");
    assert_eq!(std::fs::read(&rom).unwrap(), b"ROM!");
    let leftovers: Vec<_> = std::fs::read_dir(d.path().join("Games/GBA"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with('.'))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    assert_eq!(a.studio().unwrap().cart.stem, "Metroid Fusion (USA)");
    for _ in 0..6 {
        if !matches!(a.phase(), Phase::Shelf) {
            break;
        }
        press(&mut a, Btn::B);
        settle(&mut a);
    }
    assert!(a.studio().is_none());
    assert!(
        matches!(
            a.phase(),
            Phase::Romm {
                view: View::Games {
                    platform: Platform::Gba,
                    row: 2
                }
            }
        ),
        "{:?}",
        a.phase()
    );
    assert_eq!(selected(&a), "Metroid Fusion");
    assert_eq!(a.network_action(), Some("Open"));
    let lines = a.network_screen().unwrap().lines;
    assert_eq!(lines[2].value.as_deref(), Some("On Card"));
}

#[test]
fn a_game_on_the_card_opens_straight_in_the_studio() {
    let _g = serial();
    let (_d, mut a, _s) = booted(Server::default());
    open_romm(&mut a);
    sign_in_with_code(&mut a);
    press(&mut a, Btn::A);
    select(&mut a, "Emerald");
    press(&mut a, Btn::A);
    assert_eq!(a.studio().map(|s| s.cart.stem.as_str()), Some("Emerald"));
}

#[test]
fn a_self_signed_certificate_can_be_pinned_after_a_look_at_its_fingerprint() {
    let _g = serial();
    let (d, mut a, _s) = booted(Server {
        self_signed: true,
        ..Server::default()
    });
    open_romm(&mut a);
    set_server(&mut a);
    assert_eq!(
        labels(&a),
        [
            "Server",
            "Certificate",
            "Sign In with QR Code",
            "Sign In with Pair Code",
            "Sign In with Password"
        ]
    );
    select(&mut a, "Sign In with Pair Code");
    press(&mut a, Btn::A);
    type_text(&mut a, "abcd");
    press(&mut a, Btn::Start);
    wait(&mut a, |a| a.romm().busy.is_none());
    assert_eq!(
        a.update_text().unwrap().0,
        "RomM's certificate isn't trusted"
    );
    press(&mut a, Btn::B);
    select(&mut a, "Certificate");
    assert_eq!(
        a.network_screen().unwrap().lines[1].value.as_deref(),
        Some("Checked")
    );
    press(&mut a, Btn::A);
    wait(&mut a, |a| {
        matches!(
            a.phase(),
            Phase::Romm {
                view: View::Certificate { .. }
            }
        )
    });
    let lines = labels(&a);
    assert_eq!(lines[0], "Trust this certificate?");
    assert_eq!(lines[2], "AB:AB:AB:AB:AB:AB:AB:AB");
    assert_eq!(lines.len(), 6);
    assert_eq!(a.network_action(), Some("Trust"));
    press(&mut a, Btn::A);
    assert_eq!(load(d.path()).pin, Some(PIN));
    assert_eq!(selected(&a), "Certificate");
    assert_eq!(
        a.network_screen().unwrap().lines[1].value.as_deref(),
        Some("Pinned")
    );
    select(&mut a, "Sign In with Pair Code");
    press(&mut a, Btn::A);
    type_text(&mut a, "abcd");
    press(&mut a, Btn::Start);
    wait(&mut a, |a| a.romm().library.is_some());
    assert_eq!(labels(&a)[0], "Game Boy Advance");
    select(&mut a, "Certificate");
    assert_eq!(a.network_action(), Some("Forget"));
    press(&mut a, Btn::A);
    assert_eq!(load(d.path()).pin, None);
}

#[test]
fn http_servers_have_no_certificate_row() {
    let _g = serial();
    let (_d, mut a, _s) = booted(Server::default());
    open_romm(&mut a);
    select(&mut a, "Server");
    press(&mut a, Btn::A);
    type_text(&mut a, "http://romm.home");
    press(&mut a, Btn::Start);
    assert!(!labels(&a).contains(&"Certificate".to_string()));
}

fn value_of(a: &App, label: &str) -> Option<String> {
    a.network_screen()
        .unwrap()
        .lines
        .into_iter()
        .find(|l| l.label == label)
        .and_then(|l| l.value)
}

#[test]
fn games_queued_in_romm_land_on_the_card_by_themselves() {
    let _g = serial();
    let (d, mut a, s) = booted(Server::default());
    open_romm(&mut a);
    set_server(&mut a);
    select(&mut a, "Sign In with QR Code");
    press(&mut a, Btn::A);
    wait(&mut a, |a| a.romm().library.is_some());
    let saved = load(d.path());
    assert!(saved.remote);
    assert_eq!(saved.romm_device.as_deref(), Some("dev-1"));
    assert_eq!(*s.capability.lock().unwrap(), [true]);
    assert_eq!(value_of(&a, "Remote Install").as_deref(), Some("On"));
    press(&mut a, Btn::B);
    assert_eq!(a.quick_menu(), Some(QuickRow::Romm));
    s.queue.lock().unwrap().push(("r-1".into(), 12));
    let t = a.now();
    a.tick_ms(t + 61_000);
    wait(&mut a, |_| !s.reports.lock().unwrap().is_empty());
    wait(&mut a, |a| a.romm_delivered().is_some());
    assert_eq!(
        *s.reports.lock().unwrap(),
        [("r-1".to_string(), "done".to_string())]
    );
    assert_eq!(
        std::fs::read(d.path().join("Games/GBA/Advance Wars (USA).gba")).unwrap(),
        b"WARS"
    );
    assert_eq!(a.romm_delivered(), Some("Advance Wars"));
}

#[test]
fn remote_install_can_be_turned_off() {
    let _g = serial();
    let (d, mut a, s) = booted(Server::default());
    open_romm(&mut a);
    set_server(&mut a);
    select(&mut a, "Sign In with QR Code");
    press(&mut a, Btn::A);
    wait(&mut a, |a| a.romm().library.is_some());
    select(&mut a, "Remote Install");
    assert_eq!(a.network_action(), Some("Change"));
    press(&mut a, Btn::A);
    assert_eq!(value_of(&a, "Remote Install").as_deref(), Some("Off"));
    wait(&mut a, |_| s.capability.lock().unwrap().len() == 2);
    assert_eq!(*s.capability.lock().unwrap(), [true, false]);
    assert!(!load(d.path()).remote);
    s.queue.lock().unwrap().push(("r-1".into(), 12));
    let t = a.now();
    a.tick_ms(t + 61_000);
    settle(&mut a);
    assert!(s.reports.lock().unwrap().is_empty());
}

#[test]
fn a_pair_code_login_needs_the_qr_code_for_remote_install() {
    let _g = serial();
    let (_d, mut a, _s) = booted(Server::default());
    open_romm(&mut a);
    sign_in_with_code(&mut a);
    assert_eq!(
        value_of(&a, "Remote Install").as_deref(),
        Some("Needs QR Sign-In")
    );
    select(&mut a, "Remote Install");
    assert_eq!(a.network_action(), Some("Connect"));
    press(&mut a, Btn::A);
    assert!(matches!(
        a.phase(),
        Phase::Romm {
            view: View::Pairing
        }
    ));
}

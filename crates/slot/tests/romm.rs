use std::io::Cursor;
use std::sync::Mutex;

use slot::romm::api::{
    content_url, exchange_code, games, parse_game, parse_shelves, poll_pairing, shelves,
    start_pairing, Game, Poll,
};
use slot::romm::http::{Http, Reply, Req};
use slot::romm::login::{forget, load, save, Saved};
use slot::romm::name::{card_stem, matches, normalise, same_game};
use slot::romm::{base64, normalize_url, Login, RommFail, Server};
use slot_store::Platform;

struct Fake {
    routes: Vec<(String, u16, String)>,
    seen: Mutex<Vec<Req>>,
}

impl Fake {
    fn new(routes: &[(&str, u16, &str)]) -> Fake {
        Fake {
            routes: routes
                .iter()
                .map(|(u, s, b)| (u.to_string(), *s, b.to_string()))
                .collect(),
            seen: Mutex::new(Vec::new()),
        }
    }

    fn seen(&self) -> Vec<Req> {
        self.seen.lock().unwrap().clone()
    }
}

impl Http for Fake {
    fn call(&self, req: &Req) -> Result<Reply, RommFail> {
        self.seen.lock().unwrap().push(req.clone());
        let (_, status, body) = self
            .routes
            .iter()
            .find(|(u, _, _)| *u == req.url)
            .ok_or(RommFail::Offline)?;
        Ok(Reply {
            status: *status,
            length: Some(body.len() as u64),
            body: Box::new(Cursor::new(body.clone().into_bytes())),
        })
    }
}

fn server(login: Option<Login>) -> Server {
    Server::new("https://romm.home", login).unwrap()
}

#[test]
fn addresses_are_normalised() {
    assert_eq!(
        normalize_url(" romm.home/ ").as_deref(),
        Some("https://romm.home")
    );
    assert_eq!(
        normalize_url("HTTP://192.168.1.4:8080").as_deref(),
        Some("http://192.168.1.4:8080")
    );
    assert_eq!(
        normalize_url("https://romm.home/sub/").as_deref(),
        Some("https://romm.home/sub")
    );
    assert_eq!(normalize_url("ftp://romm.home"), None);
    assert_eq!(normalize_url("https://user:pw@romm.home"), None);
    assert_eq!(normalize_url("rom m.home"), None);
    assert_eq!(normalize_url(""), None);
}

#[test]
fn base64_matches_known_vectors() {
    assert_eq!(base64(b""), "");
    assert_eq!(base64(b"f"), "Zg==");
    assert_eq!(base64(b"fo"), "Zm8=");
    assert_eq!(base64(b"foo"), "Zm9v");
    assert_eq!(base64(b"admin:p4ss=word"), "YWRtaW46cDRzcz13b3Jk");
}

#[test]
fn login_only_goes_to_the_romm_host_over_its_scheme() {
    let s = server(Some(Login::Password {
        user: "me".into(),
        pass: "pw".into(),
    }));
    assert_eq!(
        s.auth_for("https://romm.home/api/roms").as_deref(),
        Some("Basic bWU6cHc=")
    );
    assert_eq!(
        s.auth_for("https://ROMM.home/api/roms").as_deref(),
        Some("Basic bWU6cHc=")
    );
    assert_eq!(s.auth_for("http://romm.home/api/roms"), None);
    assert_eq!(s.auth_for("https://screenscraper.fr/x.png"), None);
    let t = server(Some(Login::Token("rmm_abc".into())));
    assert_eq!(
        t.auth_for("https://romm.home/x").as_deref(),
        Some("Bearer rmm_abc")
    );
    assert_eq!(server(None).auth_for("https://romm.home/x"), None);
}

#[test]
fn resources_resolve_against_the_server() {
    let s = server(None);
    assert_eq!(
        s.resource("roms/1/cart.png"),
        "https://romm.home/assets/romm/resources/roms/1/cart.png"
    );
    assert_eq!(
        s.resource("/assets/x.png"),
        "https://romm.home/assets/x.png"
    );
    assert_eq!(
        s.resource("https://cdn.example/x.png"),
        "https://cdn.example/x.png"
    );
}

#[test]
fn only_game_boy_shelves_are_kept() {
    let v = serde_json::json!([
        {"id": 1, "slug": "gba", "fs_slug": "gba", "name": "Game Boy Advance"},
        {"id": 2, "slug": "snes", "fs_slug": "snes", "name": "SNES"},
        {"id": 3, "slug": "something", "fs_slug": "game-boy-color", "name": "Color"},
        {"id": 4, "slug": "GameBoy", "name": "GB"},
    ]);
    let found = parse_shelves(&v);
    let ids: Vec<(u64, Platform)> = found.iter().map(|s| (s.id, s.platform)).collect();
    assert_eq!(
        ids,
        vec![(1, Platform::Gba), (3, Platform::Gbc), (4, Platform::Gb)]
    );
}

#[test]
fn shelves_report_a_refused_login() {
    let fake = Fake::new(&[("https://romm.home/api/platforms", 401, "{}")]);
    assert_eq!(shelves(&fake, &server(None)), Err(RommFail::Refused));
}

#[test]
fn games_are_read_page_by_page() {
    let page = |ids: &[u64], total: u64| {
        let items: Vec<String> = ids
            .iter()
            .map(|i| format!("{{\"id\":{i},\"name\":\"G{i}\"}}"))
            .collect();
        format!("{{\"items\":[{}],\"total\":{total}}}", items.join(","))
    };
    let first = page(&(0..500).collect::<Vec<_>>(), 502);
    let second = page(&[500, 501], 502);
    let fake = Fake::new(&[
        (
            "https://romm.home/api/roms?platform_ids=7&with_files=true&limit=500&offset=0",
            200,
            &first,
        ),
        (
            "https://romm.home/api/roms?platform_ids=7&with_files=true&limit=500&offset=500",
            200,
            &second,
        ),
    ]);
    let shelf = parse_shelves(&serde_json::json!([{"id": 7, "slug": "gb"}])).remove(0);
    let s = server(Some(Login::Token("rmm_t".into())));
    let got = games(&fake, &s, &shelf, &|| false).unwrap();
    assert_eq!(got.len(), 502);
    assert_eq!(got[501].name, "G501");
    assert!(fake
        .seen()
        .iter()
        .all(|r| r.auth.as_deref() == Some("Bearer rmm_t")));
}

#[test]
fn a_game_reads_its_checksums_and_cartridge_photo() {
    let v = serde_json::json!({
        "id": 12, "name": "Pokemon Emerald", "fs_name": "Pokemon - Emerald (USA).gba",
        "fs_name_no_ext": "Pokemon - Emerald (USA)", "fs_name_no_tags": "Pokemon - Emerald",
        "fs_size_bytes": 16777216, "crc_hash": "1f1c08fb",
        "files": [{"file_name": "a.gba", "crc_hash": "1F1C08FB"}, {"crc_hash": "0000beef"}, {"crc_hash": ""}],
        "ss_metadata": {"physical_path": "", "physical_url": "https://ss.example/cart.png"},
        "missing_from_fs": false
    });
    let g = parse_game(&v).unwrap();
    assert_eq!(g.crcs, vec![0x1f1c08fb, 0xbeef]);
    assert_eq!(g.cart_photo.as_deref(), Some("https://ss.example/cart.png"));
    assert_eq!(g.size, 16777216);
    assert_eq!(g.title(), "Pokemon Emerald");
    assert!(parse_game(&serde_json::json!({"name": "no id"})).is_none());
}

#[test]
fn content_urls_escape_the_file_name() {
    let g = Game {
        id: 3,
        fs_name: "Mario & Luigi (E) #1.gba".into(),
        ..Game::default()
    };
    assert_eq!(
        content_url(&server(None), &g),
        "https://romm.home/api/roms/3/content/Mario%20%26%20Luigi%20%28E%29%20%231.gba"
    );
    let bare = Game {
        id: 4,
        ..Game::default()
    };
    assert_eq!(
        content_url(&server(None), &bare),
        "https://romm.home/api/roms/4/content/rom-4"
    );
}

#[test]
fn a_pair_code_is_exchanged_for_a_token() {
    let fake = Fake::new(&[(
        "https://romm.home/api/client-tokens/exchange",
        200,
        r#"{"id":1,"name":"slot","raw_token":"rmm_0123"}"#,
    )]);
    assert_eq!(
        exchange_code(&fake, &server(None), "mynx-dxcg ").as_deref(),
        Ok("rmm_0123")
    );
    assert_eq!(
        fake.seen()[0].json.as_deref(),
        Some(r#"{"code":"MYNXDXCG"}"#)
    );
    assert!(fake.seen()[0].post);
}

#[test]
fn an_expired_pair_code_says_so() {
    let fake = Fake::new(&[(
        "https://romm.home/api/client-tokens/exchange",
        404,
        r#"{"detail":"Invalid or expired pairing code"}"#,
    )]);
    assert_eq!(
        exchange_code(&fake, &server(None), "AAAA-BBBB"),
        Err(RommFail::Expired)
    );
    let busy = Fake::new(&[("https://romm.home/api/client-tokens/exchange", 429, "{}")]);
    assert_eq!(
        exchange_code(&busy, &server(None), "AAAA-BBBB"),
        Err(RommFail::TooMany)
    );
}

const INIT: &str = r#"{"device_code":"dc","user_code":"K7QF-2MXP","verification_path":"/pair/device","verification_path_complete":"/pair/device?user_code=K7QF-2MXP","expires_in":600,"interval":1}"#;

#[test]
fn pairing_starts_without_a_login_and_links_to_the_server() {
    let fake = Fake::new(&[("https://romm.home/api/auth/device/init", 201, INIT)]);
    let s = server(Some(Login::Token("old".into())));
    let p = start_pairing(&fake, &s, "slot-1").unwrap();
    assert_eq!(p.user_code, "K7QF-2MXP");
    assert_eq!(p.link, "https://romm.home/pair/device?user_code=K7QF-2MXP");
    assert_eq!(p.interval.as_secs(), 2);
    assert_eq!(fake.seen()[0].auth, None);
    let body: serde_json::Value =
        serde_json::from_str(fake.seen()[0].json.as_deref().unwrap()).unwrap();
    assert_eq!(body["client_device_identifier"], "slot-1");
    assert_eq!(
        body["requested_scopes"],
        serde_json::json!(["platforms.read", "roms.read"])
    );
}

#[test]
fn pairing_needs_a_recent_romm_and_a_safe_link() {
    let old = Fake::new(&[("https://romm.home/api/auth/device/init", 404, "{}")]);
    assert_eq!(
        start_pairing(&old, &server(None), "d"),
        Err(RommFail::TooOld)
    );
    let evil = INIT.replace(
        "\"/pair/device?user_code=K7QF-2MXP\"",
        "\"//evil.example/x\"",
    );
    let fake = Fake::new(&[("https://romm.home/api/auth/device/init", 201, &evil)]);
    assert_eq!(start_pairing(&fake, &server(None), "d"), Err(RommFail::Bad));
}

#[test]
fn pairing_polls_until_approved() {
    let start = Fake::new(&[("https://romm.home/api/auth/device/init", 201, INIT)]);
    let p = start_pairing(&start, &server(None), "d").unwrap();
    let token_url = "https://romm.home/api/auth/device/token";
    let answer = |status: u16, body: &str| {
        poll_pairing(&Fake::new(&[(token_url, status, body)]), &server(None), &p)
    };
    assert_eq!(
        answer(400, r#"{"detail":"authorization_pending"}"#),
        Ok(Poll::Waiting)
    );
    assert_eq!(answer(400, r#"{"detail":"slow_down"}"#), Ok(Poll::SlowDown));
    assert_eq!(answer(429, ""), Ok(Poll::SlowDown));
    assert_eq!(
        answer(400, r#"{"detail":"access_denied"}"#),
        Err(RommFail::Denied)
    );
    assert_eq!(
        answer(400, r#"{"detail":"expired_token"}"#),
        Err(RommFail::Expired)
    );
    assert_eq!(
        answer(200, r#"{"access_token":"rmm_new","expires_at":null}"#),
        Ok(Poll::Token("rmm_new".into()))
    );
    assert_eq!(
        poll_pairing(&Fake::new(&[]), &server(None), &p),
        Ok(Poll::Waiting)
    );
}

#[test]
fn card_stems_are_safe_for_the_card() {
    let g = |no_ext: &str, name: &str| Game {
        id: 9,
        fs_name_no_ext: no_ext.into(),
        name: name.into(),
        ..Game::default()
    };
    assert_eq!(
        card_stem(&g("Pokemon - Emerald (USA)", "x")),
        "Pokemon - Emerald (USA)"
    );
    assert_eq!(card_stem(&g("", "Zelda: Minish Cap?")), "Zelda Minish Cap");
    assert_eq!(card_stem(&g(" .hidden. ", "")), "hidden");
    assert_eq!(card_stem(&g("...", "")), "rom-9");
    assert_eq!(card_stem(&g("", "")), "rom-9");
    let long = "é".repeat(150);
    let stem = card_stem(&g(&long, ""));
    assert!(stem.len() <= 200 && stem.chars().all(|c| c == 'é'));
}

#[test]
fn names_match_without_tags_case_or_accents() {
    assert_eq!(
        normalise("Pokémon - Emerald (USA) [!].gba"),
        "pokemonemerald"
    );
    assert_eq!(normalise("Mario & Luigi"), "marioandluigi");
    assert!(same_game("Pokemon Emerald", "POKÉMON - EMERALD (Europe)"));
    assert!(!same_game("(USA)", "[!]"));
}

#[test]
fn search_needs_every_word() {
    assert!(matches("poke emer", "Pokémon - Emerald Version"));
    assert!(matches("", "anything"));
    assert!(!matches("poke ruby", "Pokémon - Emerald Version"));
}

#[test]
fn logins_survive_a_round_trip_and_can_be_forgotten() {
    let d = tempfile::tempdir().unwrap();
    assert_eq!(load(d.path()), Saved::default());
    let saved = Saved {
        url: Some("https://romm.home".into()),
        login: Some(Login::Password {
            user: "me".into(),
            pass: " p=w #1 ".into(),
        }),
        device: Some("slot-abc".into()),
        pin: Some([9; 32]),
    };
    save(d.path(), &saved).unwrap();
    assert_eq!(load(d.path()), saved);
    assert_eq!(load(d.path()).server().unwrap().base, "https://romm.home");
    assert_eq!(load(d.path()).server().unwrap().pin, Some([9; 32]));
    forget(d.path()).unwrap();
    assert_eq!(
        load(d.path()),
        Saved {
            login: None,
            ..saved.clone()
        }
    );
    save(
        d.path(),
        &Saved {
            login: Some(Login::Token("rmm_x".into())),
            ..saved
        },
    )
    .unwrap();
    assert_eq!(load(d.path()).login, Some(Login::Token("rmm_x".into())));
}

mod install {
    use super::*;
    use slot::romm::install::{install, label_from_photo, Order, Step};
    use slot::romm::label::{find_box, Found, Rgba};
    use std::io::Write;

    fn card() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("Games/GBA")).unwrap();
        d
    }

    fn fusion() -> Game {
        Game {
            id: 11,
            name: "Metroid Fusion".into(),
            fs_name: "Metroid Fusion (USA).zip".into(),
            fs_name_no_ext: "Metroid Fusion (USA)".into(),
            size: 100,
            ..Game::default()
        }
    }

    struct Bytes(Vec<u8>);

    impl Http for Bytes {
        fn call(&self, req: &Req) -> Result<Reply, RommFail> {
            match req.url.contains("/content/") {
                true => Ok(Reply {
                    status: 200,
                    length: Some(self.0.len() as u64),
                    body: Box::new(Cursor::new(self.0.clone())),
                }),
                false => Ok(Reply {
                    status: 404,
                    length: None,
                    body: Box::new(Cursor::new(Vec::new())),
                }),
            }
        }
    }

    fn zipped(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut out);
            for (name, bytes) in entries {
                z.start_file(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                z.write_all(bytes).unwrap();
            }
            z.finish().unwrap();
        }
        out.into_inner()
    }

    fn run(
        d: &tempfile::TempDir,
        http: &dyn Http,
        labelled: bool,
    ) -> (Result<slot::romm::install::Installed, RommFail>, Vec<Step>) {
        let mut steps = Vec::new();
        let server = server(Some(Login::Token("t".into())));
        let game = fusion();
        let order = Order {
            server: &server,
            root: d.path(),
            platform: Platform::Gba,
            game: &game,
        };
        let result = install(http, order, &|| false, &mut |s| steps.push(s), &|_| {
            labelled
        });
        (result, steps)
    }

    fn files(d: &tempfile::TempDir) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(d.path().join("Games/GBA"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn a_plain_rom_is_written_under_its_card_name() {
        let d = card();
        let (result, steps) = run(&d, &Bytes(b"plain rom".to_vec()), true);
        let done = result.unwrap();
        assert_eq!(done.stem, "Metroid Fusion (USA)");
        assert!(done.labelled);
        assert_eq!(files(&d), ["Metroid Fusion (USA).gba"]);
        assert_eq!(steps.last(), Some(&Step::Labelling));
        assert!(steps.contains(&Step::Downloading { got: 9, total: 9 }));
    }

    #[test]
    fn a_zip_gives_up_its_biggest_rom() {
        let d = card();
        let zip = zipped(&[
            ("readme.txt", &[1; 900]),
            ("small.gba", b"x"),
            ("dir/Fusion.gba", b"the real rom"),
        ]);
        let (result, _) = run(&d, &Bytes(zip), false);
        assert!(!result.unwrap().labelled);
        assert_eq!(files(&d), ["Metroid Fusion (USA).gba"]);
        assert_eq!(
            std::fs::read(d.path().join("Games/GBA/Metroid Fusion (USA).gba")).unwrap(),
            b"the real rom"
        );
    }

    #[test]
    fn archives_slot_cannot_open_leave_nothing_behind() {
        let d = card();
        let (result, _) = run(&d, &Bytes(b"7z\xBC\xAF\x27\x1Crest".to_vec()), false);
        assert_eq!(result, Err(RommFail::Archive));
        let (result, _) = run(&d, &Bytes(zipped(&[("notes.txt", b"hi")])), false);
        assert_eq!(result, Err(RommFail::Archive));
        assert!(files(&d).is_empty(), "{:?}", files(&d));
    }

    #[test]
    fn a_game_already_on_the_card_is_left_alone() {
        let d = card();
        std::fs::write(d.path().join("Games/GBA/metroid fusion (usa).gba"), b"mine").unwrap();
        let (result, _) = run(&d, &Bytes(b"new".to_vec()), false);
        assert_eq!(result, Err(RommFail::Exists));
        assert_eq!(
            std::fs::read(d.path().join("Games/GBA/metroid fusion (usa).gba")).unwrap(),
            b"mine"
        );
    }

    fn photo(w: usize, h: usize, cart: [usize; 4], label: [usize; 4]) -> Vec<u8> {
        let mut px = vec![255u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                let inside = |b: [usize; 4]| x >= b[0] && x < b[2] && y >= b[1] && y < b[3];
                let c = match (inside(label), inside(cart)) {
                    (true, _) => [200, 40, 40],
                    (false, true) => [90, 90, 100],
                    _ => [255, 255, 255],
                };
                px[i..i + 3].copy_from_slice(&c);
            }
        }
        px
    }

    #[test]
    fn the_label_is_found_on_a_gba_cartridge_photo() {
        let (w, h) = (400, 260);
        let cart = [20, 20, 380, 236];
        let (cw, ch) = (360.0, 216.0);
        let label = [
            20 + (cw * 0.142) as usize,
            20 + (ch * 0.228) as usize,
            20 + (cw * 0.868) as usize,
            20 + (ch * 0.863) as usize,
        ];
        let px = photo(w, h, cart, label);
        let (b, found) = find_box(Rgba { px: &px, w, h }, Platform::Gba);
        assert_eq!(found, Found::Template);
        for (got, want) in [
            (b.l, label[0]),
            (b.t, label[1]),
            (b.r, label[2]),
            (b.b, label[3]),
        ] {
            assert!((got - want as f32).abs() < 6.0, "{b:?} vs {label:?}");
        }
    }

    #[test]
    fn the_label_is_detected_on_a_game_boy_cartridge_photo() {
        let (w, h) = (300, 360);
        let cart = [30, 20, 270, 340];
        let label = [60, 110, 240, 270];
        let px = photo(w, h, cart, label);
        let (b, found) = find_box(Rgba { px: &px, w, h }, Platform::Gb);
        assert_eq!(found, Found::Detected);
        for (got, want) in [
            (b.l, label[0]),
            (b.t, label[1]),
            (b.r, label[2]),
            (b.b, label[3]),
        ] {
            assert!((got - want as f32).abs() < 8.0, "{b:?} vs {label:?}");
        }
    }

    #[test]
    fn a_cartridge_photo_becomes_a_label_the_size_slot_wants() {
        let (w, h) = (400, 260);
        let px = photo(w, h, [20, 20, 380, 236], [71, 69, 332, 206]);
        let mut png_bytes = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut png_bytes, w as u32, h as u32);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(&px).unwrap();
        }
        let label = label_from_photo(&png_bytes, Platform::Gba).expect("no label");
        let info = png::Decoder::new(Cursor::new(label)).read_info().unwrap();
        assert_eq!((info.info().width, info.info().height), (1280, 661));
    }
}

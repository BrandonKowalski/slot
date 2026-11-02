use std::collections::HashSet;

use slot_store::{Cart, Platform};

use super::api::{Game, Pairing};
use super::install::Step;
use super::jobs::Library;
use super::login::Saved;
use super::name::{card_stem, matches, normalise};
use super::tls::{pin_text, Pin};
use crate::networks::Line;

pub const LOADING_MESSAGE: &str = "Loading your RomM library";
pub const SIGNING_IN_MESSAGE: &str = "Signing in to RomM";
pub const EMPTY_MESSAGE: &str = "No games here";
pub const NO_MATCH_MESSAGE: &str = "No games match your search";
pub const CERTIFICATE_MESSAGE: &str = "Reading the RomM certificate";
pub const MESSAGES: [&str; 5] = [
    LOADING_MESSAGE,
    SIGNING_IN_MESSAGE,
    EMPTY_MESSAGE,
    NO_MATCH_MESSAGE,
    CERTIFICATE_MESSAGE,
];

pub const SERVER_TITLE: &str = "RomM Address";
pub const SERVER_HINT: &str = "For example romm.example.com";
pub const CODE_TITLE: &str = "Pair Code";
pub const CODE_HINT: &str = "RomM > API Tokens > Pair device";
pub const USER_TITLE: &str = "Username";
pub const PASSWORD_TITLE: &str = "RomM Password";
pub const SEARCH_TITLE: &str = "Search RomM";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    Home { row: usize },
    Games { platform: Platform, row: usize },
    Pairing,
    Installing,
    Certificate { pin: Pin },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeRow {
    Shelf(Platform),
    Search,
    Refresh,
    Server,
    Certificate,
    Remote,
    SignOut,
    SignInQr,
    SignInCode,
    SignInPassword,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Busy {
    SignIn,
    Library,
    Pairing,
    Install,
    Certificate,
}

pub const SHELVES: [Platform; 3] = [Platform::Gba, Platform::Gbc, Platform::Gb];

#[derive(Debug, Clone, Default)]
pub struct Romm {
    pub saved: Saved,
    pub library: Option<Library>,
    pub busy: Option<Busy>,
    pub pairing: Option<Pairing>,
    pub pairing_ends: u64,
    pub installing: Option<(String, Option<Step>)>,
    pub user: Option<String>,
    pub server_off: bool,
    query: String,
    shown: Vec<(Platform, Vec<usize>)>,
    card: HashSet<(Platform, String)>,
    card_names: HashSet<(Platform, String)>,
}

impl Romm {
    pub fn new(saved: Saved) -> Romm {
        Romm {
            saved,
            ..Romm::default()
        }
    }

    pub fn signed_in(&self) -> bool {
        self.saved.login.is_some() && self.saved.server().is_some()
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.refilter();
    }

    pub fn set_library(&mut self, library: Option<Library>) {
        self.library = library;
        self.refilter();
    }

    pub fn set_card(&mut self, carts: &[Cart]) {
        self.card = carts
            .iter()
            .map(|c| (c.platform, c.stem.to_lowercase()))
            .collect();
        self.card_names = carts
            .iter()
            .map(|c| (c.platform, normalise(&c.stem)))
            .collect();
    }

    fn refilter(&mut self) {
        let Some(lib) = &self.library else {
            self.shown.clear();
            return;
        };
        self.shown = SHELVES
            .iter()
            .map(|p| {
                let picked = lib
                    .games(*p)
                    .iter()
                    .enumerate()
                    .filter(|(_, g)| {
                        matches(&self.query, &format!("{} {}", g.title(), g.fs_name_no_ext))
                    })
                    .map(|(i, _)| i)
                    .collect();
                (*p, picked)
            })
            .collect();
    }

    pub fn shown(&self, platform: Platform) -> &[usize] {
        self.shown
            .iter()
            .find(|(p, _)| *p == platform)
            .map_or(&[], |(_, v)| v.as_slice())
    }

    pub fn game(&self, platform: Platform, row: usize) -> Option<&Game> {
        let i = *self.shown(platform).get(row)?;
        self.library.as_ref()?.games(platform).get(i)
    }

    pub fn on_card(&self, platform: Platform, game: &Game) -> bool {
        if self
            .card
            .contains(&(platform, card_stem(game).to_lowercase()))
        {
            return true;
        }
        [&game.fs_name_no_tags, &game.fs_name_no_ext, &game.name]
            .iter()
            .map(|n| normalise(n))
            .any(|n| !n.is_empty() && self.card_names.contains(&(platform, n)))
    }

    pub fn home_rows(&self) -> Vec<HomeRow> {
        let mut rows = self.account_rows();
        if self.saved.server().is_some_and(|s| s.secure()) {
            let at = rows
                .iter()
                .position(|r| *r == HomeRow::Server)
                .map_or(0, |i| i + 1);
            rows.insert(at, HomeRow::Certificate);
        }
        rows
    }

    fn account_rows(&self) -> Vec<HomeRow> {
        match self.signed_in() {
            true => SHELVES
                .iter()
                .map(|p| HomeRow::Shelf(*p))
                .chain([
                    HomeRow::Search,
                    HomeRow::Refresh,
                    HomeRow::Remote,
                    HomeRow::Server,
                    HomeRow::SignOut,
                ])
                .collect(),
            false => vec![
                HomeRow::Server,
                HomeRow::SignInQr,
                HomeRow::SignInCode,
                HomeRow::SignInPassword,
            ],
        }
    }

    pub fn host(&self) -> Option<String> {
        let url = self.saved.url.as_deref()?;
        Some(url.split_once("://").map_or(url, |(_, h)| h).to_string())
    }

    fn home_line(&self, row: HomeRow) -> Line {
        let count = |p: Platform| {
            let total = self.library.as_ref()?.games(p).len();
            let shown = self.shown(p).len();
            Some(match self.query.is_empty() {
                true => format!("{total}"),
                false => format!("{shown} of {total}"),
            })
        };
        match row {
            HomeRow::Shelf(p) => Line::plain(p.name(), count(p)),
            HomeRow::Search => Line::plain(
                "Search",
                Some(match self.query.is_empty() {
                    true => "All Games".into(),
                    false => self.query.clone(),
                }),
            ),
            HomeRow::Refresh => Line::plain("Refresh Library", None),
            HomeRow::Server => Line::plain(
                "Server",
                Some(self.host().unwrap_or_else(|| "Not Set".into())),
            ),
            HomeRow::Certificate => Line::plain(
                "Certificate",
                Some(match self.saved.pin {
                    Some(_) => "Pinned".into(),
                    None => "Checked".into(),
                }),
            ),
            HomeRow::Remote => Line::plain(
                "Remote Install",
                Some(
                    match (self.saved.can_remote(), self.saved.remote, self.server_off) {
                        (false, _, _) => "Needs QR Sign-In",
                        (true, _, true) => "Off on Server",
                        (true, true, false) => "On",
                        (true, false, false) => "Off",
                    }
                    .into(),
                ),
            ),
            HomeRow::SignOut => Line::plain("Sign Out", None),
            HomeRow::SignInQr => Line::plain("Sign In with QR Code", None),
            HomeRow::SignInCode => Line::plain("Sign In with Pair Code", None),
            HomeRow::SignInPassword => Line::plain("Sign In with Password", None),
        }
    }

    pub fn home_message(&self) -> Option<&'static str> {
        match self.busy {
            Some(Busy::SignIn) => Some(SIGNING_IN_MESSAGE),
            Some(Busy::Library) => Some(LOADING_MESSAGE),
            Some(Busy::Certificate) => Some(CERTIFICATE_MESSAGE),
            _ => None,
        }
    }

    pub fn home_lines(&self) -> Vec<Line> {
        self.home_rows()
            .into_iter()
            .map(|r| self.home_line(r))
            .collect()
    }

    pub fn game_line(&self, platform: Platform, game: &Game) -> Line {
        let value = match self.on_card(platform, game) {
            true => "On Card".to_string(),
            false => size_text(game.size),
        };
        Line::plain(game.title(), Some(value))
    }

    pub fn games_message(&self, platform: Platform) -> Option<&'static str> {
        match (self.shown(platform).is_empty(), self.query.is_empty()) {
            (false, _) => None,
            (true, true) => Some(EMPTY_MESSAGE),
            (true, false) => Some(NO_MATCH_MESSAGE),
        }
    }

    pub fn pairing_lines(&self, now_s: u64) -> Vec<Line> {
        let Some(p) = &self.pairing else {
            return Vec::new();
        };
        let left = self.pairing_ends.saturating_sub(now_s);
        vec![
            Line::plain("Scan with your phone, or open", None),
            Line::plain(&strip_query(&p.link), None),
            Line::plain("Code", Some(p.user_code.clone())),
            Line::plain("Time Left", Some(format!("{}:{:02}", left / 60, left % 60))),
        ]
    }

    pub fn installing_lines(&self) -> Vec<Line> {
        let Some((title, step)) = &self.installing else {
            return Vec::new();
        };
        let status = match step {
            None => Line::plain("Starting Download", None),
            Some(Step::Downloading { got, total }) if *total > 0 => Line::plain(
                "Downloading",
                Some(format!("{}%", (got * 100 / total).min(100))),
            ),
            Some(Step::Downloading { got, .. }) => {
                Line::plain("Downloading", Some(size_text(*got)))
            }
            Some(Step::Labelling) => Line::plain("Making Label", None),
        };
        vec![Line::plain(title, None), status]
    }
}

pub fn certificate_lines(pin: &Pin) -> Vec<Line> {
    let text = pin_text(pin);
    let parts: Vec<&str> = text.split(':').collect();
    std::iter::once(Line::plain("Trust this certificate?", None))
        .chain(std::iter::once(Line::plain(
            "Check its SHA-256 on your server",
            None,
        )))
        .chain(parts.chunks(8).map(|c| Line::plain(&c.join(":"), None)))
        .collect()
}

pub fn size_text(bytes: u64) -> String {
    const MB: u64 = 1024 * 1024;
    match bytes {
        0 => String::new(),
        b if b >= MB => format!("{:.1} MB", b as f64 / MB as f64),
        b => format!("{} KB", b.div_ceil(1024)),
    }
}

fn strip_query(link: &str) -> String {
    let base = link.split('?').next().unwrap_or(link);
    base.split_once("://")
        .map_or(base, |(_, rest)| rest)
        .to_string()
}

use slot_gfx::Draw;
use slot_input::{Action, Btn};
use slot_store::{Cart, Platform};
use slot_ui::QuickRow;

use super::{App, KeyboardFor, NetNotice, Phase, Rack, UpdateState};
use crate::networks::NetScreen;
use crate::romm::jobs::{Event, Job};
use crate::romm::login::{self, Saved};
use crate::romm::name::{card_stem, normalise};
use crate::romm::screen::{
    Busy, HomeRow, Romm, View, CODE_HINT, CODE_TITLE, PASSWORD_TITLE, SEARCH_TITLE, SERVER_HINT,
    SERVER_TITLE, SHELVES, USER_TITLE,
};
use crate::romm::{normalize_url, Login, RommFail};

const PAGE: usize = slot_ui::QUICK_VISIBLE;

pub fn qr_modules(text: &str) -> Option<Vec<Vec<bool>>> {
    let code =
        qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::M).ok()?;
    let width = code.width();
    Some(
        code.to_colors()
            .chunks(width)
            .map(|row| row.iter().map(|c| *c == qrcode::Color::Dark).collect())
            .collect(),
    )
}

fn step(row: usize, n: usize, btn: Btn) -> usize {
    match (btn, n) {
        (_, 0) => 0,
        (Btn::Up, _) => (row + n - 1) % n,
        (Btn::Down, _) => (row + 1) % n,
        (Btn::Left, _) => row.saturating_sub(PAGE),
        (Btn::Right, _) => (row + PAGE).min(n - 1),
        _ => row,
    }
}

impl App {
    pub fn romm(&self) -> &Romm {
        &self.romm
    }

    pub fn set_romm_jobs(&mut self, jobs: Box<dyn crate::romm::jobs::RommJobs>) {
        self.romm_jobs = jobs;
    }

    pub(super) fn open_romm(&mut self) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let saved = login::load(&root);
        if self.romm.saved != saved {
            self.romm = Romm::new(saved);
        }
        self.romm_card_changed();
        self.romm_return = None;
        self.phase = Phase::Romm {
            view: View::Home { row: 0 },
        };
        self.hold_wifi();
        if self.romm.signed_in() && self.romm.library.is_none() && self.romm.busy.is_none() {
            self.romm_load(Busy::Library);
        }
    }

    fn romm_card_changed(&mut self) {
        let carts: Vec<Cart> = self
            .shelves
            .iter()
            .filter(|(rack, _)| rack.platform().is_some())
            .flat_map(|(_, shelf)| shelf.carts.iter().cloned())
            .collect();
        self.romm.set_card(&carts);
    }

    fn romm_load(&mut self, busy: Busy) {
        let Some(server) = self.romm.saved.server() else {
            return;
        };
        self.romm.busy = Some(busy);
        self.hold_wifi();
        self.romm_jobs.ask(Job::Library(server));
    }

    fn romm_save(&mut self) {
        if let Some(root) = self.root.clone() {
            if let Err(e) = login::save(&root, &self.romm.saved) {
                eprintln!("slot: romm: could not save the login: {e}");
            }
        }
    }

    fn romm_notice(&mut self, fail: RommFail) {
        if fail != RommFail::Cancelled {
            self.set_update(Some(UpdateState::Notice(NetNotice::Romm(fail))));
        }
    }

    fn romm_keyboard(&mut self, title: &str, hint: Option<&str>, text: &str, task: KeyboardFor) {
        let kb = slot_ui::Keyboard::new(slot_ui::Layout::General, title, hint, text);
        self.keyboard = Some((kb, task));
    }

    fn romm_home_row(&self, want: HomeRow) -> usize {
        self.romm
            .home_rows()
            .iter()
            .position(|r| *r == want)
            .unwrap_or(0)
    }

    pub(super) fn romm_input(&mut self, action: Action) {
        let Phase::Romm { view } = &self.phase else {
            return;
        };
        let view = view.clone();
        let btn = match action {
            Action::GbaDown(b) => b,
            Action::QuickMenu => Btn::Menu,
            _ => return,
        };
        match view {
            View::Home { row } => self.romm_home_input(row, btn),
            View::Games { platform, row } => self.romm_games_input(platform, row, btn),
            View::Pairing if matches!(btn, Btn::B | Btn::Menu) => {
                self.romm_jobs.cancel();
                self.romm.pairing = None;
                self.romm.busy = None;
                let row = self.romm_home_row(HomeRow::SignInQr);
                self.phase = Phase::Romm {
                    view: View::Home { row },
                };
            }
            View::Installing if btn == Btn::B => self.romm_jobs.cancel(),
            View::Certificate { pin } if btn == Btn::A => self.romm_trust(pin),
            View::Certificate { .. } if matches!(btn, Btn::B | Btn::Menu) => {
                let row = self.romm_home_row(HomeRow::Certificate);
                self.phase = Phase::Romm {
                    view: View::Home { row },
                };
            }
            View::Pairing | View::Installing | View::Certificate { .. } => {}
        }
    }

    fn romm_home_input(&mut self, row: usize, btn: Btn) {
        let rows = self.romm.home_rows();
        let at = row.min(rows.len().saturating_sub(1));
        match btn {
            Btn::Up | Btn::Down => {
                let row = step(at, rows.len(), btn);
                self.phase = Phase::Romm {
                    view: View::Home { row },
                };
            }
            Btn::B | Btn::Menu => {
                self.phase = Phase::QuickMenu {
                    row: QuickRow::Romm,
                }
            }
            Btn::A => {
                if let Some(row) = rows.get(at) {
                    self.romm_home_pick(*row);
                }
            }
            _ => {}
        }
    }

    fn romm_home_pick(&mut self, row: HomeRow) {
        let signing_in = matches!(self.romm.busy, Some(Busy::SignIn | Busy::Pairing));
        match row {
            HomeRow::Shelf(platform) => match self.romm.library.is_some() {
                true => {
                    self.phase = Phase::Romm {
                        view: View::Games { platform, row: 0 },
                    }
                }
                false if self.romm.busy.is_none() => self.romm_load(Busy::Library),
                false => {}
            },
            HomeRow::Search => {
                let query = self.romm.query().to_string();
                self.romm_keyboard(SEARCH_TITLE, None, &query, KeyboardFor::RommSearch);
            }
            HomeRow::Refresh if self.romm.busy.is_none() => self.romm_load(Busy::Library),
            HomeRow::Refresh => {}
            HomeRow::Server => {
                let url = self.romm.saved.url.clone().unwrap_or_default();
                self.romm_keyboard(
                    SERVER_TITLE,
                    Some(SERVER_HINT),
                    &url,
                    KeyboardFor::RommServer,
                );
            }
            HomeRow::SignOut => {
                self.romm_jobs.cancel();
                if let Some(root) = self.root.clone() {
                    let _ = login::forget(&root);
                }
                self.romm = Romm::new(Saved {
                    login: None,
                    ..self.romm.saved.clone()
                });
                self.romm_card_changed();
                self.phase = Phase::Romm {
                    view: View::Home { row: 0 },
                };
            }
            HomeRow::Certificate if self.romm.saved.pin.is_some() => {
                self.romm_jobs.cancel();
                self.romm.busy = None;
                self.romm.saved.pin = None;
                self.romm_save();
            }
            HomeRow::Certificate if self.romm.busy.is_none() => {
                let Some(server) = self.romm.saved.server() else {
                    return;
                };
                self.romm.busy = Some(Busy::Certificate);
                self.hold_wifi();
                self.romm_jobs.ask(Job::Certificate(server));
            }
            HomeRow::Certificate => {}
            _ if signing_in => {}
            _ if self.romm.saved.server().is_none() => {
                self.romm_keyboard(SERVER_TITLE, Some(SERVER_HINT), "", KeyboardFor::RommServer)
            }
            HomeRow::SignInQr => self.romm_pair(),
            HomeRow::SignInCode => {
                self.romm_keyboard(CODE_TITLE, Some(CODE_HINT), "", KeyboardFor::RommCode)
            }
            HomeRow::SignInPassword => {
                self.romm_keyboard(USER_TITLE, None, "", KeyboardFor::RommUser)
            }
        }
    }

    fn romm_pair(&mut self) {
        let Some(server) = self.romm.saved.server() else {
            return;
        };
        let device = match self.romm.saved.device.clone() {
            Some(d) => d,
            None => {
                let seed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(1, |d| d.as_nanos() as u64)
                    ^ std::process::id() as u64;
                let d = login::new_device_id(seed);
                self.romm.saved.device = Some(d.clone());
                self.romm_save();
                d
            }
        };
        self.romm.busy = Some(Busy::Pairing);
        self.romm.pairing = None;
        self.romm_qr = None;
        self.hold_wifi();
        self.romm_jobs.ask(Job::Pair(server, device));
        self.phase = Phase::Romm {
            view: View::Pairing,
        };
    }

    fn romm_games_input(&mut self, platform: Platform, row: usize, btn: Btn) {
        let n = self.romm.shown(platform).len();
        let at = row.min(n.saturating_sub(1));
        let shelf_at = SHELVES.iter().position(|p| *p == platform).unwrap_or(0);
        let view = match btn {
            Btn::Up | Btn::Down | Btn::Left | Btn::Right => View::Games {
                platform,
                row: step(at, n, btn),
            },
            Btn::L1 | Btn::R1 => {
                let to = match btn {
                    Btn::L1 => (shelf_at + SHELVES.len() - 1) % SHELVES.len(),
                    _ => (shelf_at + 1) % SHELVES.len(),
                };
                View::Games {
                    platform: SHELVES[to],
                    row: 0,
                }
            }
            Btn::B | Btn::Menu => View::Home {
                row: self.romm_home_row(HomeRow::Shelf(platform)),
            },
            Btn::X => {
                let query = self.romm.query().to_string();
                return self.romm_keyboard(SEARCH_TITLE, None, &query, KeyboardFor::RommSearch);
            }
            Btn::A => return self.romm_pick_game(platform, at),
            _ => return,
        };
        self.phase = Phase::Romm { view };
    }

    fn romm_card_index(
        &self,
        platform: Platform,
        game: &crate::romm::api::Game,
    ) -> Option<(usize, usize)> {
        let at = self
            .shelves
            .iter()
            .position(|(r, _)| *r == Rack::Of(platform))?;
        let stem = card_stem(game).to_lowercase();
        let names: Vec<String> = [&game.fs_name_no_tags, &game.fs_name_no_ext, &game.name]
            .iter()
            .map(|n| normalise(n))
            .filter(|n| !n.is_empty())
            .collect();
        let carts = &self.shelves[at].1.carts;
        let i = carts
            .iter()
            .position(|c| c.stem.to_lowercase() == stem)
            .or_else(|| {
                carts
                    .iter()
                    .position(|c| names.contains(&normalise(&c.stem)))
            })?;
        Some((at, i))
    }

    fn romm_pick_game(&mut self, platform: Platform, row: usize) {
        let Some(game) = self.romm.game(platform, row).cloned() else {
            return;
        };
        self.romm_return = Some(View::Games { platform, row });
        if let Some((at, i)) = self.romm_card_index(platform, &game) {
            return self.romm_open_studio(at, i);
        }
        let (Some(server), Some(root)) = (self.romm.saved.server(), self.root.clone()) else {
            return;
        };
        self.romm.busy = Some(Busy::Install);
        self.romm.installing = Some((game.title().to_string(), None));
        self.hold_wifi();
        self.romm_jobs.ask(Job::Install {
            server,
            root,
            platform,
            game,
        });
        self.phase = Phase::Romm {
            view: View::Installing,
        };
    }

    fn romm_open_studio(&mut self, at: usize, i: usize) {
        self.shelf_at = at;
        self.shelves[at].1.select(i);
        self.shelf_platform = slot_ui::Printed::default();
        self.slot_letter = None;
        self.shelf_named = None;
        self.name_pending = true;
        self.phase = Phase::Shelf;
        self.open_studio();
        if self.studio.is_none() {
            self.romm_back_from_studio();
        }
    }

    fn romm_trust(&mut self, pin: crate::romm::tls::Pin) {
        self.romm.saved.pin = Some(pin);
        self.romm_save();
        let row = self.romm_home_row(HomeRow::Certificate);
        self.phase = Phase::Romm {
            view: View::Home { row },
        };
        if self.romm.signed_in() && self.romm.library.is_none() && self.romm.busy.is_none() {
            self.romm_load(Busy::Library);
        }
    }

    pub(super) fn romm_back_from_studio(&mut self) {
        if self.powering_off {
            self.romm_return = None;
            return;
        }
        if let Some(view) = self.romm_return.take() {
            self.phase = Phase::Romm { view };
        }
    }

    pub(super) fn romm_keyboard_done(&mut self, task: KeyboardFor, text: String) {
        self.keyboard = None;
        match task {
            KeyboardFor::RommSearch => {
                self.romm.set_query(text.trim().to_string());
                if let Phase::Romm {
                    view: View::Games { platform, .. },
                } = self.phase
                {
                    self.phase = Phase::Romm {
                        view: View::Games { platform, row: 0 },
                    };
                }
            }
            KeyboardFor::RommServer => {
                if text.trim().is_empty() {
                    return;
                }
                let Some(url) = normalize_url(&text) else {
                    return self.romm_notice(RommFail::Address);
                };
                if self.romm.saved.url.as_deref() != Some(url.as_str()) {
                    self.romm_jobs.cancel();
                    self.romm = Romm::new(Saved {
                        url: Some(url),
                        login: None,
                        pin: None,
                        ..self.romm.saved.clone()
                    });
                    self.romm_card_changed();
                    self.romm_save();
                }
            }
            KeyboardFor::RommCode => {
                let Some(server) = self.romm.saved.server() else {
                    return;
                };
                if text.trim().is_empty() {
                    return;
                }
                self.romm.busy = Some(Busy::SignIn);
                self.hold_wifi();
                self.romm_jobs.ask(Job::Exchange(server, text));
            }
            KeyboardFor::RommUser => {
                if text.trim().is_empty() {
                    return;
                }
                self.romm.user = Some(text.trim().to_string());
                self.romm_keyboard(PASSWORD_TITLE, None, "", KeyboardFor::RommPassword);
            }
            KeyboardFor::RommPassword => {
                let Some(user) = self.romm.user.take() else {
                    return;
                };
                self.romm.saved.login = Some(Login::Password { user, pass: text });
                self.romm_load(Busy::SignIn);
            }
            KeyboardFor::Password { .. } | KeyboardFor::NetworkName => {}
        }
    }

    pub(super) fn take_romm_events(&mut self) {
        while let Some(event) = self.romm_jobs.take() {
            self.romm_event(event);
        }
    }

    fn romm_event(&mut self, event: Event) {
        let on =
            |a: &App, v: fn(&View) -> bool| matches!(&a.phase, Phase::Romm { view } if v(view));
        match event {
            Event::Library(Ok(_)) if self.romm.saved.login.is_none() => self.romm.busy = None,
            Event::Library(Ok(lib)) => {
                if self.romm.busy == Some(Busy::SignIn) {
                    self.romm_save();
                }
                self.romm.busy = None;
                self.romm.set_library(Some(lib));
                if on(self, |v| matches!(v, View::Home { .. })) {
                    self.phase = Phase::Romm {
                        view: View::Home { row: 0 },
                    };
                }
            }
            Event::Library(Err(e)) => {
                if self.romm.busy == Some(Busy::SignIn) {
                    self.romm.saved.login = None;
                }
                self.romm.busy = None;
                if matches!(self.phase, Phase::Romm { .. }) {
                    self.romm_notice(e);
                }
            }
            Event::Token(Ok(login)) => {
                self.romm.pairing = None;
                self.romm.saved.login = Some(login);
                self.romm_save();
                if on(self, |v| *v == View::Pairing) {
                    self.phase = Phase::Romm {
                        view: View::Home { row: 0 },
                    };
                }
                self.romm_load(Busy::Library);
            }
            Event::Token(Err(e)) => {
                self.romm.pairing = None;
                self.romm.busy = None;
                if on(self, |v| *v == View::Pairing) {
                    let row = self.romm_home_row(HomeRow::SignInQr);
                    self.phase = Phase::Romm {
                        view: View::Home { row },
                    };
                }
                if matches!(self.phase, Phase::Romm { .. }) {
                    self.romm_notice(e);
                }
            }
            Event::Certificate(Ok(pin)) => {
                self.romm.busy = None;
                if on(self, |v| matches!(v, View::Home { .. })) {
                    self.phase = Phase::Romm {
                        view: View::Certificate { pin },
                    };
                }
            }
            Event::Certificate(Err(e)) => {
                self.romm.busy = None;
                if matches!(self.phase, Phase::Romm { .. }) {
                    self.romm_notice(e);
                }
            }
            Event::Pairing(p) => {
                self.romm_qr = qr_modules(&p.link);
                self.romm.pairing_ends = self.now() / 1000 + p.expires_in.as_secs();
                self.romm.pairing = Some(p);
            }
            Event::Step(s) => {
                if let Some((_, step)) = &mut self.romm.installing {
                    *step = Some(s);
                }
            }
            Event::Installed(result) => {
                self.romm.busy = None;
                self.romm.installing = None;
                let here = on(self, |v| *v == View::Installing);
                match result {
                    Ok(done) => self.romm_installed(done, here),
                    Err(e) => {
                        if here {
                            self.romm_back_from_studio();
                        }
                        self.romm_notice(e);
                    }
                }
            }
        }
    }

    fn romm_installed(&mut self, done: crate::romm::install::Installed, here: bool) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let carts = slot_store::scan(&root).unwrap_or_default();
        self.adopt_rescan(carts);
        self.romm_card_changed();
        let found = self
            .shelves
            .iter()
            .position(|(r, _)| *r == Rack::Of(done.platform))
            .and_then(|at| {
                let i = self.shelves[at]
                    .1
                    .carts
                    .iter()
                    .position(|c| c.stem == done.stem)?;
                Some((at, i))
            });
        match (here, found) {
            (true, Some((at, i))) => self.romm_open_studio(at, i),
            (true, None) => self.romm_back_from_studio(),
            (false, _) => {}
        }
    }

    pub(super) fn romm_screen(&self) -> Option<NetScreen> {
        let Phase::Romm { view } = &self.phase else {
            return None;
        };
        let plain = |lines: Vec<crate::networks::Line>, selected: usize, message| NetScreen {
            selected: selected.min(lines.len().saturating_sub(1)),
            lines,
            message,
            first: 0,
            total: None,
        };
        Some(match view {
            View::Home { row } => plain(self.romm.home_lines(), *row, self.romm.home_message()),
            View::Pairing => plain(self.romm.pairing_lines(self.now() / 1000), usize::MAX, None),
            View::Installing => plain(self.romm.installing_lines(), 0, None),
            View::Certificate { pin } => NetScreen {
                lines: crate::romm::screen::certificate_lines(pin),
                selected: usize::MAX,
                message: None,
                first: 0,
                total: None,
            },
            View::Games { platform, row } => {
                let n = self.romm.shown(*platform).len();
                if n == 0 {
                    return Some(plain(Vec::new(), 0, self.romm.games_message(*platform)));
                }
                let selected = (*row).min(n - 1);
                let (first, len) = slot_ui::net_window(n, selected);
                let lines = (first..first + len)
                    .filter_map(|i| {
                        self.romm
                            .game(*platform, i)
                            .map(|g| self.romm.game_line(*platform, g))
                    })
                    .collect();
                NetScreen {
                    lines,
                    selected,
                    message: None,
                    first,
                    total: Some(n),
                }
            }
        })
    }

    pub(super) fn romm_action(&self) -> Option<&'static str> {
        let Phase::Romm { view } = &self.phase else {
            return None;
        };
        match view {
            View::Home { row } => {
                let rows = self.romm.home_rows();
                Some(match rows.get((*row).min(rows.len().saturating_sub(1)))? {
                    HomeRow::Shelf(_) => "Open",
                    HomeRow::Search | HomeRow::Server => "Change",
                    HomeRow::Certificate if self.romm.saved.pin.is_some() => "Forget",
                    HomeRow::Certificate => "Open",
                    HomeRow::Refresh => "Refresh",
                    HomeRow::SignOut => "Sign Out",
                    HomeRow::SignInQr | HomeRow::SignInCode | HomeRow::SignInPassword => "Connect",
                })
            }
            View::Games { platform, row } => {
                let game = self.romm.game(*platform, *row)?;
                Some(match self.romm.on_card(*platform, game) {
                    true => "Open",
                    false => "Download",
                })
            }
            View::Certificate { .. } => Some("Trust"),
            View::Pairing | View::Installing => None,
        }
    }

    pub(super) fn romm_back(&self) -> Option<&'static str> {
        match &self.phase {
            Phase::Romm {
                view: View::Pairing | View::Installing | View::Certificate { .. },
            } => Some("Cancel"),
            Phase::Romm { .. } => Some("Back"),
            _ => None,
        }
    }

    pub(super) fn romm_text(fail: RommFail) -> (String, String) {
        let (title, detail) = match fail {
            RommFail::Offline => ("Could not reach RomM", "Check the address and the network."),
            RommFail::Clock => (
                "Could not connect securely",
                "Set the date and time, then try again.",
            ),
            RommFail::Refused => ("RomM refused the login", "Sign in again."),
            RommFail::Scopes => (
                "RomM refused access",
                "The login needs platforms.read and roms.read.",
            ),
            RommFail::NotFound => ("RomM could not find it", "Try a rescan in RomM."),
            RommFail::TooOld => (
                "RomM is too old for QR sign-in",
                "Use a pair code or a password.",
            ),
            RommFail::TooMany => ("Too many tries", "Wait a minute, then try again."),
            RommFail::Expired => (
                "The code has expired",
                "Make a new one in RomM and try again.",
            ),
            RommFail::Denied => ("Sign-in was turned down", ""),
            RommFail::Bad => ("RomM sent something unexpected", "Nothing was changed."),
            RommFail::Http(_) => ("RomM answered with an error", "Nothing was changed."),
            RommFail::Exists => ("Already on the card", ""),
            RommFail::Space => (
                "Not enough space on the card",
                "Free some space, then try again.",
            ),
            RommFail::Archive => (
                "This archive can't be opened",
                "slot only unpacks zip files.",
            ),
            RommFail::Write => ("Could not write to the card", "Nothing was changed."),
            RommFail::Address => (
                "That address doesn't look right",
                "Type it like romm.example.com",
            ),
            RommFail::Cancelled => ("Cancelled", ""),
            RommFail::Certificate => (
                "RomM's certificate isn't trusted",
                "If it is self-signed, open Certificate to trust it.",
            ),
        };
        let detail = match fail {
            RommFail::Http(code) => format!("HTTP {code}. {detail}"),
            _ => detail.to_string(),
        };
        (title.to_string(), detail)
    }

    pub(super) fn draw_romm_pairing(&self, out: &mut Vec<Draw>) {
        let canvas = self.canvas();
        slot_ui::draw_menu_ground(canvas, self.menu_ground(), out);
        let lines = &self.net_lines;
        let text_h: f32 = lines.iter().map(|l| l.label[0].2 as f32).sum();
        let side = match &self.romm_qr {
            Some(m) => ((canvas.h * 0.45) / (m.len() + 4) as f32).floor() * (m.len() + 4) as f32,
            None => 0.0,
        };
        let mut y = ((canvas.h - side - text_h - 24.0) / 2.0).max(8.0).floor();
        if let Some(m) = &self.romm_qr {
            let cell = side / (m.len() + 4) as f32;
            let x0 = ((canvas.w - side) / 2.0).floor();
            out.push(Draw::Rect {
                x: x0,
                y,
                w: side,
                h: side,
                colour: [1.0, 1.0, 1.0, 1.0],
            });
            for (r, row) in m.iter().enumerate() {
                for (c, dark) in row.iter().enumerate() {
                    if *dark {
                        out.push(Draw::Rect {
                            x: x0 + cell * (c + 2) as f32,
                            y: y + cell * (r + 2) as f32,
                            w: cell,
                            h: cell,
                            colour: [0.0, 0.0, 0.0, 1.0],
                        });
                    }
                }
            }
            y += side + 24.0;
        }
        for line in lines {
            let faces: Vec<slot_ui::NetFace> = std::iter::once(line.label[0])
                .chain(line.value.map(|v| v[1]))
                .collect();
            let gap = 16.0;
            let w: f32 =
                faces.iter().map(|f| f.1 as f32).sum::<f32>() + gap * (faces.len() - 1) as f32;
            let mut x = ((canvas.w - w) / 2.0).floor();
            for (tex, fw, fh) in faces {
                out.push(Draw::Tex {
                    x,
                    y,
                    w: fw as f32,
                    h: fh as f32,
                    tex,
                    alpha: 1.0,
                });
                x += fw as f32 + gap;
            }
            y += line.label[0].2 as f32;
        }
    }
}

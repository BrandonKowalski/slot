//! Controller-friendly Wi-Fi screen. Passwords are masked and never part of a render cache key.
use crate::wifi::{self, Network, Request, Security, Service};
use slot_input::Btn;
use slot_ui::UndoFace;
use std::path::Path;
use std::time::{Duration, Instant};

const KEYS: [&str; 3] = [
    "abcdefghijklmnopqrstuvwxyz0123456789-_. ",
    "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_. ",
    "!@#$%^&*()[]{}<>?/\\|;:'\",+=`~_- .0123456",
];

pub struct WifiMenu {
    service: Option<Service>,
    pub networks: Vec<Network>,
    pub status: String,
    pub row: usize,
    pub busy: bool,
    editing: Option<Network>,
    password: String,
    key: usize,
    page: usize,
    revision: u64,
    checked: Instant,
}

impl Default for WifiMenu {
    fn default() -> Self {
        Self {
            service: None,
            networks: Vec::new(),
            status: "Wi-Fi requires BaseOS on the handheld".into(),
            row: 0,
            busy: false,
            editing: None,
            password: String::new(),
            key: 0,
            page: 0,
            revision: 1,
            checked: Instant::now(),
        }
    }
}

impl WifiMenu {
    pub fn boot(&mut self, root: &Path) {
        self.service = Service::start(root.to_path_buf());
        if self.service.is_some() {
            self.status = "Select Scan networks to get started".into();
            if wifi::auto_connect(root) {
                self.request(Request::Reconnect, "Connecting to saved network...");
            }
        }
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn open(&mut self) {
        self.row = 0;
        self.editing = None;
        self.password.clear();
        if !self.busy {
            self.request(Request::Scan, "Scanning for networks...");
        }
        self.revision += 1;
    }
    fn request(&mut self, request: Request, status: &str) {
        if let Some(service) = &self.service {
            if service.send(request) {
                self.busy = true;
                self.status = status.into();
            } else {
                self.status = "Wi-Fi worker stopped; restart Slot".into();
            }
        }
        self.revision += 1;
    }
    pub fn poll(&mut self, visible: bool) {
        if let Some(result) = self.service.as_ref().and_then(Service::poll) {
            self.busy = false;
            match result {
                Ok(snapshot) => {
                    self.status = snapshot.status;
                    if let Some(networks) = snapshot.networks {
                        self.networks = networks;
                    }
                    self.row = self.row.min(self.networks.len() + 3);
                }
                Err(error) => self.status = error,
            }
            self.revision += 1;
            self.checked = Instant::now();
        }
        if visible
            && !self.busy
            && self.editing.is_none()
            && self.status.starts_with("Connected")
            && self.checked.elapsed() > Duration::from_secs(10)
        {
            let status = self.status.clone();
            self.request(Request::Status, &status);
            self.checked = Instant::now();
        }
    }
    /// True means return to settings. A worker may finish while this screen is closed.
    pub fn input(&mut self, button: Btn) -> bool {
        self.revision += 1;
        if button == Btn::B {
            if self.editing.take().is_some() {
                self.password.clear();
                return false;
            }
            return true;
        }
        if self.busy {
            return false;
        }
        if let Some(network) = self.editing.clone() {
            match button {
                Btn::Up => self.key = self.key.saturating_sub(10),
                Btn::Down => self.key = (self.key + 10).min(39),
                Btn::Left => self.key = self.key.saturating_sub(1),
                Btn::Right => self.key = (self.key + 1).min(39),
                Btn::Y => self.page = (self.page + 1) % KEYS.len(),
                Btn::X => {
                    self.password.pop();
                }
                Btn::A if self.password.len() < 64 => self
                    .password
                    .push(KEYS[self.page].as_bytes()[self.key] as char),
                Btn::Start => {
                    if let Err(error) = wifi::validate(&network, &self.password) {
                        self.status = error;
                    } else {
                        let password = std::mem::take(&mut self.password);
                        self.editing = None;
                        self.request(
                            Request::Connect(network, password),
                            "Connecting... (up to 45 seconds)",
                        );
                    }
                }
                _ => {}
            }
            return false;
        }
        match button {
            Btn::Up => self.row = self.row.saturating_sub(1),
            Btn::Down => self.row = (self.row + 1).min(self.networks.len() + 3),
            Btn::A => match self.row {
                0 => self.request(Request::Scan, "Scanning for networks..."),
                1 => self.request(Request::Reconnect, "Connecting to saved network..."),
                2 => self.request(Request::Disconnect, "Disconnecting..."),
                3 => self.request(Request::Forget, "Forgetting saved network..."),
                index => {
                    if let Some(network) = self.networks.get(index - 4).cloned() {
                        match network.security {
                            Security::Open => self
                                .request(Request::Connect(network, String::new()), "Connecting..."),
                            Security::Unsupported => {
                                self.status = "This network's security is not supported".into()
                            }
                            Security::Personal => {
                                self.editing = Some(network);
                                self.password.clear();
                                self.key = 0;
                                self.page = 0;
                                self.status = "Enter the Wi-Fi password".into();
                            }
                        }
                    }
                }
            },
            _ => {}
        }
        false
    }

    pub fn face(&self) -> UndoFace {
        let (w, h) = (640, 480);
        let mut face = UndoFace {
            rgba: vec![0; w * h * 4],
            w: w as u32,
            h: h as u32,
        };
        fill(&mut face, 0, 0, 640, 480, [20, 21, 25, 255]);
        text(&mut face, "Wi-Fi", 28, 20, 30.0, 584, [240, 240, 244]);
        text(&mut face, &self.status, 28, 63, 18.0, 584, [185, 190, 200]);
        if let Some(network) = &self.editing {
            text(&mut face, &network.ssid, 28, 99, 22.0, 584, [240, 240, 244]);
            let masked = format!(
                "{}  ({} characters)",
                "*".repeat(self.password.len().min(24)),
                self.password.len()
            );
            text(&mut face, &masked, 28, 139, 19.0, 584, [185, 190, 200]);
            for (i, key) in KEYS[self.page].chars().enumerate() {
                let x = 28 + (i % 10) as i32 * 59;
                let y = 190 + (i / 10) as i32 * 45;
                if i == self.key {
                    fill(&mut face, x - 3, y - 4, 49, 40, [65, 70, 85, 255]);
                }
                let label = if key == ' ' {
                    "SP".to_string()
                } else {
                    key.to_string()
                };
                text(&mut face, &label, x + 8, y, 23.0, 40, [240, 240, 244]);
            }
            text(
                &mut face,
                "A Type   X Erase   Y abc/ABC/123",
                28,
                391,
                19.0,
                584,
                [185, 190, 200],
            );
            text(
                &mut face,
                "START Connect   B Cancel",
                28,
                433,
                19.0,
                584,
                [240, 240, 244],
            );
        } else {
            let mut rows = vec![
                "Scan networks".into(),
                "Reconnect saved network".into(),
                "Disconnect (until reconnected)".into(),
                "Forget saved network".into(),
            ];
            rows.extend(self.networks.iter().map(|n| {
                format!(
                    "{}  [{}]",
                    n.ssid,
                    match n.security {
                        Security::Open => "Open",
                        Security::Personal => "Password",
                        Security::Unsupported => "Unsupported",
                    }
                )
            }));
            let first = self.row.saturating_sub(5);
            for (i, label) in rows.iter().enumerate().skip(first).take(6) {
                let y = 105 + (i - first) as i32 * 46;
                if i == self.row {
                    fill(&mut face, 16, y - 3, 608, 41, [65, 70, 85, 255]);
                }
                text(&mut face, label, 28, y, 22.0, 584, [240, 240, 244]);
            }
            text(
                &mut face,
                "SFTP: port 22 | user root | your BaseOS password",
                28,
                391,
                18.0,
                584,
                [185, 190, 200],
            );
            text(
                &mut face,
                if self.busy {
                    "Working...   B Back"
                } else {
                    "Up/Down Choose   A Select   B Back"
                },
                28,
                433,
                19.0,
                584,
                [240, 240, 244],
            );
        }
        face
    }
}

fn fill(face: &mut UndoFace, x: i32, y: i32, w: i32, h: i32, colour: [u8; 4]) {
    for yy in y.max(0)..(y + h).min(face.h as i32) {
        for xx in x.max(0)..(x + w).min(face.w as i32) {
            let at = (yy as usize * face.w as usize + xx as usize) * 4;
            face.rgba[at..at + 4].copy_from_slice(&colour);
        }
    }
}

/// Preserve case: SSIDs and keyboard keys are case-sensitive, unlike Slot's menu typography.
fn text(face: &mut UndoFace, s: &str, x: i32, y: i32, size: f32, width: i32, ink: [u8; 3]) {
    let Some(font) = slot_ui::text::label_font() else {
        return;
    };
    let mut pen = x as f32;
    let baseline = y + size as i32;
    for c in s.chars() {
        let (m, bitmap) = font.rasterize(c, size);
        if pen + m.advance_width > (x + width) as f32 {
            break;
        }
        for yy in 0..m.height {
            for xx in 0..m.width {
                let dx = pen as i32 + m.xmin + xx as i32;
                let dy = baseline - m.ymin - m.height as i32 + yy as i32;
                if dx < 0 || dy < 0 || dx >= face.w as i32 || dy >= face.h as i32 {
                    continue;
                }
                let a = bitmap[yy * m.width + xx] as u32;
                let at = (dy as usize * face.w as usize + dx as usize) * 4;
                for (k, channel) in ink.iter().enumerate() {
                    face.rgba[at + k] =
                        ((*channel as u32 * a + face.rgba[at + k] as u32 * (255 - a)) / 255) as u8;
                }
            }
        }
        pen += m.advance_width;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keyboard_contains_every_printable_ascii_character() {
        for keys in KEYS {
            assert_eq!(keys.len(), 40);
        }
        for b in 32..=126 {
            assert!(
                KEYS.iter().any(|s| s.as_bytes().contains(&b)),
                "missing {b}"
            );
        }
    }
    #[test]
    fn password_keyboard_masks_input_and_b_cancels_before_leaving() {
        let mut menu = WifiMenu::default();
        menu.networks.push(Network {
            ssid: "Home".into(),
            signal: -30,
            security: Security::Personal,
        });
        menu.row = 4;
        menu.input(Btn::A);
        menu.input(Btn::A);
        assert_eq!(menu.password, "a");
        menu.input(Btn::Y);
        menu.input(Btn::A);
        assert_eq!(menu.password, "aA");
        menu.input(Btn::X);
        assert_eq!(menu.password, "a");
        menu.input(Btn::Start);
        assert!(menu.status.contains("8-63"));
        assert!(!menu.input(Btn::B));
        assert!(menu.password.is_empty());
        assert!(menu.input(Btn::B));
    }
}

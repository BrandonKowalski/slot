use crate::transfer::Server;
use crate::wifi_menu::{fill, text};
use slot_ui::{UndoFace, OUT_H, OUT_W};
use std::net::Ipv4Addr;
use std::path::Path;

pub struct TransferMenu {
    server: Option<Server>,
    message: String,
    uploaded: u64,
    revision: u64,
}
impl Default for TransferMenu {
    fn default() -> Self {
        Self {
            server: None,
            message: String::new(),
            uploaded: 0,
            revision: 1,
        }
    }
}
impl TransferMenu {
    pub fn open(&mut self, root: Option<&Path>, wifi_status: &str) {
        self.stop();
        let ip = wifi_status
            .split(" | IP ")
            .nth(1)
            .and_then(|s| s.parse::<Ipv4Addr>().ok());
        self.server = match (root, ip) {
            (Some(root), Some(ip)) => match Server::start(root, ip, 8080) {
                Ok(server) => Some(server),
                Err(_) => {
                    self.message = "Cannot start. Check Wi-Fi, then press A to retry.".into();
                    None
                }
            },
            _ => {
                self.message = "Connect in Settings > Wi-Fi first, then try again.".into();
                None
            }
        };
        self.revision += 1;
        self.poll();
    }
    pub fn running(&self) -> bool {
        self.server.is_some()
    }
    pub fn stop(&mut self) {
        self.server = None;
        self.revision += 1;
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn poll(&mut self) {
        if let Some(server) = &self.server {
            let status = server.status();
            if status.message != self.message || status.uploaded != self.uploaded {
                self.message = status.message;
                self.uploaded = status.uploaded;
                self.revision += 1;
            }
        }
    }
    pub fn face(&self) -> UndoFace {
        let mut face = UndoFace {
            rgba: vec![0; (OUT_W * OUT_H * 4) as usize],
            w: OUT_W,
            h: OUT_H,
        };
        fill(
            &mut face,
            0,
            0,
            OUT_W as i32,
            OUT_H as i32,
            [20, 21, 25, 255],
        );
        let width = OUT_W as i32 - 56;
        text(
            &mut face,
            "File Transfer",
            28,
            22,
            30.0,
            width,
            [240, 240, 244],
        );
        if let Some(server) = &self.server {
            text(
                &mut face,
                "Open this address on a device on the same Wi-Fi:",
                28,
                84,
                20.0,
                width,
                [185, 190, 200],
            );
            text(
                &mut face,
                &format!("http://{}/", server.address),
                28,
                125,
                32.0,
                width,
                [240, 240, 244],
            );
            text(
                &mut face,
                "Enter this transfer code:",
                28,
                193,
                20.0,
                width,
                [185, 190, 200],
            );
            text(
                &mut face,
                &server.pin,
                28,
                228,
                44.0,
                width,
                [229, 219, 191],
            );
            text(
                &mut face,
                &format!("{} files uploaded", self.uploaded),
                28,
                312,
                20.0,
                width,
                [185, 190, 200],
            );
            text(
                &mut face,
                "Restart Slot after adding games or labels.",
                28,
                389,
                19.0,
                width,
                [185, 190, 200],
            );
            text(
                &mut face,
                "Keep this screen open.   B Stop & back",
                28,
                433,
                20.0,
                width,
                [240, 240, 244],
            );
        } else {
            text(
                &mut face,
                "A Retry   B Back",
                28,
                433,
                20.0,
                width,
                [240, 240, 244],
            );
        }
        text(
            &mut face,
            &self.message,
            28,
            352,
            19.0,
            width,
            [185, 190, 200],
        );
        face
    }
}

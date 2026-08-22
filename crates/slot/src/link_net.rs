//! The TCP transport for a link session: two handhelds on the OS's private WiFi, `10.42.0.1`
//! and `10.42.0.2`, ~2 ms round trip. This module owns exactly one job — carry a core's serial
//! packets over that link intact and promptly — and nothing else. Wiring it to `Link`, the
//! queue the core side actually touches, is the next layer's job, not this one's.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver, TryRecvError};

use slot_retro::LinkChannel;

/// One TCP connection carrying a core's serial traffic.
///
/// Reads run on their own thread into a queue, so `try_recv` is a queue poll rather than a
/// syscall: the emulator thread calls it every frame and cannot afford to block. Writes go
/// straight out, because a serial packet is a handful of bytes and buffering them would
/// only add latency to the thing latency matters most for.
pub struct TcpLink {
    out: TcpStream,
    inbox: Receiver<Vec<u8>>,
}

impl TcpLink {
    /// Wait for the other handheld. Blocking by design: the caller is a session that has
    /// nothing else to do until a peer arrives.
    pub fn host(port: u16) -> std::io::Result<TcpLink> {
        let listener = TcpListener::bind(("0.0.0.0", port))?;
        let (stream, _peer) = listener.accept()?;
        TcpLink::wrap(stream)
    }

    /// Connect to a host that is already waiting.
    pub fn join(addr: &str, port: u16) -> std::io::Result<TcpLink> {
        TcpLink::wrap(TcpStream::connect((addr, port))?)
    }

    fn wrap(stream: TcpStream) -> std::io::Result<TcpLink> {
        // Serial traffic is small and latency sensitive: Nagle would hold a packet back
        // waiting for company it will not get.
        stream.set_nodelay(true)?;
        let mut reader = stream.try_clone()?;
        let (tx, inbox) = channel();

        std::thread::spawn(move || {
            let mut header = [0u8; 2];
            loop {
                if reader.read_exact(&mut header).is_err() {
                    return; // peer gone; the session notices by starving, not by a panic
                }
                let len = u16::from_be_bytes(header) as usize;
                let mut buf = vec![0u8; len];
                if len > 0 && reader.read_exact(&mut buf).is_err() {
                    return;
                }
                if tx.send(buf).is_err() {
                    return; // our own end hung up
                }
            }
        });

        Ok(TcpLink { out: stream, inbox })
    }
}

impl LinkChannel for TcpLink {
    fn send(&mut self, _flags: i32, buf: &[u8]) {
        // TCP is a stream; the boundary has to be ours. A packet longer than u16 cannot
        // come from GBA serial hardware, so refusing one is better than truncating it.
        let Ok(len) = u16::try_from(buf.len()) else {
            return;
        };
        let _ = self.out.write_all(&len.to_be_bytes());
        let _ = self.out.write_all(buf);
    }

    fn try_recv(&mut self) -> Option<Vec<u8>> {
        match self.inbox.try_recv() {
            Ok(p) => Some(p),
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => None,
        }
    }
}

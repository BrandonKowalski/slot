use std::path::PathBuf;
use std::time::{Duration, Instant};

use slot::frontend::Frontend;
use slot::input::DeviceInput;
use slot_gfx::{Compositor, FbdevSurface, Surface};
use slot_power::DevicePlatform;

/// Where BaseOS mounts the card slot has never been checked against a running device, so
/// `launch.sh` exports `SLOT_ROOT` and this is only what is left if it did not.
const CARD: &str = "/mnt/sdcard";

/// The panel is 60 Hz and EGL is asked to lock to it, but a driver that ignores the swap
/// interval would spin this loop as fast as the GPU can clear, so the frame is timed too.
const FRAME: Duration = Duration::from_micros(16_667);

pub fn run() {
    let root = std::env::var_os("SLOT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(CARD));
    let mut surface = match FbdevSurface::new() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("slot: {e}");
            return;
        }
    };
    let mut compositor = match Compositor::new(&surface) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("slot: {e}");
            return;
        }
    };
    let platform = DevicePlatform::new(root.clone());
    eprintln!("slot: {}", platform.report());
    let mut frontend = Frontend::boot(Box::new(platform));
    frontend.upload_faces(&mut compositor);
    let mut input = DeviceInput::open(&root);
    loop {
        let began = Instant::now();
        frontend.render(&mut compositor, surface.window_size());
        if let Err(e) = surface.swap() {
            eprintln!("slot: {e}");
            return;
        }
        frontend.advance(&mut input);
        if frontend.suspending() {
            frontend.suspend();
        }
        if frontend.powering_off() {
            // One more frame, so the shutdown line is on the panel before rcK starts. The
            // teardown stops the frontend and unloads the GPU module before the kernel is
            // allowed to halt, which takes about five seconds — and a device that just goes
            // black for five seconds is one the user assumes has hung.
            frontend.render(&mut compositor, surface.window_size());
            let _ = surface.swap();
            frontend.poweroff();
            return;
        }
        if let Some(left) = FRAME.checked_sub(began.elapsed()) {
            std::thread::sleep(left);
        }
    }
}

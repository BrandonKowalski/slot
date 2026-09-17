use std::time::{Duration, Instant};

use slot::frontend::Frontend;
use slot::input::HostInput;
use slot_gfx::{Compositor, HostSurface, Surface};
use slot_power::SimPlatform;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

/// The device panel's frame, and the twin of `device_app`'s `FRAME`. That loop has always
/// timed its own frame on the grounds that "a driver that ignores the swap interval would
/// spin this loop as fast as the GPU can clear"; this loop never did, because the swap here
/// is asked for vsync (`slot_gfx::host`) and vsync was assumed to do the waiting.
///
/// It does, right up until macOS decides nobody can see the window. An occluded NSOpenGL
/// surface is not presented at all, so `CGLFlushDrawable` has no vblank to wait for and
/// returns immediately — measured at 0.22 ms against 7.9 ms visible. With `ControlFlow::Poll`
/// below and a redraw requested unconditionally after every swap, that leaves nothing at all
/// pacing the loop, and it free-runs: 1900 frames a second, a whole core, for a window that is
/// behind something else. Putting Activity Monitor in front of slot to read its CPU is enough
/// to cause it, which is how it came to be reported.
///
/// So the host gets the guard the device already had, for the reason the device already had
/// it.
///
/// On a panel faster than 60 Hz this is a cap and not only a floor: a 120 Hz Mac was drawing
/// the shelf 120 times a second and now draws it 60. That is the rate the device being
/// simulated runs at, every animation here is already driven by `dt` rather than by a frame
/// count, and the emulator worker was never paced from this loop at all — it keeps its own
/// `PRESENT` deadline (see `emu`), so a game's audio and its rate control do not notice. Half
/// the presents for the same picture is the point rather than a cost.
const FRAME: Duration = Duration::from_micros(16_667);

struct Slot {
    gfx: Option<(HostSurface, Compositor)>,
    frontend: Frontend,
    input: HostInput,
}

impl Slot {
    fn new() -> Self {
        Slot {
            gfx: None,
            frontend: Frontend::boot(Box::new(SimPlatform::new())),
            input: HostInput::new(),
        }
    }
}

impl ApplicationHandler for Slot {
    fn resumed(&mut self, events: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let built = HostSurface::new(events).and_then(|s| Compositor::new(&s).map(|c| (s, c)));
        let (surface, mut compositor) = match built {
            Ok(gfx) => gfx,
            Err(e) => {
                eprintln!("slot: {e}");
                events.exit();
                return;
            }
        };
        self.frontend.upload_faces(&mut compositor);
        self.gfx = Some((surface, compositor));
    }

    fn window_event(&mut self, events: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        self.input.on_window_event(&event);
        let Some((surface, compositor)) = self.gfx.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => events.exit(),
            WindowEvent::Resized(size) => surface.resize(size),
            WindowEvent::RedrawRequested => {
                // Taken before the render and spent after the advance, so what is timed is the
                // whole frame rather than the present alone — the same span `device_app` times.
                let began = Instant::now();
                self.frontend.render(compositor, surface.window_size());
                if let Err(e) = surface.swap() {
                    eprintln!("slot: {e}");
                    events.exit();
                    return;
                }
                surface.request_redraw();
                self.frontend.advance(&mut self.input);
                // The simulated platform ends the process outright.
                if self.frontend.restarting() {
                    self.frontend.restart();
                }
                if self.frontend.powering_off() {
                    self.frontend.poweroff();
                    events.exit();
                    // Before the wait below, as `device_app` returns before its own: a machine
                    // that has been told to stop does not owe the panel the rest of a frame.
                    return;
                }
                // Last, so what is waited out is whatever the frame did not already spend.
                if let Some(left) = FRAME.checked_sub(began.elapsed()) {
                    std::thread::sleep(left);
                }
            }
            _ => {}
        }
    }
}

pub fn run() {
    let events = match EventLoop::new() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("slot: {e}");
            return;
        }
    };
    events.set_control_flow(ControlFlow::Poll);
    if let Err(e) = events.run_app(&mut Slot::new()) {
        eprintln!("slot: {e}");
    }
}

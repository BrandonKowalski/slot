//! Drive the frontend headlessly from a script and save PNG snapshots of the canvas.
//!
//! `cargo run -p slot --example snap -- ROOT SCRIPT OUTDIR`, with `--features canvas-640` to see
//! the 640x480 layout. The script takes the `record` example's words plus `snap NAME`.

use std::time::Duration;

use slot::frontend::Frontend;
use slot_gfx::{Compositor, HeadlessSurface, OUT_H, OUT_W};
use slot_input::{Btn, InputSource, Millis, RawEvent};
use slot_power::SimPlatform;

const FPS: f64 = 60.0;
const CLOCK: i64 = 1_790_784_000;

enum Step {
    Wait(u32),
    Down(Btn),
    Up(Btn),
    Snap(String),
}

fn btn(name: &str) -> Btn {
    match name {
        "up" => Btn::Up,
        "down" => Btn::Down,
        "left" => Btn::Left,
        "right" => Btn::Right,
        "a" => Btn::A,
        "b" => Btn::B,
        "x" => Btn::X,
        "y" => Btn::Y,
        "l1" => Btn::L1,
        "r1" => Btn::R1,
        "l2" => Btn::L2,
        "r2" => Btn::R2,
        "start" => Btn::Start,
        "select" => Btn::Select,
        "menu" => Btn::Menu,
        other => panic!("unknown button {other}"),
    }
}

fn parse(script: &str) -> Vec<Step> {
    let mut steps = Vec::new();
    for line in script.lines() {
        let words: Vec<&str> = line.split('#').next().unwrap().split_whitespace().collect();
        let n = |i: usize, default: u32| words.get(i).map_or(default, |w| w.parse().unwrap());
        match words.as_slice() {
            [] => {}
            ["wait", ..] => steps.push(Step::Wait(n(1, 1))),
            ["tap", b, ..] | ["hold", b, ..] => {
                steps.push(Step::Down(btn(b)));
                steps.push(Step::Wait(n(2, 6)));
                steps.push(Step::Up(btn(b)));
            }
            ["down", b] => steps.push(Step::Down(btn(b))),
            ["up", b] => steps.push(Step::Up(btn(b))),
            ["snap", name] => steps.push(Step::Snap(name.to_string())),
            _ => panic!("cannot read {line:?}"),
        }
    }
    steps
}

#[derive(Default)]
struct Scripted(Vec<RawEvent>);

impl InputSource for Scripted {
    fn poll(&mut self, _now: Millis) -> Vec<RawEvent> {
        std::mem::take(&mut self.0)
    }
}

fn write_png(path: &str, rgba: &[u8]) {
    let file = std::fs::File::create(path).expect("create png");
    let mut e = png::Encoder::new(std::io::BufWriter::new(file), OUT_W, OUT_H);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header()
        .expect("png header")
        .write_image_data(rgba)
        .expect("png data");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [root, script, out] = args.as_slice() else {
        eprintln!("usage: snap ROOT SCRIPT OUTDIR");
        std::process::exit(2);
    };
    std::fs::create_dir_all(out).expect("out dir");
    let steps = parse(&std::fs::read_to_string(script).expect("read the script"));

    let surface = HeadlessSurface::new().expect("headless GL");
    let mut compositor = Compositor::new(&surface).expect("compositor");
    let mut frontend = Frontend::boot(Box::new(SimPlatform::at(root.into()).stopped_at(CLOCK)));
    frontend.upload_faces(&mut compositor);
    frontend.drive_emulator();

    let mut input = Scripted::default();
    let mut frame = 0u64;
    let present = Duration::from_secs_f64(1.0 / FPS);
    let mut run = |input: &mut Scripted, frame: &mut u64| {
        let loading = std::time::Instant::now();
        while frontend.core_settling() && loading.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(5));
        }
        *frame += 1;
        let now = (*frame as f64 * 1000.0 / FPS) as Millis;
        frontend.advance_at(input, now, (1.0 / FPS) as f32);
        frontend.step_emulator(present, Duration::from_secs(1));
        frontend.compose(&mut compositor);
        compositor.read_frame()
    };
    for step in steps {
        match step {
            Step::Wait(n) => {
                for _ in 0..n {
                    run(&mut input, &mut frame);
                }
            }
            Step::Down(b) => input.0.push(RawEvent::Down(b)),
            Step::Up(b) => input.0.push(RawEvent::Up(b)),
            Step::Snap(name) => {
                let pixels = run(&mut input, &mut frame);
                let path = format!("{out}/{name}-{OUT_W}x{OUT_H}.png");
                write_png(&path, &pixels);
                eprintln!("snap: {path}");
            }
        }
    }
}

//! Headless core benchmark: how long a libretro core takes per frame on the machine it runs on.
//!
//! It drives the core through slot-retro's own `LibretroCore`, so the time is what slot's
//! emulator thread pays for a core step, including the RGB565 to XRGB8888 conversion in
//! `video_refresh` and the audio hand-off, and none of what it doesn't (no window, GPU or audio
//! device). It exists to compare builds of a core, or of slot-retro, on the SP. It never ships.
//!
//! Each repeat restores the same starting point (the `--state` file, or the moment after
//! loading and `--warmup` frames), runs `--warmup` untimed frames, then times `--frames` core
//! frames grouped into presents of `--steps`, the way fast forward groups them. `--frameskip`
//! sets each core's fixed-interval frameskip to `steps - 1`, so one frame in each present
//! renders, which is how fast forward runs. Quote the median; the spread says how far to trust
//! it. The hash of the last frame should match across repeats and across builds of one core:
//! a build that changes it has changed the emulation, not just its speed.
//!
//! Build it for the SP with `task bench:device`, then:
//!
//! ```text
//! adb push target-device/release/examples/core_bench /tmp/core_bench
//! adb shell '/tmp/core_bench /mnt/sdcard/System/gpsp_libretro.so /mnt/sdcard/Games/Apotris.gba \
//!     --state /mnt/sdcard/States/gpsp/Apotris/resume.state --system /mnt/sdcard/BIOS \
//!     --steps 4 --frameskip'
//! ```

use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use slot_retro::{ButtonMask, LibretroCore, RetroCore};

const USAGE: &str = "usage: core_bench CORE ROM [--state FILE] [--system DIR] [--steps S] \
[--frames N] [--warmup W] [--repeats R] [--frameskip] [--option KEY=VALUE]...";

struct Args {
    core: PathBuf,
    rom: PathBuf,
    state: Option<PathBuf>,
    /// Where the core looks for `gba_bios.bin`. slot hands it the content root's `BIOS`.
    system: Option<PathBuf>,
    steps: u32,
    frames: u32,
    warmup: u32,
    repeats: u32,
    frameskip: bool,
    options: Vec<(String, String)>,
}

fn value(it: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    it.next()
        .ok_or_else(|| format!("{flag} needs a value\n{USAGE}"))
}

fn count(it: &mut impl Iterator<Item = String>, flag: &str) -> Result<u32, String> {
    match value(it, flag)?.parse() {
        Ok(n) if n > 0 => Ok(n),
        _ => Err(format!("{flag} takes a whole number above 0")),
    }
}

fn parse(mut it: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut positional = Vec::new();
    let mut a = Args {
        core: PathBuf::new(),
        rom: PathBuf::new(),
        state: None,
        system: None,
        steps: 1,
        frames: 1200,
        warmup: 300,
        repeats: 5,
        frameskip: false,
        options: Vec::new(),
    };
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--state" => a.state = Some(value(&mut it, &arg)?.into()),
            "--system" => a.system = Some(value(&mut it, &arg)?.into()),
            "--steps" => a.steps = count(&mut it, &arg)?,
            "--frames" => a.frames = count(&mut it, &arg)?,
            "--warmup" => a.warmup = value(&mut it, &arg)?.parse().map_err(|e| format!("{e}"))?,
            "--repeats" => a.repeats = count(&mut it, &arg)?,
            "--frameskip" => a.frameskip = true,
            "--option" => {
                let kv = value(&mut it, &arg)?;
                let (k, v) = kv.split_once('=').ok_or("--option takes KEY=VALUE")?;
                a.options.push((k.to_string(), v.to_string()));
            }
            s if s.starts_with('-') => return Err(USAGE.to_string()),
            _ => positional.push(PathBuf::from(arg)),
        }
    }
    let [core, rom] = <[PathBuf; 2]>::try_from(positional).map_err(|_| USAGE.to_string())?;
    (a.core, a.rom) = (core, rom);
    Ok(a)
}

/// FNV-1a: enough to tell two frames apart, and no dependency.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn median(sorted: &[f64]) -> f64 {
    let mid = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
}

fn run(a: &Args) -> Result<(), Box<dyn Error>> {
    let system = a
        .system
        .clone()
        .or_else(|| a.core.parent().map(PathBuf::from))
        .unwrap_or_default();
    let mut core = LibretroCore::open_with(&a.core, &system, &std::env::temp_dir())?;
    for (k, v) in &a.options {
        core.set_option(k, v);
    }
    if a.frameskip {
        // Both cores' keys, whichever this is: neither reads the other's prefix.
        let interval = (a.steps - 1).to_string();
        for prefix in ["mgba", "gpsp"] {
            core.set_option(&format!("{prefix}_frameskip"), "fixed_interval");
            core.set_option(&format!("{prefix}_frameskip_interval"), &interval);
        }
    }
    core.load(&a.rom)?;

    let idle = ButtonMask::default();
    let start = match &a.state {
        Some(path) => std::fs::read(path)?,
        None => {
            for _ in 0..a.warmup {
                core.run_frame(idle);
            }
            core.serialize()?
        }
    };

    let presents = a.frames.div_ceil(a.steps);
    let frames = presents * a.steps;
    println!(
        "core {} rom {} state {} steps {} frameskip {} frames {frames} warmup {} repeats {}",
        a.core.display(),
        a.rom.display(),
        a.state
            .as_ref()
            .map_or("none".into(), |p| p.display().to_string()),
        a.steps,
        if a.frameskip { "on" } else { "off" },
        a.warmup,
        a.repeats,
    );

    let mut times = Vec::new();
    for repeat in 1..=a.repeats {
        core.unserialize(&start)?;
        for _ in 0..a.warmup {
            core.run_frame(idle);
        }
        core.take_audio();
        let t = Instant::now();
        for _ in 0..presents {
            for _ in 0..a.steps {
                core.run_frame(idle);
            }
            core.take_audio();
        }
        let ms = t.elapsed().as_secs_f64() * 1000.0 / f64::from(frames);
        let hash = fnv1a(core.video_xrgb8888());
        println!("repeat {repeat}: {ms:.3} ms/frame, last frame {hash:016x}");
        times.push(ms);
    }

    times.sort_by(f64::total_cmp);
    let (mid, lo, hi) = (median(&times), times[0], times[times.len() - 1]);
    let fps = 1000.0 / mid;
    println!(
        "median {mid:.3} ms/frame (min {lo:.3}, max {hi:.3}, spread {:.1}%), {fps:.1} frames/s, \
         {:.2}x at 60 Hz in presents of {}",
        (hi - lo) / mid * 100.0,
        (fps / 60.0).min(f64::from(a.steps)),
        a.steps,
    );
    Ok(())
}

fn main() -> ExitCode {
    let args = match parse(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("core_bench: {e}");
            ExitCode::FAILURE
        }
    }
}

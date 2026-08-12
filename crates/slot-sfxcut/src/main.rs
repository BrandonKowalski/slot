//! Cuts `crates/slot/assets/{insert,eject}.pcm` out of a recording.
//!
//! ```text
//! # what is in the recording, and which takes are usable
//! cargo run -p slot-sfxcut -- takes.wav
//!
//! # cut the two you picked, by their time in seconds
//! cargo run -p slot-sfxcut -- takes.wav --insert 4.812 --eject 9.140
//!
//! # and drop wavs somewhere to listen before committing
//! cargo run -p slot-sfxcut -- takes.wav --insert 4.812 --eject 9.140 --wav /tmp/audition
//! ```
//!
//! A phone voice memo needs converting first, which macOS can do on its own:
//!
//! ```text
//! afconvert -f WAVE -d LEI16@48000 -c 1 memo.m4a takes.wav
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use slot_sfxcut::{
    cut, read_wav, takes, to_le_bytes, Take, EJECT_LEAD, EJECT_LEN, EJECT_PEAK, HZ, INSERT_LEAD,
    INSERT_LEN, INSERT_PEAK,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("slot-sfxcut: {e}");
            ExitCode::FAILURE
        }
    }
}

struct Args {
    recording: PathBuf,
    insert_at: Option<f32>,
    eject_at: Option<f32>,
    wav_dir: Option<PathBuf>,
    lift_db: f32,
}

fn run() -> Result<(), String> {
    let args = parse()?;
    let pcm =
        read_wav(&args.recording).map_err(|e| format!("{}: {e}", args.recording.display()))?;
    println!(
        "{}: {:.1} s at 48 kHz mono\n",
        args.recording.display(),
        pcm.len() as f32 / HZ
    );

    let found = takes(&pcm);
    if found.is_empty() {
        return Err("no transients found. Is the recording silent, or very quiet?".into());
    }

    match (args.insert_at, args.eject_at) {
        (None, None) => {
            list(&found);
            Ok(())
        }
        (insert, eject) => write(
            &pcm,
            &found,
            insert,
            eject,
            args.wav_dir.as_deref(),
            args.lift_db,
        ),
    }
}

/// Every candidate, with whether each direction would fit around it. Fitting is about the
/// silence either side: an insert needs 97 ms before the transient and 143 ms after, an
/// eject 21 ms before and 294 ms after.
fn list(found: &[Take]) {
    println!(
        "  {:>3}  {:>8}  {:>6}  {:>10}  {:>10}",
        "n", "at", "peak", "insert?", "eject?"
    );
    for (n, t) in found.iter().enumerate() {
        let yes = |ok: bool| if ok { "yes" } else { "no" };
        println!(
            "  {:>3}  {:>7.3}s  {:>5.0}%  {:>10}  {:>10}",
            n,
            t.at,
            t.peak * 100.0,
            yes(t.fits(INSERT_LEAD, INSERT_LEN)),
            yes(t.fits(EJECT_LEAD, EJECT_LEN)),
        );
    }
    println!(
        "\nPick one of each and pass its time:\n  \
         cargo run -p slot-sfxcut -- <recording> --insert <at> --eject <at>"
    );
}

fn write(
    pcm: &[f32],
    found: &[Take],
    insert_at: Option<f32>,
    eject_at: Option<f32>,
    wav_dir: Option<&Path>,
    lift_db: f32,
) -> Result<(), String> {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../slot/assets");
    let jobs = [
        ("insert", insert_at, INSERT_LEAD, INSERT_LEN, INSERT_PEAK),
        ("eject", eject_at, EJECT_LEAD, EJECT_LEN, EJECT_PEAK),
    ];

    for (name, at, lead, len, peak) in jobs {
        let Some(at) = at else { continue };
        let at = snap(found, at);
        let clip = cut(pcm, at, lead, len, peak, lift_db)
            .map_err(|e| format!("{name} at {at:.3}s: {e}"))?;
        let bytes = to_le_bytes(&clip);

        std::fs::write(assets.join(format!("{name}.pcm")), &bytes)
            .map_err(|e| format!("{name}.pcm: {e}"))?;
        println!(
            "{:>7}.pcm  cut at {:.3}s, {} samples, transient {:.0} ms in{}",
            name,
            at,
            clip.len(),
            lead * 1000.0,
            match lift_db {
                0.0 => String::new(),
                db => format!(", pre-transient lifted {db:+.0} dB"),
            }
        );

        if let Some(dir) = wav_dir {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            let path = dir.join(format!("{name}.wav"));
            std::fs::write(&path, wav(&bytes)).map_err(|e| format!("{}: {e}", path.display()))?;
            println!("{:>7}.wav  {}", name, path.display());
        }
    }
    Ok(())
}

/// A time typed off the listing is close to a detected transient but rarely exactly on it,
/// and being 10 ms out moves the sound against the picture. Anything within 25 ms of a
/// candidate snaps to it; anything further is taken as meant.
fn snap(found: &[Take], at: f32) -> f32 {
    found
        .iter()
        .map(|t| t.at)
        .filter(|t| (t - at).abs() <= 0.025)
        .min_by(|a, b| (a - at).abs().total_cmp(&(b - at).abs()))
        .unwrap_or(at)
}

fn parse() -> Result<Args, String> {
    let mut recording = None;
    let mut insert_at = None;
    let mut eject_at = None;
    let mut wav_dir = None;
    let mut lift_db = 0.0;

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut number = |what: &str| -> Result<f32, String> {
            args.next()
                .ok_or_else(|| format!("--{what} needs a value"))?
                .parse()
                .map_err(|_| format!("--{what}: not a number"))
        };
        match a.as_str() {
            "--insert" => insert_at = Some(number("insert")?),
            "--eject" => eject_at = Some(number("eject")?),
            "--wav" => wav_dir = Some(PathBuf::from(args.next().ok_or("--wav needs a directory")?)),
            "--lift" => lift_db = number("lift")?,
            _ if recording.is_none() => recording = Some(PathBuf::from(a)),
            other => return Err(format!("unexpected argument {other}")),
        }
    }

    Ok(Args {
        recording: recording.ok_or("give me a recording to cut from")?,
        insert_at,
        eject_at,
        wav_dir,
        lift_db,
    })
}

/// A 44 byte canonical header in front of the same bytes the .pcm holds. Only so the output
/// can be listened to; nothing reads these back.
fn wav(pcm: &[u8]) -> Vec<u8> {
    let hz = HZ as u32;
    let mut out = Vec::with_capacity(44 + pcm.len());
    out.extend(b"RIFF");
    out.extend(((36 + pcm.len()) as u32).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes()); // pcm chunk size
    out.extend(1u16.to_le_bytes()); // uncompressed
    out.extend(1u16.to_le_bytes()); // mono
    out.extend(hz.to_le_bytes());
    out.extend((hz * 2).to_le_bytes()); // bytes per second
    out.extend(2u16.to_le_bytes()); // block align
    out.extend(16u16.to_le_bytes()); // bits
    out.extend(b"data");
    out.extend((pcm.len() as u32).to_le_bytes());
    out.extend(pcm);
    out
}

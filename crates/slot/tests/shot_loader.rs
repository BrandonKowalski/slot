use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use slot::shot_loader::{Loaded, ShotLoader, Want};

fn write_png(path: &Path, w: u32, h: u32, first: [u8; 3]) {
    let mut rgb = vec![0x40u8; (w * h * 3) as usize];
    rgb[..3].copy_from_slice(&first);
    let file = std::fs::File::create(path).expect("create png");
    let mut e = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    e.write_header()
        .expect("png header")
        .write_image_data(&rgb)
        .expect("png data");
}

fn collect(loader: &ShotLoader, want: usize, within: Duration) -> Vec<Loaded> {
    let until = Instant::now() + within;
    let mut got = Vec::new();
    while got.len() < want && Instant::now() < until {
        got.extend(loader.poll());
        std::thread::sleep(Duration::from_millis(2));
    }
    got
}

fn shots(dir: &Path, n: usize, w: u32, h: u32) -> Vec<PathBuf> {
    (0..n)
        .map(|i| {
            let p = dir.join(format!("{i:02}.png"));
            write_png(&p, w, h, [0xff, 0, 0]);
            p
        })
        .collect()
}

#[test]
fn thumbs_fit_the_cell_and_keep_their_shape() {
    let d = tempfile::tempdir().unwrap();
    let wide = d.path().join("wide.png");
    let square = d.path().join("square.png");
    write_png(&wide, 720, 480, [0xff, 0, 0]);
    write_png(&square, 640, 480, [0xff, 0, 0]);
    let mut loader = ShotLoader::spawn();
    loader.queue_thumbs(vec![wide.clone(), square.clone()], 218, 145);
    let got = collect(&loader, 2, Duration::from_secs(2));
    let size = |p: &Path| got.iter().find(|l| l.path == p).map(|l| (l.want, l.w, l.h));
    assert_eq!(size(&wide), Some((Want::Thumb, 217, 145)));
    assert_eq!(size(&square), Some((Want::Thumb, 193, 145)));
}

#[test]
fn a_full_shot_jumps_the_thumb_queue() {
    let d = tempfile::tempdir().unwrap();
    let paths = shots(d.path(), 30, 720, 480);
    let mut loader = ShotLoader::spawn();
    loader.queue_thumbs(paths.clone(), 218, 145);
    loader.full(paths[29].clone());
    let got = collect(&loader, 31, Duration::from_secs(10));
    let full = got.iter().position(|l| l.want == Want::Full).expect("no full shot");
    let last_thumb = got
        .iter()
        .position(|l| l.want == Want::Thumb && l.path == paths[29])
        .expect("no thumb for the last shot");
    assert!(full < last_thumb, "full at {full}, last thumb at {last_thumb}");
}

#[test]
fn a_full_shot_is_full_size() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("shot.png");
    write_png(&p, 720, 480, [0x12, 0x34, 0x56]);
    let mut loader = ShotLoader::spawn();
    loader.full(p.clone());
    let got = collect(&loader, 1, Duration::from_secs(2));
    let l = got.first().expect("no full shot");
    assert_eq!((l.want, l.w, l.h, l.rgba.len()), (Want::Full, 720, 480, 720 * 480 * 4));
    assert_eq!(&l.rgba[..4], &[0x12, 0x34, 0x56, 0xff]);
}

#[test]
fn cancel_bumps_the_generation_and_drops_the_queue() {
    let d = tempfile::tempdir().unwrap();
    let paths = shots(d.path(), 30, 720, 480);
    let mut loader = ShotLoader::spawn();
    let g = loader.cancel();
    loader.queue_thumbs(paths, 218, 145);
    let after = loader.cancel();
    let got = collect(&loader, 30, Duration::from_millis(500));
    assert!(got.is_empty(), "{} results after cancel", got.len());
    assert_eq!(after, g + 1);
}

#[test]
fn an_unreadable_file_is_skipped() {
    let d = tempfile::tempdir().unwrap();
    let good = d.path().join("good.png");
    write_png(&good, 720, 480, [0xff, 0, 0]);
    let mut loader = ShotLoader::spawn();
    loader.queue_thumbs(vec![d.path().join("missing.png"), good.clone()], 218, 145);
    let got = collect(&loader, 2, Duration::from_millis(800));
    assert_eq!(got.iter().map(|l| l.path.clone()).collect::<Vec<_>>(), [good]);
}

#[test]
fn queueing_thumbs_replaces_the_queue_but_keeps_the_generation() {
    let d = tempfile::tempdir().unwrap();
    let paths = shots(d.path(), 30, 720, 480);
    let mut loader = ShotLoader::spawn();
    let g = loader.cancel();
    loader.queue_thumbs(paths[..20].to_vec(), 218, 145);
    loader.queue_thumbs(paths[25..].to_vec(), 218, 145);
    let got = collect(&loader, 7, Duration::from_secs(5));
    assert!(got.iter().all(|l| l.generation == g));
    for p in &paths[25..] {
        assert!(got.iter().any(|l| &l.path == p), "{} never loaded", p.display());
    }
    let early = got.iter().filter(|l| paths[..20].contains(&l.path)).count();
    assert!(early <= 2, "{early} replaced thumbs still loaded");
}

#[test]
fn a_thumb_already_decoding_is_not_queued_again() {
    let d = tempfile::tempdir().unwrap();
    let paths = shots(d.path(), 4, 720, 480);
    let mut loader = ShotLoader::spawn();
    loader.cancel();
    loader.queue_thumbs(paths.clone(), 218, 145);
    std::thread::sleep(Duration::from_millis(2));
    for _ in 0..20 {
        loader.queue_thumbs(paths.clone(), 218, 145);
    }
    let got = collect(&loader, 4, Duration::from_secs(5));
    std::thread::sleep(Duration::from_millis(300));
    let more = loader.poll();
    assert_eq!(got.len() + more.len(), 4, "decoded {} for 4 shots", got.len() + more.len());
    assert_eq!(loader.decoded(), 4);
}

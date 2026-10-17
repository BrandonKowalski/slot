#![cfg(target_os = "macos")]

mod common;

use std::collections::VecDeque;
use std::path::Path;
use std::time::{Duration, Instant};

use common::{clocked, repo_root, tmp_root_with_gbc_carts};
use slot::frontend::Frontend;
use slot_gfx::{Compositor, HeadlessSurface, OUT_H, OUT_W};
use slot_input::{Btn, InputSource, Millis, RawEvent};
use slot_power::SimPlatform;
use slot_store::{read_slot_state, write_slot_state, BezelPick};

struct Script(VecDeque<Vec<RawEvent>>);

impl InputSource for Script {
    fn poll(&mut self, _now: Millis) -> Vec<RawEvent> {
        self.0.pop_front().unwrap_or_default()
    }
}

fn tap(f: &mut Frontend, input: &mut Script, btn: Btn) {
    input.0.push_back(vec![RawEvent::Down(btn)]);
    f.advance(input);
    input.0.push_back(vec![RawEvent::Up(btn)]);
    f.advance(input);
}

fn run_for(f: &mut Frontend, input: &mut Script, secs: f32) {
    let until = Instant::now() + Duration::from_secs_f32(secs);
    while Instant::now() < until {
        f.advance(input);
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn at(px: &[u8], x: usize, y: usize) -> [u8; 3] {
    let o = (y * OUT_W as usize + x) * 4;
    [px[o], px[o + 1], px[o + 2]]
}

fn rgba(path: &Path) -> Vec<u8> {
    let decoder = png::Decoder::new(std::io::BufReader::new(
        std::fs::File::open(path).expect("open bezel"),
    ));
    let mut reader = decoder.read_info().expect("png header");
    let mut buf = vec![0; reader.output_buffer_size()];
    reader.next_frame(&mut buf).expect("png data");
    buf
}

/// How many of the bezel's solid pixels in the bar are on screen, out of how many it has there.
fn bezel_shown(px: &[u8], bezel: &[u8], xs: std::ops::Range<usize>) -> (usize, usize) {
    let solid: Vec<(usize, usize)> = (0..OUT_H as usize)
        .flat_map(|y| xs.clone().map(move |x| (x, y)))
        .filter(|&(x, y)| bezel[(y * OUT_W as usize + x) * 4 + 3] == 255)
        .collect();
    let shown = solid
        .iter()
        .filter(|&&(x, y)| {
            let o = (y * OUT_W as usize + x) * 4;
            (0..3).all(|ch| bezel[o + ch].abs_diff(at(px, x, y)[ch]) <= 3)
        })
        .count();
    (shown, solid.len())
}

const LEFT_BAR: std::ops::Range<usize> = 0..93;
const RIGHT_BAR: std::ops::Range<usize> = 627..720;

fn playing_with_bezel(
    root: &Path,
    pick: BezelPick,
) -> (HeadlessSurface, Compositor, Frontend, Script) {
    let mut s = read_slot_state(root);
    s.bezel_gbc = pick;
    write_slot_state(root, &s).expect("write slot.state");
    let surface = HeadlessSurface::new().expect("headless surface");
    let mut c = Compositor::new(&surface).expect("compositor");
    let mut f = Frontend::boot(Box::new(SimPlatform::at(root.to_path_buf())));
    f.upload_faces(&mut c);
    let mut input = Script(VecDeque::new());
    tap(&mut f, &mut input, Btn::A);
    run_for(&mut f, &mut input, 4.0);
    (surface, c, f, input)
}

fn frame(f: &mut Frontend, c: &mut Compositor) -> Vec<u8> {
    f.compose(c);
    c.read_frame()
}

#[test]
fn the_default_bezel_in_fit_mode_lights_the_power_lamp_and_labels_the_right_bar() {
    if HeadlessSurface::new().is_err() {
        return;
    }
    let d = tmp_root_with_gbc_carts(&["Crystal", "Zzz"]);
    clocked(d.path());
    let (_s, mut c, mut f, mut input) = playing_with_bezel(d.path(), BezelPick::Default);
    tap(&mut f, &mut input, Btn::L1);
    run_for(&mut f, &mut input, 0.3);
    let px = frame(&mut f, &mut c);
    let bezel = rgba(&repo_root().join("assets/bezel_gbc_fit.png"));
    for (side, xs) in [("left", LEFT_BAR), ("right", RIGHT_BAR)] {
        let (shown, solid) = bezel_shown(&px, &bezel, xs);
        assert!(solid > 100, "the fit bezel has nothing in its {side} bar");
        assert!(
            shown * 100 >= solid * 99,
            "{side} bar: only {shown} of the fit bezel's {solid} pixels are on screen"
        );
    }
}

#[test]
fn a_bezel_from_the_card_shows_at_actual_size_and_stays_off_the_fitted_picture() {
    if HeadlessSurface::new().is_err() {
        return;
    }
    let d = tmp_root_with_gbc_carts(&["Crystal", "Zzz"]);
    clocked(d.path());
    std::fs::copy(
        repo_root().join("assets/bezel_gbc.png"),
        d.path().join("Bezels/GBC/Mine.png"),
    )
    .expect("plant a bezel");
    let (_s, mut c, mut f, mut input) =
        playing_with_bezel(d.path(), BezelPick::File("Mine.png".into()));
    let bezel = rgba(&d.path().join("Bezels/GBC/Mine.png"));
    let actual = frame(&mut f, &mut c);
    let (shown, solid) = bezel_shown(&actual, &bezel, LEFT_BAR);
    assert!(
        shown * 100 >= solid * 99,
        "only {shown} of the card's bezel's {solid} pixels are on screen at actual size"
    );

    tap(&mut f, &mut input, Btn::L1);
    run_for(&mut f, &mut input, 0.3);
    let fitted = frame(&mut f, &mut c);
    let (shown, solid) = bezel_shown(&fitted, &bezel, LEFT_BAR);
    assert!(
        shown * 100 < solid,
        "{shown} of {solid} pixels of an actual-size bezel were drawn over the fitted picture"
    );
}

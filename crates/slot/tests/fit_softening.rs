//! Fork-only (FORK-NOTES.md): the fitted picture is softened for Game Boy and Game Boy Color
//! carts alike, and only while it is fitted.

mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use slot::app::Phase;
use slot::session::Session;
use slot_gfx::WHOLE_TEXTURE;
use slot_input::{Btn, RawEvent};
use slot_store::Platform;

const GB_WINDOW: [f32; 4] = [40.0 / 240.0, 0.0, 160.0 / 240.0, 1.0];
const FIT_WINDOW: [f32; 4] = [12.0 / 240.0, 8.0 / 160.0, 216.0 / 240.0, 144.0 / 160.0];

fn session_playing(root: &Path) -> Session {
    common::clocked(root);
    let mut s = Session::boot(root.to_path_buf());
    s.feed([RawEvent::Down(Btn::A)], 16);
    s.feed([RawEvent::Up(Btn::A)], 32);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        s.update(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(1));
    }
    s
}

/// Picture and softening at each mode, stepping up with L to Stretch and back down with R.
fn modes_up_and_down(s: &mut Session) -> Vec<([f32; 4], bool)> {
    let mut seen = vec![(s.app().source_rect(), s.app().softens_picture())];
    let mut at = 100;
    for btn in [Btn::L1, Btn::L1, Btn::R1, Btn::R1] {
        s.feed([RawEvent::Down(btn)], at);
        s.feed([RawEvent::Up(btn)], at + 20);
        at += 100;
        seen.push((s.app().source_rect(), s.app().softens_picture()));
    }
    seen
}

#[test]
fn a_game_boy_color_cart_is_softened_only_while_fitted() {
    let d = common::tmp_root_with_gbc_carts(&["Crystal", "Zzz"]);
    let mut s = session_playing(d.path());
    assert_eq!(
        s.app().playing_platform(),
        Platform::Gbc,
        "the cart did not play as a Game Boy Color game"
    );
    assert_eq!(
        modes_up_and_down(&mut s),
        [
            (WHOLE_TEXTURE, false),
            (FIT_WINDOW, true),
            (GB_WINDOW, false),
            (FIT_WINDOW, true),
            (WHOLE_TEXTURE, false),
        ]
    );
}

#[test]
fn game_boy_and_game_boy_color_carts_fit_and_soften_alike() {
    let gb = common::tmp_root_with_gb_carts(&["Tetris", "Zzz"]);
    let gbc = common::tmp_root_with_gbc_carts(&["Crystal", "Zzz"]);
    let gb_modes = modes_up_and_down(&mut session_playing(gb.path()));
    let gbc_modes = modes_up_and_down(&mut session_playing(gbc.path()));
    assert_eq!(gb_modes, gbc_modes);
}

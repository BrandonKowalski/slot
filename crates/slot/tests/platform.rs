//! Which platform's folders a seated cart's files actually land in, through the real `Session`.
//!
//! `Platform::default()` is `Gba` and every other cart in this crate's tests is a GBA cart, so
//! a session that never stores the platform it opened the core with — or stores it and then
//! ignores it — reads identically to one that does. This is the Game Boy half: the one shape
//! where the value `session.rs` resolves and the value `App` would fall back to differ, and
//! therefore the only thing that can tell storing it apart from assuming it.
//!
//! The same pairing as `gpsp.rs` does for `Core`, and the more dangerous of the two: a wrong
//! core writes a state the other engine cannot read, which shows up the first time the player
//! resumes. A wrong platform writes one game's battery save over another game's, and the two
//! games are told apart by nothing but a folder name.

mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use slot::app::Phase;
use slot::session::Session;
use slot_input::{Btn, RawEvent};
use slot_store::{Core, Platform, StateRing};

/// Taps A on the shelf to seat whatever is under it, waits for the insert to finish, and runs
/// past the autosave deadline — the cheapest way to get the core's own state and save ram
/// written back out through the path the binary uses, same as `play.rs`'s `counter_after`.
fn seat_and_autosave(root: &Path) {
    common::clocked(root);
    let mut s = Session::boot(root.to_path_buf());
    s.feed([RawEvent::Down(Btn::A)], 16);
    s.feed([RawEvent::Up(Btn::A)], 32);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut now = 32;
    while !matches!(s.app().phase(), Phase::Playing { .. }) {
        assert!(Instant::now() < deadline, "the cart never seated");
        now += 16;
        s.feed([], now);
        s.update(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(1));
    }

    s.app_mut().tick_ms(60_000);
}

/// The requirement Task 4 exists for, checked where it is actually decided: `session.rs` reads
/// the `Platform` off the `Cart` the shelf scanned and hands it to `App`, which files every
/// later flush under it. Delete that one hand-off and the app falls back to `Platform::default()`
/// — `Gba` — for a cart that is not a GBA cart at all, and the Game Boy game's state is written
/// into the GBA folder, where a GBA game of the same name would later read or overwrite it.
///
/// Two carts because one cart on the card is a dedicated device: it boots straight past the
/// shelf and there is no press to make. Both are Game Boy carts, so the shelf's first entry —
/// the one A seats — is the one this is about; the scan sorts GBA ahead of GB, and a GBA cart
/// in this fixture would be the cart that seated instead.
#[test]
fn a_game_boy_carts_autosave_lands_under_gb_and_never_under_gba() {
    let d = common::tmp_root_with_gb_carts(&["Tetris", "Zelda"]);

    seat_and_autosave(d.path());

    assert!(
        StateRing::new(d.path(), Platform::Gb, Core::Mgba, "Tetris")
            .read_resume()
            .unwrap()
            .is_some(),
        "the Game Boy cart's autosave did not land under its own platform's directory"
    );
    assert!(
        StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Tetris")
            .read_resume()
            .unwrap()
            .is_none(),
        "the Game Boy cart's autosave was filed as a GBA cart's, which is where a GBA game \
         of the same name keeps its own"
    );

    // The other half of the same collision, and the half that loses a real save rather than a
    // save state: the battery file. A GBA cart named `Tetris` writes 128 KB here; a Game Boy
    // one writes a few. Sharing the path, whichever wrote last truncated or shadowed the other.
    assert!(
        d.path().join("Saves/GB/Tetris.sav").is_file(),
        "the Game Boy cart's battery save did not land under its own platform's directory"
    );
    assert!(
        !d.path().join("Saves/GBA/Tetris.sav").exists(),
        "the Game Boy cart's battery save was written where a GBA game of the same name \
         keeps its own"
    );
}

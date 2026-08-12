mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use common::{app_playing_in, boot, tmp_root_with_carts, StubSnapshot};
use slot::app::Phase;
use slot::emu::Speed;
use slot::persist::eject;
use slot_input::Action;
use slot_store::{read_slot_state, write_slot_state, SlotState, StateRing};
use slot_ui::{Draw, CART_W, OUT_H, OUT_W};

fn seated(cart: &str) -> SlotState {
    SlotState {
        cart: Some(cart.into()),
        clock_set: true,
        utc_offset_min: 0,
        ..Default::default()
    }
}

fn set_mode(dir: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode)).expect("chmod");
}

#[test]
fn eject_clears_the_slot_only_after_the_state_is_durable() {
    let d = tmp_root_with_carts(&["Emerald"]);
    write_slot_state(
        d.path(),
        &SlotState {
            cart: Some("Emerald".into()),
            ..Default::default()
        },
    )
    .unwrap();
    eject(d.path(), "Emerald", &[9u8; 1024], Some(b"savdata")).unwrap();
    assert_eq!(read_slot_state(d.path()).cart, None);
    let r = StateRing::new(d.path(), "Emerald");
    assert_eq!(r.read_resume().unwrap().unwrap().len(), 1024);
    assert_eq!(
        std::fs::read(d.path().join("Saves/Emerald.sav")).unwrap(),
        b"savdata"
    );
    assert!(
        r.list().unwrap().is_empty(),
        "eject must not create a polaroid"
    );
}

/// The clear is last for a reason. A resume that did not land has to leave the cart
/// seated, so the next boot resumes the session rather than losing it to a full card.
#[test]
fn a_resume_that_cannot_be_written_leaves_the_cart_in_the_slot() {
    let d = tmp_root_with_carts(&["Emerald"]);
    write_slot_state(d.path(), &seated("Emerald")).unwrap();
    std::fs::write(d.path().join("States/Emerald"), b"in the way").unwrap();
    assert!(eject(d.path(), "Emerald", &[9u8; 1024], None).is_err());
    assert_eq!(read_slot_state(d.path()).cart, Some("Emerald".into()));
}

/// The core hands back the whole save ram every eject and almost none of them touched a
/// byte of it.
#[test]
fn an_unchanged_battery_save_is_not_rewritten() {
    let d = tmp_root_with_carts(&["Emerald"]);
    std::fs::write(d.path().join("Saves/Emerald.sav"), b"savdata").unwrap();
    let saves = d.path().join("Saves");
    set_mode(&saves, 0o555);
    let unchanged = eject(d.path(), "Emerald", &[0u8; 8], Some(b"savdata"));
    let changed = eject(d.path(), "Emerald", &[0u8; 8], Some(b"changed"));
    set_mode(&saves, 0o755);
    unchanged.expect("identical bytes must not touch the card");
    assert!(
        changed.is_err(),
        "the directory stayed writable, so the first half proved nothing"
    );
}

/// `slot.state` is one file. Clearing the cart must not take the levels with it.
#[test]
fn eject_preserves_the_levels() {
    let d = tmp_root_with_carts(&["Emerald"]);
    write_slot_state(
        d.path(),
        &SlotState {
            cart: Some("Emerald".into()),
            brightness: 2,
            blue_light: 7,
            volume: 35,
            muted: true,
            clock_set: true,
            utc_offset_min: 0,
        },
    )
    .unwrap();
    eject(d.path(), "Emerald", &[0u8; 8], None).unwrap();
    let s = read_slot_state(d.path());
    assert_eq!((s.brightness, s.blue_light, s.volume), (2, 7, 35));
    assert!(s.muted, "the cart came out and the sound came back");
}

/// The writes hang off leaving `Playing`, not off the end of the animation: the card can
/// be pulled while the cart is still sliding out.
#[test]
fn ejecting_a_playing_cart_flushes_before_the_animation_starts() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::Eject);
    assert!(matches!(a.phase(), Phase::Ejecting { .. }));
    let r = StateRing::new(d.path(), "Emerald");
    assert_eq!(
        r.read_resume().unwrap().expect("nothing was flushed").len(),
        1024
    );
    assert_eq!(read_slot_state(d.path()).cart, None);
}

/// A cart that never loaded has no state of its own, and a resume written here would be
/// whatever the snapshot last had to hand.
#[test]
fn a_refused_cart_writes_no_resume() {
    let d = tmp_root_with_carts(&["Emerald"]);
    write_slot_state(d.path(), &seated("Emerald")).unwrap();
    let mut a = boot(d.path());
    a.set_snapshot(StubSnapshot::boxed());
    a.on_core_failed();
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    assert!(StateRing::new(d.path(), "Emerald")
        .read_resume()
        .unwrap()
        .is_none());
    assert_eq!(read_slot_state(d.path()).cart, None);
}

/// A cart whose core never arrived is still in the slot and still has to come out. Without
/// this the only way out of a stalled insert is a reboot, and `SLOT_NO_CORE=1`, which holds
/// the slot on screen so the travel can be watched, would be a one way trip.
#[test]
fn a_cart_can_be_ejected_before_its_core_is_ready() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    let mut a = boot(d.path());
    a.apply(Action::Insert);
    assert!(matches!(a.phase(), Phase::Inserting { .. }));
    a.apply(Action::Eject);
    assert!(
        matches!(a.phase(), Phase::Ejecting { .. }),
        "a cart with no core behind it cannot be got out"
    );
}

/// The insert run backwards. The row closes back up behind the cart as it comes out; before
/// this the shelf appeared all at once the instant the travel ended.
#[test]
fn the_row_closes_back_up_as_the_cart_comes_out() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion", "Metroid"]);
    let mut a = boot(d.path());
    a.apply(Action::Insert);
    for _ in 0..90 {
        a.update(1.0 / 60.0);
    }
    a.apply(Action::Eject);
    assert!(matches!(a.phase(), Phase::Ejecting { .. }));

    let carts = |a: &slot::app::App| {
        let mut out = Vec::new();
        a.draw(&mut out);
        out.iter()
            .filter(|d| matches!(d, Draw::Rect { w, .. } | Draw::Tex { w, .. } if *w < CART_W as f32 + 1.0 && *w > 1.0))
            .count()
    };
    let early = carts(&a);
    for _ in 0..40 {
        a.update(1.0 / 60.0);
    }
    let late = carts(&a);
    assert!(
        late > early,
        "the row went from {early} carts to {late}: it is not coming back"
    );
}

/// The eject is the insert played backwards: every part of the screen at the same rate, in
/// the opposite direction. It used to be driven off two clocks of different lengths, so the
/// row closed a third faster than it opened, and the veil darkened on the way out as well as
/// on the way in rather than lifting.
#[test]
fn the_eject_is_the_insert_run_backwards() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion", "Metroid"]);
    let mut a = boot(d.path());
    let dt = 1.0 / 60.0;

    // What each frame of the travel moves the cart by, which is the thing that has to
    // mirror. Positions cannot be compared directly: the two runs are sampled either side of
    // a frame boundary and sit one step apart for their whole length.
    let steps = |a: &mut slot::app::App, done: fn(f32) -> bool| {
        let mut out = Vec::new();
        let mut last = a.seat();
        while !done(a.seat()) {
            a.update(dt);
            out.push((a.seat() - last).abs());
            last = a.seat();
        }
        out
    };

    a.apply(Action::Insert);
    let going_in = steps(&mut a, |s| s >= 1.0);
    for _ in 0..120 {
        a.update(dt);
    }

    a.apply(Action::Eject);
    // Past the picture going out and the beat after it, neither of which is travel.
    while a.seat() >= 1.0 {
        a.update(dt);
    }
    let coming_out = steps(&mut a, |s| s <= 0.0);

    assert!(
        going_in.len() > 10,
        "the insert took {} frames",
        going_in.len()
    );
    assert!(
        going_in.len().abs_diff(coming_out.len()) <= 1,
        "in over {} frames, out over {}",
        going_in.len(),
        coming_out.len()
    );
    // Ends excluded: both are clipped by the clamp at their own end of the travel.
    let n = going_in.len().min(coming_out.len()) - 1;
    for i in 1..n {
        let (a_in, a_out) = (going_in[i], coming_out[coming_out.len() - 1 - i]);
        assert!(
            (a_in - a_out).abs() < 0.01,
            "frame {i}: moved {a_in} going in against {a_out} coming out"
        );
    }
}

/// Both directions are drawn from `seat` alone, so nothing on the screen can run on a clock
/// of its own and quietly stop mirroring: the veil, the row and the cart all move together.
#[test]
fn the_veil_lifts_on_the_way_out_instead_of_falling_again() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion", "Metroid"]);
    let mut a = boot(d.path());
    let dt = 1.0 / 60.0;
    a.apply(Action::Insert);
    for _ in 0..120 {
        a.update(dt);
    }
    a.apply(Action::Eject);

    let veil = |a: &slot::app::App| {
        let mut out = Vec::new();
        a.draw(&mut out);
        out.iter()
            .find_map(|d| match *d {
                Draw::Rect { w, h, colour, .. }
                    if w >= OUT_W as f32 && h >= OUT_H as f32 && colour[..3] == [0.0; 3] =>
                {
                    Some(colour[3])
                }
                _ => None,
            })
            .unwrap_or(0.0)
    };
    // Past the picture going out and the beat after it, which is stillness by design.
    while a.seat() >= 1.0 {
        a.update(dt);
    }
    let first = veil(&a);
    for _ in 0..12 {
        a.update(dt);
    }
    let later = veil(&a);
    assert!(
        later < first,
        "the veil went from {first} to {later}: it is darkening on the way out"
    );
}

/// The game is over the moment the button is held. A core left running behind a dark screen
/// is a game still being heard after the player ended it, and the cart used to start coming
/// out over the top of it.
#[test]
fn the_core_stops_before_the_cart_moves() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    common::clocked(d.path());
    let mut s = slot::session::Session::boot(d.path().to_path_buf());
    s.app_mut().apply(Action::Insert);
    for _ in 0..90 {
        s.update(1.0 / 60.0);
    }
    assert!(s.has_core(), "no core to stop");

    s.app_mut().apply(Action::Eject);
    // One update to carry the new phase through `sync_speed`, which is the only place
    // anything tells the worker to pause.
    s.update(1.0 / 60.0);

    // Wait for the worker to say it read `Paused`, not for the frame count to sit still.
    // Stillness is ambiguous from the outside: a descheduled worker holds at whatever count it
    // last reached for exactly as long as the scheduler ignores it, then wakes and publishes
    // anyway, and no number of quiet updates tells the two apart.
    //
    // Nor may this wait keep calling `update`: that steps the eject's own travel, and a `dt`
    // of a sixtieth of a second per call covers the whole animation in far less real time than
    // the worker's own ~16.6ms pace needs to take even one more turn — driving the wait this
    // way would spend the travel before the worker was ever scheduled to answer, the same gap
    // that made the frame count an unreliable witness in the first place. Spending wall-clock
    // time instead, with `seat` held still, actually gives the worker the turn being waited on.
    //
    // `observed_speed` is not an inference from the outside. The worker stores it right where
    // it reads `speed`, with `Release`, so a reader who sees `Paused` here has a proof, not a
    // guess: if the worker read Paused on some iteration, that iteration ran zero steps and
    // published nothing; nothing sets the speed back during an eject, so no later iteration can
    // have published either; and `Release` paired with this load's `Acquire` means any publish
    // from an earlier iteration is ordered before the store this loop just observed. A count
    // that has not moved yet only suggests the core stopped. This confirms it.
    let deadline = Instant::now() + Duration::from_secs(2);
    while s.observed_speed() != Some(Speed::Paused) {
        assert!(
            Instant::now() < deadline,
            "the worker never reported pausing"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        s.app().seat() >= 1.0,
        "the eject ran out of travel before the core stopped, so the check below is vacuous"
    );
    let published = s.frames_published();
    while s.app().seat() >= 1.0 {
        s.update(1.0 / 60.0);
    }
    assert_eq!(
        s.frames_published(),
        published,
        "the core kept running while the picture was out"
    );
}

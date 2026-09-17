mod common;

use common::tmp_root_with_carts;
use slot::app::{INSERT_S, SEATED_AT};
use slot::audio::{ring_capacity, Ring, Sfx, GBA_HZ};
use slot::session::Session;

/// The clip is started early enough that the contacts in it land on the frame the cart does.
/// That offset is the whole of the sync, so it is checked against the asset itself rather
/// than trusted: regenerate a clip or retime the animation and this is what says the two have
/// come apart.
#[test]
fn the_contacts_in_the_clip_are_where_the_lead_says() {
    for s in [Sfx::Insert, Sfx::Eject] {
        let v = s.render(48_000);
        let at = v
            .chunks_exact(2)
            .enumerate()
            .max_by_key(|(_, c)| c[0].unsigned_abs())
            .map(|(i, _)| i as f32 / 48_000.0)
            .unwrap();
        assert!(
            (at - s.lead()).abs() < 0.008,
            "{s:?}: the contacts are {:.0} ms into the clip and lead says {:.0} ms",
            at * 1000.0,
            s.lead() * 1000.0
        );
    }
}

/// The insert has to start after the cart is already moving, or there is nothing left of the
/// travel to hear it over.
#[test]
fn the_insert_starts_inside_the_travel() {
    assert!(
        Sfx::Insert.lead() < SEATED_AT,
        "the clip is longer than the travel it belongs to"
    );
}

/// Played as recorded: started at the right moment and otherwise left alone. Stretching,
/// enveloping and joining are all things this deliberately does not do, and this is what says
/// so — a clip that came back different twice, or panned, has had something done to it.
#[test]
fn the_clips_are_the_recording_and_nothing_else() {
    for s in [Sfx::Insert, Sfx::Eject] {
        let a = s.render(48_000);
        let b = s.render(48_000);
        assert_eq!(a, b, "{s:?} is not the same twice");
        // Mono content in both channels, so nothing has been panned or filtered per side.
        assert!(
            a.chunks_exact(2).all(|c| c[0] == c[1]),
            "{s:?} is not the mono recording"
        );
    }
}

/// Each direction is its own take. One clip serving both, or the same take cut twice, is a
/// cart that sounds the same going out as it does going in.
#[test]
fn the_two_directions_are_different_clips() {
    let ins = Sfx::Insert.render(48_000);
    let ej = Sfx::Eject.render(48_000);
    assert_ne!(ins.len(), ej.len());
    // Their shapes are opposite, which is the point: going in the shell runs the rails and
    // then the contacts, coming out the contacts go first and the shell follows.
    assert!(
        Sfx::Eject.lead() * 3.0 < Sfx::Insert.lead(),
        "the eject leads with {} ms and the insert with {} ms: one is the wrong take",
        Sfx::Eject.lead() * 1000.0,
        Sfx::Insert.lead() * 1000.0
    );
}

/// A window cut out of a longer recording has no reason to land on zero at either end, and a
/// clip that starts or stops partway through its noise floor is a click on every insert.
#[test]
fn neither_clip_starts_or_ends_on_a_step() {
    for (name, s) in [("insert", Sfx::Insert), ("eject", Sfx::Eject)] {
        let v = s.render(48_000);
        assert!(
            v[..8].iter().all(|x| x.abs() < 400),
            "{name} starts on a step"
        );
        assert!(
            v[v.len() - 200..].iter().all(|x| x.abs() < 400),
            "{name} ends on a step"
        );
    }
}

/// The clip is mixed into whatever the game is already playing, so it lands with the thing on
/// screen rather than a buffer behind it.
#[test]
fn a_clip_is_mixed_into_queued_game_audio_rather_than_played_after_it() {
    let r = Ring::new(ring_capacity(48_000));
    let c = Sfx::Insert.render(48_000);
    // More queued than the clip is long, or mixing legitimately extends the run and the
    // question the test is asking cannot be answered.
    r.push(&vec![1_000i16; c.len() * 2]);
    let queued = r.queued_frames();
    r.mix(&c);
    assert_eq!(
        r.queued_frames(),
        queued,
        "the clip was appended, so it plays after the game caught up"
    );
    let mut out = vec![0i16; c.len()];
    r.fill(&mut out);
    assert_eq!(out[0], 1_000i16.saturating_add(c[0]));
}

/// Both clips are longer than the ring is: 240 ms and 315 ms against the 133 ms the ring holds
/// at every rate it has ever opened at. Mixing wrote what fitted and dropped the rest, so 45%
/// of every insert and 58% of every eject has never been played — cut at whatever sample 133 ms
/// landed on rather than at a zero crossing, and taking the whole of the tail `Sfx::tail`
/// exists to keep the picture off with it.
#[test]
fn the_whole_of_a_cart_sound_reaches_the_device() {
    for rate in [48_000, GBA_HZ] {
        for s in [Sfx::Insert, Sfx::Eject] {
            let clip = s.render(rate);
            assert!(
                clip.len() > ring_capacity(rate) * 2,
                "{s:?} now fits the ring at {rate} Hz, so this no longer exercises anything"
            );
            let r = Ring::new(ring_capacity(rate));
            r.reopen(rate);
            r.mix(&clip);
            // Read it back the way the device does: a period at a time, for as long as it
            // takes. The ring opens on a cushion of silence, so what comes out is the clip.
            let mut got: Vec<i16> = Vec::new();
            while got.len() < clip.len() {
                let mut out = vec![0i16; 512 * 2];
                r.fill(&mut out);
                got.extend_from_slice(&out);
            }
            let first = got
                .iter()
                .zip(&clip)
                .position(|(a, b)| a != b)
                .unwrap_or(clip.len());
            assert_eq!(
                first,
                clip.len(),
                "{s:?} at {rate} Hz came apart {:.0} ms in, {:.0} ms short of the {:.0} ms it runs",
                1000.0 * first as f32 / 2.0 / rate as f32,
                1000.0 * (clip.len() - first) as f32 / 2.0 / rate as f32,
                1000.0 * clip.len() as f32 / 2.0 / rate as f32,
            );
        }
    }
}

/// The game keeps playing underneath the part of a clip that had to wait for room, and the two
/// are added rather than one replacing the other. A clip that only mixed with what was queued
/// when it arrived would cut a hole in the game for its own second half.
#[test]
fn the_rest_of_a_clip_is_mixed_into_the_game_it_lands_over() {
    let rate = GBA_HZ;
    let r = Ring::new(ring_capacity(rate));
    r.reopen(rate);
    let clip = Sfx::Insert.render(rate);
    r.mix(&clip);
    let mut got: Vec<i16> = Vec::new();
    while got.len() < clip.len() {
        // A game running behind it, pushed a period at a time the way the worker does.
        r.push(&vec![100i16; 512 * 2]);
        let mut out = vec![0i16; 512 * 2];
        r.fill(&mut out);
        got.extend_from_slice(&out);
    }
    // Past the ring's own length, so this is the half of the clip that had to wait.
    let late = ring_capacity(rate) * 2 + 1000;
    assert!(
        got[late..clip.len()]
            .iter()
            .zip(&clip[late..])
            .all(|(a, b)| *a == b.saturating_add(100)),
        "the clip and the game did not add up"
    );
}

/// The insert plays while the cart is seating, when no core has started. If the sink still
/// belongs to the worker there is nothing to play it through.
#[test]
fn a_clip_plays_with_no_core_running() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut s = Session::boot(d.path().to_path_buf());
    assert!(!s.has_core());
    s.play_sfx(Sfx::Insert);
    assert!(s.audio_queued() > 0, "the clip went nowhere");
}

/// One sound per movement, fired where the recording begins: the shell touching the rails on
/// the way in, the contacts letting go on the way out.
#[test]
fn each_movement_makes_one_sound() {
    for (action, want) in [
        (slot_input::Action::Insert, Sfx::Insert),
        (slot_input::Action::Eject, Sfx::Eject),
    ] {
        let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
        let mut a = common::boot(d.path());
        if action == slot_input::Action::Eject {
            a.apply(slot_input::Action::Insert);
            for _ in 0..80 {
                a.update(1.0 / 60.0);
                a.take_sfx();
            }
        }
        a.apply(action);
        let mut heard: Vec<Sfx> = a.take_sfx().into_iter().collect();
        for _ in 0..90 {
            a.update(1.0 / 60.0);
            if let Some(s) = a.take_sfx() {
                heard.push(s);
            }
        }
        assert_eq!(heard, vec![want], "{action:?} sounded like {heard:?}");
    }
}

/// The insert plays while the cart is still sliding in, and the core is paused for the whole
/// of that so it does not burn through the bios intro behind the animation. The worker used
/// to mute the ring for any speed but normal, which gated the sound of the cart going in as
/// well: with a core it was silent and only `SLOT_NO_CORE=1` ever let it through.
#[test]
fn a_paused_core_does_not_silence_the_insert() {
    let r = Ring::new(ring_capacity(48_000));
    r.reopen(48_000);
    // What the worker does on the way into a pause: leave the gate open. Whatever the core
    // already handed over runs out on its own, and `fill` pads the rest with silence.
    r.set_muted(false);
    r.mix(&Sfx::Insert.render(48_000));
    let mut out = vec![0i16; 4_000];
    r.fill(&mut out);
    assert!(
        out.iter().any(|v| v.abs() > 100),
        "the insert came out silent behind a paused core"
    );
}

/// Fast forward is the one speed that does produce audio nobody asked to hear, so that gate
/// stays. Losing it would put a chipmunk under every held R2.
#[test]
fn fast_forward_is_still_gated() {
    let r = Ring::new(ring_capacity(48_000));
    r.reopen(48_000);
    r.set_muted(true);
    r.mix(&Sfx::Insert.render(48_000));
    let mut out = vec![0i16; 4_000];
    r.fill(&mut out);
    assert!(out.iter().all(|v| *v == 0), "muting no longer silences");
}

/// The picture waits out the whole sound of the cart landing, and then a beat more. Cutting
/// in over the shell still settling reads as the game interrupting the cart rather than the
/// cart starting the game.
#[test]
fn the_game_waits_for_the_cart_to_finish_landing() {
    let hold = INSERT_S - SEATED_AT;
    let tail = Sfx::Insert.tail();
    assert!(
        hold > tail,
        "the picture arrives {:.0} ms after the cart lands and the sound runs {:.0} ms",
        hold * 1000.0,
        tail * 1000.0
    );
    let beat = hold - tail;
    assert!(
        (0.05..0.30).contains(&beat),
        "{:.0} ms of air between the sound and the game",
        beat * 1000.0
    );
}

/// What the loudest sample in the clip comes out as, once it is in the ring the device
/// drains.
fn sfx_peak(setup: impl Fn(&mut Session)) -> u16 {
    let d = tmp_root_with_carts(&["Emerald"]);
    // Past the clock screen, which is where a fresh card starts and where a volume press is
    // the picker's rather than the level's.
    slot_store::write_slot_state(
        d.path(),
        &slot_store::SlotState {
            clock_set: true,
            ..slot_store::SlotState::default()
        },
    )
    .unwrap();
    let mut s = Session::boot(d.path().to_path_buf());
    setup(&mut s);
    s.play_sfx(Sfx::Insert);
    let ring = s.audio_ring();
    let mut out = vec![0i16; ring.queued_frames() * 2];
    ring.fill(&mut out);
    out.iter().map(|v| v.unsigned_abs()).max().unwrap_or(0)
}

/// The slot's own sounds are mixed straight into the ring rather than produced by the core,
/// so the level the game is played at never reaches them unless it is applied on the way in.
/// A cart landing at full volume under a game turned down to nothing is the complaint.
#[test]
fn a_cart_sound_is_played_at_the_volume_that_is_set() {
    let loud = sfx_peak(|_| {});
    let quiet = sfx_peak(|s| {
        for _ in 0..8 {
            s.app_mut().apply(slot_input::Action::VolumeDown);
        }
    });
    assert!(loud > 0, "the clip was silent at the volume it booted with");
    assert!(
        quiet * 2 < loud,
        "turning the volume down did not quieten the cart: {quiet} against {loud}"
    );
}

/// Mute is silence, and a cart sliding home is not exempt from it.
#[test]
fn a_cart_sound_is_silent_when_muted() {
    let muted = sfx_peak(|s| s.app_mut().apply(slot_input::Action::MuteToggle));
    assert_eq!(muted, 0, "the cart was heard through a mute");
}

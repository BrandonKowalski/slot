mod common;

use std::sync::atomic::Ordering;

use common::{app_playing_with_jack, tmp_root_with_carts};
use slot_input::Action;
use slot_store::read_slot_state;
use slot_ui::Icon;

#[test]
fn speaker_and_headphones_keep_their_own_volume() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let (mut a, jack) = app_playing_with_jack(d.path(), "Emerald");
    let speaker = a.volume();

    jack.store(true, Ordering::Relaxed);
    let at = a.now() + 1_000;
    a.tick_ms(at);
    assert_eq!(
        a.hud_icon(),
        Icon::Headphones,
        "plugging in did not show headphones"
    );
    for _ in 0..3 {
        a.apply(Action::VolumeDown);
    }
    let headphones = a.volume();
    assert!(
        headphones < speaker,
        "the buttons did not move the headphone level"
    );

    jack.store(false, Ordering::Relaxed);
    let at = a.now() + 1_000;
    a.tick_ms(at);
    assert_eq!(
        a.volume(),
        speaker,
        "the speaker came back at the headphone level"
    );
    assert_eq!(a.hud_icon(), Icon::Volume);

    jack.store(true, Ordering::Relaxed);
    let at = a.now() + 1_000;
    a.tick_ms(at);
    assert_eq!(
        a.volume(),
        headphones,
        "the headphone level was not remembered"
    );

    a.flush_state();
    let saved = read_slot_state(d.path());
    assert_eq!((saved.volume, saved.volume_hp), (speaker, headphones));
}

#[test]
fn muting_the_headphones_leaves_the_speaker_alone() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let (mut a, jack) = app_playing_with_jack(d.path(), "Emerald");
    let speaker = a.volume();

    jack.store(true, Ordering::Relaxed);
    let at = a.now() + 1_000;
    a.tick_ms(at);
    a.apply(Action::MuteToggle);
    assert_eq!(a.output_volume(), 0);
    assert_eq!(a.hud_icon(), Icon::HeadphonesMuted);

    jack.store(false, Ordering::Relaxed);
    let at = a.now() + 1_000;
    a.tick_ms(at);
    assert_eq!(a.output_volume(), speaker, "the speaker came back muted");

    jack.store(true, Ordering::Relaxed);
    let at = a.now() + 1_000;
    a.tick_ms(at);
    assert_eq!(
        a.output_volume(),
        0,
        "the headphones forgot they were muted"
    );

    a.flush_state();
    let saved = read_slot_state(d.path());
    assert_eq!((saved.muted, saved.muted_hp), (false, true));
}

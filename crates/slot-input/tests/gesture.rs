use slot_input::RawEvent::{Down, Up};
use slot_input::{
    Action::*, Btn, Btn::*, Gestures, RawEvent, MENU_HOLD_MS, POWER_HOLD_MS, SELECT_CHORD_MS,
    SELECT_TAP_MS, VOLUME_REPEAT_DELAY_MS, VOLUME_REPEAT_MS,
};

/// The reported bug, and the half of it this fixes. SELECT used to be withheld for the whole
/// chord window, so a *held* SELECT arrived at the game `SELECT_CHORD_MS` late whether or not a
/// chord ever followed it. On a Game Boy cart that uses SELECT to hold a piece, 600 ms is the
/// whole gesture — and disabling chords for Game Boy carts would not have helped, because the
/// latency was never the chord's, it was the waiting.
///
/// The press goes straight through now and the chord arms off the same hold, so nothing about
/// the gesture moves: only the game stops being kept waiting to find out.
#[test]
fn a_held_select_reaches_the_game_on_the_press() {
    let mut g = Gestures::new();
    assert_eq!(g.feed(Down(Select), 0), vec![GbaDown(Select)]);
    // And the hold is still the chord's to arm from.
    assert_eq!(g.feed(Down(Btn::Up), 120), vec![BrightnessUp]);
    // The second key is the chord's alone, on both of its edges.
    assert!(g.feed(Up(Btn::Up), 160).is_empty());
    // And the release the game is owed for that press reaches it.
    assert_eq!(g.feed(Up(Select), 900), vec![GbaUp(Select)]);
    assert!(g.tick(5_000).is_empty());
}

/// The chord takes the *second* key and nothing else now. SELECT is the game's from the press,
/// which is the cost of the fix above: a player reaching for brightness hands the game a SELECT
/// it did not mean to send. That is inherent to sharing the button — the press is already out
/// by the time the second key says what it was for, and taking it back would be a release the
/// player never made.
#[test]
fn a_chord_takes_the_second_key_and_leaves_select_with_the_game() {
    let mut g = Gestures::new();
    assert_eq!(g.feed(Down(Select), 0), vec![GbaDown(Select)]);
    assert_eq!(g.feed(Down(R1), 50), vec![SaveState]);
    assert!(
        g.feed(Up(R1), 60).is_empty(),
        "the chord key reached the game"
    );
    assert_eq!(g.feed(Up(Select), 70), vec![GbaUp(Select)]);
}

/// A chord already fired under this hold keeps the window open for the rest of it, which is
/// what lets a held SELECT ramp the brightness rather than handing the game every press after
/// the first. The window alone would have expired between the second press and the third.
#[test]
fn a_held_select_keeps_chording_long_past_the_window() {
    let mut g = Gestures::new();
    g.feed(Down(Select), 0);
    assert_eq!(g.feed(Down(Btn::Up), 100), vec![BrightnessUp]);
    g.feed(Up(Btn::Up), 150);
    assert_eq!(
        g.feed(Down(Btn::Up), SELECT_CHORD_MS + 500),
        vec![BrightnessUp],
        "the second nudge of the brightness reached the game as a direction"
    );
}

#[test]
fn select_chords_map_to_all_four_axes() {
    for (btn, want) in [
        (Btn::Up, BrightnessUp),
        (Btn::Down, BrightnessDown),
        (Right, BlueLightUp),
        (Left, BlueLightDown),
    ] {
        let mut g = Gestures::new();
        g.feed(RawEvent::Down(Select), 0);
        assert_eq!(g.feed(RawEvent::Down(btn), 10), vec![want]);
    }
}

/// A tap inside the chord window is an ordinary press and an ordinary release, both on the
/// edges the player actually made. Nothing waits for the window: releasing SELECT was never
/// what settled the question, since the press had already gone.
#[test]
fn select_tapped_inside_the_window_is_a_plain_press_and_release() {
    let mut g = Gestures::new();
    assert_eq!(g.feed(Down(Select), 0), vec![GbaDown(Select)]);
    assert_eq!(g.feed(Up(Select), 80), vec![GbaUp(Select)]);
    assert!(g.tick(1_000).is_empty());
}

/// The release a tap still owes, when the next press lands before the tick can hand it over.
/// `SELECT_TAP_MS` is 50 ms — three frames — so this is not a gesture anyone performs on
/// purpose: it is a switch that bounced, or a worn membrane making one press twice.
///
/// The press that arrives inside that window used to overwrite the state owing the release, and
/// a press that then became a chord delivered no SELECT of its own to end the tap with. The
/// core was left holding SELECT with no up ever coming — for the rest of the session, if the
/// player kept chording, since every chorded press was swallowed the same way.
///
/// Handing the press straight to the game takes the second half of that away for good: a
/// chorded press is no longer swallowed, so there is no press that cannot end its own tap. The
/// hand-back stays, because the first half is still real — the bounce still lands inside a
/// window a release is owed in.
///
/// The invariant is the one the pad reads: SELECT goes down exactly as often as it comes up.
#[test]
fn a_select_press_inside_the_tap_window_hands_back_the_release_it_interrupted() {
    let mut g = Gestures::new();
    let mut log = Vec::new();
    log.extend(g.feed(Down(Select), 0));
    log.extend(g.feed(Up(Select), 20));
    // The bounce, inside the window the tap's release is still owed in.
    log.extend(g.feed(Down(Select), 20 + SELECT_TAP_MS - 1));
    // And that press goes on to be a chord, which used to hand the core nothing of its own.
    log.extend(g.feed(Down(Btn::Up), 100));
    log.extend(g.feed(Up(Btn::Up), 140));
    log.extend(g.feed(Up(Select), 200));
    for t in 200..2_000 {
        log.extend(g.tick(t));
    }
    let downs = log.iter().filter(|a| **a == GbaDown(Select)).count();
    let ups = log.iter().filter(|a| **a == GbaUp(Select)).count();
    assert_eq!(
        (downs, ups),
        (2, 2),
        "the core was handed {downs} SELECT press(es) and {ups} release(s): {log:?}"
    );
    assert_eq!(
        log.last(),
        Some(&GbaUp(Select)),
        "the pad was left holding SELECT: {log:?}"
    );
    assert!(
        log.contains(&BrightnessUp),
        "the chord off the second press stopped working: {log:?}"
    );
}

/// A tap of MENU opens the quick menu, on the release. The other two MENU gestures are
/// unchanged: a tap was the one press this button did not already mean something by.
#[test]
fn menu_single_tap_opens_the_quick_menu() {
    let mut g = Gestures::new();
    assert!(g.feed(Down(Menu), 0).is_empty(), "acted on the press");
    assert_eq!(g.feed(Up(Menu), 100), vec![QuickMenu]);
    assert!(g.tick(451).is_empty(), "fired a second time on the timer");
}

/// The hold is an eject and nothing else. A press long enough to eject is not also a tap, or
/// letting go of one would drop the quick menu over the shelf the cart just came back to.
#[test]
fn a_menu_hold_is_not_also_a_tap() {
    let mut g = Gestures::new();
    g.feed(Down(Menu), 0);
    assert_eq!(g.tick(MENU_HOLD_MS), vec![Eject]);
    assert!(g.feed(Up(Menu), MENU_HOLD_MS + 50).is_empty());
}

#[test]
fn menu_double_tap_opens_polaroids_immediately() {
    let mut g = Gestures::new();
    g.feed(Down(Menu), 0);
    g.feed(Up(Menu), 100);
    assert_eq!(g.feed(Down(Menu), 300), vec![Polaroids]);
}

#[test]
fn menu_hold_ejects_at_the_hold_time_and_not_before() {
    let mut g = Gestures::new();
    g.feed(Down(Menu), 0);
    assert!(g.tick(MENU_HOLD_MS - 1).is_empty());
    assert_eq!(g.tick(MENU_HOLD_MS), vec![Eject]);
}

#[test]
fn menu_hold_released_early_ejects_nothing() {
    let mut g = Gestures::new();
    g.feed(Down(Menu), 0);
    g.feed(Up(Menu), MENU_HOLD_MS - 200);
    assert!(g.tick(3000).is_empty());
}

#[test]
fn r2_hold_is_momentary() {
    let mut g = Gestures::new();
    assert_eq!(g.feed(Down(R2), 0), vec![FfStart]);
    assert_eq!(g.feed(Up(R2), 900), vec![FfStop]);
}

#[test]
fn r2_double_tap_latches_and_single_press_clears() {
    let mut g = Gestures::new();
    g.feed(Down(R2), 0);
    g.feed(Up(R2), 50); // tap 1
    g.feed(Down(R2), 100);
    assert!(g.feed(Up(R2), 150).is_empty()); // latched, FF stays on
    g.feed(Down(R2), 5000);
    assert_eq!(g.feed(Up(R2), 5050), vec![FfStop]);
}

/// The flush hangs off the press, because a button being held may be cut by the PMIC before
/// there is any release to see. The lock waits for the release, so that a press on its way
/// to becoming a hold does not darken the panel on the way through.
#[test]
fn power_flushes_on_the_press_and_locks_on_the_release() {
    let mut g = Gestures::new();
    assert_eq!(g.feed(Down(Btn::Power), 0), vec![PowerPress]);
    assert_eq!(g.feed(Up(Btn::Power), 80), vec![PowerTap]);
}

/// The hold arms while the button is still down and the release commits, so the shutdown
/// screen is on the panel for as long as the user holds rather than flashing past.
#[test]
fn power_held_past_the_threshold_raises_the_menu_and_the_release_does_nothing() {
    let mut g = Gestures::new();
    g.feed(Down(Btn::Power), 0);
    assert!(g.tick(POWER_HOLD_MS - 1).is_empty());
    assert_eq!(g.tick(POWER_HOLD_MS), vec![PowerHold]);
    assert!(g.tick(4000).is_empty(), "the hold fires once, not per tick");
    assert_eq!(g.feed(Up(Btn::Power), 4500), vec![PowerOff]);
}

/// And a release that never reached the threshold is a lock, never a shutdown.
#[test]
fn a_press_just_short_of_the_threshold_is_a_lock() {
    let mut g = Gestures::new();
    g.feed(Down(Btn::Power), 0);
    assert!(g.tick(POWER_HOLD_MS - 1).is_empty());
    assert_eq!(g.feed(Up(Btn::Power), POWER_HOLD_MS - 1), vec![PowerTap]);
}

/// A press L2 turned away is not half of a double tap. It produced nothing, so its release
/// must be remembered as nothing either — recorded as an ordinary release it stood as the
/// first tap of a latch the player never made, and the *single* press that followed it turned
/// fast forward on and left it on with nobody holding R2.
///
/// The whole thing is one ordinary sequence: rewinding, a stab at R2 that does nothing, the
/// rewind ends, and R2 is pressed once. There is no timing to aim for beyond the 250 ms every
/// double tap already has.
#[test]
fn a_press_the_rewind_turned_away_is_not_half_of_a_latch() {
    let mut g = Gestures::new();
    assert_eq!(g.feed(Down(L2), 0), vec![RewindStart]);
    assert!(
        g.feed(Down(R2), 50).is_empty(),
        "the rewind let a fast forward start under it"
    );
    assert!(g.feed(Up(R2), 100).is_empty());
    assert_eq!(g.feed(Up(L2), 150), vec![RewindStop]);
    // One press of R2, inside the double tap window of that refused release.
    assert_eq!(g.feed(Down(R2), 200), vec![FfStart]);
    assert!(!g.ff_latched(), "a single press latched fast forward");
    assert_eq!(
        g.feed(Up(R2), 400),
        vec![FfStop],
        "the finger came off R2 and the speed stayed"
    );
}

#[test]
fn rewind_beats_latched_fast_forward() {
    let mut g = Gestures::new();
    g.feed(Down(R2), 0);
    g.feed(Up(R2), 50);
    g.feed(Down(R2), 100);
    g.feed(Up(R2), 150); // latched
    assert_eq!(g.feed(Down(L2), 200), vec![FfStop, RewindStart]);
}

/// 120 ms was not enough time to land the second key of a chord, so the window is generous.
/// It now governs only the arming: SELECT is the game's from the press either way, and this is
/// how long a second key on top of it still means brightness rather than a direction.
#[test]
fn the_chord_window_governs_the_second_key_and_nothing_else() {
    let mut g = Gestures::new();
    assert_eq!(g.feed(Down(Select), 0), vec![GbaDown(Select)]);
    assert!(
        g.tick(400).is_empty(),
        "the tick still had something to hand over"
    );
    assert!(
        g.tick(SELECT_CHORD_MS).is_empty(),
        "the window's expiry is still an event the game hears about"
    );
    // Past the window, with no chord to have kept it open, a chord key is the game's own.
    assert_eq!(
        g.feed(Down(Btn::Up), SELECT_CHORD_MS + 1),
        vec![GbaDown(Btn::Up)]
    );
}

#[test]
fn a_chord_landing_late_is_still_a_chord() {
    let mut g = Gestures::new();
    g.feed(Down(Select), 0);
    g.tick(400);
    assert_eq!(g.feed(Down(R1), 400), vec![SaveState]);
    assert!(
        g.tick(5_000).is_empty(),
        "the tick handed something over behind a late chord"
    );
}

/// Down and up in one batch net out to nothing: the mask is set and cleared before the core
/// ever reads it, so the press would be invisible to the game. A tap shorter than three frames
/// is therefore held on until the tick can let go of it — measured from the press, which is
/// where the game got it, rather than from the release.
#[test]
fn a_select_tap_too_short_to_be_polled_is_held_on_to() {
    let mut g = Gestures::new();
    assert_eq!(g.feed(Down(Select), 0), vec![GbaDown(Select)]);
    assert!(
        g.feed(Up(Select), 20).is_empty(),
        "released before the core could poll it"
    );
    assert!(g.tick(SELECT_TAP_MS - 1).is_empty());
    assert_eq!(g.tick(SELECT_TAP_MS), vec![GbaUp(Select)]);
    assert!(
        g.tick(5_000).is_empty(),
        "the release was handed over twice"
    );
}

/// And a tap the core has certainly already polled ends on its own release, with nothing owed
/// and nothing deferred. Three frames is the whole of the guarantee.
#[test]
fn a_select_tap_the_core_has_seen_ends_on_its_own_release() {
    let mut g = Gestures::new();
    g.feed(Down(Select), 0);
    assert_eq!(g.feed(Up(Select), SELECT_TAP_MS), vec![GbaUp(Select)]);
    assert!(g.tick(5_000).is_empty());
}

#[test]
fn both_volume_keys_together_mute_once() {
    let mut g = Gestures::new();
    g.feed(Down(VolUp), 0);
    let out = g.feed(Down(VolDown), 80);
    assert!(out.contains(&MuteToggle), "the pair did not mute");
    assert!(
        g.tick(400).is_empty(),
        "it kept firing while both were held"
    );
}

#[test]
fn volume_keys_far_apart_are_not_a_chord() {
    let mut g = Gestures::new();
    g.feed(Down(VolUp), 0);
    let out = g.feed(Down(VolDown), 400);
    assert!(!out.contains(&MuteToggle), "two separate presses muted");
}

/// The app rolls the chord's own two presses back, so it has to see them before it sees the
/// chord. Reversed, the second press would move the level after the mute remembered it.
#[test]
fn the_chord_arrives_behind_the_press_that_completed_it() {
    let mut g = Gestures::new();
    g.feed(Down(VolUp), 0);
    assert_eq!(g.feed(Down(VolDown), 80), vec![VolumeDown, MuteToggle]);
}

/// Unmuting is the same gesture, so a pair that never rearmed would be a one way trip.
#[test]
fn releasing_both_rearms_the_chord() {
    let mut g = Gestures::new();
    g.feed(Down(VolUp), 0);
    g.feed(Down(VolDown), 80);
    g.feed(Up(VolUp), 200);
    g.feed(Up(VolDown), 220);
    g.feed(Down(VolUp), 1_000);
    assert!(g.feed(Down(VolDown), 1_050).contains(&MuteToggle));
}

/// A held volume key ramps. One step per press would mean tapping a dozen times to cross the
/// range, and the press already records when it went down for exactly this.
#[test]
fn a_held_volume_key_repeats() {
    let mut g = Gestures::new();
    assert_eq!(g.feed(Down(VolUp), 0), vec![VolumeUp]);
    assert!(
        g.tick(VOLUME_REPEAT_DELAY_MS - 1).is_empty(),
        "it repeated before the ramp was due"
    );
    assert_eq!(g.tick(VOLUME_REPEAT_DELAY_MS), vec![VolumeUp]);
    assert!(g.tick(VOLUME_REPEAT_DELAY_MS + 1).is_empty());
    assert_eq!(
        g.tick(VOLUME_REPEAT_DELAY_MS + VOLUME_REPEAT_MS),
        vec![VolumeUp]
    );
}

#[test]
fn a_released_volume_key_stops_repeating() {
    let mut g = Gestures::new();
    g.feed(Down(VolDown), 0);
    assert_eq!(g.tick(VOLUME_REPEAT_DELAY_MS), vec![VolumeDown]);
    assert!(g.feed(Up(VolDown), VOLUME_REPEAT_DELAY_MS + 10).is_empty());
    assert!(
        g.tick(VOLUME_REPEAT_DELAY_MS * 4).is_empty(),
        "a key nobody is holding kept ramping"
    );
}

/// Both keys held is the mute chord, not two levels moving at once. The ramp would fight the
/// mute it just fired and leave the level somewhere nobody asked for.
#[test]
fn the_mute_chord_does_not_ramp() {
    let mut g = Gestures::new();
    g.feed(Down(VolUp), 0);
    assert!(g.feed(Down(VolDown), 50).contains(&MuteToggle));
    assert!(
        g.tick(VOLUME_REPEAT_DELAY_MS * 3).is_empty(),
        "the mute chord ramped the volume while it was held"
    );
}

/// A second press after the ramp starts from the top again, rather than inheriting the pace
/// of the press before it.
#[test]
fn each_press_starts_its_own_ramp() {
    let mut g = Gestures::new();
    g.feed(Down(VolUp), 0);
    g.tick(VOLUME_REPEAT_DELAY_MS);
    g.feed(Up(VolUp), VOLUME_REPEAT_DELAY_MS + 5);
    assert_eq!(g.feed(Down(VolUp), 5_000), vec![VolumeUp]);
    assert!(
        g.tick(5_000 + VOLUME_REPEAT_DELAY_MS - 1).is_empty(),
        "the new press repeated early"
    );
    assert_eq!(g.tick(5_000 + VOLUME_REPEAT_DELAY_MS), vec![VolumeUp]);
}

/// X on its own is the game's X. A chord key is only a chord while SELECT is down.
#[test]
fn x_without_select_is_still_the_games_x() {
    let mut g = Gestures::new();
    assert_eq!(g.feed(Down(X), 0), vec![GbaDown(Btn::X)]);
    assert_eq!(g.feed(Up(X), 40), vec![GbaUp(Btn::X)]);
}

/// SELECT+MENU, which is what opens the in-game menu. One gesture, not two: it fires on the
/// MENU press and the release says nothing at all.
#[test]
fn select_and_menu_open_the_in_game_menu() {
    let mut g = Gestures::new();
    g.feed(Down(Select), 0);
    assert_eq!(g.feed(Down(Menu), 50), vec![GameMenu]);
    assert!(
        g.feed(Up(Menu), 150).is_empty(),
        "the release landed as a second gesture on top of the menu"
    );
    // The game was handed SELECT on the press, as it is for every chord, so it is owed the
    // release. The menu is up by the time this lands and `Session::overlaid` keeps both edges
    // off the pad; what matters here is only that the gesture layer never leaves one owed.
    assert_eq!(g.feed(Up(Select), 200), vec![GbaUp(Select)]);
}

/// The chorded press must not also arm the eject hold, or reading the menu with the buttons
/// still down would eject the cart out from under it.
#[test]
fn a_chorded_menu_never_arms_the_eject_hold() {
    let mut g = Gestures::new();
    g.feed(Down(Select), 0);
    assert_eq!(g.feed(Down(Menu), 50), vec![GameMenu]);
    assert!(
        g.tick(50 + MENU_HOLD_MS + 1).is_empty(),
        "holding the menu open ejected the cart"
    );
}

/// The whole test is the ordering inside `menu_down`. Behind the double tap check, a
/// SELECT+MENU that follows a recent MENU tap opens the switcher instead of the menu.
#[test]
fn a_chord_after_a_recent_menu_tap_is_still_the_menu() {
    let mut g = Gestures::new();
    g.feed(Down(Menu), 0);
    assert_eq!(g.feed(Up(Menu), 100), vec![QuickMenu]);
    g.feed(Down(Select), 150);
    assert_eq!(
        g.feed(Down(Menu), 200),
        vec![GameMenu],
        "a recent tap turned the chord into the switcher"
    );
}

/// The difference between a chord and a trap. The chord window closes after `SELECT_CHORD_MS`,
/// and MENU under a SELECT that has stopped being able to chord is an ordinary MENU.
#[test]
fn menu_under_a_select_the_game_already_has_is_not_the_menu() {
    let mut g = Gestures::new();
    assert_eq!(g.feed(Down(Select), 0), vec![GbaDown(Select)]);
    assert!(g.tick(SELECT_CHORD_MS).is_empty());
    assert!(
        g.feed(Down(Menu), 700).is_empty(),
        "a SELECT the game already owns still chorded"
    );
    assert_eq!(g.feed(Up(Menu), 800), vec![QuickMenu]);
}

/// The chord clears both of MENU's own windows, so the press after it has to start its own.
///
/// It has to begin with a real tap, or the half of that which matters is invisible: the tap
/// leaves a double tap window open, the chord lands inside it, and a chord that does not
/// close that window leaves the very next MENU tap opening the switcher instead of the quick
/// menu — a press whose meaning depends on a chord two presses ago.
#[test]
fn the_menu_button_still_works_after_a_chord() {
    let mut g = Gestures::new();
    g.feed(Down(Menu), 0);
    assert_eq!(g.feed(Up(Menu), 100), vec![QuickMenu]);
    g.feed(Down(Select), 150);
    assert_eq!(g.feed(Down(Menu), 200), vec![GameMenu]);
    assert!(g.feed(Up(Menu), 250).is_empty());
    assert_eq!(g.feed(Up(Select), 260), vec![GbaUp(Select)]);
    assert!(
        g.feed(Down(Menu), 400).is_empty(),
        "the tap before the chord was still standing as half of a double tap"
    );
    assert_eq!(
        g.feed(Up(Menu), 450),
        vec![QuickMenu],
        "the chord left the menu button dead"
    );
}

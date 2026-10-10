mod common;

use std::path::{Path, PathBuf};

use common::{session_with_platform, tmp_root_with_real_carts};
use slot::prefetch::{Prefetch, PREFETCH_REST_MS};
use slot_input::{Btn, Millis, RawEvent};

fn rom(dir: &Path, name: &str, len: usize) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, vec![7u8; len]).unwrap();
    p
}

#[test]
fn a_cart_is_read_once_it_has_been_rested_on() {
    let d = tempfile::tempdir().unwrap();
    let a = rom(d.path(), "a.gba", 3 << 20);
    let mut p = Prefetch::default();
    p.browse(Some(&a), 1_000);
    p.browse(Some(&a), 1_000 + PREFETCH_REST_MS - 1);
    assert_eq!(p.reading(), None, "read before the cart was rested on");
    p.browse(Some(&a), 1_000 + PREFETCH_REST_MS);
    assert_eq!(p.reading(), Some(a.as_path()));
    assert_eq!(p.wait(), Some(3 << 20), "the whole rom was not read");
}

#[test]
fn scrolling_past_carts_reads_none_of_them() {
    let d = tempfile::tempdir().unwrap();
    let carts: Vec<_> = (0..4)
        .map(|i| rom(d.path(), &format!("{i}.gba"), 1 << 20))
        .collect();
    let mut p = Prefetch::default();
    for (i, c) in carts.iter().enumerate() {
        p.browse(Some(c), i as u64 * (PREFETCH_REST_MS / 2));
        assert_eq!(
            p.reading(),
            None,
            "read {} while scrolling past it",
            c.display()
        );
    }
}

#[test]
fn a_cart_is_not_read_again_while_it_stays_selected() {
    let d = tempfile::tempdir().unwrap();
    let a = rom(d.path(), "a.gba", 1 << 20);
    let mut p = Prefetch::default();
    p.browse(Some(&a), 0);
    p.browse(Some(&a), PREFETCH_REST_MS);
    assert!(p.wait().is_some());
    p.browse(Some(&a), 10 * PREFETCH_REST_MS);
    assert_eq!(p.wait(), None, "the same cart was read twice");
}

#[test]
fn leaving_the_shelf_and_coming_back_waits_for_a_fresh_rest() {
    let d = tempfile::tempdir().unwrap();
    let a = rom(d.path(), "a.gba", 1 << 20);
    let b = rom(d.path(), "b.gba", 1 << 20);
    let mut p = Prefetch::default();
    p.browse(Some(&a), 0);
    p.browse(None, PREFETCH_REST_MS / 2);
    p.browse(Some(&b), PREFETCH_REST_MS);
    p.browse(Some(&b), PREFETCH_REST_MS + PREFETCH_REST_MS / 2);
    assert_eq!(p.reading(), None);
    p.browse(Some(&b), 2 * PREFETCH_REST_MS);
    assert_eq!(p.reading(), Some(b.as_path()));
}

#[test]
fn resting_on_the_shelf_reads_the_selected_cart_and_inserting_lets_it_go() {
    let d = tmp_root_with_real_carts(&["Advance Wars", "Emerald"]);
    let (mut s, _) = session_with_platform(d.path());
    let mut now: Millis = 0;
    let mut step = |s: &mut slot::session::Session, now: &mut Millis, evs: Vec<RawEvent>| {
        *now += 16;
        s.feed(evs, *now);
        s.update(1.0 / 60.0);
    };
    let start = s.app().now();
    while s.app().now() < start + PREFETCH_REST_MS + 32 {
        step(&mut s, &mut now, vec![]);
    }
    let selected = s.app().browsed_rom().expect("no cart on the shelf").to_path_buf();
    assert_eq!(s.prefetching(), Some(selected.as_path()));

    step(&mut s, &mut now, vec![RawEvent::Down(Btn::A)]);
    step(&mut s, &mut now, vec![RawEvent::Up(Btn::A)]);
    step(&mut s, &mut now, vec![]);
    assert_eq!(s.prefetching(), None, "kept reading once the cart went in");
}

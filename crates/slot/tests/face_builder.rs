use std::time::{Duration, Instant};

use slot::face_builder::{BuiltFaces, FaceBuilder};
use slot_store::Cart;

fn cart(stem: &str) -> Cart {
    Cart {
        stem: stem.into(),
        rom: format!("Games/{stem}.gba").into(),
        label: None,
        title: stem.to_uppercase(),
        code: String::new(),
    }
}

/// Everything the worker sends back within a few seconds, in order.
fn collect(builder: &FaceBuilder, want: usize) -> Vec<BuiltFaces> {
    let mut got = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while got.len() < want && Instant::now() < deadline {
        if let Some(faces) = builder.take() {
            got.push(faces);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    got
}

#[test]
fn a_request_comes_back_as_the_open_carts_faces() {
    let builder = FaceBuilder::spawn();
    builder.request(cart("Metroid Fusion"));
    let got = collect(&builder, 1);
    assert_eq!(got.len(), 1, "the worker never answered");
    assert_eq!(got[0].stem, "Metroid Fusion");
    assert_eq!((got[0].board.w, got[0].board.h), (372, 209));
    assert_eq!((got[0].lid.w, got[0].lid.h), (244, 139));
}

/// A caret that ran along the shelf has no use for the carts it passed: the last one asked for
/// is the last one built.
#[test]
fn the_newest_request_of_a_burst_is_the_last_built() {
    let builder = FaceBuilder::spawn();
    for stem in ["Advance Wars", "Drill Dozer", "Metroid Fusion"] {
        builder.request(cart(stem));
    }
    let mut last = None;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Some(faces) = builder.take() {
            let done = faces.stem == "Metroid Fusion";
            last = Some(faces.stem);
            if done {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(last.as_deref(), Some("Metroid Fusion"));
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        builder.take().is_none(),
        "a build older than the newest came back after it"
    );
}

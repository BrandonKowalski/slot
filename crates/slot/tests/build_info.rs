use slot::build_info::Build;

fn sample() -> Build {
    Build {
        version: "0.1.0",
        hash: "9e11a10c",
        date: "2026-08-12",
    }
}

#[test]
fn the_serial_is_upper_case_hex() {
    assert_eq!(sample().serial(), "9E11A10C");
    assert!(sample().serial().chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn a_build_with_no_git_still_has_a_serial() {
    let unknown = Build {
        hash: "unknown",
        date: "unknown",
        ..sample()
    };
    assert!(!unknown.serial().is_empty());
    let payload = format!("*SLOT-{}*", unknown.serial());
    assert!(slot_ui::code39(&payload).is_some());
}

#[test]
fn the_current_build_is_populated() {
    let b = Build::current();
    assert!(!b.version.is_empty());
    assert!(!b.hash.is_empty());
    assert!(!b.date.is_empty());
}

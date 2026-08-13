use slot::build_info::Build;

fn sample() -> Build {
    Build {
        version: "0.1.0",
        hash: "9e11a10",
        dirty: false,
        date: "2026-08-12",
    }
}

/// The serial is what the barcode encodes, and Code 39 has no lower case. A hash that fell
/// outside the alphabet would refuse to encode and the label would draw no bars at all.
#[test]
fn the_serial_is_upper_case_hex() {
    assert_eq!(sample().serial(), "9E11A10");
    assert!(sample().serial().chars().all(|c| c.is_ascii_hexdigit()));
}

/// The boxed digit beside the bars is the dirty marker, which is the only place a build from
/// a modified tree owns up to it.
#[test]
fn the_boxed_digit_says_whether_the_tree_was_dirty() {
    let dirty = Build {
        dirty: true,
        ..sample()
    };
    assert_eq!(sample().dirty_digit(), '0');
    assert_eq!(dirty.dirty_digit(), '1');
}

/// Every probe fails soft. The device build runs in a container as root against a bind mount
/// owned by another uid, which is the case git refuses outright, and a frontend that would
/// not build because it could not name itself would be a poor trade.
#[test]
fn a_build_with_no_git_still_has_a_serial() {
    let unknown = Build {
        hash: "unknown",
        date: "unknown",
        ..sample()
    };
    assert!(!unknown.serial().is_empty());
    // Not encodable, which the label has to handle by drawing no bars rather than refusing to
    // draw the screen.
    assert!(slot_ui::code39(&format!("*{}*", unknown.serial())).is_none());
}

/// Whatever this build's environment gave it, it is populated rather than empty.
#[test]
fn the_current_build_is_populated() {
    let b = Build::current();
    assert!(!b.version.is_empty());
    assert!(!b.hash.is_empty());
    assert!(!b.date.is_empty());
}

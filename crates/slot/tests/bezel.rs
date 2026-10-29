use slot::bezel::{pick, BEZELS};

fn card(files: &[&str]) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join(BEZELS)).unwrap();
    for f in files {
        std::fs::write(d.path().join(BEZELS).join(f), b"").unwrap();
    }
    d
}

fn picked(d: &tempfile::TempDir, panel: (u32, u32)) -> Option<String> {
    pick(d.path(), panel).map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
}

#[test]
fn the_panel_takes_the_bezel_named_for_its_size() {
    let d = card(&["640x480.png", "720x720.png", "1024x768.png"]);
    assert_eq!(picked(&d, (640, 480)).as_deref(), Some("640x480.png"));
    assert_eq!(picked(&d, (720, 720)).as_deref(), Some("720x720.png"));
}

#[test]
fn a_panel_with_no_bezel_of_its_size_takes_the_nearest_of_its_shape() {
    let d = card(&["640x480.png", "1024x768.png", "720x720.png"]);
    assert_eq!(picked(&d, (1280, 960)).as_deref(), Some("1024x768.png"));
    assert_eq!(picked(&d, (720, 480)), None);
}

#[test]
fn files_that_are_not_sized_pngs_are_ignored() {
    let d = card(&["notes.txt", "sp.png", "640x480.jpg", "._640x480.png"]);
    assert_eq!(picked(&d, (640, 480)), None);
    assert_eq!(picked(&tempfile::tempdir().unwrap(), (640, 480)), None);
}

#[test]
fn a_bezel_suffix_names_the_scale_it_is_drawn_for() {
    use slot_store::{bezel_scale, Scale};
    assert_eq!(bezel_scale("Shell_spscreen.png"), Some(Scale::SpFrame));
    assert_eq!(bezel_scale("Shell_integer.png"), Some(Scale::Integer));
    assert_eq!(bezel_scale("Shell_FIT.PNG"), Some(Scale::Fit));
    assert_eq!(bezel_scale("Shell.png"), None);
    assert_eq!(bezel_scale("fitted.png"), None);
}

#[test]
fn a_suffixed_bezel_has_a_name_for_each_scale() {
    use slot_store::{bezel_for_scale, Scale};
    assert_eq!(
        bezel_for_scale("Shell_fit.png", Scale::Integer).as_deref(),
        Some("Shell_integer.png")
    );
    assert_eq!(
        bezel_for_scale("Shell_spscreen.png", Scale::Fit).as_deref(),
        Some("Shell_fit.png")
    );
    assert_eq!(bezel_for_scale("Shell.png", Scale::Fit), None);
}

#[test]
fn a_suffixed_bezel_is_labelled_without_its_suffix() {
    use slot_store::BezelPick;
    assert_eq!(BezelPick::File("Shell_fit.png".into()).label(), "Shell");
    assert_eq!(
        BezelPick::File("Shell_spscreen.png".into()).label(),
        "Shell"
    );
    assert_eq!(BezelPick::File("Wood.png".into()).label(), "Wood");
}

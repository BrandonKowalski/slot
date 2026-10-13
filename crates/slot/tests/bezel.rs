use slot::bezel::{install, list, pick, BEZELS};
use slot_store::Platform;

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
fn the_shipped_frames_land_once_and_a_replacement_stays() {
    let d = tempfile::tempdir().unwrap();
    install(d.path());
    let gb = d.path().join("bezels/GB/bezel_gb.png");
    let gbc = d.path().join("bezels/GBC/bezel_gbc.png");
    assert!(gb.is_file());
    assert!(gbc.is_file());
    let shipped = std::fs::read(&gb).unwrap();
    assert!(!shipped.is_empty());
    std::fs::write(&gb, b"mine").unwrap();
    std::fs::write(d.path().join("bezels/GB/extra.png"), b"png").unwrap();
    std::fs::write(d.path().join("bezels/GB/notes.txt"), b"no").unwrap();
    std::fs::write(d.path().join("bezels/GB/._hidden.png"), b"no").unwrap();
    let narrow = d.path().join("bezels 640/GB/bezel_gb_640.png");
    assert!(narrow.is_file());
    assert!(d.path().join("bezels 640/GBC/bezel_gbc_640.png").is_file());
    std::fs::write(&narrow, b"kept").unwrap();
    install(d.path());
    assert_eq!(std::fs::read(&gb).unwrap(), b"mine");
    assert_eq!(std::fs::read(&narrow).unwrap(), b"kept");
    assert_eq!(
        list(d.path(), Platform::Gb, 720),
        vec!["bezel_gb.png".to_string(), "extra.png".to_string()]
    );
    assert_eq!(
        list(d.path(), Platform::Gbc, 720),
        vec!["bezel_gbc.png".to_string()]
    );
    assert_eq!(
        list(d.path(), Platform::Gb, 640),
        vec!["bezel_gb_640.png".to_string()]
    );
    assert_eq!(
        list(d.path(), Platform::Gbc, 640),
        vec!["bezel_gbc_640.png".to_string()]
    );
    assert!(list(d.path(), Platform::Gba, 640).is_empty());
}

#[test]
fn files_that_are_not_sized_pngs_are_ignored() {
    let d = card(&["notes.txt", "sp.png", "640x480.jpg", "._640x480.png"]);
    assert_eq!(picked(&d, (640, 480)), None);
    assert_eq!(picked(&tempfile::tempdir().unwrap(), (640, 480)), None);
}

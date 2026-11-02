use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use slot_power::{
    Battery, Charge, DevicePlatform, LedState, LidPolicy, Platform, Power, SimPlatform,
};
use tempfile::TempDir;

const LCD: &str = "firmware/devicetree/base/soc@03000000/lcd0@01c0c000";

fn sysfs() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let tree = dir.path().join("firmware/devicetree/base");
    fs::create_dir_all(tree.join("aliases")).unwrap();
    fs::write(tree.join("aliases/lcd0"), b"/soc@03000000/lcd0@01c0c000\0").unwrap();
    let panel = dir.path().join(LCD);
    fs::create_dir_all(&panel).unwrap();
    fs::write(panel.join("lcd_driver_name"), b"rg34xxsp_v1\0").unwrap();
    for (name, value) in [
        ("lcd_x", 720u32),
        ("lcd_y", 480),
        ("lcd_ht", 820),
        ("lcd_vt", 536),
        ("lcd_dclk_freq", 26),
    ] {
        fs::write(panel.join(name), value.to_be_bytes()).unwrap();
    }
    let dump = dir.path().join("class/sunxi_dump");
    fs::create_dir_all(&dump).unwrap();
    fs::write(dump.join("write"), b"untouched writer\n").unwrap();
    fs::write(dump.join("dump"), b"untouched reader\n").unwrap();
    dir
}

fn platform(dir: &TempDir) -> DevicePlatform {
    DevicePlatform::probe(dir.path(), PathBuf::from("/mnt/sdcard"), Some("rgsp"))
}

fn assert_untouched(dir: &TempDir) {
    assert_eq!(
        fs::read(dir.path().join("class/sunxi_dump/write")).unwrap(),
        b"untouched writer\n"
    );
    assert_eq!(
        fs::read(dir.path().join("class/sunxi_dump/dump")).unwrap(),
        b"untouched reader\n"
    );
}

#[test]
fn the_verified_panel_is_detected_without_touching_registers() {
    let dir = sysfs();
    let mut p = platform(&dir);
    assert!(p.supports_high_refresh());
    assert!(!p.high_refresh_enabled());
    p.resume_display();
    assert_untouched(&dir);
}

#[test]
fn only_known_model_and_panel_pairs_can_control_refresh() {
    for (target, driver) in [
        ("rgsp", b"rg34xxsp_v1\0".as_slice()),
        ("rg34xxsp", b"rg34xxsp_v1\0".as_slice()),
        ("rg34xx", b"fog_fj035fhd05_v1\0".as_slice()),
    ] {
        let dir = sysfs();
        fs::write(dir.path().join(LCD).join("lcd_driver_name"), driver).unwrap();
        let p = DevicePlatform::probe(dir.path(), PathBuf::from("/mnt/sdcard"), Some(target));
        assert!(p.supports_high_refresh(), "rejected {target}");
        assert_untouched(&dir);
    }
    for (target, driver) in [
        ("rgsp", b"fog_fj035fhd05_v1\0".as_slice()),
        ("rg34xxsp", b"rg34xxsp_v2\0".as_slice()),
        ("rg34xx", b"rg34xxsp_v1\0".as_slice()),
    ] {
        let dir = sysfs();
        fs::write(dir.path().join(LCD).join("lcd_driver_name"), driver).unwrap();
        let mut p = DevicePlatform::probe(dir.path(), PathBuf::from("/mnt/sdcard"), Some(target));
        assert!(
            !p.supports_high_refresh(),
            "accepted {target} with {driver:?}"
        );
        assert!(!p.set_high_refresh(true));
        assert_untouched(&dir);
    }
}

#[test]
fn unknown_models_cannot_control_a_matching_panel() {
    for target in [None, Some("rg35xxsp"), Some("h700"), Some("RGSP"), Some("")] {
        let dir = sysfs();
        let mut p = DevicePlatform::probe(dir.path(), PathBuf::from("/mnt/sdcard"), target);
        assert!(!p.supports_high_refresh(), "accepted {target:?}");
        assert!(!p.set_high_refresh(true));
        assert!(!p.set_high_refresh(false));
        p.resume_display();
        assert_untouched(&dir);
    }
}

#[test]
fn the_shared_vendor_driver_does_not_enable_a_640_pixel_panel() {
    let dir = sysfs();
    fs::write(
        dir.path().join(LCD).join("lcd_driver_name"),
        b"fog_fj035fhd05_v1\0",
    )
    .unwrap();
    fs::write(dir.path().join(LCD).join("lcd_x"), 640u32.to_be_bytes()).unwrap();
    let mut p = DevicePlatform::probe(dir.path(), PathBuf::from("/mnt/sdcard"), Some("rg34xx"));
    assert!(!p.supports_high_refresh());
    assert!(!p.set_high_refresh(true));
    assert_untouched(&dir);
}

#[test]
fn the_panels_duplicated_alias_is_detected_without_touching_registers() {
    let dir = sysfs();
    fs::write(
        dir.path().join("firmware/devicetree/base/aliases/lcd0"),
        b"/soc@03000000/lcd0@01c0c000\0/soc@03000000/lcd0@01c0c000\0",
    )
    .unwrap();
    assert!(platform(&dir).supports_high_refresh());
    assert_untouched(&dir);
}

#[test]
fn conflicting_or_empty_alias_entries_never_enable_refresh_control() {
    for alias in [
        b"/soc@03000000/lcd0@01c0c000\0/other\0".as_slice(),
        b"/soc@03000000/lcd0@01c0c000\0\0",
        b"\0/soc@03000000/lcd0@01c0c000\0",
        b"\0",
    ] {
        let dir = sysfs();
        fs::write(
            dir.path().join("firmware/devicetree/base/aliases/lcd0"),
            alias,
        )
        .unwrap();
        let mut p = platform(&dir);
        assert!(!p.supports_high_refresh());
        assert!(!p.set_high_refresh(true));
        assert_untouched(&dir);
    }
}

#[test]
fn a_failed_readback_does_not_report_high_refresh_enabled() {
    let dir = sysfs();
    let mut p = platform(&dir);
    assert!(p.supports_high_refresh());
    assert!(!p.set_high_refresh(true));
    assert!(!p.high_refresh_enabled());
    p.resume_display();
    assert!(!p.high_refresh_enabled());
}

#[test]
fn platforms_without_refresh_control_report_it_disabled() {
    let platform = SimPlatform::at(PathBuf::from("sdcard"));
    let mut p = Power::new(Box::new(platform), Duration::from_secs(60));
    assert!(!p.high_refresh_enabled());
    assert!(!p.set_high_refresh(true));
    assert!(!p.high_refresh_enabled());
}

#[test]
fn other_panel_names_never_write_registers() {
    for name in [
        b"rg34xxsp\0".as_slice(),
        b"rg34xxsp_v2\0",
        b"rg34xxsp_v1-other\0",
        b"rg34xxsp_v1",
    ] {
        let dir = sysfs();
        fs::write(dir.path().join(LCD).join("lcd_driver_name"), name).unwrap();
        let mut p = platform(&dir);
        assert!(!p.supports_high_refresh(), "accepted {name:?}");
        assert!(!p.high_refresh_enabled());
        assert!(!p.set_high_refresh(true));
        assert!(!p.set_high_refresh(false));
        p.resume_display();
        assert_untouched(&dir);
    }
}

#[test]
fn different_or_missing_timings_disable_refresh_control() {
    for name in ["lcd_x", "lcd_y", "lcd_ht", "lcd_vt", "lcd_dclk_freq"] {
        let dir = sysfs();
        let property = dir.path().join(LCD).join(name);
        fs::write(&property, 1u32.to_be_bytes()).unwrap();
        let mut p = platform(&dir);
        assert!(!p.supports_high_refresh(), "accepted different {name}");
        assert!(!p.set_high_refresh(true));
        assert_untouched(&dir);
        fs::remove_file(property).unwrap();
        assert!(
            !platform(&dir).supports_high_refresh(),
            "accepted missing {name}"
        );
    }
}

#[test]
fn the_alias_must_point_inside_the_device_tree() {
    for alias in [
        b"/../outside\0".as_slice(),
        b"soc@03000000/lcd0@01c0c000\0",
        b"/absent\0",
    ] {
        let dir = sysfs();
        fs::write(
            dir.path().join("firmware/devicetree/base/aliases/lcd0"),
            alias,
        )
        .unwrap();
        let mut p = platform(&dir);
        assert!(!p.supports_high_refresh());
        assert!(!p.set_high_refresh(true));
        assert_untouched(&dir);
    }
}

#[test]
fn absent_or_read_only_register_interfaces_disable_the_control() {
    for name in ["write", "dump"] {
        let dir = sysfs();
        let node = dir.path().join("class/sunxi_dump").join(name);
        fs::set_permissions(&node, fs::Permissions::from_mode(0o444)).unwrap();
        assert!(!platform(&dir).supports_high_refresh());
        assert_untouched(&dir);
        fs::remove_file(node).unwrap();
        assert!(!platform(&dir).supports_high_refresh());
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Event {
    Backlight(u8),
    Refresh(bool),
    Resume,
}

struct Panel {
    events: Arc<Mutex<Vec<Event>>>,
    high: bool,
}

impl Platform for Panel {
    fn set_backlight(&mut self, step: u8) {
        self.events.lock().unwrap().push(Event::Backlight(step));
    }
    fn supports_high_refresh(&self) -> bool {
        true
    }

    fn set_high_refresh(&mut self, high: bool) -> bool {
        self.events.lock().unwrap().push(Event::Refresh(high));
        self.high = high;
        true
    }

    fn high_refresh_enabled(&self) -> bool {
        self.high
    }

    fn resume_display(&mut self) {
        self.events.lock().unwrap().push(Event::Resume);
    }

    fn battery(&self) -> Option<Battery> {
        None
    }

    fn charge(&self) -> Charge {
        Charge::Unknown
    }

    fn set_led(&mut self, _: LedState) {}

    fn poweroff(&mut self) -> ! {
        panic!("test platform")
    }

    fn restart(&mut self) -> ! {
        panic!("test platform")
    }

    fn root(&self) -> &Path {
        Path::new("sdcard")
    }

    fn now(&self) -> i64 {
        0
    }

    fn set_clock(&mut self, _: i64) {}

    fn set_rumble(&mut self, _: u16) {}
}

#[test]
fn power_resumes_the_display_before_restoring_its_backlight() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let panel = Panel {
        events: events.clone(),
        high: false,
    };
    let mut p = Power::new(Box::new(panel), Duration::from_secs(60));
    assert!(p.supports_high_refresh());
    assert!(!p.high_refresh_enabled());
    assert!(p.set_high_refresh(true));
    assert!(p.high_refresh_enabled());
    p.set_backlight(5);
    p.on_close();
    p.on_open();
    assert_eq!(
        *events.lock().unwrap(),
        [
            Event::Refresh(true),
            Event::Backlight(5),
            Event::Backlight(0),
            Event::Resume,
            Event::Backlight(5),
        ]
    );
}

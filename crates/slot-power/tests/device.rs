use std::fs;
use std::path::{Path, PathBuf};

use slot_power::{motor_change, rumble_node, Battery, Charge, DevicePlatform, LedState, Platform};
use tempfile::TempDir;

fn sysfs_with(max_brightness: &str, capacity: &str, status: Option<&str>) -> TempDir {
    let d = tempfile::tempdir().unwrap();
    let bl = d.path().join("class/backlight/backlight-lcd0");
    fs::create_dir_all(&bl).unwrap();
    fs::write(bl.join("max_brightness"), format!("{max_brightness}\n")).unwrap();
    fs::write(bl.join("brightness"), "5\n").unwrap();
    let supply = d.path().join("class/power_supply");
    fs::create_dir_all(supply.join("axp2202-usb")).unwrap();
    fs::write(supply.join("axp2202-usb/type"), "USB\n").unwrap();
    fs::create_dir_all(supply.join("axp2202-battery")).unwrap();
    fs::write(supply.join("axp2202-battery/type"), "Battery\n").unwrap();
    fs::write(
        supply.join("axp2202-battery/capacity"),
        format!("{capacity}\n"),
    )
    .unwrap();
    if let Some(s) = status {
        fs::write(supply.join("axp2202-battery/status"), format!("{s}\n")).unwrap();
    }
    d
}

fn sysfs(max_brightness: &str, capacity: &str) -> TempDir {
    sysfs_with(max_brightness, capacity, Some("Discharging"))
}

fn charge_of(status: Option<&str>) -> Charge {
    platform(&sysfs_with("255", "87", status)).charge()
}

fn brightness(d: &TempDir) -> u32 {
    let text = fs::read_to_string(
        d.path()
            .join("class/backlight/backlight-lcd0")
            .join("brightness"),
    )
    .unwrap();
    text.trim().parse().unwrap()
}

fn platform(d: &TempDir) -> DevicePlatform {
    DevicePlatform::probe(d.path(), PathBuf::from("/mnt/sdcard"))
}

#[test]
fn the_backlight_steps_span_whatever_range_the_kernel_reports() {
    let d = sysfs("255", "87");
    let mut p = platform(&d);
    p.set_backlight(9);
    assert_eq!(brightness(&d), 255);
    p.set_backlight(5);
    let mid = brightness(&d);
    assert!((110..=160).contains(&mid), "step 5 of 9 wrote {mid} of 255");
}

#[test]
fn step_zero_is_dark() {
    let d = sysfs("255", "87");
    let mut p = platform(&d);
    p.set_backlight(0);
    assert_eq!(brightness(&d), 0);
}

#[test]
fn the_battery_is_found_past_the_supplies_that_are_not_one() {
    let d = sysfs("255", "87");
    assert_eq!(platform(&d).battery().map(|b| b.percent), Some(87));
}

#[test]
fn a_gauge_that_reads_as_nonsense_is_no_reading_rather_than_zero() {
    let d = sysfs("255", "not a number");
    assert!(platform(&d).battery().is_none());
}

#[test]
fn a_tree_with_neither_a_panel_nor_a_gauge_still_boots() {
    let d = tempfile::tempdir().unwrap();
    let mut p = platform(&d);
    p.set_backlight(9);
    assert!(p.battery().is_none());
}

#[test]
fn the_three_states_the_kernel_names_are_taken_at_face_value() {
    assert_eq!(charge_of(Some("Charging")), Charge::Charging);
    assert_eq!(charge_of(Some("Discharging")), Charge::Discharging);
    assert_eq!(charge_of(Some("Full")), Charge::Full);
}

#[test]
fn everything_else_is_unknown_rather_than_a_guess() {
    assert_eq!(charge_of(Some("Not charging")), Charge::Unknown);
    assert_eq!(charge_of(Some("Unknown")), Charge::Unknown);
    assert_eq!(charge_of(Some("")), Charge::Unknown);
    assert_eq!(charge_of(None), Charge::Unknown);
}

#[test]
fn the_gauge_and_the_charge_state_come_back_together() {
    let d = sysfs_with("255", "87", Some("Charging"));
    assert_eq!(
        platform(&d).battery(),
        Some(Battery {
            percent: 87,
            charge: Charge::Charging,
        })
    );
}

#[test]
fn a_tree_with_no_gauge_reads_as_unknown_rather_than_absent() {
    let d = tempfile::tempdir().unwrap();
    let p = platform(&d);
    assert_eq!(p.battery(), None);
    assert_eq!(p.charge(), Charge::Unknown);
}

#[test]
fn the_probe_reports_what_it_found() {
    let report = platform(&sysfs("255", "87")).report();
    assert!(report.contains("backlight-lcd0"), "{report}");
    assert!(report.contains("255"), "the range was left out: {report}");
    assert!(report.contains("axp2202-battery"), "{report}");
}

#[test]
fn a_probe_that_found_nothing_says_so_rather_than_saying_nothing() {
    let d = tempfile::tempdir().unwrap();
    let report = DevicePlatform::probe(d.path(), PathBuf::from("/mnt/sdcard")).report();
    assert!(report.contains("no backlight"), "{report}");
    assert!(report.contains("no battery"), "{report}");
    assert!(report.contains("no rtc"), "{report}");
    assert!(report.contains("no motor"), "{report}");
}

#[test]
fn the_report_says_whether_there_is_an_rtc() {
    let d = sysfs("255", "87");
    fs::create_dir_all(d.path().join("class/rtc/rtc0")).unwrap();
    assert!(platform(&d).report().contains("rtc0"));
}

fn dispdbg_tree() -> TempDir {
    let d = tempfile::tempdir().unwrap();
    let dbg = d.path().join("kernel/debug/dispdbg");
    fs::create_dir_all(&dbg).unwrap();
    for f in ["name", "command", "param", "start"] {
        fs::write(dbg.join(f), "").unwrap();
    }
    d
}

fn dispdbg(d: &TempDir, file: &str) -> String {
    fs::read_to_string(d.path().join("kernel/debug/dispdbg").join(file))
        .unwrap()
        .trim()
        .to_string()
}

#[test]
fn a_tree_with_no_backlight_class_drives_the_panel_through_dispdbg() {
    let d = dispdbg_tree();
    let mut p = DevicePlatform::probe(d.path(), PathBuf::from("/mnt/sdcard"));
    p.set_backlight(9);
    assert_eq!(dispdbg(&d, "name"), "lcd0");
    assert_eq!(dispdbg(&d, "command"), "setbl");
    assert_eq!(
        dispdbg(&d, "param"),
        "255",
        "top step is not full brightness"
    );
    assert_eq!(dispdbg(&d, "start"), "1", "the write was never committed");
    p.set_backlight(0);
    assert_eq!(dispdbg(&d, "param"), "0", "step zero has to be dark");
}

#[test]
fn a_backlight_class_still_wins_over_dispdbg() {
    let d = sysfs("255", "87");
    let dbg = d.path().join("kernel/debug/dispdbg");
    fs::create_dir_all(&dbg).unwrap();
    fs::write(dbg.join("param"), "").unwrap();
    platform(&d).set_backlight(9);
    assert_eq!(brightness(&d), 255);
    assert_eq!(dispdbg(&d, "param"), "", "dispdbg was written to as well");
}

#[test]
fn the_motor_is_found_by_capability_rather_than_by_position() {
    let d = tempfile::tempdir().unwrap();
    for (node, ff) in [("event0", "0"), ("event1", "107030000 0"), ("event2", "0")] {
        let caps = d
            .path()
            .join("class/input")
            .join(node)
            .join("device/capabilities");
        fs::create_dir_all(&caps).unwrap();
        fs::write(caps.join("ff"), format!("{ff}\n")).unwrap();
    }
    assert_eq!(rumble_node(d.path()).as_deref(), Some("event1"));
}

#[test]
fn a_tree_with_no_force_feedback_has_no_motor() {
    let d = tempfile::tempdir().unwrap();
    let caps = d.path().join("class/input/event0/device/capabilities");
    fs::create_dir_all(&caps).unwrap();
    fs::write(caps.join("ff"), "20000 0\n").unwrap();
    assert!(rumble_node(d.path()).is_none());
}

#[test]
fn only_the_edge_between_still_and_moving_reaches_the_motor() {
    assert_eq!(motor_change(0, false), None);
    assert_eq!(motor_change(1, false), Some(true));
    assert_eq!(motor_change(65_535, true), None);
    assert_eq!(motor_change(1, true), None);
    assert_eq!(motor_change(0, true), Some(false));
}

fn leds(d: &TempDir, name: &str, attrs: &[(&str, &str)]) {
    let led = d.path().join("class/leds").join(name);
    fs::create_dir_all(&led).unwrap();
    for (k, v) in attrs {
        fs::write(led.join(k), format!("{v}\n")).unwrap();
    }
}

fn read(d: &TempDir, path: &str) -> String {
    fs::read_to_string(d.path().join("class/leds").join(path))
        .unwrap()
        .trim()
        .to_string()
}

#[test]
fn a_multicolour_led_gets_a_colour_per_state() {
    let d = sysfs("255", "87");
    leds(
        &d,
        "power",
        &[("multi_intensity", "0 0 0"), ("brightness", "0")],
    );
    let mut p = platform(&d);
    p.set_led(LedState::Charging);
    let amber = read(&d, "power/multi_intensity");
    p.set_led(LedState::Low);
    let red = read(&d, "power/multi_intensity");
    p.set_led(LedState::Running);
    let green = read(&d, "power/multi_intensity");
    assert_ne!(amber, red);
    assert_ne!(red, green);
    assert_ne!(amber, green);
}

#[test]
fn a_mono_led_is_on_except_when_the_battery_is_low() {
    let d = sysfs("255", "87");
    leds(
        &d,
        "power",
        &[("brightness", "0"), ("max_brightness", "255")],
    );
    let mut p = platform(&d);
    p.set_led(LedState::Running);
    assert_eq!(read(&d, "power/brightness"), "255");
    p.set_led(LedState::Charging);
    assert_eq!(read(&d, "power/brightness"), "255");
    p.set_led(LedState::Low);
    assert_eq!(read(&d, "power/brightness"), "0");
}

#[test]
fn a_tree_with_no_led_is_a_silent_no_op() {
    let d = sysfs("255", "87");
    let mut p = platform(&d);
    p.set_led(LedState::Charging);
    p.set_led(LedState::Low);
    assert!(!d.path().join("class/leds").exists());
}

#[test]
fn relinking_adb_rebinds_the_gadget_to_the_controller() {
    let d = tempfile::tempdir().unwrap();
    let udc = d.path().join("class/udc/5100000.udc-controller");
    fs::create_dir_all(&udc).unwrap();
    let gadget = d.path().join("kernel/config/usb_gadget/g1");
    fs::create_dir_all(&gadget).unwrap();
    fs::write(gadget.join("UDC"), "\n").unwrap();

    let mut p = DevicePlatform::probe(d.path(), PathBuf::from("/mnt/sdcard"));
    assert!(
        p.relink_adb(),
        "the gadget and a controller were both there"
    );
    assert_eq!(
        fs::read_to_string(gadget.join("UDC")).unwrap().trim(),
        "5100000.udc-controller"
    );
}

#[test]
fn relinking_adb_without_a_gadget_does_nothing() {
    let d = tempfile::tempdir().unwrap();
    fs::create_dir_all(d.path().join("class/udc/5100000.udc-controller")).unwrap();
    let mut p = DevicePlatform::probe(d.path(), PathBuf::from("/mnt/sdcard"));
    assert!(!p.relink_adb());
}

fn with_jack(d: &TempDir, level: &str) {
    let dir = d.path().join("kernel/debug");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("gpio"),
        format!(
            "gpiochip0: GPIOs 0-31, parent: platform/pio, pio:\n gpio-259 (                    |Headphone detection ) in  {level}    \n gpio-260 (                    |usb_id_det          ) in  lo    \n"
        ),
    )
    .unwrap();
}

#[test]
fn headphones_follow_the_detection_pin() {
    let d = sysfs("255", "87");
    assert!(!platform(&d).headphones(), "no gpio file reads as speaker");
    with_jack(&d, "hi");
    assert!(platform(&d).headphones());
    with_jack(&d, "lo");
    assert!(!platform(&d).headphones());
}

const MOUNTS: &str = "\
/dev/mmcblk0p5 / ext4 rw,noatime 0 0
/dev/mmcblk0p7 /mnt/sdcard vfat ro,noatime,fmask=0022,errors=remount-ro 0 0
tmpfs /mnt/sdcard/System/slot tmpfs rw,relatime 0 0
/dev/mmcblk1p1 /mnt/other exfat rw,noatime,errors=remount-ro 0 0
";

#[test]
fn a_card_the_kernel_remounted_read_only_reads_as_read_only() {
    assert!(slot_power::read_only_in(MOUNTS, Path::new("/mnt/sdcard")));
}

#[test]
fn the_closest_mount_decides_not_the_root() {
    assert!(!slot_power::read_only_in(MOUNTS, Path::new("/mnt/other")));
    assert!(!slot_power::read_only_in(
        MOUNTS,
        Path::new("/mnt/sdcard/System/slot")
    ));
}

#[test]
fn an_option_that_merely_mentions_ro_is_not_read_only() {
    let m = "/dev/x /mnt/card vfat rw,errors=remount-ro 0 0\n";
    assert!(!slot_power::read_only_in(m, Path::new("/mnt/card")));
}

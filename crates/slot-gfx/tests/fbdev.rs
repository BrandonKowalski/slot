use slot_gfx::{egl_error, panel_mode, panel_size};

#[test]
fn the_panel_size_is_read_from_the_framebuffer_rather_than_assumed() {
    assert_eq!(panel_size("640,480\n"), Some((640, 480)));
    assert_eq!(panel_size("720,1280"), Some((720, 1280)));
}

/// A zero sized window surface is an EGL error at best and a black panel at worst, so
/// anything the framebuffer will not answer for has to read as no answer.
#[test]
fn a_framebuffer_that_reports_nothing_usable_has_no_size() {
    assert_eq!(panel_size(""), None);
    assert_eq!(panel_size("640"), None);
    assert_eq!(panel_size("0,480"), None);
    assert_eq!(panel_size("wide,tall"), None);
}

/// EGL answers a plain refusal by returning false with EGL_SUCCESS still queued. Printing
/// the code then says "error 0x3000", which is the success constant, and bring-up over SSH
/// goes looking for a fault that never happened.
#[test]
fn a_refusal_egl_never_flagged_does_not_print_as_an_error_code() {
    let quiet = egl_error("eglChooseConfig", 0x3000).to_string();
    assert!(!quiet.contains("0x3000"), "{quiet}");
    assert!(quiet.contains("eglChooseConfig"), "{quiet}");

    let flagged = egl_error("eglInitialize", 0x3001).to_string();
    assert!(flagged.contains("0x3001"), "{flagged}");
}

/// `/sys/class/graphics/fb0/modes`, printed as `<name>:<w>x<h><p|i>-<hz>`. This is the panel;
/// `virtual_size` is the scrollback the driver allocated, which on a double buffered device
/// is two screens tall and describes no panel that exists.
#[test]
fn the_visible_mode_is_read_rather_than_the_virtual_framebuffer() {
    assert_eq!(panel_mode("U:720x480p-59\n"), Some((720, 480)));
    assert_eq!(panel_mode("D:640x480i-60"), Some((640, 480)));
    // A driver that lists what it supports puts the one in use first.
    assert_eq!(
        panel_mode("U:720x480p-59\nU:640x480p-60\n"),
        Some((720, 480))
    );
    // No name in front of it is still a size.
    assert_eq!(panel_mode("720x480p-59"), Some((720, 480)));
}

#[test]
fn a_mode_that_says_nothing_useful_is_none_rather_than_a_guess() {
    assert_eq!(panel_mode(""), None);
    assert_eq!(panel_mode("U:"), None);
    assert_eq!(panel_mode("U:720"), None);
    assert_eq!(panel_mode("U:0x480p-60"), None);
    assert_eq!(panel_mode("U:widexta11p-60"), None);
}

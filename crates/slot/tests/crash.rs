use std::os::unix::process::ExitStatusExt;
use std::process::Command;

const CHILD: &str = "SLOT_CRASH_CHILD";

#[test]
fn a_crash_says_where_it_happened_and_still_dies_by_its_signal() {
    if std::env::var_os(CHILD).is_some() {
        slot::crash::install();
        std::thread::Builder::new()
            .name("slot-emu".into())
            .spawn(|| unsafe { std::ptr::read_volatile(8 as *const u8) })
            .unwrap()
            .join()
            .ok();
        return;
    }
    let out = Command::new(std::env::current_exe().unwrap())
        .args([
            "a_crash_says_where_it_happened_and_still_dies_by_its_signal",
            "--exact",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&out.stderr);
    let line = log
        .lines()
        .find(|l| l.starts_with("slot: crash: "))
        .unwrap_or_else(|| panic!("no crash line in the log:\n{log}"));
    assert!(
        line.contains("SIGSEGV") || line.contains("SIGBUS"),
        "{line}"
    );
    assert!(line.contains("thread slot-emu"), "{line}");
    assert!(line.contains("addr 0x8 "), "{line}");
    let exe = std::env::current_exe().unwrap();
    let name = exe.file_name().unwrap().to_string_lossy();
    assert!(
        line.contains(&*name),
        "the crash does not name the code it was in: {line}"
    );
    assert!(
        matches!(out.status.signal(), Some(11) | Some(10)),
        "the process did not die by its signal, so no core is kept: {:?}",
        out.status
    );
}

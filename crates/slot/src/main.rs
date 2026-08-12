#[cfg(not(feature = "host"))]
mod device_app;
#[cfg(feature = "host")]
mod host_app;

fn main() {
    // Build tooling needs the content layout without opening a window. Not a user setting:
    // the folders come from root::DIRS either way, so there is only one of them to diverge.
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("--init-root") {
        let Some(dir) = args.next() else {
            eprintln!("slot: --init-root needs a directory");
            std::process::exit(2);
        };
        slot::root::ensure(std::path::Path::new(&dir));
        return;
    }

    #[cfg(feature = "host")]
    host_app::run();

    #[cfg(not(feature = "host"))]
    device_app::run();
}

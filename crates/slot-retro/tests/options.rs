use slot_retro::MgbaCore;

/// A libretro core keeps its machine in dylib globals, so two live cores is not a thing.
/// These tests share one, in one process, exactly as the other core tests here do.
fn dylib() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor")
        .join(if cfg!(target_os = "macos") {
            "mgba_libretro.dylib"
        } else {
            "mgba_libretro.so"
        })
}

#[test]
fn an_unset_option_reads_back_as_absent() {
    let Ok(core) = MgbaCore::open(&dylib()) else {
        eprintln!("no core available on this host, skipping");
        return;
    };
    assert_eq!(core.option("gpsp_serial"), None);
}

#[test]
fn a_set_option_reads_back() {
    let Ok(mut core) = MgbaCore::open(&dylib()) else {
        eprintln!("no core available on this host, skipping");
        return;
    };
    core.set_option("gpsp_serial", "rfu");
    assert_eq!(core.option("gpsp_serial"), Some("rfu".to_string()));

    // Changing it must be visible, because a link session may reload a game to switch
    // serial mode and the core has to see the new value rather than the first one.
    core.set_option("gpsp_serial", "mul_poke");
    assert_eq!(core.option("gpsp_serial"), Some("mul_poke".to_string()));
}

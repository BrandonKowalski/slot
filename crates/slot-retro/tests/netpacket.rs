use std::path::PathBuf;
use std::sync::Mutex;

use slot_retro::{
    ButtonMask, LibretroCore, Link, LinkChannel, LoopbackLink, MockCore, RetroCore,
    NETPACKET_BROADCAST, NETPACKET_RELIABLE,
};

#[test]
fn the_loopback_double_returns_what_was_sent() {
    let mut link = LoopbackLink::default();
    assert_eq!(link.try_recv(), None, "a fresh link has nothing waiting");

    link.send(NETPACKET_RELIABLE, b"hello");
    link.send(NETPACKET_RELIABLE, b"again");

    assert_eq!(link.try_recv().as_deref(), Some(&b"hello"[..]));
    assert_eq!(
        link.try_recv().as_deref(),
        Some(&b"again"[..]),
        "order was not kept"
    );
    assert_eq!(link.try_recv(), None);
}

#[test]
fn flag_values_match_libretro() {
    assert_eq!(NETPACKET_RELIABLE, 1);
    assert_eq!(NETPACKET_BROADCAST, 0xFFFF);
}

// --- Link ----------------------------------------------------------------------------
//
// `Link` is where a core's serial traffic actually goes once a session is live — the
// `LibretroCore`/`Host` half of the seam is exercised in `libretro.rs`'s own `#[cfg(test)]`
// module, since that is the only place the private ABI plumbing (`environment`, the
// trampolines, `drain_link`) is reachable. What is public here is `Link` itself.

#[test]
fn link_hands_back_inbound_packets_in_the_order_they_arrived() {
    let link = Link::default();
    assert_eq!(
        link.take_inbound(),
        None,
        "a fresh link has nothing waiting"
    );

    link.push_inbound(b"first".to_vec());
    link.push_inbound(b"second".to_vec());

    assert_eq!(link.take_inbound().as_deref(), Some(&b"first"[..]));
    assert_eq!(
        link.take_inbound().as_deref(),
        Some(&b"second"[..]),
        "order was not kept"
    );
    assert_eq!(link.take_inbound(), None);
}

#[test]
fn link_hands_back_outbound_packets_in_the_order_they_were_sent() {
    let link = Link::default();
    assert_eq!(link.take_outbound(), None);

    link.push_outbound(b"a".to_vec());
    link.push_outbound(b"b".to_vec());

    assert_eq!(link.take_outbound().as_deref(), Some(&b"a"[..]));
    assert_eq!(
        link.take_outbound().as_deref(),
        Some(&b"b"[..]),
        "order was not kept"
    );
    assert_eq!(link.take_outbound(), None);
}

#[test]
fn link_is_inactive_until_something_marks_it_live() {
    let link = Link::default();
    assert!(!link.is_active(), "a fresh link is not backing a session");

    link.set_active(true);
    assert!(link.is_active());

    link.set_active(false);
    assert!(
        !link.is_active(),
        "ending the session must be observable too"
    );
}

#[test]
fn a_clone_shares_the_same_queues_as_the_original() {
    // `Link` is the same kind of handle `Rumble` is: cheap to clone, every clone the same
    // shared state. This is what lets the frontend hold one clone while the transport (or,
    // through the core, `pump_link`) holds another.
    let link = Link::default();
    let other = link.clone();

    other.push_inbound(b"shared".to_vec());

    assert_eq!(link.take_inbound().as_deref(), Some(&b"shared"[..]));
}

#[test]
fn a_core_that_never_registers_netpacket_hands_back_an_inert_link() {
    // `MockCore` never overrides `RetroCore::net`, so this exercises the trait's default —
    // the same thing `tests/rumble.rs` does for `RetroCore::rumble`'s default.
    let core: Box<dyn RetroCore> = Box::new(MockCore::new());
    let link = core.net();
    assert!(!link.is_active());
    assert_eq!(link.take_inbound(), None);
    assert_eq!(link.take_outbound(), None);
}

// --- LibretroCore::net / pump_link, against the real vendored core -------------------
//
// mGBA's libretro build carries no netpacket support at all, so neither of these tests can
// reach the ABI seam — only `LibretroCore::net`'s existence and `pump_link`'s safety with no
// registered core. The seam itself is `libretro.rs`'s job, per its own module doc above.

/// libretro cores keep their state in dylib globals, so two live cores over one dylib is not
/// a supported configuration and these tests must not overlap with each other.
static CORE_LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    CORE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn mgba() -> Option<LibretroCore> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/mgba_libretro.dylib");
    if !p.exists() {
        return None;
    }
    Some(LibretroCore::open(&p).expect("vendored core is present but would not open"))
}

/// A header and nothing else — enough for mGBA to accept the cart. What it executes does
/// not matter to either test below.
fn header_only_rom() -> PathBuf {
    let mut rom = vec![0u8; 0x8000];
    rom[0..4].copy_from_slice(&0xea00002eu32.to_le_bytes());
    rom[0xa0..0xac].copy_from_slice(b"SLOT TEST\0\0\0");
    rom[0xac..0xb0].copy_from_slice(b"SLTE");
    rom[0xb0..0xb2].copy_from_slice(b"00");
    rom[0xb2] = 0x96;
    let sum = rom[0xa0..0xbd].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    rom[0xbd] = 0u8.wrapping_sub(sum).wrapping_sub(0x19);
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("netpacket.gba");
    std::fs::write(&path, &rom).expect("write test rom");
    path
}

#[test]
fn a_libretrocores_net_handle_is_shared_across_clones() {
    let _g = lock();
    let Some(core) = mgba() else {
        eprintln!("no mgba dylib on this host, skipping");
        return;
    };

    let a = core.net();
    let b = core.net();
    a.push_inbound(b"seen from a clone".to_vec());

    assert_eq!(b.take_inbound().as_deref(), Some(&b"seen from a clone"[..]));
}

#[test]
fn pump_link_is_harmless_on_a_core_that_never_registered_netpacket() {
    let _g = lock();
    let Some(mut core) = mgba() else {
        eprintln!("no mgba dylib on this host, skipping");
        return;
    };
    core.load(&header_only_rom()).expect("load");
    core.run_frame(ButtonMask::default());

    // mGBA's libretro build never registers netpacket. This must not panic, and must not
    // manufacture a call into a core that offered nothing.
    core.pump_link();
}

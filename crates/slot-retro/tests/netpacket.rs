use slot_retro::{LinkChannel, LoopbackLink, NETPACKET_BROADCAST, NETPACKET_RELIABLE};

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

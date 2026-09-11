use slot_ui::*;

fn px(face: &CartFace, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * face.w + x) * 4) as usize;
    [
        face.rgba[at],
        face.rgba[at + 1],
        face.rgba[at + 2],
        face.rgba[at + 3],
    ]
}

#[test]
fn every_face_is_the_size_the_layout_expects() {
    let a = link_art();
    assert_eq!((a.port.w, a.port.h), (PORT_W, PORT_H));
    for plug in [&a.plug_host, &a.plug_join] {
        assert_eq!((plug.w, plug.h), (PLUG_W, PLUG_H));
    }
    assert_eq!((a.adapter.w, a.adapter.h), (ADAPTER_W, ADAPTER_H));
    for (i, (_, _, w, h)) in ARCS.iter().enumerate() {
        assert_eq!((a.arcs_right[i].w, a.arcs_right[i].h), (*w, *h));
        assert_eq!((a.arcs_left[i].w, a.arcs_left[i].h), (*w, *h));
    }
    assert_eq!((a.clicks.w, a.clicks.h), (CLICKS_W, CLICKS_H));
    assert_eq!((a.arrow_left.w, a.arrow_left.h), (ARROW_W, ARROW_H));
}

/// The ends of the cable differ the way the real ones do: the host's purple, a joiner's gray.
#[test]
fn the_host_plug_is_purple_and_the_joiners_gray() {
    let a = link_art();
    let (x, y) = (PLUG_TIP_X as u32 - 10, PLUG_H - 90);
    let host = px(&a.plug_host, x, y);
    let join = px(&a.plug_join, x, y);
    assert!(
        host[3] > 200 && join[3] > 200,
        "the housing is not where the layout puts it"
    );
    assert!(
        host[2] > host[1] + 30,
        "the host plug is not purple: {host:?}"
    );
    assert!(
        (join[0] as i32 - join[2] as i32).abs() < 12,
        "the joiner's plug is not gray: {join:?}"
    );
}

#[test]
fn the_tip_is_at_the_bottom_and_the_cable_fades_out_at_the_top() {
    let a = link_art();
    assert!(px(&a.plug_host, PLUG_TIP_X as u32, PLUG_H - 4)[3] > 200);
    assert!(px(&a.plug_host, PLUG_TIP_X as u32, 2)[3] < 40);
}

#[test]
fn the_port_opens_at_the_top_of_the_strip() {
    let a = link_art();
    let slot = px(&a.port, 360, 12);
    assert!(
        slot[0] < 0x12 && slot[3] == 255,
        "no dark port at the centre: {slot:?}"
    );
    let strip = px(&a.port, 100, 60);
    assert_eq!(&strip[..3], &[0x1a, 0x1a, 0x20]);
}

/// The label is set by code over the raster, so it has to leave light ink on the plate.
#[test]
fn the_adapter_label_carries_its_rows() {
    let a = link_art();
    let plate = |x: f32, y: f32| {
        px(
            &a.adapter,
            (ADAPTER_BASE_X + x * 1.75) as u32,
            (ADAPTER_BASE_Y + y * 1.75) as u32,
        )
    };
    let lit = (-30..30)
        .flat_map(|x| (-24..-6).map(move |y| (x as f32, y as f32)))
        .filter(|(x, y)| plate(*x, *y)[0] > 0x90)
        .count();
    assert!(
        lit > 40,
        "the plate has no label on it ({lit} light pixels)"
    );
}

#[test]
fn the_left_arcs_mirror_the_right() {
    let a = link_art();
    for i in 0..3 {
        let (r, l) = (&a.arcs_right[i], &a.arcs_left[i]);
        for y in 0..r.h {
            for x in 0..r.w {
                assert_eq!(px(r, x, y), px(l, r.w - 1 - x, y));
            }
        }
    }
}

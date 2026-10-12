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
    assert_eq!((a.port_light.w, a.port_light.h), (PORT_W, PORT_H));
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
    let shell = px(&a.port, 40, 40);
    assert_eq!(slot[3], 255, "the port bitmap is transparent: {slot:?}");
    assert_eq!(shell[3], 255, "the shell bitmap is transparent: {shell:?}");
    let luma = |p: [u8; 4]| p[0] as u32 + p[1] as u32 + p[2] as u32;
    assert!(
        luma(slot).abs_diff(luma(shell)) > 30,
        "the port mouth does not read apart from the shell: {slot:?} against {shell:?}"
    );
}

#[test]
fn the_adapter_connector_matches_the_plug_tip() {
    let a = link_art();
    for row in 0..31 {
        let plug = span(&a.plug_host, PLUG_H - 31 + row);
        let adapter = span(&a.adapter, ADAPTER_H - 31 + row);
        assert_eq!(
            plug.1 - plug.0,
            adapter.1 - adapter.0,
            "connector row {row} is a different width"
        );
        let plug_mid = (plug.0 + plug.1) as f32 / 2.0;
        let adapter_mid = (adapter.0 + adapter.1) as f32 / 2.0;
        assert!(
            (plug_mid - (PLUG_TIP_X - 0.5)).abs() < 1.0,
            "the plug tip is off centre"
        );
        assert!(
            (adapter_mid - (ADAPTER_BASE_X - 0.5)).abs() < 1.0,
            "the adapter connector is off its seating point"
        );
    }
    assert_eq!(ADAPTER_H as f32 - ADAPTER_BASE_Y, 31.0);
}

fn span(face: &CartFace, y: u32) -> (u32, u32) {
    let mut xs = (0..face.w).filter(|x| px(face, *x, y)[3] > 16);
    let first = xs.next().expect("a connector row");
    let last = xs.next_back().unwrap_or(first);
    (first, last)
}

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

use slot::bootlogo::{
    chosen, compose, decode, encode, install, size, Bmp, Outcome, CARD_LOGO, USER_LOGO,
};

const BG: [u8; 3] = [11, 14, 17];
const INK: [u8; 3] = [240, 240, 240];

fn mark() -> Bmp {
    let (width, height) = (5, 3);
    let mut rgb = vec![BG; width * height];
    rgb[width + 1] = INK;
    rgb[width + 3] = [200, 0, 0];
    Bmp { width, height, rgb }
}

fn screen(w: usize, h: usize, fill: u8) -> Vec<u8> {
    encode(&Bmp {
        width: w,
        height: h,
        rgb: vec![[fill; 3]; w * h],
    })
}

fn at(b: &Bmp, x: usize, y: usize) -> [u8; 3] {
    b.rgb[y * b.width + x]
}

#[test]
fn a_bmp_survives_encode_and_decode_with_padded_rows() {
    let m = mark();
    let back = decode(&encode(&m)).unwrap();
    assert_eq!((back.width, back.height), (5, 3));
    assert_eq!(back.rgb, m.rgb);
}

#[test]
fn the_wordmark_is_centred_on_a_panel_of_its_background() {
    let logo = compose(&mark(), (11, 7)).unwrap();
    assert_eq!((logo.width, logo.height), (11, 7));
    assert_eq!(at(&logo, 0, 0), BG);
    assert_eq!(at(&logo, 4, 3), INK);
    assert_eq!(at(&logo, 6, 3), [200, 0, 0]);
}

#[test]
fn a_portrait_panel_gets_the_wordmark_turned_a_quarter_counter_clockwise() {
    let logo = compose(&mark(), (7, 11)).unwrap();
    assert_eq!((logo.width, logo.height), (7, 11));
    assert_eq!(at(&logo, 3, 6), INK);
    assert_eq!(at(&logo, 3, 4), [200, 0, 0]);
}

#[test]
fn a_wordmark_bigger_than_the_panel_is_refused() {
    assert!(compose(&mark(), (4, 3)).is_none());
}

#[test]
fn the_logo_takes_the_size_of_the_one_already_on_the_panel() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("bootlogo.bmp"), screen(64, 48, 1)).unwrap();
    assert_eq!(
        install(&encode(&mark()), d.path()).unwrap(),
        Outcome::Installed
    );
    let logo = std::fs::read(d.path().join("bootlogo.bmp")).unwrap();
    assert_eq!(size(&logo), Some((64, 48)));
    assert_eq!(logo.len(), screen(64, 48, 1).len());
    assert_eq!(
        std::fs::read(d.path().join("bootlogo.baseos.bmp")).unwrap(),
        screen(64, 48, 1)
    );
    assert!(!d.path().join("bootlogo.tmp").exists());
    assert_eq!(install(&encode(&mark()), d.path()).unwrap(), Outcome::Same);
}

#[test]
fn a_wrong_sized_logo_is_rebuilt_at_the_size_of_the_original() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("bootlogo.baseos.bmp"), screen(64, 48, 1)).unwrap();
    std::fs::write(d.path().join("bootlogo.bmp"), screen(72, 48, 2)).unwrap();
    assert_eq!(
        install(&encode(&mark()), d.path()).unwrap(),
        Outcome::Installed
    );
    let logo = std::fs::read(d.path().join("bootlogo.bmp")).unwrap();
    assert_eq!(size(&logo), Some((64, 48)));
    assert_eq!(
        std::fs::read(d.path().join("bootlogo.baseos.bmp")).unwrap(),
        screen(64, 48, 1),
        "the original was overwritten"
    );
}

#[test]
fn a_bad_wordmark_or_no_logo_to_measure_never_touches_the_partition() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("bootlogo.bmp"), screen(64, 48, 1)).unwrap();
    assert_eq!(
        install(&encode(&mark())[..20], d.path()).unwrap(),
        Outcome::Invalid
    );
    assert_eq!(
        std::fs::read(d.path().join("bootlogo.bmp")).unwrap(),
        screen(64, 48, 1)
    );
    assert!(!d.path().join("bootlogo.baseos.bmp").exists());

    let empty = tempfile::tempdir().unwrap();
    assert_eq!(
        install(&encode(&mark()), empty.path()).unwrap(),
        Outcome::Invalid
    );
    assert!(!empty.path().join("bootlogo.bmp").exists());
}

#[test]
fn the_shipped_wordmark_fits_every_panel_baseos_supports() {
    let card = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../card/System/bootlogo.bmp"
    ))
    .unwrap();
    let m = decode(&card).unwrap();
    for panel in [(720, 480), (640, 480), (720, 720), (480, 640)] {
        assert!(compose(&m, panel).is_some(), "{panel:?}");
    }
}

fn card_with(logos: &[(&str, &[u8])]) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    for (name, bytes) in logos {
        let path = d.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    d
}

#[test]
fn a_logo_in_config_wins_over_the_shipped_one() {
    let user = screen(640, 480, 9);
    let card = card_with(&[(CARD_LOGO, &encode(&mark())), (USER_LOGO, &user)]);
    assert_eq!(chosen(card.path()), Some(user));
}

#[test]
fn the_shipped_logo_is_used_without_one_in_config() {
    let shipped = encode(&mark());
    let card = card_with(&[(CARD_LOGO, &shipped)]);
    assert_eq!(chosen(card.path()), Some(shipped));
}

#[test]
fn a_config_logo_that_is_not_a_24_bit_bmp_falls_back_to_the_shipped_one() {
    let shipped = encode(&mark());
    let card = card_with(&[(CARD_LOGO, &shipped), (USER_LOGO, b"not a bmp")]);
    assert_eq!(chosen(card.path()), Some(shipped));
}

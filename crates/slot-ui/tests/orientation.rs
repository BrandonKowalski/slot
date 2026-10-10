use std::path::Path;

const RED: [u8; 3] = [255, 0, 0];
const BLUE: [u8; 3] = [0, 0, 255];

fn exif(orientation: u16, big_endian: bool) -> Vec<u8> {
    let u16b = |v: u16| {
        if big_endian {
            v.to_be_bytes()
        } else {
            v.to_le_bytes()
        }
    };
    let u32b = |v: u32| {
        if big_endian {
            v.to_be_bytes()
        } else {
            v.to_le_bytes()
        }
    };
    let mut e = Vec::new();
    e.extend_from_slice(if big_endian { b"MM" } else { b"II" });
    e.extend_from_slice(&u16b(42));
    e.extend_from_slice(&u32b(8));
    e.extend_from_slice(&u16b(1));
    e.extend_from_slice(&u16b(0x0112));
    e.extend_from_slice(&u16b(3));
    e.extend_from_slice(&u32b(1));
    e.extend_from_slice(&u16b(orientation));
    e.extend_from_slice(&[0, 0]);
    e.extend_from_slice(&u32b(0));
    e
}

fn red_then_blue(path: &Path, exif: Option<Vec<u8>>) {
    let f = std::fs::File::create(path).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), 2, 1);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    let mut w = e.write_header().unwrap();
    if let Some(exif) = exif {
        w.write_chunk(png::chunk::ChunkType(*b"eXIf"), &exif)
            .unwrap();
    }
    w.write_image_data(&[RED, BLUE].concat()).unwrap();
}

fn shown(exif: Option<Vec<u8>>, w: u32, h: u32) -> Vec<[u8; 3]> {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("art.png");
    red_then_blue(&path, exif);
    slot_ui::cover(&path, w, h)
        .unwrap()
        .chunks_exact(4)
        .map(|p| [p[0], p[1], p[2]])
        .collect()
}

#[test]
fn art_without_exif_is_left_alone() {
    assert_eq!(shown(None, 2, 1), [RED, BLUE]);
    assert_eq!(shown(Some(exif(1, true)), 2, 1), [RED, BLUE]);
}

#[test]
fn a_rotated_180_tag_turns_the_picture_round() {
    assert_eq!(shown(Some(exif(3, true)), 2, 1), [BLUE, RED]);
    assert_eq!(shown(Some(exif(3, false)), 2, 1), [BLUE, RED]);
}

#[test]
fn a_mirrored_tag_flips_the_picture() {
    assert_eq!(shown(Some(exif(2, true)), 2, 1), [BLUE, RED]);
    assert_eq!(shown(Some(exif(4, true)), 2, 1), [RED, BLUE]);
}

#[test]
fn quarter_turn_tags_stand_the_picture_up() {
    assert_eq!(shown(Some(exif(6, true)), 1, 2), [RED, BLUE]);
    assert_eq!(shown(Some(exif(8, true)), 1, 2), [BLUE, RED]);
    assert_eq!(shown(Some(exif(5, true)), 1, 2), [RED, BLUE]);
    assert_eq!(shown(Some(exif(7, true)), 1, 2), [BLUE, RED]);
}

#[test]
fn a_damaged_exif_chunk_still_shows_the_art() {
    assert_eq!(shown(Some(b"MM\0*junk".to_vec()), 2, 1), [RED, BLUE]);
}

use slot_store::scan;
use slot_ui::{
    cart_face, clean_label, foot_y, gb_label_panel, gb_silhouette, label_colour, label_panel,
    label_text, rest_y, shell_for, silhouette, Finish, GbShell, CART_H, CART_W, GB_CART_H,
    GB_CART_W, GB_LABEL_H, GB_LABEL_W, GB_LABEL_X, GB_LABEL_Y, LABEL_H, LABEL_W, LABEL_X, LABEL_Y,
    MOUTH_H, OUT_H, OUT_W, PLATE_H,
};
use tempfile::TempDir;

fn tmp_root() -> TempDir {
    let d = tempfile::tempdir().expect("tempdir");
    for sub in ["Games", "Games/GBA", "Labels", "Saves", "States", "System"] {
        std::fs::create_dir(d.path().join(sub)).expect("create content dir");
    }
    d
}

/// A Game Boy rom long enough to carry the header fields the scan reads: the title at 0x134 and
/// the CGB flag at 0x143, which is the only thing in the file that says what colour plastic the
/// cart shipped in.
fn write_gb_rom(d: &TempDir, dir: &str, name: &str, cgb: u8) {
    let mut rom = vec![0u8; 0x150];
    rom[0x143] = cgb;
    let games = d.path().join("Games").join(dir);
    std::fs::create_dir_all(&games).expect("create games dir");
    std::fs::write(games.join(name), rom).expect("write rom");
}

fn write_rom(d: &TempDir, name: &str, title: &str) {
    let mut rom = vec![0u8; 0x100];
    rom[0xa0..0xa0 + title.len()].copy_from_slice(title.as_bytes());
    std::fs::write(d.path().join("Games/GBA").join(name), rom).expect("write rom");
}

fn write_label(d: &TempDir, name: &str, w: u32, h: u32, px: impl Fn(u32, u32) -> [u8; 3]) {
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            rgb.extend_from_slice(&px(x, y));
        }
    }
    std::fs::create_dir_all(d.path().join("Labels/GBA")).expect("create labels dir");
    let f = std::fs::File::create(d.path().join("Labels/GBA").join(name)).expect("create label");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .expect("png header")
        .write_image_data(&rgb)
        .expect("png data");
}

fn pixel(face: &slot_ui::CartFace, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * face.w + x) * 4) as usize;
    [face.rgba[i], face.rgba[i + 1], face.rgba[i + 2]]
}

/// Whether the cart is there at all at this pixel. The face is clipped to the outline, so a
/// hole cut in the shell reads as nothing rather than as a different colour.
fn pixel_alpha(face: &slot_ui::CartFace, x: u32, y: u32) -> u8 {
    face.rgba[((y * face.w + x) * 4 + 3) as usize]
}

/// The label sits inset in the shell, so a face pixel is only the label's business inside
/// this rect. Coordinates are relative to the label's own top left.
fn label_pixel(face: &slot_ui::CartFace, x: u32, y: u32) -> [u8; 3] {
    pixel(face, LABEL_X + x, LABEL_Y + y)
}

#[test]
fn a_malformed_label_falls_back_to_a_generated_one() {
    let d = tmp_root();
    write_rom(&d, "Broken.gba", "BROKEN");
    std::fs::create_dir_all(d.path().join("Labels/GBA")).unwrap();
    std::fs::write(d.path().join("Labels/GBA/Broken.png"), b"not a png").unwrap();
    let cart = &scan(d.path()).unwrap()[0];
    let face = cart_face(cart);
    assert_eq!((face.w, face.h), (CART_W, CART_H));
    assert!(face.rgba.iter().any(|b| *b != 0), "face is blank");
}

#[test]
fn label_colour_is_stable_across_calls() {
    assert_eq!(label_colour("POKEMON EMER"), label_colour("POKEMON EMER"));
    assert_ne!(label_colour("POKEMON EMER"), label_colour("ADVANCEWARS"));
}

#[test]
fn a_label_that_decodes_is_what_the_face_shows() {
    let d = tmp_root();
    write_rom(&d, "Labelled.gba", "LABELLED");
    write_label(&d, "Labelled.png", 64, 64, |_, _| [0xd0, 0x20, 0xa0]);
    let face = cart_face(&scan(d.path()).unwrap()[0]);
    assert_eq!((face.w, face.h), (CART_W, CART_H));
    for y in [0, LABEL_H / 2, LABEL_H - 1] {
        for x in [0, LABEL_W / 2, LABEL_W - 1] {
            assert_eq!(label_pixel(&face, x, y), [0xd0, 0x20, 0xa0], "at {x},{y}");
        }
    }
}

#[test]
fn a_label_of_the_wrong_aspect_is_cropped_not_squashed() {
    let d = tmp_root();
    write_rom(&d, "Tall.gba", "TALL");
    // Thirds: a squashed fit would drag red and blue into the label, a centre crop cannot.
    write_label(&d, "Tall.png", 160, 480, |_, y| match y / 160 {
        0 => [0xff, 0x00, 0x00],
        1 => [0xff, 0xff, 0xff],
        _ => [0x00, 0x00, 0xff],
    });
    let face = cart_face(&scan(d.path()).unwrap()[0]);
    for y in 0..LABEL_H {
        for x in 0..LABEL_W {
            assert_eq!(label_pixel(&face, x, y), [0xff, 0xff, 0xff], "at {x},{y}");
        }
    }
}

/// The second title is the one the landscape label made dangerous: three short words that
/// each fit the width at the largest size, three lines of which are taller than the label.
#[test]
fn a_long_title_stays_inside_the_label() {
    for stem in [
        "Supercalifragilisticexpialidocious Anniversary Edition",
        "Super Mario Kart",
    ] {
        let d = tmp_root();
        std::fs::write(d.path().join(format!("Games/GBA/{stem}.gba")), [0u8; 8]).unwrap();
        let face = cart_face(&scan(d.path()).unwrap()[0]);
        let bg = label_colour(stem);
        let margin = 5;
        for y in 0..LABEL_H {
            for x in 0..LABEL_W {
                let edge =
                    x < margin || y < margin || x >= LABEL_W - margin || y >= LABEL_H - margin;
                if edge {
                    assert_eq!(
                        label_pixel(&face, x, y),
                        bg,
                        "{stem}: text spills into the border at {x},{y}"
                    );
                }
            }
        }
        assert!(
            (0..LABEL_H * LABEL_W).any(|i| label_pixel(&face, i % LABEL_W, i / LABEL_W) != bg),
            "{stem}: no text was drawn"
        );
    }
}

#[test]
fn the_cart_box_matches_the_traced_outline() {
    let ratio = CART_W as f32 / CART_H as f32;
    assert!(
        (ratio - 1.778).abs() < 0.02,
        "aspect {ratio:.3}, the svg is being stretched"
    );
    assert_eq!(
        CART_W * 3,
        OUT_W,
        "three carts no longer span the shelf exactly"
    );
}

/// The label is wide and low: thin plastic beside it, a broad moulded grip above. A
/// uniform border reads as a frame, and a narrow label reads as a coaster.
#[test]
fn the_label_is_wide_and_sits_low() {
    let (x0, y0, x1, y1) = label_panel(CART_W, CART_H);
    let (w, h) = (CART_W as f32, CART_H as f32);
    let side = x0 as f32 / w;
    let top = y0 as f32 / h;
    let bottom = 1.0 - y1 as f32 / h;
    let width = (x1 - x0) as f32 / w;

    // Wide, not an exact number. 0.82 was the value on the day this was written, and pinning
    // it meant editing the test every time the label was nudged by a percent.
    assert!(
        width > 0.78,
        "the label is only {:.0}% of the cart wide, that reads as a panel not a label",
        width * 100.0
    );
    assert!(
        top > 0.18,
        "only {:.0}% of grip band above the label",
        top * 100.0
    );
    assert!(
        top > side * 2.0,
        "top band {top:.2} against side margin {side:.2}: that is a uniform border"
    );
    assert!(
        top > bottom * 1.6,
        "the label is not sitting low, top {top:.2} bottom {bottom:.2}"
    );
}

/// The feature that makes the outline read as a GBA cart is not corner rounding, it is the
/// grip ears: the body is narrower than its top. An earlier version of this test asserted
/// the top corners were rounder than the bottom, which described a rounded rectangle I had
/// invented rather than the cartridge that was traced.
#[test]
fn the_body_is_narrower_than_its_grip_ears() {
    let m = silhouette(CART_W, CART_H);
    let solid = |x: u32, y: u32| m[(y * CART_W + x) as usize] > 128;
    let width_at = |y: u32| (0..CART_W).filter(|&x| solid(x, y)).count();

    let ears = width_at(CART_H / 12);
    let body = width_at(CART_H / 2);
    assert!(
        ears > body,
        "top spans {ears}px and the body {body}px: the grip ears are missing"
    );
    assert!(
        ears - body >= 3,
        "the ears stick out by only {}px, which will not read at all",
        ears - body
    );
    assert!(solid(CART_W / 2, CART_H / 2), "the middle is not solid");
    assert!(
        solid(CART_W / 2, CART_H - 2),
        "the bottom edge is not straight"
    );
}

#[test]
fn cart_faces_are_clipped_to_the_silhouette() {
    let d = tmp_root();
    write_rom(&d, "Emerald.gba", "EMERALD");
    let cart = &scan(d.path()).unwrap()[0];
    let f = cart_face(cart);
    let px = |x: u32, y: u32| f.rgba[((y * f.w + x) * 4 + 3) as usize];
    assert_eq!(
        px(2, 2),
        0,
        "the rounded corner is opaque, the mask was not applied"
    );
    assert!(px(f.w / 2, f.h / 2) > 250);
}

#[test]
fn a_supplied_label_is_clipped_to_the_silhouette_too() {
    let d = tmp_root();
    write_rom(&d, "Labelled.gba", "LABELLED");
    write_label(&d, "Labelled.png", LABEL_W, LABEL_H, |_, _| {
        [0x40, 0x80, 0xc0]
    });
    let f = cart_face(&scan(d.path()).unwrap()[0]);
    let px = |x: u32, y: u32| f.rgba[((y * f.w + x) * 4 + 3) as usize];
    assert_eq!(px(2, 2), 0, "the label escaped the rounded corner");
    assert!(px(f.w / 2, f.h / 2) > 250);
}

/// Every case here is a real filename from the development card.
#[test]
fn the_label_is_the_filename_without_its_tags() {
    let cases = [
        (
            "Pokemon - Emerald Version (USA, Europe)",
            "Pokemon Emerald Version",
        ),
        (
            "Pokemon - LeafGreen Version (USA, Europe) (Rev 1)",
            "Pokemon LeafGreen Version",
        ),
        ("Shrek (USA) (Rev 6)", "Shrek"),
        ("Metroid Fusion", "Metroid Fusion"),
        ("Button Test", "Button Test"),
        ("Pokemon - Corrupt", "Pokemon Corrupt"),
        ("Some Game [!]", "Some Game"),
        ("Spaced   Out  (USA)", "Spaced Out"),
    ];
    for (stem, want) in cases {
        assert_eq!(clean_label(stem), want, "for {stem}");
    }
}

/// A hyphen inside a word is part of the word.
#[test]
fn an_unspaced_hyphen_survives() {
    assert_eq!(clean_label("Spider-Man 2 (USA)"), "Spider-Man 2");
    assert_eq!(
        clean_label("Wario Land 4 - Time Attack"),
        "Wario Land 4 Time Attack"
    );
}

#[test]
fn a_name_that_is_all_tags_falls_back_rather_than_going_blank() {
    assert_eq!(clean_label("(USA) (Rev 1)"), "(USA) (Rev 1)");
    assert_eq!(clean_label(""), "");
}

/// The twelve character header title is what this task exists to stop using.
#[test]
fn the_header_title_no_longer_reaches_the_label() {
    let d = tmp_root();
    write_rom(
        &d,
        "Pokemon - Emerald Version (USA, Europe).gba",
        "POKEMON EMER",
    );
    let cart = &scan(d.path()).unwrap()[0];
    assert_eq!(label_text(cart), "Pokemon Emerald Version");
}

#[test]
fn two_regions_of_one_game_get_the_same_generated_colour() {
    let a = label_colour(&clean_label("Pokemon - Ruby Version (USA, Europe) (Rev 2)"));
    let b = label_colour(&clean_label("Pokemon - Ruby Version (Japan)"));
    assert_eq!(a, b, "the same game came out two colours");
}

#[test]
fn a_rom_with_no_header_title_is_labelled_from_its_stem() {
    let d = tmp_root();
    std::fs::write(d.path().join("Games/GBA/Homebrew Demo.gba"), [0u8; 8]).unwrap();
    let face = cart_face(&scan(d.path()).unwrap()[0]);
    let bg = label_colour("Homebrew Demo");
    assert!(
        (0..LABEL_H * LABEL_W).any(|i| label_pixel(&face, i % LABEL_W, i / LABEL_W) != bg),
        "an untitled rom got a blank label"
    );
}

/// The published dimensions are 65.5 x 57 mm for a Game Boy Game Pak against 35 x 57 mm for a
/// GBA one, so the two are the same width and the Game Boy is taller by the ratio of the
/// heights. The rule is what is asserted rather than the 253 it comes to: a typed literal
/// would let the rule be edited away with the test still green.
#[test]
fn the_game_boy_pak_is_the_published_ratio_taller_at_the_same_width() {
    assert_eq!(
        GB_CART_W, CART_W,
        "both paks are 57 mm wide, so they share a canvas width"
    );
    let want = (CART_H as f64 * 65.5 / 35.0).round() as u32;
    assert_eq!(
        GB_CART_H, want,
        "the height is no longer CART_H scaled by 65.5/35"
    );
}

/// The GBA cart's canvas spans its grip ridge and its body is inset below that. A Game Boy Game
/// Pak has no ridge — that ridge is what physically stops a GBA cart entering a Game Boy — so
/// its sides are parallel, and at the same 57 mm they are the GBA body's own width. Both shells
/// are checked: the two differ at the top corners and nowhere else, so a pak that tapered would
/// be a mistake in whichever one it appeared in.
#[test]
fn the_game_boy_outline_has_parallel_sides_and_no_grip_ears() {
    for shell in [GbShell::Notched, GbShell::Rounded] {
        let gb = gb_silhouette(shell, GB_CART_W, GB_CART_H);
        let width_at = |y: u32| {
            (0..GB_CART_W)
                .filter(|&x| gb[(y * GB_CART_W + x) as usize] > 128)
                .count()
        };
        // Clear of the corner radii at both ends, so what is compared is the straight run.
        let (top, middle, bottom) = (
            width_at(GB_CART_H / 8),
            width_at(GB_CART_H / 2),
            width_at(GB_CART_H * 7 / 8),
        );
        assert_eq!(top, middle, "{shell:?}: wider at the top than the middle");
        assert_eq!(middle, bottom, "{shell:?}: tapers toward the bottom");

        let gba = silhouette(CART_W, CART_H);
        let gba_body = (0..CART_W)
            .filter(|&x| gba[((CART_H / 2) * CART_W + x) as usize] > 128)
            .count();
        assert!(
            middle.abs_diff(gba_body) <= 2,
            "{shell:?}: the pak's body is {middle}px against the GBA body's {gba_body}px, \
             and both are 57 mm"
        );
    }
}

/// The two shells differ in exactly two places and both are at the top. The notch is a bite out
/// of the top right corner of the class A/B shell, so at the height of the notch floor the
/// notched pak is the narrower of the two; the rounded shell's corners are cut back much
/// further than the older mould's shallow step, so along the very top row it is the narrower.
/// Below the shoulder they are the same object.
#[test]
fn the_colour_only_shell_loses_the_notch_and_rounds_the_corners() {
    let notched = gb_silhouette(GbShell::Notched, GB_CART_W, GB_CART_H);
    let rounded = gb_silhouette(GbShell::Rounded, GB_CART_W, GB_CART_H);
    fn covered(m: &[u8], y: u32) -> Vec<u32> {
        (0..GB_CART_W)
            .filter(|&x| m[(y * GB_CART_W + x) as usize] > 128)
            .collect()
    }
    let width_at = |m: &[u8], y: u32| covered(m, y).len();
    let left_at = |m: &[u8], y: u32| covered(m, y)[0];
    let right_at = |m: &[u8], y: u32| *covered(m, y).last().expect("an empty row");

    // Inside the notch, which runs about eleven rows down from the top edge. The clear shell
    // has plastic here and the notched one has a hole.
    let y = 6;
    assert!(
        right_at(&rounded, y) > right_at(&notched, y) + 10,
        "the clear pak ends at {} against the notched {} at row {y}: the notch is not cut",
        right_at(&rounded, y),
        right_at(&notched, y)
    );
    // The top left corner, where no notch confuses the reading: the rounded shell's corner is
    // cut back much further than the older mould's shallow step.
    assert!(
        left_at(&rounded, 0) > left_at(&notched, 0) + 10,
        "the top row starts at {} against the notched {}: the corners are not rounded",
        left_at(&rounded, 0),
        left_at(&notched, 0)
    );
    // Everything from the shoulder down is one shape drawn twice.
    for y in 30..GB_CART_H {
        assert_eq!(
            width_at(&notched, y),
            width_at(&rounded, y),
            "the shells disagree at row {y}, which is below the shoulder"
        );
    }
}

/// The real label is 42 x 37 mm on a 57 x 65.5 mm face: near square, against the GBA label's
/// 2.28:1 landscape. It is centred across the pak and sits **low**, under the moulded lettering
/// plate that fills the whole shoulder, with only the arrow below it. That placement is read
/// off the user's own square-on drawing, which is the authority on it — no published dimension
/// gives the offset, and the number that used to be here put the label over the plate.
#[test]
fn the_game_boy_label_well_is_near_square_and_sits_under_the_lettering_plate() {
    let (x0, y0, x1, y1) = gb_label_panel(GB_CART_W, GB_CART_H);
    assert_eq!((x1 - x0, y1 - y0), (176, 150));
    let aspect = (x1 - x0) as f32 / (y1 - y0) as f32;
    assert!(
        (aspect - 1.17).abs() < 0.02,
        "the well is {aspect:.2}:1, which is not the 42x37 label's shape"
    );
    assert_eq!(x0, GB_CART_W - x1, "the well is not centred across the pak");
    let (above, below) = (y0, GB_CART_H - y1);
    assert!(
        above > below * 2,
        "{above}px above the label and {below}px below: the shoulder has lost its plate"
    );
}

#[test]
fn a_game_boy_cart_face_is_drawn_at_the_game_boy_size() {
    let d = tmp_root();
    write_gb_rom(&d, "GB", "Tetris.gb", 0x00);
    let face = cart_face(&scan(d.path()).unwrap()[0]);
    assert_eq!((face.w, face.h), (GB_CART_W, GB_CART_H));
    assert!(face.rgba.iter().any(|b| *b != 0), "face is blank");
}

/// Every cartridge is centred on the carousel, which is what a GBA cart has always been and
/// what the user asked for the paks to be. They share a centre and not a floor: a pak stood on
/// a GBA cart's floor sat 59 px higher up the frame, top-heavy against the HUD plate with a gap
/// under it, and the user named that as the thing to fix. The floors are per platform now and
/// fall out of the centre rather than the other way round.
///
/// The consequence the user accepted in as many words is that a pak ends up closer to the slot.
/// Closer is not touching, and the numbers that say so are pinned here: a centred pak's foot is
/// at 366.5 against a lip at 422, which is 55.5 px of ground between the cartridge and the
/// machine. It also buys back headroom at the top — 113.5 against 54.5 — so the clearance the
/// old test was written to protect got better, not worse.
#[test]
fn every_cartridge_is_centred_on_the_row_and_clears_both_the_plate_and_the_slot() {
    let lip = OUT_H as f32 - MOUTH_H;
    for (name, h) in [("the GBA cart", CART_H), ("the Game Boy pak", GB_CART_H)] {
        let (top, foot) = (rest_y(h as f32), foot_y(h as f32));
        assert_eq!(
            top + foot,
            OUT_H as f32,
            "{name} stands at {top}..{foot}, which is not centred on a {OUT_H}px screen"
        );
        assert!(
            top > PLATE_H,
            "{name}'s top edge at {top} is under the {PLATE_H}px HUD plate"
        );
        assert!(
            foot < lip,
            "{name}'s foot at {foot} has reached the lip at {lip}: it is standing in the slot"
        );
    }
    // The GBA cart is where it has always been, which is the point of centring rather than
    // moving: nothing about the shelf the user already liked changed.
    assert_eq!(
        (rest_y(CART_H as f32), foot_y(CART_H as f32)),
        (172.5, 307.5)
    );
    assert_eq!(
        (rest_y(GB_CART_H as f32), foot_y(GB_CART_H as f32)),
        (113.5, 366.5)
    );
}

/// A Game Boy cart carries no four character game code, so the shell table has nothing to key
/// on. The CGB flag is what the header does say about the plastic, and it says three things,
/// not two: 0x00 is grey, 0x80 is **black**, 0xc0 is clear. slot used to draw 0x80 translucent
/// on the reasoning that a Colour-enhanced cart shipped in the same plastic as a Colour-only
/// one. It did not — a 0x80 cart runs on original hardware and was sold in a black shell — so
/// the three are pinned apart here rather than left to collapse back onto two.
#[test]
fn each_of_the_three_cgb_flags_gets_its_own_plastic() {
    let d = tmp_root();
    write_gb_rom(&d, "GB", "Tetris.gb", 0x00);
    write_gb_rom(&d, "GBC", "Colour Enhanced.gbc", 0x80);
    write_gb_rom(&d, "GBC", "Colour Only.gbc", 0xc0);
    let carts = scan(d.path()).unwrap();
    let by_stem = |stem: &str| {
        carts
            .iter()
            .find(|c| c.stem == stem)
            .unwrap_or_else(|| panic!("{stem} was not scanned"))
    };

    let grey = shell_for(by_stem("Tetris"));
    let black = shell_for(by_stem("Colour Enhanced"));
    let clear = shell_for(by_stem("Colour Only"));

    assert_eq!(grey.finish, Finish::Solid);
    assert_eq!(
        black.finish,
        Finish::Solid,
        "the 0x80 pak is black plastic, not clear"
    );
    assert_eq!(clear.finish, Finish::Translucent);

    let luma = |s: slot_ui::Shell| s.colour.iter().map(|c| *c as u32).sum::<u32>();
    assert!(
        luma(black) + 120 < luma(grey),
        "the black pak at {} is not darker than the grey one at {}",
        luma(black),
        luma(grey)
    );
    for (a, b) in [(grey, black), (black, clear), (grey, clear)] {
        assert_ne!(
            a.colour, b.colour,
            "two of the three plastics are one colour"
        );
    }
}

/// The shape follows the flag too, and not the folder. A `.gb` file is routinely Colour-only
/// and a `.gbc` file routinely DMG-compatible — the extension is a dumping convention and the
/// flag is the cart — so a misfiled rom must still be drawn as the object Nintendo made. Read
/// off the drawn face rather than the table: the notch is a hole in the top right corner, so
/// the pixel there is the shell on one cart and nothing at all on the other.
#[test]
fn the_notch_follows_the_cgb_flag_and_not_the_folder() {
    let d = tmp_root();
    write_gb_rom(&d, "GB", "Filed As Mono.gb", 0xc0);
    write_gb_rom(&d, "GBC", "Filed As Colour.gbc", 0x80);
    let carts = scan(d.path()).unwrap();
    // Inside the notch of the class A/B shell, and clear of the rounded shell's own corner:
    // twenty pixels in from the right edge, eight rows down.
    let notched_away = |stem: &str| {
        let cart = carts.iter().find(|c| c.stem == stem).expect("scanned");
        pixel_alpha(&cart_face(cart), GB_CART_W - 20, 8) < 8
    };
    assert!(
        !notched_away("Filed As Mono"),
        "a 0xc0 rom in the GB folder was drawn with a notch it never had"
    );
    assert!(
        notched_away("Filed As Colour"),
        "a 0x80 rom in the GBC folder lost the notch its shell was moulded with"
    );
}

/// The moulding follows the flag too, and the shoulder is where the two shells part company.
/// A class A/B pak wears five ribs a side running most of the way across its shoulder; a class C
/// pak's shoulder is smooth, with its ribbing left only as short ridges on the outer side edges.
/// The user named this looking at the rendered shelf — "gbc carts don't have lines on the
/// header" — and a square-on photograph of a class C shell bears it out.
///
/// Read off the drawn face, in the band the class A/B ribs occupy and far enough in from the
/// edge to clear the class C ridges. A rib is a horizontal line, so it shows as a step between
/// one row and the next; flat plastic has no steps in it at all.
#[test]
fn only_the_notched_shell_has_lines_across_its_shoulder() {
    let d = tmp_root();
    write_gb_rom(&d, "GB", "Grey.gb", 0x00);
    write_gb_rom(&d, "GB", "Black.gb", 0x80);
    write_gb_rom(&d, "GBC", "Clear.gbc", 0xc0);
    let carts = scan(d.path()).unwrap();
    // The rib band: rows 21 to 57, from past the class C ridges at x 14 to short of x 33,
    // where the lettering plate's own rounded left cap starts bulging into the shoulder.
    let steps = |stem: &str| {
        let cart = carts.iter().find(|c| c.stem == stem).expect("scanned");
        let face = cart_face(cart);
        let lum = |x: u32, y: u32| {
            let p = pixel(&face, x, y);
            p.iter().map(|c| u32::from(*c)).sum::<u32>() / 3
        };
        (16..32)
            .flat_map(|x| (21..58).map(move |y| (x, y)))
            .filter(|(x, y)| lum(*x, *y).abs_diff(lum(*x, y - 1)) > 10)
            .count()
    };
    for stem in ["Grey", "Black"] {
        assert!(
            steps(stem) > 200,
            "the {stem} pak's shoulder came up smooth: {} stepped pixels, and five ribs a \
             side should leave hundreds",
            steps(stem)
        );
    }
    assert_eq!(
        steps("Clear"),
        0,
        "the Colour pak has lines across its header, which that shell does not have"
    );
}

/// The finish has to reach the drawn pixels, not merely the table: a clear shell lightens
/// toward its rim where a solid one is one colour all the way out. Sampled beside the label,
/// where nothing but the shell is drawn.
#[test]
fn the_clear_shell_lightens_at_its_rim_and_the_plain_one_does_not() {
    let d = tmp_root();
    write_gb_rom(&d, "GB", "Tetris.gb", 0x00);
    write_gb_rom(&d, "GBC", "Colour Only.gbc", 0xc0);
    let carts = scan(d.path()).unwrap();
    let luma = |face: &slot_ui::CartFace, x: u32, y: u32| {
        let p = pixel(face, x, y);
        p[0] as u32 + p[1] as u32 + p[2] as u32
    };
    let y = GB_CART_H / 2;
    for cart in &carts {
        let face = cart_face(cart);
        let (rim, body) = (luma(&face, 8, y), luma(&face, 20, y));
        match cart.stem.as_str() {
            "Tetris" => assert_eq!(rim, body, "the plain pak has a lit rim"),
            _ => assert!(
                rim > body + 30,
                "the clear pak's rim is {rim} against {body} inside: it reads as solid"
            ),
        }
    }
}

/// The GBA label is landscape, so three lines ran out of height long before any line ran out of
/// width and one bound was enough. On a 1.17:1 panel the bounds cross, and which one binds
/// depends on the title — so a short title that fits vertically at any size must still be held
/// inside the width.
#[test]
fn a_game_boy_title_stays_inside_its_near_square_label() {
    for stem in [
        "Supercalifragilisticexpialidocious Anniversary Edition",
        "Wario Land 3",
        "Tetris",
    ] {
        let d = tmp_root();
        write_gb_rom(&d, "GB", &format!("{stem}.gb"), 0x00);
        let face = cart_face(&scan(d.path()).unwrap()[0]);
        let bg = label_colour(stem);
        let at = |x: u32, y: u32| pixel(&face, GB_LABEL_X + x, GB_LABEL_Y + y);
        let margin = 5;
        for y in 0..GB_LABEL_H {
            for x in 0..GB_LABEL_W {
                let edge = x < margin
                    || y < margin
                    || x >= GB_LABEL_W - margin
                    || y >= GB_LABEL_H - margin;
                if edge {
                    assert_eq!(
                        at(x, y),
                        bg,
                        "{stem}: text spills into the border at {x},{y}"
                    );
                }
            }
        }
        assert!(
            (0..GB_LABEL_H * GB_LABEL_W).any(|i| at(i % GB_LABEL_W, i / GB_LABEL_W) != bg),
            "{stem}: no text was drawn"
        );
    }
}

/// A side cart is dimmed by sitting a translucent face on this, not by letting the ground
/// show through it. Only the compositor can mint a `TexId`, so what reaches the screen is not
/// reachable here; the shape and the colour are.
#[test]
fn the_cart_shadow_is_the_cart_in_black() {
    let s = slot_ui::cart_shadow();
    assert_eq!((s.w, s.h), (CART_W, CART_H));
    let mask = silhouette(CART_W, CART_H);
    for (px, cover) in s.rgba.chunks_exact(4).zip(&mask) {
        assert_eq!(&px[..3], &[0, 0, 0], "the shadow is not black");
        assert_eq!(px[3], *cover, "the shadow is not the cart's shape");
    }
    assert!(
        s.rgba.chunks_exact(4).any(|p| p[3] > 250),
        "the shadow is transparent everywhere, so it backs nothing"
    );
}

/// The same black backing in the Game Boy pak's own shape. Stretching the GBA one to a taller
/// box would put a tapered shadow under a straight sided cart.
///
/// One texture per shell, exactly that shell's outline. It used to be one for both, and it had
/// to be their *intersection*: backing wider than the cart paints black over the wallpaper
/// beside it, so the only safe way to share was to meet in the middle. What that cost was 72 px
/// of a notched pak and 169 px of a rounded one left with nothing behind them, in the two corner
/// wedges where the moulds disagree — and dimmed over a light wallpaper those are not
/// invisible: the class C corner came up as a pale bite out of the cart. So they are separate,
/// and each is required to be its own shell precisely rather than merely to stay inside it.
#[test]
fn each_game_boy_shell_gets_a_black_shadow_of_its_own_exact_outline() {
    for shell in [GbShell::Notched, GbShell::Rounded] {
        let s = slot_ui::gb_cart_shadow(shell);
        assert_eq!((s.w, s.h), (GB_CART_W, GB_CART_H));
        let own = gb_silhouette(shell, GB_CART_W, GB_CART_H);
        for (i, px) in s.rgba.chunks_exact(4).enumerate() {
            assert_eq!(&px[..3], &[0, 0, 0], "{shell:?}: the shadow is not black");
            assert_eq!(
                px[3], own[i],
                "{shell:?}: the shadow is not the shell's own shape at pixel {i}"
            );
        }
    }
    // And the two really are different objects, so sharing one was a choice with a cost rather
    // than a tidy-up: this is the count of cartridge the shared intersection used to leave bare.
    let notched = gb_silhouette(GbShell::Notched, GB_CART_W, GB_CART_H);
    let rounded = gb_silhouette(GbShell::Rounded, GB_CART_W, GB_CART_H);
    let bare: u32 = notched
        .iter()
        .zip(&rounded)
        .map(|(n, r)| u32::from(n.abs_diff(*r)))
        .sum();
    assert!(
        bare / 255 > 100,
        "the two shells now differ by {} px, so the shared backing was harmless after all",
        bare / 255
    );
}

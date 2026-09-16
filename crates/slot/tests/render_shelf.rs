//! START on the shelf, composited on the GPU and read back as pixels.
//!
//! The draw list is not the screen. This crate has had list assertions pass while the panel was
//! visibly wrong, so the claim "START on a Game Boy cart leaves the plain shelf up" is settled
//! here, against the composited frame, with a GBA cart beside it proving the comparison can tell
//! the two apart at all.
//!
//! `SCRATCH_PNG_DIR=/tmp cargo test -p slot --test render_shelf -- --nocapture`

#![cfg(target_os = "macos")]

mod common;

use std::sync::{Mutex, MutexGuard, PoisonError};

use slot::app::App;
use slot_gfx::{Compositor, HeadlessSurface, TexId, OUT_H, OUT_W};
use slot_input::{Action, Btn};
use slot_store::{write_slot_state, Core, Platform, SlotState};
use slot_ui::{
    arrows_hint_face, board_face, cart_face, cart_shadow, chip_face, chip_shadow_face, clean_label,
    edge, gb_cart_shadow, hint_face, housing, label_colour, opening, padded, recess, socket_face,
    GbShell, TURN_PAD,
};
use tempfile::TempDir;

/// `gl::load_with` writes global function pointers, so two GL tests must not overlap.
static GL: Mutex<()> = Mutex::new(());

fn compositor() -> Option<(MutexGuard<'static, ()>, HeadlessSurface, Compositor)> {
    let guard = GL.lock().unwrap_or_else(PoisonError::into_inner);
    let surface = HeadlessSurface::new().ok()?;
    let compositor = Compositor::new(&surface).ok()?;
    Some((guard, surface, compositor))
}

fn tex(c: &mut Compositor, w: u32, h: u32, rgba: &[u8]) -> TexId {
    c.create_texture(w, h, rgba)
}

/// Everything the frontend uploads before this screen can draw itself, through the same
/// functions it uses: the row's cart faces and the shadow under a side cart, then every part of
/// the core picker — the sockets, the chips, the blank in flight and its shadow, the legend, and
/// the highlighted cart's own board and lid. The board and lid are built here rather than off a
/// worker, because `FaceBuilder` is only where the frontend puts the cost and not what decides
/// the picture; without them the picker opens and then waits on the shelf, which would make
/// "the screen did not change" true for entirely the wrong reason.
fn upload_faces(app: &mut App, c: &mut Compositor) {
    // `carts` walks every shelf end to end, which is the order `set_faces` hands them back out
    // in. Collected before the row is set, so the borrow of `app` is over by then.
    let row: Vec<TexId> = app
        .carts()
        .map(|cart| {
            let f = cart_face(cart);
            tex(c, f.w, f.h, &f.rgba)
        })
        .collect();
    app.set_faces(row);
    // Both outlines, as the frontend uploads both: a row of Game Boy paks with only the GBA
    // shadow to hand draws no black under a dimmed cart, and the side paks would come out as
    // ghosts over the wallpaper rather than as carts in shadow.
    let shadow = cart_shadow();
    let shadow = tex(c, shadow.w, shadow.h, &shadow.rgba);
    app.set_cart_shadow(shadow);
    // One per Game Pak mould, as the frontend uploads them: the two shells' top corners
    // disagree, and a shared backing showed through a dimmed cart where they do.
    let notched = gb_cart_shadow(GbShell::Notched);
    let notched = tex(c, notched.w, notched.h, &notched.rgba);
    let rounded = gb_cart_shadow(GbShell::Rounded);
    let rounded = tex(c, rounded.w, rounded.h, &rounded.rgba);
    app.set_gb_cart_shadows(notched, rounded);

    let sockets = Core::ALL
        .iter()
        .map(|k| {
            let f = socket_face(*k);
            tex(c, f.w, f.h, &f.rgba)
        })
        .collect();
    let chips = Core::ALL
        .iter()
        .map(|k| {
            let f = chip_face(Some(*k));
            tex(c, f.w, f.h, &f.rgba)
        })
        .collect();
    let blank = chip_face(None);
    let blank = tex(c, blank.w, blank.h, &blank.rgba);
    let chip_shadow = chip_shadow_face();
    let chip_shadow = tex(c, chip_shadow.w, chip_shadow.h, &chip_shadow.rgba);
    app.set_core_part_faces(sockets, chips, blank, chip_shadow);

    let legend = [
        hint_face("B", "Cancel"),
        arrows_hint_face("Swap"),
        hint_face("A", "Choose"),
    ]
    .into_iter()
    .map(|f| (tex(c, f.w, f.h, &f.rgba), f.w))
    .collect();
    app.set_core_legend_faces(legend);

    let highlighted = app.selected_stem().map(str::to_string);
    let Some(cart) = app
        .carts()
        .find(|c| highlighted.as_deref() == Some(c.stem.as_str()))
        .cloned()
    else {
        return;
    };
    let board = board_face(&cart);
    let board = tex(c, board.w, board.h, &board.rgba);
    let lid = padded(&cart_face(&cart), TURN_PAD);
    let lid = tex(c, lid.w, lid.h, &lid.rgba);
    app.set_core_board_faces(board, lid);
}

/// A booted app on a shelf of the given carts, with every face it draws already on the GPU.
fn shelf(d: &TempDir, c: &mut Compositor) -> App {
    write_slot_state(
        d.path(),
        &SlotState {
            clock_set: true,
            ..Default::default()
        },
    )
    .unwrap();
    let mut app = App::boot(d.path());
    upload_faces(&mut app, c);
    app
}

/// The composited frame, and a PNG of it wherever `SCRATCH_PNG_DIR` names somewhere to look.
fn shot(app: &App, c: &mut Compositor, name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    app.draw(&mut out);
    c.begin_frame();
    c.draw_list(&out);
    let px = c.read_frame();
    if let Ok(dir) = std::env::var("SCRATCH_PNG_DIR") {
        let path = format!("{dir}/shelf-{name}.png");
        let file = std::fs::File::create(&path).expect("create png");
        let mut e = png::Encoder::new(std::io::BufWriter::new(file), OUT_W, OUT_H);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header()
            .expect("png header")
            .write_image_data(&px)
            .expect("png data");
        println!("wrote {path}");
    }
    px
}

/// Long enough for a lid to be well clear of the cart: the picker's open runs in a quarter of a
/// second, so a frame this far past the press is either a board or a shelf and never halfway.
fn let_it_hop(app: &mut App) {
    app.update(0.25);
}

/// The pixels behind `start_opens_no_picker_on_a_game_boy_cart`. The unpressed twin is the same
/// card booted a second time and advanced beside the pressed one, so the two frames are the same
/// instant of the same shelf and any difference between them is the press and nothing else.
#[test]
fn start_draws_the_plain_shelf_on_a_game_boy_cart() {
    let Some((_g, _s, mut c)) = compositor() else {
        eprintln!("no GL on this host, skipping");
        return;
    };

    let d = common::tmp_root_with_gb_carts(&["Tetris", "Zzz"]);
    let twin = common::tmp_root_with_gb_carts(&["Tetris", "Zzz"]);
    let mut pressed = shelf(&d, &mut c);
    let mut untouched = shelf(&twin, &mut c);
    pressed.apply(Action::GbaDown(Btn::Start));
    let_it_hop(&mut pressed);
    let_it_hop(&mut untouched);
    let plain = shot(&untouched, &mut c, "gb-plain");
    let after = shot(&pressed, &mut c, "gb-after-start");
    assert_eq!(
        pressed.core_picker(),
        None,
        "a Game Boy cart opened the GBA picker"
    );
    assert!(
        after == plain,
        "START put something on screen over a Game Boy cart"
    );

    // The control. Without it a frame comparison that can no longer see a whole cartridge board
    // lift off the shelf would report the Game Boy half as passing.
    let d = common::tmp_root_with_carts(&["Emerald", "Zzz"]);
    let twin = common::tmp_root_with_carts(&["Emerald", "Zzz"]);
    let mut pressed = shelf(&d, &mut c);
    let mut untouched = shelf(&twin, &mut c);
    pressed.apply(Action::GbaDown(Btn::Start));
    let_it_hop(&mut pressed);
    let_it_hop(&mut untouched);
    let plain = shot(&untouched, &mut c, "gba-plain");
    let after = shot(&pressed, &mut c, "gba-after-start");
    assert_eq!(
        pressed.core_picker(),
        Some(Core::Mgba),
        "START stopped opening the picker"
    );
    assert!(
        after != plain,
        "the picker opened and the panel showed the same shelf"
    );
}

/// Which Tetris came back, read off the panel.
///
/// `cart=Tetris` names two cartridges on a card holding `Games/GBA/Tetris.gba` and
/// `Games/GB/Tetris.gb`, and `cart_platform` is the only thing that says which of them the
/// player left in the slot. That claim is about an object on screen, so it is settled here
/// rather than by asking the app what it thinks it seated: a resume that picked the wrong
/// cartridge and a resume that picked the right one are two different pictures of the machine.
///
/// The two are told apart by what they are made of rather than by where they are. A seated GBA
/// cart and a seated Game Boy pak stand in the slot at the same depth showing the same run of
/// themselves — `render_shelves.rs` pins exactly that — so a reading of *where* the cartridge is
/// cannot tell them apart at all. What differs is the plastic, charcoal against the pak's pale
/// grey, and the paper: a GBA cart's label well clears the lip and a pak's, 27.7% down a body
/// nearly twice as tall, is swallowed whole.
#[test]
fn the_card_says_which_tetris_is_in_the_slot() {
    let Some((_g, _s, mut c)) = compositor() else {
        eprintln!("no GL on this host, skipping");
        return;
    };
    // Emerald is there so the GBA shelf is not a one cart library, which boots past the shelf
    // for reasons of its own and would resume the same cartridge whatever the card said.
    let d = common::tmp_root_with_carts(&["Tetris", "Emerald"]);
    common::write_gb_cart(&d, "Tetris", "TETRIS");

    let gba = resumed_shot(&d, &mut c, Some(Platform::Gba), "resume-gba");
    let gb = resumed_shot(&d, &mut c, Some(Platform::Gb), "resume-gb");
    let unstated = resumed_shot(&d, &mut c, None, "resume-unstated");

    // The plastic. A pak is pale grey and a GBA cart charcoal, and the reading is taken half way
    // down whatever run of cartridge the slot is showing rather than at a row typed out here, so
    // it stays on the cartridge if the recess ever swallows more or less of one.
    let (top, bottom) = cartridge_rows(&gba).expect("no cartridge in the slot at all");
    assert_eq!(
        cartridge_rows(&gb),
        Some((top, bottom)),
        "the two cartridges are not seated at the same depth, so what follows would be \
         comparing different parts of them"
    );
    let row = (top + bottom) / 2;
    let (dark, pale) = (centre(&gba, row), centre(&gb, row));
    assert!(
        (0..3).all(|k| pale[k] as i32 - dark[k] as i32 > 40),
        "the card named the Game Boy shelf and the slot is holding {pale:?} where the Game Boy \
         Advance cartridge reads {dark:?}: the wrong cartridge came back"
    );

    // And the paper, which says the same thing the other way round: a GBA cart's label clears
    // the lip, and the pak's is inside the machine.
    let ink = label_colour(&clean_label("Tetris"));
    assert!(
        paper(&gba, ink) > 200,
        "the Game Boy Advance cartridge is in the slot without its label showing"
    );
    assert_eq!(
        paper(&gb, ink),
        0,
        "a pak is seated and its label well is above the lip, which no pak's is"
    );

    // A card that never said is a card from before there was anything to say, and every one of
    // those held GBA carts alone. Frame for frame the same picture, not merely a cartridge that
    // reads alike.
    assert!(
        unstated == gba,
        "a card with no cart_platform line did not resume the Game Boy Advance cartridge"
    );
}

/// The card resumed with `platform` on it, booted and composited.
fn resumed_shot(
    d: &TempDir,
    c: &mut Compositor,
    platform: Option<Platform>,
    name: &str,
) -> Vec<u8> {
    write_slot_state(
        d.path(),
        &SlotState {
            cart: Some("Tetris".into()),
            cart_platform: platform,
            clock_set: true,
            ..Default::default()
        },
    )
    .unwrap();
    let mut app = App::boot(d.path());
    upload_faces(&mut app, c);
    shot(&app, c, name)
}

/// The first and last screen rows the cartridge covers. Everything else in this frame is flat:
/// the black the compositor clears to, and the four theme colours the machine's own bands are
/// painted in. Whatever is none of those is the cartridge — the same reading
/// `render_shelves.rs::shell_rows` takes, and for the same reason: nothing here names a
/// coordinate, so the answer is wherever the cartridge turns out to be.
fn cartridge_rows(px: &[u8]) -> Option<(usize, usize)> {
    let flat = [[0.0, 0.0, 0.0, 1.0], housing(), opening(), edge(), recess()];
    let cart = |o: usize| {
        !flat
            .iter()
            .any(|f| (0..3).all(|k| px[o + k].abs_diff((f[k] * 255.0).round() as u8) <= 8))
    };
    let mut rows = (0..OUT_H as usize)
        .filter(|y| (0..OUT_W as usize).any(|x| cart((y * OUT_W as usize + x) * 4)));
    let first = rows.next()?;
    Some((first, rows.next_back().unwrap_or(first)))
}

/// The pixel half way across the screen on `row`, which is the middle of the cartridge: the slot
/// is centred and so is what is standing in it.
fn centre(px: &[u8], row: usize) -> [u8; 3] {
    let o = (row * OUT_W as usize + (OUT_W / 2) as usize) * 4;
    [px[o], px[o + 1], px[o + 2]]
}

/// How much of this cartridge's own label paper is on screen. The colour is a hash of the title,
/// so on this card it is Tetris's and nothing else in the frame wears it.
fn paper(px: &[u8], ink: [u8; 3]) -> usize {
    px.chunks(4)
        .filter(|p| (0..3).all(|k| p[k].abs_diff(ink[k]) <= 24))
        .count()
}

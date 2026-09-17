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
use slot_store::{write_slot_state, Core, SlotState};
use slot_ui::{
    arrows_hint_face, board_face, cart_face, cart_shadow, chip_face, chip_shadow_face,
    gb_cart_shadow, hint_face, padded, socket_face, GbShell, TURN_PAD,
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

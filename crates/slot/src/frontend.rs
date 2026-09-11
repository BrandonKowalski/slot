//! Everything the binary does with a compositor except own one. The window is the only
//! difference between the host and the device, so it is the only thing left above this.

use std::time::{Duration, Instant};

use slot_gfx::{Compositor, Draw, TexId, OUT_H, OUT_W};
use slot_input::{InputSource, Millis};
use slot_power::{Platform, Power};
use slot_store::format_stamp;
use slot_ui::{
    arrows_hint_face, badge_face, cart_face, cart_shadow, chip_face, chip_shadow_face, hhmm,
    hint_face, icon_face, menu_face, photo_face, set_clock_hint_face, socket_face, sticker_face,
    title_face, toast_face, wallpaper_face, word_face, Icon, LinkBadge, PowerChoice, StickerFields,
    Toast, ALERT_PX, BOLT_PX, HUD_ICON_PX, HUD_INK, LEGEND,
};

use crate::app::{App, LinkRow, Phase};
use crate::build_info::Build;
use crate::face_builder::FaceBuilder;
use crate::link_art_builder::LinkArtBuilder;
use crate::link_screen::{LinkSprites, Sprite};
use crate::link_start::{LinkFail, LinkStep};
use crate::session::Session;
use crate::wallpaper;

/// How long a dark panel waits before the machine actually stops. The dark is immediate —
/// the lid or the button kills the backlight on the edge — but the device is still running
/// flat out behind it at 400-700 mA, so this is the window in which the user might come
/// straight back, not a power saving.
///
/// Three minutes, and then the device powers off rather than sleeping. It cannot wake itself
/// from a sleep — the RTC alarm never fires on this board — so a standby would be a leak with
/// no end, and a power off is the honest version of putting it down.
const DOZE_TIMEOUT: Duration = Duration::from_secs(180);

/// Amber. The only warning colour in the tree, and the reason it is not the HUD's ink: a
/// refusal that looks like a volume glyph is a refusal nobody reads as one.
const ALERT_INK: [u8; 3] = [0xf0, 0xb4, 0x3c];

pub struct Frontend {
    session: Session,
    start: Instant,
    last: Instant,
    draws: Vec<Draw>,
    /// One texture per ring slot, reused every time the switcher opens.
    polaroid_texes: Vec<TexId>,
    /// The top plate's line of type, re-rasterised whenever the selection moves.
    title_tex: Option<TexId>,
    /// Builds the open cart's faces off the frame loop.
    faces: FaceBuilder,
    /// Builds the link screen's artwork off the frame loop, once, at boot.
    link_art: LinkArtBuilder,
    /// Whether the link art has been uploaded and handed to `App` already.
    link_art_done: bool,
    /// The cart last asked for.
    core_asked: Option<String>,
    /// The open cart and its lid, and which cart they were built for.
    core_board_tex: Option<TexId>,
    core_lid_tex: Option<TexId>,
    core_built: Option<String>,
    /// The undo cap's label, which changes with what is on offer.
    undo_tex: Option<TexId>,
    switcher: Switcher,
    clocks: Clocks,
    about: AboutFace,
}

/// The about label, and what it was last built for. The gauge is the only thing on it that
/// moves, so the reading is what decides whether it is rebuilt.
#[derive(Default)]
struct AboutFace {
    tex: Option<TexId>,
    /// `None` is a board with no gauge, which is a different thing from not having built one
    /// yet — `tex` says that.
    battery: Option<u8>,
}

/// The clock screen's two faces and the shelf's one, with what each was last built for. The
/// picker's line changes under the caret; the shelf clock changes once a minute; the battery
/// percent changes whenever the reading does.
#[derive(Default)]
struct Clocks {
    line: Option<TexId>,
    hint: Option<TexId>,
    shelf: Option<TexId>,
    picked: Option<String>,
    shown: String,
    battery: String,
    battery_tex: Option<TexId>,
}

/// What the switcher's textures were built for. The photos and the undo cap are per opening;
/// the title is per selection.
#[derive(Default)]
struct Switcher {
    open: bool,
    titled: Option<String>,
}

impl Frontend {
    pub fn boot(platform: Box<dyn Platform>) -> Self {
        let now = Instant::now();
        let mut session = Session::boot(platform.root().to_path_buf());
        session
            .app_mut()
            .set_power(Power::new(platform, DOZE_TIMEOUT));
        Frontend {
            session,
            start: now,
            last: now,
            draws: Vec::new(),
            polaroid_texes: Vec::new(),
            title_tex: None,
            faces: FaceBuilder::spawn(),
            link_art: LinkArtBuilder::spawn(),
            link_art_done: false,
            core_asked: None,
            core_board_tex: None,
            core_lid_tex: None,
            core_built: None,
            undo_tex: None,
            switcher: Switcher::default(),
            clocks: Clocks::default(),
            about: AboutFace::default(),
        }
    }

    /// Everything that never changes: the carts, the HUD glyphs and the key caps. All of it
    /// needs a live context, so it happens after the compositor and not at boot.
    pub fn upload_faces(&mut self, compositor: &mut Compositor) {
        let faces = self
            .session
            .app()
            .carts()
            .iter()
            .map(|c| {
                let f = cart_face(c);
                compositor.create_texture(f.w, f.h, &f.rgba)
            })
            .collect();
        self.session.app_mut().set_faces(faces);
        let icons = Icon::ALL
            .iter()
            .map(|i| {
                let f = icon_face(*i, HUD_ICON_PX, HUD_INK);
                compositor.create_texture(f.w, f.h, &f.rgba)
            })
            .collect();
        self.session.app_mut().set_icon_faces(icons);
        let link_badges = LinkBadge::FACES
            .iter()
            .map(|b| {
                let (badge, ink) = (
                    b.badge().expect("a face has a glyph"),
                    b.colour().expect("and a colour"),
                );
                let f = badge_face(badge, HUD_ICON_PX, ink);
                compositor.create_texture(f.w, f.h, &f.rgba)
            })
            .collect();
        self.session.app_mut().set_link_badge_faces(link_badges);
        // Its own upload rather than one of the HUD's: it is drawn on a cart, at its own
        // size, and in a warning colour the level glyphs have no business borrowing.
        let alert = icon_face(Icon::Alert, ALERT_PX, ALERT_INK);
        let alert = compositor.create_texture(alert.w, alert.h, &alert.rgba);
        self.session.app_mut().set_alert_face(alert);
        // Uploaded at boot like everything else: a shutdown is the one moment there is no
        // time to rasterise anything, and the GPU is about to be taken away. One line per
        // choice, in `PowerChoice::ALL` order, at the menu's own size so the screen that
        // follows a choice is set in the same voice as the row that was chosen.
        let lines = PowerChoice::ALL
            .iter()
            .map(|c| {
                let f = menu_face(match c {
                    PowerChoice::Restart => "Restarting",
                    PowerChoice::PowerOff => "Powering Down",
                });
                (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
            })
            .collect();
        self.session.app_mut().set_shutdown_faces(lines);
        let menu = PowerChoice::ALL
            .iter()
            .map(|c| {
                let f = menu_face(c.text());
                (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
            })
            .collect();
        self.session.app_mut().set_power_menu_faces(menu);
        // The open cart's parts that never change: each socket, the chip seated in each, the
        // blank chip in flight and its shadow, in `Core::ALL` order. At boot like the power
        // menu's rows, so the first frame of a lid coming off is not spent in a rasteriser.
        let sockets = slot_store::Core::ALL
            .iter()
            .map(|c| {
                let f = socket_face(*c);
                compositor.create_texture(f.w, f.h, &f.rgba)
            })
            .collect();
        let chips = slot_store::Core::ALL
            .iter()
            .map(|c| {
                let f = chip_face(Some(*c));
                compositor.create_texture(f.w, f.h, &f.rgba)
            })
            .collect();
        let blank = chip_face(None);
        let blank = compositor.create_texture(blank.w, blank.h, &blank.rgba);
        let shadow = chip_shadow_face();
        let shadow = compositor.create_texture(shadow.w, shadow.h, &shadow.rgba);
        self.session
            .app_mut()
            .set_core_part_faces(sockets, chips, blank, shadow);
        // Every action the picker takes, the way out first and the choice last, as the
        // switcher's legend is ordered.
        let legend = [
            hint_face("B", "Cancel"),
            arrows_hint_face("Swap"),
            hint_face("A", "Choose"),
        ]
        .into_iter()
        .map(|f| (compositor.create_texture(f.w, f.h, &f.rgba), f.w))
        .collect();
        self.session.app_mut().set_core_legend_faces(legend);
        // The in-game menu, its two link rows, and the sentences the screen says while a
        // link is coming up or after it did not. All of it at the same size and through the
        // same rasteriser as the two menus above, because they are the same object — and all
        // of it at boot, because a link that is failing is the worst moment to be asking a
        // font for a sentence.
        let roles = menu_faces(compositor, LinkRow::ALL.iter().map(|r| r.text()));
        self.session.app_mut().set_link_menu_faces(roles);
        if let Some(linked) = menu_faces(compositor, ["Linked"].into_iter()).pop() {
            self.session.app_mut().set_link_linked_face(linked);
        }
        let legend = [
            hint_face("B", "Cancel"),
            arrows_hint_face("Swap"),
            hint_face("A", "Link"),
            hint_face("A", "OK"),
        ]
        .into_iter()
        .map(|f| (compositor.create_texture(f.w, f.h, &f.rgba), f.w))
        .collect();
        self.session.app_mut().set_link_legend_faces(legend);
        let steps = menu_faces(compositor, LinkStep::ALL.iter().map(|s| s.line()));
        self.session.app_mut().set_link_step_faces(steps);
        let fails = menu_faces(compositor, LinkFail::SHOWN.iter().map(|f| f.line()));
        self.session.app_mut().set_link_fail_faces(fails);
        let toasts = Toast::ALL
            .iter()
            .map(|t| {
                let f = toast_face(*t);
                compositor.create_texture(f.w, f.h, &f.rgba)
            })
            .collect();
        self.session.app_mut().set_toast_faces(toasts);
        let legend = legend_faces(compositor, &LEGEND);
        self.session.app_mut().set_legend_faces(legend);
        let shadow = cart_shadow();
        let id = compositor.create_texture(shadow.w, shadow.h, &shadow.rgba);
        self.session.app_mut().set_cart_shadow(id);
        // `draw_gauge` now draws the bolt beside the capsule, on the housing, in its own
        // reserved slot rather than over the fill. The housing tint was only ever needed to
        // hide the bolt inside the fill it sat on; out here it sits where every other HUD
        // glyph does, so it takes the same ink they do.
        let bolt = icon_face(Icon::Charging, BOLT_PX, HUD_INK);
        let bolt_id = compositor.create_texture(bolt.w, bolt.h, &bolt.rgba);
        self.session.app_mut().set_bolt_face(bolt_id);
        self.upload_wallpaper(compositor);
    }

    /// One decode, at boot. A card with no `Wallpapers`, no readable picture in it, or a
    /// picture the decoder will not take, gets the plain ground it had before.
    fn upload_wallpaper(&mut self, compositor: &mut Compositor) {
        let app = self.session.app();
        let seed = app.wall_secs().unsigned_abs();
        let Some(rgba) = app
            .root()
            .and_then(|root| wallpaper::pick(root, seed))
            .and_then(|path| wallpaper_face(&path))
        else {
            return;
        };
        let id = compositor.create_texture(OUT_W, OUT_H, &rgba);
        self.session.app_mut().set_wallpaper(id);
    }

    /// One frame into the offscreen target and out to a surface of `window` pixels. The
    /// caller swaps: only it knows what presenting costs.
    pub fn render(&mut self, compositor: &mut Compositor, window: (u32, u32)) {
        // Set every frame rather than on the edge: the grade is part of the final blit, so
        // it has to be right whether or not anything just changed it.
        compositor.set_blue_light(self.session.app().blue_light());
        compositor.set_shake(self.session.app().screen_shake());
        compositor.set_screen_power(self.session.app().screen_power());
        compositor.begin_frame();
        if let Some(frame) = self.session.frame() {
            compositor.upload_game(&frame);
        }
        sync_clock(self.session.app_mut(), compositor, &mut self.clocks);
        sync_about(self.session.app_mut(), compositor, &mut self.about);
        sync_core_picker(
            self.session.app_mut(),
            compositor,
            &self.faces,
            &mut self.core_asked,
            &mut self.core_board_tex,
            &mut self.core_lid_tex,
            &mut self.core_built,
        );
        if !self.link_art_done {
            if let Some(art) = self.link_art.take() {
                let mut up = |f: &slot_ui::CartFace| Sprite {
                    tex: compositor.create_texture(f.w, f.h, &f.rgba),
                    w: f.w,
                    h: f.h,
                };
                let sprites = LinkSprites {
                    port: up(&art.port),
                    plug_host: up(&art.plug_host),
                    plug_join: up(&art.plug_join),
                    adapter: up(&art.adapter),
                    glow_host: up(&art.glow_host),
                    glow_neutral: up(&art.glow_neutral),
                    arcs_right: [
                        up(&art.arcs_right[0]),
                        up(&art.arcs_right[1]),
                        up(&art.arcs_right[2]),
                    ],
                    arcs_left: [
                        up(&art.arcs_left[0]),
                        up(&art.arcs_left[1]),
                        up(&art.arcs_left[2]),
                    ],
                    clicks: up(&art.clicks),
                    arrow_left: up(&art.arrow_left),
                    arrow_right: up(&art.arrow_right),
                };
                self.session.app_mut().set_link_sprites(sprites);
                self.link_art_done = true;
            }
        }
        sync_switcher(
            self.session.app_mut(),
            compositor,
            Faces {
                pool: &mut self.polaroid_texes,
                title: &mut self.title_tex,
                undo: &mut self.undo_tex,
            },
            &mut self.switcher,
        );
        self.draws.clear();
        self.session.app().draw(&mut self.draws);
        compositor.draw_list(&self.draws);
        compositor.end_frame(window);
    }

    /// Input and time, after the frame is on screen. The gesture windows expire on this
    /// whether or not anything was pressed, so it is called every frame.
    pub fn advance(&mut self, input: &mut dyn InputSource) {
        let now = self.now();
        let events = input.poll(now);
        self.session.feed(events, now);
        let dt = self.last.elapsed().as_secs_f32();
        self.last = Instant::now();
        self.session.update(dt);
    }

    fn now(&self) -> Millis {
        self.start.elapsed().as_millis() as Millis
    }

    pub fn powering_off(&self) -> bool {
        self.session.app().ready_to_power_off()
    }

    pub fn restarting(&self) -> bool {
        self.session.app().ready_to_restart()
    }

    pub fn restart(&mut self) {
        self.session.app_mut().restart();
    }

    /// The state was flushed on the edge that set `powering_off`, so there is nothing left to
    /// do but go.
    pub fn poweroff(&mut self) {
        self.session.app_mut().poweroff();
    }
}

/// A line of menu type per label, in the order they were handed over, each with the size it
/// was rastered at. Every menu on the device is drawn from a list shaped exactly like this,
/// so the four the in-game menu needs are built through one function rather than four copies
/// of the same three lines.
fn menu_faces<'a>(
    compositor: &mut Compositor,
    labels: impl Iterator<Item = &'a str>,
) -> Vec<(TexId, u32, u32)> {
    labels
        .map(|label| {
            let f = menu_face(label);
            (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
        })
        .collect()
}

/// A screen's key caps, in the order the legend names them. None of them ever changes what
/// it says, so they are uploaded once and outlive every visit to that screen.
fn legend_faces(compositor: &mut Compositor, legend: &[(&str, &str)]) -> Vec<TexId> {
    legend
        .iter()
        .map(|(key, label)| {
            let f = hint_face(key, label);
            compositor.create_texture(f.w, f.h, &f.rgba)
        })
        .collect()
}

/// The switcher's textures, which outlive any one opening.
struct Faces<'a> {
    pool: &'a mut Vec<TexId>,
    title: &'a mut Option<TexId>,
    undo: &'a mut Option<TexId>,
}

/// Photos and the undo cap are built once per opening, on the way in, while the game is
/// already paused. Rebuilt each time rather than cached because the ring changes underneath
/// them. The title names the selection, so it follows a flick instead.
fn sync_switcher(app: &mut App, compositor: &mut Compositor, texes: Faces, state: &mut Switcher) {
    if !matches!(app.phase(), Phase::Polaroids { .. }) {
        state.open = false;
        return;
    }
    if !state.open {
        state.open = true;
        state.titled = None;
        let faces: Vec<_> = app.polaroid_entries().iter().map(photo_face).collect();
        let ids = faces
            .iter()
            .enumerate()
            .map(|(i, f)| match texes.pool.get(i) {
                Some(id) => {
                    compositor.update_texture(*id, f.w, f.h, &f.rgba);
                    *id
                }
                None => {
                    let id = compositor.create_texture_nearest(f.w, f.h, &f.rgba);
                    texes.pool.push(id);
                    id
                }
            })
            .collect();
        app.set_polaroid_faces(ids);

        // An offer can expire while the switcher is up but it cannot change into the other
        // kind, so the cap only has to be rasterised on the way in. Whether it is drawn at
        // all is the app's call.
        let label = app
            .undo_label()
            .map(|l| upload(compositor, texes.undo, hint_face("X", l)));
        app.set_undo_face(label);
    }
    if state.titled.as_deref() != app.polaroid_stamp() {
        state.titled = app.polaroid_stamp().map(str::to_string);
        let face = title_face(&app.polaroid_title(&format_stamp(app.wall_secs())));
        let id = upload(compositor, texes.title, face);
        app.set_polaroid_title_face(id);
    }
}

/// The picker is rasterised on every change under the caret, which is once per press. The
/// shelf clock follows the wall clock, so it is rebuilt when the minute turns and not on the
/// fifty nine seconds either side of it.
fn sync_clock(app: &mut App, compositor: &mut Compositor, clocks: &mut Clocks) {
    let picked = app.picker().map(|p| p.text());
    if picked != clocks.picked {
        clocks.picked = picked;
        if let Some(face) = app.picker().map(|p| p.face()) {
            let line = upload(compositor, &mut clocks.line, face);
            let hint = upload(compositor, &mut clocks.hint, set_clock_hint_face());
            app.set_clock_faces(line, hint);
        }
    }
    let shown = hhmm(app.wall_secs());
    if shown != clocks.shown {
        let face = word_face(&shown);
        clocks.shown = shown;
        let w = face.w;
        let id = upload(compositor, &mut clocks.shelf, face);
        app.set_shelf_clock_face(id, w);
    }
    let battery_shown = app
        .battery()
        .map(|b| format!("{}%", b.percent))
        .unwrap_or_default();
    if battery_shown != clocks.battery {
        clocks.battery = battery_shown.clone();
        if !battery_shown.is_empty() {
            let face = word_face(&battery_shown);
            let w = face.w;
            let id = upload(compositor, &mut clocks.battery_tex, face);
            app.set_battery_percent_face(id, w);
        }
    }
}

/// Built only once the screen is up: it is a 660 by 228 rasterisation and most sessions never
/// open it.
fn sync_about(app: &mut App, compositor: &mut Compositor, state: &mut AboutFace) {
    if !matches!(app.phase(), Phase::About) {
        return;
    }
    let battery = app.battery().map(|b| b.percent);
    if state.tex.is_some() && state.battery == battery {
        return;
    }
    state.battery = battery;
    let build = Build::current();
    let face = sticker_face(&StickerFields {
        battery,
        serial: &build.serial(),
        dirty_digit: build.dirty_digit(),
    });
    let id = upload(compositor, &mut state.tex, face);
    app.set_sticker_face(id);
}

/// The open cart's faces, asked for as soon as the caret lands on a cart and uploaded when the
/// worker hands them back, so they are normally on the GPU before SELECT + START. The worker is
/// the only place they are built: rasterised on the frame loop, a board freezes the shelf for
/// the better part of half a second on the H700.
fn sync_core_picker(
    app: &mut App,
    compositor: &mut Compositor,
    builder: &FaceBuilder,
    asked: &mut Option<String>,
    board: &mut Option<TexId>,
    lid: &mut Option<TexId>,
    built: &mut Option<String>,
) {
    let highlighted = app.selected_stem().map(str::to_string);
    if highlighted.is_some() && *asked != highlighted {
        if let Some(cart) = app
            .carts()
            .iter()
            .find(|c| highlighted.as_deref() == Some(c.stem.as_str()))
        {
            builder.request(cart.clone());
        }
        *asked = highlighted.clone();
    }
    let Some(faces) = builder.take() else {
        return;
    };
    // A build for a cart the caret has since left is dropped; the one it is on is on its way.
    if highlighted.as_deref() != Some(faces.stem.as_str()) || *built == highlighted {
        return;
    }
    let board_id = upload_rgba(
        compositor,
        board,
        faces.board.w,
        faces.board.h,
        &faces.board.rgba,
    );
    let lid_id = upload_rgba(compositor, lid, faces.lid.w, faces.lid.h, &faces.lid.rgba);
    app.set_core_board_faces(board_id, lid_id);
    *built = Some(faces.stem);
}

fn upload(compositor: &mut Compositor, slot: &mut Option<TexId>, face: slot_ui::UndoFace) -> TexId {
    upload_rgba(compositor, slot, face.w, face.h, &face.rgba)
}

/// Into the slot's own texture if it has one, so the pool stops growing after the first time.
fn upload_rgba(
    compositor: &mut Compositor,
    slot: &mut Option<TexId>,
    w: u32,
    h: u32,
    rgba: &[u8],
) -> TexId {
    match *slot {
        Some(id) => {
            compositor.update_texture(id, w, h, rgba);
            id
        }
        None => {
            let id = compositor.create_texture(w, h, rgba);
            *slot = Some(id);
            id
        }
    }
}

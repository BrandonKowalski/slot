use std::time::{Duration, Instant};

use slot_gfx::{Compositor, Draw, ScreenEffect, TexId, OUT_H, OUT_W};

const PAPER: &[u8] = include_bytes!("../assets/paper.png");
use slot_input::{InputSource, Millis};
use slot_power::{Platform, Power};
use slot_store::{format_stamp, BezelPick, Platform as Console};
use slot_ui::{
    badge_face, bezel_face, cart_shadow, chip_face, chip_shadow_face, core_button_legends,
    date_time_text, default_game_bezel_face, fit_game_bezel_face, footer_ink, game_bezel_face,
    gb_cart_shadow, hhmm, hint_face, icon_face, link_button_legends, menu_detail_face, menu_face,
    photo_face, prompt_legends, quick_caret_face, quick_description_face, quick_label_face,
    quick_legend_faces, quick_pointer_face, quick_value_face, rebuild_count_face,
    rebuild_title_face, repair_legends, set_clock_hint_face, slot_back, slot_back_light,
    slot_front, slot_front_light, socket_face, sticker_face, title_face, toast_face,
    wallpaper_face, word_ink, CartFace, GbShell, Icon, LinkBadge, PowerChoice, QuickMenuFaces,
    QuickRow, QuickValue, RebuildScreen, StickerFields, Toast, UndoFace, ALERT_PX, BOLT_PX,
    HUD_ICON_PX, HUD_INK, LEGEND,
};

use crate::app::{App, LinkRow, Phase};
use crate::build_info::Build;
use crate::cart_faces::CartFaces;
use crate::face_builder::FaceBuilder;
use crate::label_cache::{self, Rebuild};
use crate::link_art_builder::LinkArtBuilder;
use crate::link_screen::{LinkSprites, Sprite};
use crate::link_start::{LinkFail, LinkStep};
use crate::session::Session;

const DOZE_TIMEOUT: Duration = Duration::from_secs(180);

const ALERT_INK: [u8; 3] = [0xf0, 0xb4, 0x3c];

pub struct Frontend {
    session: Session,
    start: Instant,
    last: Instant,
    draws: Vec<Draw>,
    polaroid_texes: Vec<TexId>,
    title_tex: Option<TexId>,
    faces: FaceBuilder,
    cart_faces: CartFaces,
    rebuild: Option<Rebuild>,
    stale_check: Option<std::sync::mpsc::Receiver<Vec<slot_store::Cart>>>,
    ui_faces: Option<std::sync::mpsc::Receiver<UiFaces>>,
    rebuild_faces: RebuildFaces,
    link_art: LinkArtBuilder,
    link_art_done: bool,
    core_asked: Option<String>,
    core_tex: CoreTextures,
    core_built: Option<String>,
    undo_tex: Option<TexId>,
    switcher: Switcher,
    clocks: Clocks,
    about: AboutFace,
    quick_clock: QuickClock,
    quick_palette: QuickPalette,
    wallpaper: WallpaperSync,
    game_bezel: BezelSync,
    look: Option<ShelfLook>,
    link_shot: Option<TexId>,
    link_shot_size: (u32, u32),
    link_shot_shown: bool,
}

#[derive(Default)]
struct RebuildFaces {
    title: Option<(TexId, u32, u32)>,
    count: Option<TexId>,
    count_size: (u32, u32),
    shown: Option<usize>,
}

#[derive(Default)]
struct QuickClock {
    dim: Option<TexId>,
    lit: Option<TexId>,
    shown: String,
}

#[derive(Default)]
struct QuickPalette {
    dim: Option<TexId>,
    lit: Option<TexId>,
    shown: &'static str,
}

#[derive(Default)]
struct BezelSync {
    tex: Option<TexId>,
    image: Option<(Console, BezelPick, bool)>,
    labels: [String; 3],
    faces: [(Option<TexId>, Option<TexId>); 3],
}

#[derive(Default)]
struct WallpaperSync {
    tex: Option<TexId>,
    image: Option<String>,
    label: String,
    dim: Option<TexId>,
    lit: Option<TexId>,
}

#[derive(Default)]
struct AboutFace {
    tex: Option<TexId>,
    battery: Option<u8>,
}

#[derive(Default)]
struct Clocks {
    line: Option<TexId>,
    hint: Option<TexId>,
    shelf: Option<TexId>,
    picked: Option<String>,
    shown: String,
    battery: String,
    battery_tex: Option<TexId>,
    platform: String,
    platform_tex: Option<TexId>,
    title: String,
    title_tex: Option<TexId>,
    ink: [u8; 3],
}

struct ShelfLook {
    slot: [[TexId; 2]; 2],
    plugged: [TexId; 2],
    bolt: [TexId; 2],
    port: [Option<TexId>; 2],
    legend: [Vec<(TexId, u32)>; 2],
    applied: Option<(bool, bool, bool)>,
}

#[derive(Default)]
struct Switcher {
    open: bool,
    titled: Option<String>,
}

impl Frontend {
    pub fn boot(platform: Box<dyn Platform>) -> Self {
        Self::boot_with(platform, crate::audio::opened_sink())
    }

    pub fn boot_with(platform: Box<dyn Platform>, sink: Box<dyn crate::audio::AudioSink>) -> Self {
        let now = Instant::now();
        let cart_faces = CartFaces::spawn(platform.root().to_path_buf());
        let mut session = Session::boot_with(platform.root().to_path_buf(), sink);
        crate::boot_time::mark("session");
        let stale_check =
            Self::check_labels(platform.root(), session.app().carts().cloned().collect());
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
            cart_faces,
            rebuild: None,
            stale_check,
            ui_faces: None,
            rebuild_faces: RebuildFaces::default(),
            link_art: LinkArtBuilder::spawn(),
            link_art_done: false,
            core_asked: None,
            core_tex: CoreTextures::default(),
            core_built: None,
            undo_tex: None,
            switcher: Switcher::default(),
            clocks: Clocks::default(),
            about: AboutFace::default(),
            quick_clock: QuickClock::default(),
            quick_palette: QuickPalette::default(),
            wallpaper: WallpaperSync::default(),
            game_bezel: BezelSync::default(),
            look: None,
            link_shot: None,
            link_shot_size: (0, 0),
            link_shot_shown: false,
        }
    }

    fn upload_paper(compositor: &mut Compositor) {
        let mut dec = png::Decoder::new(std::io::Cursor::new(PAPER));
        dec.set_transformations(png::Transformations::normalize_to_color8());
        let Ok(mut reader) = dec.read_info() else {
            return;
        };
        let mut buf = vec![0u8; reader.output_buffer_size()];
        let Ok(info) = reader.next_frame(&mut buf) else {
            return;
        };
        if info.color_type != png::ColorType::Grayscale || info.width != info.height {
            return;
        }
        compositor.set_paper(info.width, &buf[..info.buffer_size()]);
    }

    pub fn upload_faces(&mut self, compositor: &mut Compositor) {
        self.upload_boot_faces(compositor);
        if let Some(faces) = self.ui_faces.take().and_then(|rx| rx.recv().ok()) {
            self.place_ui_faces(compositor, faces);
        }
    }

    pub fn upload_boot_faces(&mut self, compositor: &mut Compositor) {
        Self::upload_paper(compositor);
        crate::boot_time::mark("paper");
        if self.rebuild.is_none() {
            self.cart_faces.sync(self.session.app_mut(), compositor);
        }
        crate::boot_time::mark("cart faces");
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("slot-ui-faces".into())
            .spawn(move || {
                let _ = tx.send(UiFaces::build());
            });
        match spawned {
            Ok(_) => self.ui_faces = Some(rx),
            Err(_) => self.place_ui_faces(compositor, UiFaces::build()),
        }
        let icons = Icon::ALL
            .iter()
            .map(|i| {
                let f = icon_face(*i, HUD_ICON_PX, HUD_INK);
                compositor.create_texture(f.w, f.h, &f.rgba)
            })
            .collect();
        self.session.app_mut().set_icon_faces(icons);
        let alert = icon_face(Icon::Alert, ALERT_PX, ALERT_INK);
        let alert = compositor.create_texture(alert.w, alert.h, &alert.rgba);
        self.session.app_mut().set_alert_face(alert);
        let legend = legend_faces(compositor, &LEGEND);
        self.session.app_mut().set_legend_faces(legend);
        let hint = set_clock_hint_face();
        self.clocks.hint = Some(compositor.create_texture(hint.w, hint.h, &hint.rgba));
        let back = hint_face("B", "Back");
        let tex = compositor.create_texture(back.w, back.h, &back.rgba);
        self.session.app_mut().set_clock_back_face((tex, back.w));
        let star = slot_ui::star_face(slot_ui::STAR_SCALE, slot_ui::STAR_INK);
        let id = compositor.create_texture_nearest(star.w, star.h, &star.rgba);
        self.session.app_mut().set_star_face(id, star.w, star.h);
        let shadow = cart_shadow();
        let id = compositor.create_texture(shadow.w, shadow.h, &shadow.rgba);
        self.session.app_mut().set_cart_shadow(id);
        let notched = gb_cart_shadow(GbShell::Notched);
        let notched = compositor.create_texture(notched.w, notched.h, &notched.rgba);
        let rounded = gb_cart_shadow(GbShell::Rounded);
        let rounded = compositor.create_texture(rounded.w, rounded.h, &rounded.rgba);
        self.session.app_mut().set_gb_cart_shadows(notched, rounded);
        let board = slot_ui::cart_backing(None);
        let board = compositor.create_texture(board.w, board.h, &board.rgba);
        self.session.app_mut().set_cart_backing(board);
        let notched = slot_ui::cart_backing(Some(GbShell::Notched));
        let notched = compositor.create_texture(notched.w, notched.h, &notched.rgba);
        let rounded = slot_ui::cart_backing(Some(GbShell::Rounded));
        let rounded = compositor.create_texture(rounded.w, rounded.h, &rounded.rgba);
        self.session
            .app_mut()
            .set_gb_cart_backings(notched, rounded);
        let slot = {
            let mut upload =
                |bytes: &[u8]| compositor.create_texture(slot_ui::SLOT_W, slot_ui::SLOT_H, bytes);
            [
                [upload(slot_back()), upload(slot_front())],
                [upload(slot_back_light()), upload(slot_front_light())],
            ]
        };
        let plugged = [
            compositor.create_texture(slot_ui::SLOT_W, slot_ui::SLOT_H, slot_ui::slot_plugged()),
            compositor.create_texture(
                slot_ui::SLOT_W,
                slot_ui::SLOT_H,
                slot_ui::slot_plugged_light(),
            ),
        ];
        let dark_bolt = icon_face(Icon::Charging, BOLT_PX, footer_ink(false));
        let light_bolt = icon_face(Icon::Charging, BOLT_PX, footer_ink(true));
        let bolt = [
            compositor.create_texture(dark_bolt.w, dark_bolt.h, &dark_bolt.rgba),
            compositor.create_texture(light_bolt.w, light_bolt.h, &light_bolt.rgba),
        ];
        self.look = Some(ShelfLook {
            slot,
            plugged,
            bolt,
            port: [None, None],
            legend: [Vec::new(), Vec::new()],
            applied: None,
        });
        self.apply_look();
        crate::boot_time::mark("shelf faces");
        self.upload_ground(compositor);
    }

    fn upload_ground(&mut self, compositor: &mut Compositor) {
        let Some(rgba) = slot_ui::default_background() else {
            return;
        };
        let id = compositor.create_texture(OUT_W, OUT_H, rgba);
        self.session.app_mut().set_ground(id);
        if let Some(rgba) = slot_ui::blur_background() {
            let id = compositor.create_texture(OUT_W, OUT_H, rgba);
            self.session.app_mut().set_blur(id);
        }
    }

    fn apply_look(&mut self) {
        let light = self.session.app().light();
        let ready = self.look.as_ref().map(|look| {
            let i = usize::from(light);
            let legend_ready = !look.legend[i].is_empty();
            let port_ready = look.port[i].is_some();
            (
                look.applied == Some((light, legend_ready, port_ready)),
                look.slot[i],
                look.plugged[i],
                look.bolt[i],
                look.bolt[0],
                look.port[i],
                look.legend[i].clone(),
                legend_ready,
                port_ready,
            )
        });
        let Some((done, slot, plugged, bolt, hud_bolt, port, legend, legend_ready, port_ready)) =
            ready
        else {
            return;
        };
        if done {
            return;
        }
        self.session.app_mut().set_slot_faces(slot[0], slot[1]);
        self.session.app_mut().set_plugged_face(plugged);
        self.session.app_mut().set_bolt_face(bolt);
        self.session.app_mut().set_hud_bolt_face(hud_bolt);
        if let Some(tex) = port {
            self.session.app_mut().set_link_port(tex);
        }
        if legend_ready {
            self.session.app_mut().set_link_legend_faces(legend);
        }
        if let Some(look) = &mut self.look {
            look.applied = Some((light, legend_ready, port_ready));
        }
    }

    fn poll_ui_faces(&mut self, compositor: &mut Compositor) {
        let Some(rx) = &self.ui_faces else {
            return;
        };
        match rx.try_recv() {
            Ok(faces) => {
                self.ui_faces = None;
                self.place_ui_faces(compositor, faces);
                crate::boot_time::mark("ui faces");
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => self.ui_faces = None,
        }
    }

    fn place_ui_faces(&mut self, compositor: &mut Compositor, faces: UiFaces) {
        let mut cart = |f: CartFace| compositor.create_texture(f.w, f.h, &f.rgba);
        let link_badges = faces.link_badges.into_iter().map(&mut cart).collect();
        let toasts = faces.toasts.into_iter().map(&mut cart).collect();
        let sockets = faces.sockets.into_iter().map(&mut cart).collect();
        let chips = faces.chips.into_iter().map(&mut cart).collect();
        let blank = cart(faces.blank_chip);
        let shadow = cart(faces.chip_shadow);
        let mut up = |f: UndoFace| (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h);
        let lines = faces.shutdown.into_iter().map(&mut up).collect();
        let repair = faces.repair.into_iter().map(&mut up).collect();
        let reset = faces.reset.into_iter().map(&mut up).collect();
        let labels = faces
            .quick_labels
            .into_iter()
            .map(|l| l.map(&mut up))
            .collect();
        let pointer = up(faces.quick_pointer);
        let values = faces
            .quick_values
            .into_iter()
            .map(|v| v.map(&mut up))
            .collect();
        let carets = faces.quick_carets.map(&mut up);
        let descriptions = faces.descriptions.map(&mut up);
        let mut pack_legend = |faces: [UndoFace; 8]| {
            faces.map(|f| {
                let (tex, w, _) = up(f);
                (tex, w)
            })
        };
        let legend = pack_legend(faces.quick_legend);
        let legend_light = pack_legend(faces.quick_legend_light);
        let core_legend = faces
            .core_legend
            .into_iter()
            .map(|f| {
                let (tex, w, _) = up(f);
                (tex, w)
            })
            .collect();
        let roles = faces.link_roles.into_iter().map(&mut up).collect();
        let linked = up(faces.linked);
        let mut pack = |faces: Vec<UndoFace>| -> Vec<(TexId, u32)> {
            faces
                .into_iter()
                .map(|f| {
                    let (tex, w, _) = up(f);
                    (tex, w)
                })
                .collect()
        };
        let link_legend = pack(faces.link_legend);
        let link_legend_light = pack(faces.link_legend_light);
        let steps = faces.link_steps.into_iter().map(&mut up).collect();
        let fails = faces.link_fails.into_iter().map(&mut up).collect();
        if let Some(look) = &mut self.look {
            look.legend = [link_legend.clone(), link_legend_light];
            look.applied = None;
        }
        let app = self.session.app_mut();
        app.set_link_badge_faces(link_badges);
        app.set_shutdown_faces(lines);
        app.set_repair_faces(repair);
        app.set_reset_faces(reset);
        app.set_quick_menu_faces(QuickMenuFaces {
            labels,
            pointer,
            values,
            carets,
            legend,
            legend_light,
            descriptions,
        });
        app.set_core_part_faces(sockets, chips, blank, shadow);
        app.set_core_legend_faces(core_legend);
        app.set_link_menu_faces(roles);
        app.set_link_linked_face(linked);
        app.set_link_legend_faces(link_legend);
        app.set_link_step_faces(steps);
        app.set_link_fail_faces(fails);
        app.set_toast_faces(toasts);
    }

    pub fn upload_bezel(&mut self, compositor: &mut Compositor, panel: (u32, u32)) {
        self.session.app_mut().set_panel(panel);
        if !slot_gfx::framed(panel) {
            return;
        }
        let Some(path) = self
            .session
            .app()
            .root()
            .and_then(|root| crate::bezel::pick(root, panel))
        else {
            return;
        };
        if let Some(rgba) = bezel_face(&path, panel.0, panel.1) {
            compositor.set_bezel(panel.0, panel.1, &rgba);
            eprintln!("slot: bezel {}", path.display());
        }
    }

    pub fn render(&mut self, compositor: &mut Compositor, window: (u32, u32)) {
        self.session.app_mut().set_panel(window);
        compositor.fit(window);
        self.compose(compositor);
        compositor.end_frame(window);
    }

    fn grab_link_shot(&mut self, compositor: &mut Compositor) {
        let open = self.session.app().game_menu_open();
        if !open {
            if self.link_shot_shown {
                self.link_shot_shown = false;
                self.session.app_mut().set_link_shot(None);
            }
            return;
        }
        if self.link_shot_shown {
            return;
        }
        let (w, h) = compositor.frame_size();
        if w == 0 || h == 0 {
            return;
        }
        let rgba = compositor.read_frame();
        if rgba.len() < (w * h * 4) as usize {
            return;
        }
        let tex = match self.link_shot {
            Some(id) if self.link_shot_size == (w, h) => {
                compositor.update_texture(id, w, h, &rgba);
                id
            }
            _ => {
                let id = compositor.create_texture_nearest(w, h, &rgba);
                self.link_shot = Some(id);
                self.link_shot_size = (w, h);
                id
            }
        };
        self.session.app_mut().set_link_shot(Some(tex));
        self.link_shot_shown = true;
    }

    pub fn compose(&mut self, compositor: &mut Compositor) {
        compositor.set_blue_light(self.session.app().blue_light());
        compositor.set_picture(self.session.app().picture_rect());
        compositor.set_screen_effect(match self.session.app().screen_shader() {
            slot_store::Shader::Off => ScreenEffect::None,
            slot_store::Shader::Lcd3x => ScreenEffect::Lcd3x,
            slot_store::Shader::Grid => ScreenEffect::Grid,
            slot_store::Shader::Dot => ScreenEffect::Dot,
            slot_store::Shader::Simpletex => ScreenEffect::Simpletex,
        });
        compositor.set_shake(self.session.app().screen_shake());
        compositor.set_screen_power(self.session.app().screen_power());
        compositor.set_screen_footer(self.session.app().screen_footer());
        compositor.set_overlay_slide(self.session.app().overlay_slide());
        compositor.set_game_source_rect(self.session.app().source_rect());
        compositor.set_soft_picture(self.session.app().softens_picture());
        self.grab_link_shot(compositor);
        compositor.begin_frame(self.session.app().layout());
        if self.draw_rebuild(compositor) {
            return;
        }
        if let Some([r, g, b]) = self.session.app().blank_panel() {
            let px = (slot_gfx::SRC_W * slot_gfx::SRC_H) as usize;
            compositor.upload_game(&[b, g, r, 0xff].repeat(px));
        } else if let Some(frame) = self.session.frame() {
            compositor.upload_game(&frame);
            crate::latency::taken();
        }
        self.cart_faces.sync(self.session.app_mut(), compositor);
        sync_clock(self.session.app_mut(), compositor, &mut self.clocks);
        sync_binding_faces(self.session.app_mut(), compositor);
        sync_about(self.session.app_mut(), compositor, &mut self.about);
        sync_quick_clock(self.session.app_mut(), compositor, &mut self.quick_clock);
        sync_quick_palette(self.session.app_mut(), compositor, &mut self.quick_palette);
        sync_wallpaper(self.session.app_mut(), compositor, &mut self.wallpaper);
        sync_game_bezel(self.session.app_mut(), compositor, &mut self.game_bezel);
        compositor.set_game_bezel(self.session.app().shown_game_bezel());
        compositor.set_sp_frame(self.session.app().sp_frame());
        sync_core_picker(
            self.session.app_mut(),
            compositor,
            &self.faces,
            &mut self.core_asked,
            &mut self.core_tex,
            &mut self.core_built,
        );
        if !self.link_art_done {
            if let Some(art) = self.link_art.take() {
                let mut up = |f: &slot_ui::CartFace| Sprite {
                    tex: compositor.create_texture(f.w, f.h, &f.rgba),
                    w: f.w,
                    h: f.h,
                };
                let port_dark = up(&art.port);
                let port_light = up(&art.port_light);
                if let Some(look) = &mut self.look {
                    look.port = [Some(port_dark.tex), Some(port_light.tex)];
                    look.applied = None;
                }
                let sprites = LinkSprites {
                    port: port_dark,
                    plug_host: up(&art.plug_host),
                    plug_join: up(&art.plug_join),
                    adapter: up(&art.adapter),
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
        self.apply_look();
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
    }

    pub fn drive_emulator(&mut self) {
        self.session.set_driven(true);
    }

    pub fn step_emulator(&self, present: Duration, timeout: Duration) -> bool {
        self.session.step_emulator(present, timeout)
    }

    pub fn rebuilding_labels(&self) -> bool {
        self.rebuild.is_some()
    }

    fn check_labels(
        root: &std::path::Path,
        carts: Vec<slot_store::Cart>,
    ) -> Option<std::sync::mpsc::Receiver<Vec<slot_store::Cart>>> {
        let (tx, rx) = std::sync::mpsc::channel();
        let root = root.to_path_buf();
        std::thread::Builder::new()
            .name("slot-label-check".into())
            .spawn(move || {
                let _ = tx.send(label_cache::stale(&root, carts.iter()));
            })
            .ok()
            .map(|_| rx)
    }

    fn poll_label_check(&mut self) {
        let Some(rx) = &self.stale_check else {
            return;
        };
        let stale = match rx.try_recv() {
            Ok(stale) => stale,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Vec::new(),
        };
        self.stale_check = None;
        let Some(root) = self.session.app().root().map(|r| r.to_path_buf()) else {
            return;
        };
        if !stale.is_empty() {
            eprintln!("slot: label cache: {} new labels", stale.len());
            self.rebuild = Some(Rebuild::start(&root, stale));
        }
    }

    fn draw_rebuild(&mut self, compositor: &mut Compositor) -> bool {
        self.poll_label_check();
        self.poll_ui_faces(compositor);
        let Some(rebuild) = &self.rebuild else {
            return false;
        };
        if rebuild.finished() {
            self.rebuild = None;
            self.last = Instant::now();
            return false;
        }
        let faces = &mut self.rebuild_faces;
        let title = *faces.title.get_or_insert_with(|| {
            let f = rebuild_title_face("Caching New Labels");
            (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
        });
        let done = rebuild.done();
        if faces.shown != Some(done) {
            let f = rebuild_count_face(&format!("{done} / {}", rebuild.total()));
            faces.count_size = (f.w, f.h);
            upload(compositor, &mut faces.count, f);
            faces.shown = Some(done);
        }
        let screen = RebuildScreen {
            title: Some(title),
            count: faces
                .count
                .map(|tex| (tex, faces.count_size.0, faces.count_size.1)),
            fraction: done as f32 / rebuild.total().max(1) as f32,
        };
        self.draws.clear();
        screen.draw(self.session.app().canvas(), &mut self.draws);
        compositor.draw_list(&self.draws);
        true
    }

    pub fn advance(&mut self, input: &mut dyn InputSource) {
        let now = self.now();
        let events = input.poll(now);
        if self.rebuild.is_some() {
            return;
        }
        self.session.feed(events, now);
        let dt = self.last.elapsed().as_secs_f32();
        self.last = Instant::now();
        self.session.update(dt);
    }

    pub fn app(&self) -> &crate::app::App {
        self.session.app()
    }

    pub fn set_left_stick(&mut self, present: bool) {
        self.session.app_mut().set_left_stick(present);
    }

    pub fn core_settling(&self) -> bool {
        self.session.core_settling()
    }

    pub fn advance_at(&mut self, input: &mut dyn InputSource, now: Millis, dt: f32) {
        let events = input.poll(now);
        self.session.feed(events, now);
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

    pub fn repairing(&self) -> bool {
        self.session.app().ready_to_repair()
    }

    pub fn repair(&mut self) {
        self.session.app_mut().repair();
    }

    pub fn poweroff(&mut self) {
        self.session.app_mut().poweroff();
    }
}

fn link_legend_faces(ink: [u8; 3]) -> Vec<UndoFace> {
    link_button_legends(ink)
}

fn legend_faces(compositor: &mut Compositor, legend: &[(&str, &str)]) -> Vec<TexId> {
    legend
        .iter()
        .map(|(key, label)| {
            let f = hint_face(key, label);
            compositor.create_texture(f.w, f.h, &f.rgba)
        })
        .collect()
}

struct UiFaces {
    link_badges: Vec<CartFace>,
    shutdown: Vec<UndoFace>,
    quick_labels: Vec<[UndoFace; 2]>,
    quick_pointer: UndoFace,
    quick_values: Vec<[UndoFace; 2]>,
    quick_carets: [UndoFace; 2],
    quick_legend: [UndoFace; 8],
    quick_legend_light: [UndoFace; 8],
    descriptions: [UndoFace; 2],
    sockets: Vec<CartFace>,
    chips: Vec<CartFace>,
    blank_chip: CartFace,
    chip_shadow: CartFace,
    core_legend: Vec<UndoFace>,
    link_roles: Vec<UndoFace>,
    linked: UndoFace,
    link_legend: Vec<UndoFace>,
    link_legend_light: Vec<UndoFace>,
    link_steps: Vec<UndoFace>,
    link_fails: Vec<UndoFace>,
    toasts: Vec<CartFace>,
    repair: Vec<UndoFace>,
    reset: Vec<UndoFace>,
}

impl UiFaces {
    fn build() -> Self {
        UiFaces {
            link_badges: LinkBadge::FACES
                .iter()
                .map(|b| {
                    let (badge, ink) = (
                        b.badge().expect("a face has a glyph"),
                        b.colour().expect("and a colour"),
                    );
                    badge_face(badge, HUD_ICON_PX, ink)
                })
                .collect(),
            shutdown: PowerChoice::ALL
                .iter()
                .map(|c| {
                    menu_face(match c {
                        PowerChoice::Restart => "Restarting",
                        PowerChoice::PowerOff => "Powering Down",
                        PowerChoice::Repair => "Repairing SD Card",
                    })
                })
                .collect(),
            quick_labels: QuickRow::ALL
                .iter()
                .map(|r| [false, true].map(|lit| quick_label_face(*r, lit)))
                .collect(),
            quick_pointer: quick_pointer_face(),
            quick_values: QuickValue::ALL
                .iter()
                .map(|v| [false, true].map(|lit| quick_value_face(v.text(), lit)))
                .collect(),
            quick_carets: [false, true].map(quick_caret_face),
            quick_legend: quick_legend_faces(footer_ink(false)),
            quick_legend_light: quick_legend_faces(footer_ink(true)),
            descriptions: [
                quick_description_face(&["Supports single button and MENU+button."]),
                quick_description_face(&["Supports single button, SELECT+button and MENU+button."]),
            ],
            sockets: slot_store::Core::ALL
                .iter()
                .map(|c| socket_face(*c))
                .collect(),
            chips: slot_store::Core::ALL
                .iter()
                .map(|c| chip_face(Some(*c)))
                .collect(),
            blank_chip: chip_face(None),
            chip_shadow: chip_shadow_face(),
            core_legend: core_button_legends(),
            link_roles: LinkRow::ALL.iter().map(|r| menu_face(r.text())).collect(),
            linked: menu_face("Linked"),
            link_legend: link_legend_faces(footer_ink(false)),
            link_legend_light: link_legend_faces(footer_ink(true)),
            link_steps: LinkStep::ALL.iter().map(|s| menu_face(s.line())).collect(),
            link_fails: LinkFail::SHOWN
                .iter()
                .map(|f| menu_face(f.line()))
                .collect(),
            toasts: Toast::all().into_iter().map(toast_face).collect(),
            repair: {
                let [not_now, repair] = repair_legends(footer_ink(false));
                vec![
                    menu_face("Repair the SD card?"),
                    menu_detail_face("slot restarts when it is done."),
                    not_now,
                    repair,
                ]
            },
            reset: {
                let [cancel, reset] = prompt_legends(footer_ink(false), "Cancel", "Reset");
                vec![
                    menu_face("Reset all controls?"),
                    menu_detail_face("Every button goes back to its default."),
                    menu_face("Reset all shortcuts?"),
                    menu_detail_face("Every shortcut goes back to its default."),
                    menu_face("Reset all shelf keys?"),
                    menu_detail_face("Every shelf key goes back to its default."),
                    cancel,
                    reset,
                ]
            },
        }
    }
}

struct Faces<'a> {
    pool: &'a mut Vec<TexId>,
    title: &'a mut Option<TexId>,
    undo: &'a mut Option<TexId>,
}

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

fn sync_binding_faces(app: &mut App, compositor: &mut Compositor) {
    if app.quick_menu().is_none() && app.in_game_menu().is_none() {
        return;
    }
    for text in app.binding_texts() {
        if app.has_binding_face(&text) {
            continue;
        }
        let faces = [false, true].map(|lit| {
            let f = quick_value_face(&text, lit);
            (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
        });
        app.add_binding_face(text, faces);
    }
}

fn sync_clock(app: &mut App, compositor: &mut Compositor, clocks: &mut Clocks) {
    let ink = app.status_ink();
    if clocks.ink != ink {
        clocks.ink = ink;
        clocks.shown.clear();
        clocks.battery.clear();
    }
    let picked = app.picker().map(|p| p.text());
    if picked != clocks.picked {
        clocks.picked = picked;
        if let (Some(face), Some(hint)) = (app.picker().map(|p| p.face()), clocks.hint) {
            let line = upload(compositor, &mut clocks.line, face);
            app.set_clock_faces(line, hint);
        }
    }
    let shown = hhmm(app.wall_secs());
    if shown != clocks.shown {
        let face = word_ink(&shown, clocks.ink);
        clocks.shown = shown;
        let w = face.w;
        let id = upload(compositor, &mut clocks.shelf, face);
        app.set_shelf_clock_face(id, w);
    }
    let platform_shown = app.slot_text().unwrap_or_default();
    if platform_shown != clocks.platform {
        clocks.platform = platform_shown.clone();
        if platform_shown.is_empty() {
            app.clear_shelf_platform();
        } else {
            let face = word_ink(&platform_shown, [0xff, 0xff, 0xff]);
            let w = face.w;
            let id = upload(compositor, &mut clocks.platform_tex, face);
            app.set_shelf_platform_face(id, w);
        }
    }
    let title_shown = app.title_text().unwrap_or_default();
    if title_shown != clocks.title {
        clocks.title = title_shown.clone();
        if title_shown.is_empty() {
            app.clear_shelf_title();
        } else {
            let face = slot_ui::title_line_face(&title_shown);
            let w = face.w;
            let id = upload(compositor, &mut clocks.title_tex, face);
            app.set_shelf_title_face(id, w);
        }
    }
    let battery_shown = app
        .battery()
        .map(|b| format!("{}%", b.percent))
        .unwrap_or_default();
    if battery_shown != clocks.battery {
        clocks.battery = battery_shown.clone();
        if !battery_shown.is_empty() {
            let face = word_ink(&battery_shown, clocks.ink);
            let w = face.w;
            let id = upload(compositor, &mut clocks.battery_tex, face);
            app.set_battery_percent_face(id, w);
        }
    }
}

fn sync_game_bezel(app: &mut App, compositor: &mut Compositor, state: &mut BezelSync) {
    let platform = app.playing_platform();
    let pick = app.resolved_bezel(platform);
    let shown = (platform, pick, app.fits_picture());
    if state.image.as_ref() != Some(&shown) {
        let rgba = match (&shown.1, shown.2) {
            (BezelPick::None | BezelPick::Ags102, _) => None,
            (BezelPick::Default, false) => default_game_bezel_face(platform == Console::Gbc),
            (BezelPick::Default, true) => fit_game_bezel_face(platform == Console::Gbc),
            // A card bezel is drawn for the actual-size picture and would cover a fitted one.
            (BezelPick::File(_), true) => None,
            (BezelPick::File(name), false) => app
                .root()
                .map(|root| {
                    root.join(crate::bezel::GAME_BEZELS)
                        .join(platform.dir_name())
                        .join(name)
                })
                .and_then(|path| game_bezel_face(&path)),
        };
        let tex = rgba.map(|rgba| match state.tex {
            Some(id) => {
                compositor.update_texture(id, OUT_W, OUT_H, &rgba);
                id
            }
            None => {
                let id = compositor.create_texture(OUT_W, OUT_H, &rgba);
                state.tex = Some(id);
                id
            }
        });
        app.set_game_bezel(tex);
        state.image = Some(shown);
    }
    if app.quick_menu().is_none() && app.in_game_menu().is_none() {
        return;
    }
    for (at, platform) in [Console::Gba, Console::Gb, Console::Gbc]
        .into_iter()
        .enumerate()
    {
        let label = app.resolved_bezel(platform).label();
        if state.labels[at] == label {
            continue;
        }
        let (dim, lit) = (
            quick_value_face(&label, false),
            quick_value_face(&label, true),
        );
        let (dim_size, lit_size) = ((dim.w, dim.h), (lit.w, lit.h));
        let dim = upload(compositor, &mut state.faces[at].0, dim);
        let lit = upload(compositor, &mut state.faces[at].1, lit);
        app.set_quick_bezel_faces(
            platform,
            (dim, dim_size.0, dim_size.1),
            (lit, lit_size.0, lit_size.1),
        );
        state.labels[at] = label;
    }
}

fn sync_wallpaper(app: &mut App, compositor: &mut Compositor, state: &mut WallpaperSync) {
    let shown = app.shown_wallpaper();
    if state.image != shown {
        state.image = shown.clone();
        if let Some(name) = &shown {
            if let Some(rgba) = app
                .root()
                .map(|root| root.join("Wallpapers").join(name))
                .and_then(|path| wallpaper_face(&path))
            {
                let id = match state.tex {
                    Some(id) => {
                        compositor.update_texture(id, OUT_W, OUT_H, &rgba);
                        id
                    }
                    None => {
                        let id = compositor.create_texture(OUT_W, OUT_H, &rgba);
                        state.tex = Some(id);
                        id
                    }
                };
                app.set_wallpaper(id);
            }
        }
    }
    if app.quick_menu().is_none() {
        return;
    }
    let label = app.resolved_wallpaper().label();
    if state.label == label {
        return;
    }
    let (dim, lit) = (
        quick_value_face(&label, false),
        quick_value_face(&label, true),
    );
    let (dim_size, lit_size) = ((dim.w, dim.h), (lit.w, lit.h));
    let dim = upload(compositor, &mut state.dim, dim);
    let lit = upload(compositor, &mut state.lit, lit);
    app.set_quick_wallpaper_faces((dim, dim_size.0, dim_size.1), (lit, lit_size.0, lit_size.1));
    state.label = label;
}

fn sync_quick_clock(app: &mut App, compositor: &mut Compositor, state: &mut QuickClock) {
    if app.quick_menu().is_none() {
        return;
    }
    let text = date_time_text(app.wall_secs());
    if text == state.shown {
        return;
    }
    let (dim, lit) = (
        quick_value_face(&text, false),
        quick_value_face(&text, true),
    );
    let (dim_size, lit_size) = ((dim.w, dim.h), (lit.w, lit.h));
    let dim = upload(compositor, &mut state.dim, dim);
    let lit = upload(compositor, &mut state.lit, lit);
    app.set_quick_clock_faces((dim, dim_size.0, dim_size.1), (lit, lit_size.0, lit_size.1));
    state.shown = text;
}

fn sync_quick_palette(app: &mut App, compositor: &mut Compositor, state: &mut QuickPalette) {
    if app.in_game_menu().is_none() {
        return;
    }
    let text = app.gb_palette_label();
    if text == state.shown {
        return;
    }
    let (dim, lit) = (quick_value_face(text, false), quick_value_face(text, true));
    let (dim_size, lit_size) = ((dim.w, dim.h), (lit.w, lit.h));
    let dim = upload(compositor, &mut state.dim, dim);
    let lit = upload(compositor, &mut state.lit, lit);
    app.set_quick_palette_faces((dim, dim_size.0, dim_size.1), (lit, lit_size.0, lit_size.1));
    state.shown = text;
}

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

/// The textures the core picker reuses from one cart to the next.
#[derive(Default)]
struct CoreTextures {
    interior: Option<TexId>,
    board: Option<TexId>,
    lid: Option<TexId>,
}

fn sync_core_picker(
    app: &mut App,
    compositor: &mut Compositor,
    builder: &FaceBuilder,
    asked: &mut Option<String>,
    tex: &mut CoreTextures,
    built: &mut Option<String>,
) {
    let highlighted = app.selected_stem().map(str::to_string);
    if highlighted.is_some() && *asked != highlighted {
        if let Some(cart) = app
            .carts()
            .find(|c| highlighted.as_deref() == Some(c.stem.as_str()))
        {
            builder.request(cart.clone());
        }
        *asked = highlighted.clone();
    }
    let Some(faces) = builder.take() else {
        return;
    };
    if highlighted.as_deref() != Some(faces.stem.as_str()) || *built == highlighted {
        return;
    }
    let interior_id = upload_rgba(
        compositor,
        &mut tex.interior,
        faces.interior.w,
        faces.interior.h,
        &faces.interior.rgba,
    );
    app.set_core_interior_face(interior_id);
    let board_id = upload_rgba(
        compositor,
        &mut tex.board,
        faces.board.w,
        faces.board.h,
        &faces.board.rgba,
    );
    let lid_id = upload_rgba(
        compositor,
        &mut tex.lid,
        faces.lid.w,
        faces.lid.h,
        &faces.lid.rgba,
    );
    app.set_core_board_faces(board_id, lid_id);
    *built = Some(faces.stem);
}

fn upload(compositor: &mut Compositor, slot: &mut Option<TexId>, face: slot_ui::UndoFace) -> TexId {
    upload_rgba(compositor, slot, face.w, face.h, &face.rgba)
}

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

use slot_gfx::{Draw, TexId, OUT_H, OUT_W};
use slot_store::Cart;

use crate::cart::{cart_box, gb_shell_of, label_colour, label_text, CART_W};
use crate::hud::Millis;
use crate::silhouette::GbShell;
use crate::slot_chrome::draw_empty_slot;

/// Distance between cart centres. Wider than a cart so the neighbours peek in at both
/// edges and the row reads as continuing past them.
/// Chosen so the outer two carts sit fully on screen with the margin at the edge equal to
/// the gap beside the centre cart. At 286 the side carts were clipped 24px off each edge.
const PITCH: f32 = 240.0;
const SIDE_SCALE: f32 = 0.78;
const SIDE_ALPHA: f32 = 0.55;
/// Where a cartridge of this height stands on the row: centred on the screen. Every shelf holds
/// one platform — the card is one folder per system — so a row never mixes heights, and what the
/// eye reads on the carousel is where the selected cartridge sits in the frame. A GBA cart has
/// always been centred; a Game Boy pak measured from a shared floor instead sat 59 px higher,
/// crowding the top of the screen and leaving a gap above the slot, which is what the user
/// objected to. The two now share a centre rather than a floor.
///
/// This is where the *full size* cartridge rests. A side cart is smaller, and it keeps its foot
/// on the line the selection's foot is on rather than shrinking about the middle, so the row
/// still reads as objects standing on a shelf: see `foot_y`.
pub fn rest_y(h: f32) -> f32 {
    (OUT_H as f32 - h) / 2.0
}

/// The line this platform's cartridges stand on, which is `rest_y` plus that cartridge's own
/// height. Asked with the cart's full height even for a shrunken neighbour: the foot stays put
/// as a cart shrinks away, which is what stops the row reading as carts floating.
pub fn foot_y(h: f32) -> f32 {
    rest_y(h) + h
}
/// Critically damped, so a flick lands on a cart instead of bouncing past and returning.
const OMEGA: f32 = 16.0;
/// How far the cart next to the selection is pushed aside as the chosen one goes in. Enough
/// to clear the frame from where it stands.
const PART: f32 = 130.0;

/// Slots considered either side of the selection. Two reach the edges of a 720 row, the
/// third covers the lag while the spring is still catching up with a flick.
const SLOTS: i32 = 3;

/// Before the first repeat. Long enough that a press meaning one cart cannot become two.
const REPEAT_DELAY_MS: Millis = 400;
/// Between repeats after that. Fast enough to cross a thirty cart library, slow enough to
/// stop on one.
const REPEAT_MS: Millis = 110;

pub struct Shelf {
    pub carts: Vec<Cart>,
    pub index: usize,
    pub scroll: f32,
    faces: Vec<TexId>,
    /// The cart silhouette in black, drawn under a dimmed cart. One texture per *mould* rather
    /// than one for the row: a row can hold GBA carts or Game Boy paks, the two Game Pak shells
    /// differ at their top corners, and all three are different objects. Backing of the wrong
    /// outline either draws black over the wallpaper beside the cart or leaves part of the
    /// dimmed face with nothing behind it, and both of those are visible.
    shadow: Option<TexId>,
    gb_shadow: Option<TexId>,
    gbc_shadow: Option<TexId>,
    /// Which mould each cart in `carts` came out of, `None` for a GBA cart. Worked out once
    /// here because the answer is in the rom's header: asking it while drawing would open a
    /// file on every cart of every frame.
    shells: Vec<Option<GbShell>>,
    /// The presses added up, in the same continuous coordinate `scroll` lives in, so it counts
    /// laps rather than wrapping. Only a ring of two reads it — see `scroll_target` — but it is
    /// kept for every row because `step` is the one place a press is known about, and a row
    /// never changes how many carts it holds without being built again.
    ride: f32,
    vel: f32,
    /// The direction being held and when it next repeats. Repeat lives here rather than in
    /// the gesture layer so nothing in game starts auto firing.
    held: Option<(i32, Millis)>,
}

impl Shelf {
    pub fn new(carts: Vec<Cart>) -> Self {
        Shelf {
            shells: carts.iter().map(gb_shell_of).collect(),
            carts,
            index: 0,
            scroll: 0.0,
            faces: Vec::new(),
            shadow: None,
            gb_shadow: None,
            gbc_shadow: None,
            ride: 0.0,
            vel: 0.0,
            held: None,
        }
    }

    /// Put the row on a cart without a ride: the selection, where the row stands and where its
    /// spring is heading all become this cart at once. Assigning `index` on its own leaves the
    /// spring aiming at the cart that was selected before, so this is how anything outside the
    /// left and right presses moves the shelf — the carousel opening on a resumed cart, say.
    pub fn select(&mut self, i: usize) {
        self.index = i;
        self.scroll = i as f32;
        self.ride = i as f32;
        self.vel = 0.0;
    }

    /// Face textures in `carts` order. The caller uploads them because only the compositor
    /// can mint a `TexId`.
    pub fn set_shadow(&mut self, face: TexId) {
        self.shadow = Some(face);
    }

    /// The Game Boy pak's outline in black, one per shell mould. A row whose carts are paks and
    /// whose only uploaded shadow is the GBA one draws no black at all rather than a tapered
    /// shape stretched under a straight sided cart.
    pub fn set_gb_shadow(&mut self, shell: GbShell, face: TexId) {
        match shell {
            GbShell::Notched => self.gb_shadow = Some(face),
            GbShell::Rounded => self.gbc_shadow = Some(face),
        }
    }

    pub fn set_faces(&mut self, faces: Vec<TexId>) {
        self.faces = faces;
    }

    /// In `hints` order.
    pub fn find(&self, stem: &str) -> Option<(&Cart, Option<TexId>)> {
        let i = self.carts.iter().position(|c| c.stem == stem)?;
        Some((&self.carts[i], self.faces.get(i).copied()))
    }

    pub fn left(&mut self) {
        self.step(-1);
    }

    pub fn right(&mut self) {
        self.step(1);
    }

    pub fn hold_left(&mut self, now: Millis) {
        self.hold(-1, now);
    }

    pub fn hold_right(&mut self, now: Millis) {
        self.hold(1, now);
    }

    /// The press moves a cart itself, so the repeat is what the delay is measured from
    /// rather than what it produces.
    fn hold(&mut self, by: i32, now: Millis) {
        self.step(by);
        self.held = Some((by, now + REPEAT_DELAY_MS));
    }

    pub fn release_left(&mut self) {
        self.release(-1);
    }

    pub fn release_right(&mut self) {
        self.release(1);
    }

    /// Only the direction that is being held stops it. Letting go of the other one is a
    /// change of direction the shelf has already acted on.
    fn release(&mut self, by: i32) {
        if matches!(self.held, Some((held, _)) if held == by) {
            self.held = None;
        }
    }

    /// Whatever is held, let go of. Nothing on screen is holding it.
    pub fn release_hold(&mut self) {
        self.held = None;
    }

    /// Fires the repeat. Due from `now` rather than from the deadline it passed, so a frame
    /// the app was late for costs one cart instead of a burst of catching up.
    pub fn tick(&mut self, now: Millis) {
        let Some((by, due)) = self.held else {
            return;
        };
        if now < due {
            return;
        }
        self.step(by);
        self.held = Some((by, now + REPEAT_MS));
    }

    fn step(&mut self, by: i32) {
        let n = self.carts.len();
        if n == 0 {
            return;
        }
        self.index = (self.index as i32 + by).rem_euclid(n as i32) as usize;
        self.ride += by as f32;
    }

    /// Where the spring is heading, in the continuous coordinate `scroll` lives in. The row is
    /// a ring, so the selected cart has an image every `n` slots; this is the one nearest the
    /// place measured from, which is what stops a wrap unwinding the whole row.
    ///
    /// A row of three or more measures from where the row already stands. A row of two cannot:
    /// on a ring of two the selection's two nearest images sit exactly one slot either side, so
    /// "the nearest" has nothing to choose between them, and a press arriving while the spring
    /// is still moving lands on the image behind the row and sends it back the way it came. On a
    /// shelf of two both neighbours are the same cart, so the direction of travel is the only
    /// thing on screen that says which button was pressed, and a reversal reads as the row
    /// glitching. It measures from `ride` — the presses added up, laps and all — instead, so a
    /// second right press carries on rightwards through the wrap.
    ///
    /// Wrapping `ride` back onto the ring is what keeps this honest if `index` was moved without
    /// it: the answer is still an image of the cart that is actually selected.
    pub fn scroll_target(&self) -> f32 {
        let n = self.carts.len();
        if n == 0 {
            return 0.0;
        }
        let from = if n == 2 { self.ride } else { self.scroll };
        let n = n as f32;
        from + (self.index as f32 - from + n / 2.0).rem_euclid(n) - n / 2.0
    }

    /// The cart `off` slots right of the selection, or `None` when the row is empty or when this
    /// slot falls off the end of a row too short to reach it.
    ///
    /// A ring of two fills every slot, which means one of the two carts is drawn twice at once.
    /// The user asked for that having seen the alternatives running on the device: the row was
    /// first left with a hole where the repeat would have been, then stood as a centred pair,
    /// and their answer to both was "if there are only two carts the carts should repeat to fill
    /// all three carousel slots". Do not take it back out — a row that shows a cart twice is
    /// what a carousel of two *is*, and it is the only one of the three that scrolls, since the
    /// other two had nothing to put in the slot the row moves into.
    ///
    /// Filling every slot rather than only the three on screen is what makes the scroll
    /// continuous: with every slot taken, each offset along the row holds the same cart before
    /// and after a press, so the row slides by a pitch instead of a cart blinking out at one
    /// edge and back in at the other. The ones past the edges are thrown away by `draw_row`'s
    /// own bounds check, as they are on any other row.
    ///
    /// One cart stays alone in the middle. Repeating it would put three identical faces across a
    /// row that cannot scroll — the selection never changes, so nothing would ever move — and
    /// three copies of one cart standing still read as a drawing fault, not as a ring. Two carts
    /// differ on both counts: the neighbours are a different cart from the selection, and the
    /// row does turn.
    pub fn cart_at_offset(&self, off: i32) -> Option<usize> {
        let n = self.carts.len() as i32;
        if n == 0 {
            return None;
        }
        let at = |off: i32| (self.index as i32 + off).rem_euclid(n) as usize;
        if n == 2 {
            return Some(at(off));
        }
        let r = off.rem_euclid(n);
        let nearest = if r * 2 > n { r - n } else { r };
        (nearest == off).then(|| at(off))
    }

    /// Where the selected cart stands once the row has settled, in offscreen pixels: dead
    /// centre, whatever the row holds. The cart going into the slot and the cart the picker
    /// opens are both drawn by somebody else, starting from where this row left it, so the row
    /// has to be able to say where that is rather than have each of them assume it.
    ///
    /// The width is asked of the selected cart rather than assumed to be `CART_W`, so this is
    /// `draw_row`'s own placement of that cart with `offset` at zero rather than a second copy
    /// of the sum. Both cartridges are 240 wide today, so the number is the same either way;
    /// the point is that it stays the same as what is drawn if a later one is not. `CART_W`
    /// stands in for an empty row, which draws no cart to measure.
    pub fn rest_x(&self) -> f32 {
        let w = self
            .carts
            .get(self.index)
            .map_or(CART_W, |c| cart_box(c.platform).0);
        (OUT_W as f32 - w as f32) / 2.0
    }

    pub fn update(&mut self, dt: f32) {
        let accel = -2.0 * OMEGA * self.vel - OMEGA * OMEGA * (self.scroll - self.scroll_target());
        self.vel += accel * dt;
        self.scroll += self.vel * dt;
    }

    /// The shelf screen: the row of carts and the slot under it. What is printed on the case
    /// is drawn after this, by whoever holds the type.
    pub fn draw(&self, shake: f32, out: &mut Vec<Draw>) {
        self.draw_row(None, shake, 0.0, 1.0, out);
        draw_empty_slot(out);
    }

    /// The row alone. The cart on its way into the slot is drawn by the chrome, at the same
    /// place the row would draw it; leaving it in the row as well puts two of one cart on
    /// screen and the travel then reads as a copy sliding away from the original.
    ///
    /// `shake` displaces the carts and nothing else. On the shelf the frame is mostly
    /// backdrop, so shaking that slides the letterbox in at the edges rather than reading as
    /// a refusal.
    ///
    /// `recede` clears the row for the cart going into the slot: 0.0 leaves it alone, 1.0
    /// has every other cart gone. They part outwards rather than fading in place, so the row
    /// reads as making way for the one that was chosen.
    ///
    /// `dim` darkens the faces further and nothing else: 1.0 leaves them as `recede` has them.
    /// The black under a dimmed cart stays as `recede` alone makes it, so a dimmed cart reads
    /// as a cart in shadow rather than a ghost over the wallpaper.
    pub fn draw_row(
        &self,
        hidden: Option<&str>,
        shake: f32,
        recede: f32,
        dim: f32,
        out: &mut Vec<Draw>,
    ) {
        let recede = recede.clamp(0.0, 1.0);
        let dim = dim.clamp(0.0, 1.0);
        let target = self.scroll_target();
        for slot in -SLOTS..=SLOTS {
            let Some(i) = self.cart_at_offset(slot) else {
                continue;
            };
            let cart = &self.carts[i];
            if hidden == Some(cart.stem.as_str()) {
                continue;
            }
            // How far this cart is from the selection, which is what decides both its size and
            // where on the row it stands: every row is centred on the cart it has selected.
            let offset = target + slot as f32 - self.scroll;
            let t = offset.abs().min(1.0);
            let scale = 1.0 + (SIDE_SCALE - 1.0) * t;
            let alpha = (1.0 + (SIDE_ALPHA - 1.0) * t) * (1.0 - recede);
            let (cw, ch) = cart_box(cart.platform);
            let (w, h) = (cw as f32 * scale, ch as f32 * scale);
            // Away from the middle, and further the further out it already was, so the row
            // opens rather than sliding sideways.
            let away = offset.signum() * (1.0 + offset.abs());
            let x = OUT_W as f32 / 2.0 + offset * PITCH - w / 2.0 + away * PART * recede;
            if x + w <= 0.0 || x >= OUT_W as f32 || alpha <= 0.0 {
                continue;
            }
            let x = x + shake;
            // The floor is this platform's, asked of the cartridge's own full height rather than
            // of the scaled one: a neighbour shrinks upward off a floor it shares with the
            // selection instead of shrinking about its own middle.
            let y = foot_y(ch as f32) - h;
            // Black in the cart's own shape, under the dimmed face. Without it the dimming is
            // transparency, and over a wallpaper the row reads as ghosts of carts.
            if alpha < 1.0 {
                // Three backings for three moulds: it is the cart's own outline, and a class C
                // pak's corners are not a class A/B pak's. See `cart::gb_cart_shadow`.
                //
                // Asked for rather than indexed, the way `faces` is asked for eighteen lines
                // below and for the same reason: `carts` is public, `shells` is not, and a push
                // through the public field would leave this one entry short. Indexed, that is a
                // panic inside the draw loop — on the device a black screen and a dead handset,
                // with no message anywhere — for a row that would otherwise have drawn. A cart
                // whose mould was never recorded gets the straight sided backing, which is the
                // same degrading a cart whose face was never uploaded already gets.
                let backing = match self.shells.get(i).copied().flatten() {
                    None => self.shadow,
                    Some(GbShell::Notched) => self.gb_shadow,
                    Some(GbShell::Rounded) => self.gbc_shadow,
                };
                if let Some(tex) = backing {
                    out.push(Draw::Tex {
                        x,
                        y,
                        w,
                        h,
                        tex,
                        alpha: recede_alpha(alpha),
                    });
                }
            }
            out.push(match self.faces.get(i) {
                Some(tex) => Draw::Tex {
                    x,
                    y,
                    w,
                    h,
                    tex: *tex,
                    alpha: alpha * dim,
                },
                // A cart whose face has not been uploaded still holds its place. A gap in
                // the row would read as a missing game.
                None => {
                    let c = label_colour(&label_text(cart));
                    Draw::Rect {
                        x,
                        y,
                        w,
                        h,
                        colour: [
                            c[0] as f32 / 255.0,
                            c[1] as f32 / 255.0,
                            c[2] as f32 / 255.0,
                            alpha * dim,
                        ],
                    }
                }
            });
        }
    }
}

/// How solid the shadow under a dimmed cart is. It carries the whole of the cart's opacity
/// while the face is translucent over it, and leaves with the face as the row parts.
fn recede_alpha(face_alpha: f32) -> f32 {
    (face_alpha / SIDE_ALPHA).clamp(0.0, 1.0)
}

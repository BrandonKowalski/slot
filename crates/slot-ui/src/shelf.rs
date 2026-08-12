use slot_gfx::{Draw, TexId, OUT_H, OUT_W};
use slot_store::Cart;

use crate::cart::{label_colour, label_text, CART_H, CART_W};
use crate::hud::Millis;
use crate::slot_chrome::draw_empty_slot;

/// Distance between cart centres. Wider than a cart so the neighbours peek in at both
/// edges and the row reads as continuing past them.
/// Chosen so the outer two carts sit fully on screen with the margin at the edge equal to
/// the gap beside the centre cart. At 286 the side carts were clipped 24px off each edge.
const PITCH: f32 = 240.0;
const SIDE_SCALE: f32 = 0.78;
const SIDE_ALPHA: f32 = 0.55;
/// Carts stand on the row rather than float: the foot stays put as a cart shrinks away.
const FOOT_Y: f32 = (OUT_H + CART_H) as f32 / 2.0;
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
    /// The cart silhouette in black, drawn under a dimmed cart. One texture for the whole
    /// row: every cart is the same shape.
    shadow: Option<TexId>,
    vel: f32,
    /// The direction being held and when it next repeats. Repeat lives here rather than in
    /// the gesture layer so nothing in game starts auto firing.
    held: Option<(i32, Millis)>,
}

impl Shelf {
    pub fn new(carts: Vec<Cart>) -> Self {
        Shelf {
            carts,
            index: 0,
            scroll: 0.0,
            faces: Vec::new(),
            shadow: None,
            vel: 0.0,
            held: None,
        }
    }

    /// Face textures in `carts` order. The caller uploads them because only the compositor
    /// can mint a `TexId`.
    pub fn set_shadow(&mut self, face: TexId) {
        self.shadow = Some(face);
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
    }

    /// Where the spring is heading, in the continuous coordinate `scroll` lives in. The row
    /// is a ring, so the selected cart has an image every `n` slots; this is the one nearest
    /// where the row already is, which is what stops a wrap unwinding the whole row.
    pub fn scroll_target(&self) -> f32 {
        let n = self.carts.len();
        if n == 0 {
            return 0.0;
        }
        let n = n as f32;
        self.scroll + (self.index as f32 - self.scroll + n / 2.0).rem_euclid(n) - n / 2.0
    }

    /// The cart `off` slots right of the selection. `None` when the row is empty, or when
    /// this slot would repeat a cart another slot is already showing: with two carts the
    /// left and right neighbours are the same one, and a row holding it twice reads as a
    /// bug. The row is left with a gap instead.
    pub fn cart_at_offset(&self, off: i32) -> Option<usize> {
        let n = self.carts.len() as i32;
        if n == 0 {
            return None;
        }
        let r = off.rem_euclid(n);
        let nearest = if r * 2 > n { r - n } else { r };
        (nearest == off).then(|| (self.index as i32 + off).rem_euclid(n) as usize)
    }

    pub fn update(&mut self, dt: f32) {
        let accel = -2.0 * OMEGA * self.vel - OMEGA * OMEGA * (self.scroll - self.scroll_target());
        self.vel += accel * dt;
        self.scroll += self.vel * dt;
    }

    /// The shelf screen: the row of carts and the slot under it. What is printed on the case
    /// is drawn after this, by whoever holds the type.
    pub fn draw(&self, shake: f32, out: &mut Vec<Draw>) {
        self.draw_row(None, shake, 0.0, out);
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
    pub fn draw_row(&self, hidden: Option<&str>, shake: f32, recede: f32, out: &mut Vec<Draw>) {
        let recede = recede.clamp(0.0, 1.0);
        let target = self.scroll_target();
        for slot in -SLOTS..=SLOTS {
            let Some(i) = self.cart_at_offset(slot) else {
                continue;
            };
            let cart = &self.carts[i];
            if hidden == Some(cart.stem.as_str()) {
                continue;
            }
            let offset = target + slot as f32 - self.scroll;
            let t = offset.abs().min(1.0);
            let scale = 1.0 + (SIDE_SCALE - 1.0) * t;
            let alpha = (1.0 + (SIDE_ALPHA - 1.0) * t) * (1.0 - recede);
            let (w, h) = (CART_W as f32 * scale, CART_H as f32 * scale);
            // Away from the middle, and further the further out it already was, so the row
            // opens rather than sliding sideways.
            let away = offset.signum() * (1.0 + offset.abs());
            let x = OUT_W as f32 / 2.0 + offset * PITCH - w / 2.0 + away * PART * recede;
            if x + w <= 0.0 || x >= OUT_W as f32 || alpha <= 0.0 {
                continue;
            }
            let x = x + shake;
            let y = FOOT_Y - h;
            // Black in the cart's own shape, under the dimmed face. Without it the dimming is
            // transparency, and over a wallpaper the row reads as ghosts of carts.
            if alpha < 1.0 {
                if let Some(tex) = self.shadow {
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
                    alpha,
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
                            alpha,
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

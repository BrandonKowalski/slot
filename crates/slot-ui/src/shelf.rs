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
/// Between repeats, and they get shorter the longer a direction is held.
///
/// A single flat rate has to answer two questions with one number: fast enough to cross a long
/// library, slow enough to stop on the cart you meant. Those pull opposite ways, and at thirty
/// carts the flat 110 ms this used to be was the wrong answer to the first one. Holding longer
/// is the signal that the player is travelling rather than choosing, so the rate reads it: the
/// first repeats stay at the old 110 ms, where stopping on one cart is what matters, and a hold
/// that keeps going winds down to 50 ms, which crosses thirty carts in about two seconds.
///
/// The last entry is the floor and repeats stay there for as long as the direction is held.
/// Nothing here accelerates a *tap*: each press starts the sequence again from the top, so a
/// row of deliberate single presses is paced exactly as it always was.
const REPEAT_MS: [Millis; 4] = [110, 85, 65, 50];

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
    /// laps rather than wrapping. This is what the spring aims at — see `scroll_target` — because
    /// it is the only thing that remembers which button was pressed once the row has wrapped.
    ride: f32,
    vel: f32,
    /// The direction being held, when it next repeats, and how many repeats it has already
    /// fired. Repeat lives here rather than in the gesture layer so nothing in game starts auto
    /// firing, and the count lives with it because the rate is a function of how long this one
    /// hold has been going: a new press resets it, which is what keeps taps unaccelerated.
    held: Option<(i32, Millis, usize)>,
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

    /// Up and Down: to the first cart of the next letter, or the previous one.
    ///
    /// The row is a ring, so the last letter's Down reaches the first letter by carrying on
    /// forwards rather than by turning round, and the first letter's Up reaches the last the same
    /// way. That costs a long slide once round a big row, which is the honest picture of what the
    /// press asked for; turning round instead would show the row travelling one way while the
    /// player pressed the other.
    ///
    /// Up goes to the *start* of the previous letter rather than to the cart before this one, so
    /// a row stopped halfway through the Ms lands on the first M rather than stepping back into
    /// the Ls. Pressed again from there it does reach the Ls, because by then it is already at
    /// the letter's start.
    pub fn jump_next_letter(&mut self) {
        self.jump(1);
    }

    pub fn jump_prev_letter(&mut self) {
        self.jump(-1);
    }

    /// The first cart of the letter `from` is filed under.
    fn start_of_letter(&self, from: usize) -> usize {
        let n = self.carts.len();
        let letter = slot_store::initial(&self.carts[from].stem);
        let mut at = from;
        for _ in 0..n {
            let before = (at as i32 - 1).rem_euclid(n as i32) as usize;
            if slot_store::initial(&self.carts[before].stem) != letter {
                break;
            }
            at = before;
        }
        at
    }

    fn jump(&mut self, dir: i32) {
        let n = self.carts.len();
        if n < 2 {
            return;
        }
        let wrap = |i: i32| i.rem_euclid(n as i32) as usize;
        let here = slot_store::initial(&self.carts[self.index].stem);
        // A row filed under one letter has nowhere to go, in either direction. Said once here
        // rather than left to the walks below, which have no previous letter to find and would
        // wander the ring looking for one.
        if self
            .carts
            .iter()
            .all(|c| slot_store::initial(&c.stem) == here)
        {
            return;
        }
        let target = match dir > 0 {
            true => {
                let mut at = self.index;
                for _ in 0..n {
                    at = wrap(at as i32 + 1);
                    if slot_store::initial(&self.carts[at].stem) != here {
                        break;
                    }
                }
                at
            }
            false => {
                let start = self.start_of_letter(self.index);
                match start == self.index {
                    true => self.start_of_letter(wrap(start as i32 - 1)),
                    false => start,
                }
            }
        };
        // Signed the way the press asked, never the short way round, so the row is never seen
        // travelling one way while the player is pressing the other.
        let ahead = (target as i32 - self.index as i32).rem_euclid(n as i32);
        let delta = match dir > 0 {
            true => ahead,
            false => ahead - n as i32,
        };
        self.index = target;
        self.ride += delta as f32;
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
        self.held = Some((by, now + REPEAT_DELAY_MS, 0));
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
        if matches!(self.held, Some((held, _, _)) if held == by) {
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
        let Some((by, due, fired)) = self.held else {
            return;
        };
        if now < due {
            return;
        }
        self.step(by);
        // Due from `now` rather than from the deadline it passed, and at the rate this hold has
        // wound down to. `fired` saturates on the last entry, so a long hold settles at the
        // floor instead of ever reaching zero.
        let rate = REPEAT_MS[fired.min(REPEAT_MS.len() - 1)];
        self.held = Some((by, now + rate, fired + 1));
    }

    /// A press, and nothing at all on a row with fewer than two carts. An empty row has nothing
    /// to select and a row of one has nothing else to select, so neither can answer a shoulder —
    /// and `cart_at_offset` says as much: a lone cart is drawn in the middle and the row holds no
    /// other slot to move into.
    ///
    /// The one-cart case has to stop here rather than fall through to arithmetic that happens to
    /// leave `index` alone, because `ride` is not `index`. `ride` counts presses rather than
    /// wrapping — that is what lets `scroll_target` answer a wrap the way the button asked — and
    /// on a ring of one the wrap is degenerate: `(index - ride + 0.5).rem_euclid(1.0)` is 0.5 for
    /// every whole `ride`, so the target *is* `ride`, and a press that added to it would send the
    /// row a whole pitch after a cart that is not there. Composited, that is the only cartridge
    /// on the shelf thrown a pitch to the right and shrunk to `SIDE_SCALE` on the press, sliding
    /// back to the middle over the next third of a second; held, the 110 ms repeat kicks it out
    /// again before it lands and the lone cart shimmies sideways for as long as the button is
    /// down. Which is a carousel of one turning, and a carousel of one does not turn.
    fn step(&mut self, by: i32) {
        let n = self.carts.len();
        if n < 2 {
            return;
        }
        self.index = (self.index as i32 + by).rem_euclid(n as i32) as usize;
        self.ride += by as f32;
    }

    /// Where the spring is heading, in the continuous coordinate `scroll` lives in. The row is a
    /// ring, so the selected cart has an image every `n` slots, and the one to head for is the
    /// one a single press away in the direction that press asked for — never a lap of the row.
    ///
    /// This used to measure from `scroll`, taking whichever image stood nearest where the row
    /// already was. That reads as the short way round and mostly is, but the row is rarely where
    /// it is heading: under the 110 ms repeat the spring is still up to half a pitch behind its
    /// target when the next press lands, and that press asks for an image one slot further on
    /// again. Measured from a row that is `lag` pitches behind, the image the press asked for is
    /// `lag + 1` away, so once `lag` passes `n / 2 - 1` the image *behind* the row is the nearer
    /// one and the row sets off against the button being held. That threshold is zero pitches on
    /// a ring of two, half a pitch on a ring of three — which both a held scroll and a second tap
    /// inside five frames clear — and a whole pitch or more from four carts up, which nothing the
    /// shelf can produce reaches. So a row of two reversed on every press; a row of three ran
    /// backwards for about eight frames out of every twenty eight under a hold, and answered two
    /// quick right taps by sliding one pitch *left* instead of two right, reaching the correct
    /// cart by the wrong road; and longer rows were never wrong. That is the report exactly: it
    /// looks wrong on two and three carts, and the ten cart shelf is fine.
    ///
    /// Adding the presses up answers it for every length at once. `ride` counts laps instead of
    /// wrapping, so one press is one slot the way it was pressed whatever the row is doing at the
    /// time, and the row still never unwinds: a single step round a ring *is* the short way round.
    /// Simulated against the old rule frame by frame under a held scroll, this is identical from
    /// four carts up — which is every row size nobody has reported anything wrong with.
    ///
    /// Wrapping `ride` back onto the ring is what keeps this honest if `index` was moved without
    /// it: the answer is still an image of the cart that is actually selected, and it is the image
    /// nearest where the row was already heading.
    pub fn scroll_target(&self) -> f32 {
        let n = self.carts.len();
        if n == 0 {
            return 0.0;
        }
        let from = self.ride;
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
    ///
    /// Every other length fills every slot too, for exactly the reason the row of two does.
    /// This used to hand each cart *one* image — the slot nearest the selection — and hold the
    /// rest of the ring empty. That reads correctly while the row is still, because at rest only
    /// the three middle slots are on screen and three or more carts fill all three. It does not
    /// survive a press. A press moves the selection before the spring has moved the row, so for
    /// the length of the travel every image is one slot further from the selection than it is
    /// from the screen's middle, and the image the rule refuses to hand out is the one leaving
    /// the frame. At three carts and at four, the cart standing in the left slot was still fully
    /// on screen when the press struck it out of the row: it vanished where it stood instead of
    /// sliding off the edge, and the left third of the screen then stayed bare — 343 px of it
    /// under a held scroll — until the row settled. Composited and looked at, a ten cart row
    /// carries a cart off the left edge on the same frames that a three cart row shows black.
    ///
    /// So the ring is a ring at every length, and which images are on screen is `draw_row`'s
    /// bounds check to decide rather than this. At rest that check still leaves exactly the three
    /// middle slots standing, so a settled row of three or more shows each of its carts once and
    /// nothing is repeated; from five carts up the ring is wider than the screen and nothing is
    /// ever repeated at all. Three and four carts do show one cart at both edges at once while
    /// the row is moving, each half out of frame — which is a ring shorter than the window, drawn
    /// honestly, and is the same thing the row of two does on every frame.
    pub fn cart_at_offset(&self, off: i32) -> Option<usize> {
        let n = self.carts.len() as i32;
        if n == 0 {
            return None;
        }
        if n == 1 {
            return (off == 0).then_some(self.index);
        }
        Some((self.index as i32 + off).rem_euclid(n) as usize)
    }

    /// Where the row is drawing its selected cart *this frame*: the left edge of its quad, and
    /// how big that quad is against the cartridge's own size. The cart going into the slot and
    /// the cart the core picker opens are both drawn by somebody else, taking over from this
    /// row mid-movement, so the row has to be able to say where it had the cart rather than
    /// have each of them assume.
    ///
    /// This frame and not once the spring has settled, which is the whole of what it is for. A
    /// settled row has its selection dead centre at full size and that is what this answers for
    /// it — but nothing makes the player wait for the spring before pressing A or START. Press
    /// either while the row is still travelling and the selection is somewhere between two
    /// slots, shrunken as much as `SIDE_SCALE`, with its foot on the row's floor; answering
    /// "dead centre, full size" then is a cartridge that jumps up to 266 px sideways and grows a
    /// third on the frame the button goes down. Composited and looked at, the chosen cart
    /// teleports into the middle of the screen while the cart it was passing is still sliding.
    ///
    /// The sum is `draw_row`'s own for the slot the selection is in, with `recede` at zero
    /// because nothing has begun to part yet — not a second copy of it. The width is asked of
    /// the selected cart rather than assumed to be `CART_W`: both cartridges are 240 wide today
    /// and the number is the same either way, and the point is that it stays the same as what is
    /// drawn if a later one is not. `CART_W` stands in for an empty row, which draws no cart to
    /// measure.
    pub fn selected_at(&self) -> (f32, f32) {
        let w = self
            .carts
            .get(self.index)
            .map_or(CART_W, |c| cart_box(c.platform).0) as f32;
        let offset = self.scroll_target() - self.scroll;
        let scale = shrink(offset);
        (OUT_W as f32 / 2.0 + offset * PITCH - w * scale / 2.0, scale)
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
            let scale = shrink(offset);
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

/// How big a cart `offset` pitches from the selection is drawn, against its own size. Full at
/// the selection and `SIDE_SCALE` from a pitch out, and one function rather than one sum in
/// `draw_row` and another in `selected_at`: those two have to agree about the selected cart on
/// every frame of the spring, or the cart handed to the slot is not the size the row had it.
fn shrink(offset: f32) -> f32 {
    1.0 + (SIDE_SCALE - 1.0) * offset.abs().min(1.0)
}

/// How solid the shadow under a dimmed cart is. It carries the whole of the cart's opacity
/// while the face is translucent over it, and leaves with the face as the row parts.
fn recede_alpha(face_alpha: f32) -> f32 {
    (face_alpha / SIDE_ALPHA).clamp(0.0, 1.0)
}

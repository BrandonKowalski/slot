use std::sync::OnceLock;

use slot_gfx::{Draw, TexId, OUT_H, OUT_W};
use slot_store::Cart;
use slot_store::Theme;

use crate::cart::{label_colour, label_text, CART_H, CART_W};
use crate::icon::icon_box;

/// Big enough to read as a symbol on a 240 px cart rather than as a mark on its label.
pub const ALERT_PX: f32 = 44.0;

/// The bottom of the device. Deep enough to read as the cart bay it is: a 40px line under a
/// 135px cart was a detail the cart towered over, and the insert only touched it for the
/// last seventh of the travel.
pub const MOUTH_H: f32 = 58.0;
const BAND_Y: f32 = OUT_H as f32 - MOUTH_H;

/// The opening. It is the one piece of the slot that sits *behind* the cart: it is the
/// inside of the machine, so a cart on its way through fills it. Everything else is plastic
/// and draws in front, which is what cuts the cart off.
pub const MOUTH_W: f32 = CART_W as f32 + 14.0;
const SLIT_H: f32 = 9.0;
const MOUTH_X: f32 = (OUT_W as f32 - MOUTH_W) / 2.0;
const SLIT_Y: f32 = BAY_Y + 5.0;

/// The lit front edge of the slot: the line the cart is cut off at.
pub const LIP_H: f32 = 2.0;

/// The bay the slot sits in, stepped down from the outer shell and wider than the opening.
const BAY_W: f32 = MOUTH_W + 18.0;
const BAY_X: f32 = (OUT_W as f32 - BAY_W) / 2.0;
const BAY_Y: f32 = BAND_Y + LIP_H;
/// The thumb scoop: one broad arc across the middle of the bay's near wall, which is how you
/// get hold of a cart to pull it out. Not two notches at the ends, which is what was here
/// before and is not what an SP has. It runs nearly the whole opening.
const SCOOP_W: f32 = MOUTH_W * 0.88;
const SCOOP_D: f32 = RECESS_H - (SCOOP_Y - BAY_Y);
const SCOOP_Y: f32 = SLIT_Y + SLIT_H;

/// How deep you can see into the slot, and so how much of a seated cart shows in it. Set just
/// past the cart's own label inset, so a seated cart shows its moulded grip and the top edge
/// of its label through the thumb scoop, and nothing readable.
const RECESS_H: f32 = 42.0;
/// The lit edge of the plastic where it is cut away for the scoop.
const RIM_W: f32 = 2.0;
const CX: f32 = OUT_W as f32 / 2.0;
/// Flatness of the arc through the middle. An ellipse bottoms out in a curve where the real
/// scoop runs almost level and then turns up hard at the ends.
const SCOOP_FLAT: f32 = 4.0;

/// The palette, from `System/theme.txt` if the card carries one. Set once at boot and read
/// every frame after, because the slot is drawn from four screens and threading a theme
/// through all of them buys nothing: it cannot change while the device is on.
static THEME: OnceLock<Theme> = OnceLock::new();

/// Once, at boot. A second call is ignored rather than fought over: the card is read once and
/// there is no screen that changes this.
pub fn set_theme(theme: Theme) {
    let _ = THEME.set(theme);
}

pub fn theme() -> &'static Theme {
    THEME.get_or_init(Theme::default)
}

fn rgb(c: [u8; 3]) -> [f32; 4] {
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        1.0,
    ]
}

/// The case. Every band has to clear its neighbour, which `slot-ui/tests/contrast.rs` holds
/// for the default palette. A theme is the card's own business past that.
pub fn housing() -> [f32; 4] {
    rgb(theme().housing)
}

pub fn opening() -> [f32; 4] {
    rgb(theme().opening)
}

pub fn edge() -> [f32; 4] {
    rgb(theme().edge)
}

/// The floor of the bay: a step down from the shell, not a second opening. Subtle on purpose,
/// since it is a moulding line rather than something to read.
pub fn recess() -> [f32; 4] {
    rgb(theme().recess)
}

const LIP_Y: f32 = BAND_Y;

/// Where a cart stands before it is pushed in. Same place the shelf draws the selected cart,
/// so the handoff out of the shelf is not a jump.
const REST_Y: f32 = (OUT_H - CART_H) as f32 / 2.0;

/// Where the cart stops. In means *in*, not gone: it comes to rest filling the opening, so
/// the base of the slot is covered by the cart rather than going dark again. Four pixels
/// below the top of the recess, which leaves the far wall showing above the cart's rounded
/// top edge instead of butting it flat against the lip.
const SEATED_Y: f32 = BAY_Y + 4.0;
const CART_X: f32 = (OUT_W - CART_W) as f32 / 2.0;

/// How far into the travel the cart's bottom edge reaches the lip. Derived rather than
/// tuned, because it is where the catch has to be to read as one.
const CATCH_AT: f32 = (LIP_Y - CART_H as f32 - REST_Y) / (SEATED_Y - REST_Y);
/// The seat either side of the catch. It opens a little before halfway because the cart is
/// resting on the lip for the whole of it, and the push comes after.
const CATCH_IN: f32 = 0.42;
const CATCH_OUT: f32 = 0.62;
/// How far the cart creeps while it is caught. Not zero: a dead stop reads as a dropped
/// frame, a crawl reads as resistance.
const CREEP: f32 = 0.03;

pub struct SlotChrome<'a> {
    pub cart: &'a Cart,
    pub face: Option<TexId>,
    /// 0.0 standing where the shelf left it, 1.0 swallowed by the mouth.
    pub seat: f32,
    /// The refusal symbol and how far into its fade it is. A cart that will not seat says so
    /// this way: it is right there and can carry a mark, where a jitter beside the slot reads
    /// as a rendering fault. `None` once the binary has nothing to say, or before it has
    /// uploaded the glyph.
    pub alert: Option<(TexId, f32)>,
    /// Alpha of the black veil over the layer behind: the shelf on the way in, the live
    /// game on the way out.
    pub dim: f32,
    /// How far up the screen behind the slot is, 0.0 dark and 1.0 fully on. The housing is
    /// solid over a dark screen and gone over a lit one, so the picture is never left with
    /// a black bar across the bottom of it.
    pub screen: f32,
    /// Whether there is a picture to show at all. False while the core is still loading,
    /// where the game texture still holds whatever the last cart left in it.
    pub game: bool,
}

impl SlotChrome<'_> {
    pub fn draw(&self, out: &mut Vec<Draw>) {
        let seat = self.seat.clamp(0.0, 1.0);
        let dim = self.dim.clamp(0.0, 1.0);
        if dim > 0.0 {
            out.push(Draw::Rect {
                x: 0.0,
                y: 0.0,
                w: OUT_W as f32,
                h: OUT_H as f32,
                colour: [0.0, 0.0, 0.0, dim],
            });
        }

        let chrome = 1.0 - self.screen.clamp(0.0, 1.0);

        // Behind the cart: the opening. It is the inside of the machine, so the cart fills it
        // on the way through rather than sliding behind a painted bar.
        draw_slot_back(chrome, out);

        let x = CART_X;
        let y = REST_Y + (SEATED_Y - REST_Y) * travel(seat);
        // The cart fades with the case rather than through it. A seated cart is really in the
        // slot and has to be drawn, so the whole device face has to leave as one object as the
        // picture takes over. Held at full while the screen is off, which is all of the travel.
        let cart_alpha = if seat >= 1.0 { chrome } else { 1.0 };
        out.push(match self.face {
            Some(tex) => Draw::Tex {
                x,
                y,
                w: CART_W as f32,
                h: CART_H as f32,
                tex,
                alpha: cart_alpha,
            },
            None => {
                let c = label_colour(&label_text(self.cart));
                Draw::Rect {
                    x,
                    y,
                    w: CART_W as f32,
                    h: CART_H as f32,
                    colour: [
                        c[0] as f32 / 255.0,
                        c[1] as f32 / 255.0,
                        c[2] as f32 / 255.0,
                        cart_alpha,
                    ],
                }
            }
        });

        // On the cart, so it goes behind the mouth with it: the alert leaves the way the
        // cart does rather than hanging in the opening after it.
        if let Some((tex, alpha)) = self.alert {
            let (w, h) = icon_box(ALERT_PX);
            let (w, h) = (w as f32, h as f32);
            out.push(Draw::Tex {
                x: x + (CART_W as f32 - w) / 2.0,
                y: y + (CART_H as f32 - h) / 2.0,
                w,
                h,
                tex,
                alpha,
            });
        }

        // After the cart and before the housing: the panel is the front surface of the
        // device, so a cart already in the slot is behind the picture the moment it lights.
        if self.game && self.screen > 0.0 {
            out.push(Draw::Game);
        }

        // In front of the cart: the plastic. This is what occludes, and it is what the cart
        // disappears behind.
        draw_slot_front(chrome, out);
    }
}

fn band(x: f32, y: f32, w: f32, h: f32, c: [f32; 4], alpha: f32) -> Draw {
    Draw::Rect {
        x,
        y,
        w,
        h,
        colour: [c[0], c[1], c[2], c[3] * alpha],
    }
}

/// Everything you can see *into*: the bay floor, the opening, and the thumb scoop. All of it
/// draws behind the cart, because all of it is a hole. The scoop especially: it is a
/// cut-away in the near plastic, and the whole point of it is that you can see and grip the
/// cart through it. Painted in front, it was a dark arc lying on top of the cart.
fn draw_slot_back(alpha: f32, out: &mut Vec<Draw>) {
    // The top bar of the slot goes back here with the hole it spans. It is the front edge of
    // the case and a cart really does pass behind it, but at two pixels over a 240 px cart
    // all it does is draw a line across the label, and the cart reads as sliding behind a bar
    // rather than into an opening.
    out.push(band(BAY_X, BAND_Y, BAY_W, LIP_H, housing(), alpha));
    out.push(band(MOUTH_X, BAND_Y, MOUTH_W, LIP_H, edge(), alpha));
    out.push(band(BAY_X, BAY_Y, BAY_W, RECESS_H, recess(), alpha));
    out.push(band(MOUTH_X, SLIT_Y, MOUTH_W, SLIT_H, opening(), alpha));
    for_each_scoop_span(|x, w, depth| {
        out.push(band(x, SCOOP_Y, w, depth, opening(), alpha));
    });
}

/// The plastic. Pieces around the hole, never over it: this is the only thing that occludes
/// the cart, and what it leaves uncovered is exactly the shape of the recess.
fn draw_slot_front(alpha: f32, out: &mut Vec<Draw>) {
    let w = OUT_W as f32;
    let right = BAY_X + BAY_W;
    let floor = SCOOP_Y + SCOOP_D + RIM_W;
    out.push(band(0.0, BAND_Y, BAY_X, MOUTH_H, housing(), alpha));
    out.push(band(right, BAND_Y, w - right, MOUTH_H, housing(), alpha));

    // Beside the arc, where the bay is wider than the scoop.
    let near = CX - SCOOP_W / 2.0;
    out.push(band(
        BAY_X,
        SCOOP_Y,
        near - BAY_X,
        floor - SCOOP_Y,
        housing(),
        alpha,
    ));
    let far = CX + SCOOP_W / 2.0;
    out.push(band(
        far,
        SCOOP_Y,
        right - far,
        floor - SCOOP_Y,
        housing(),
        alpha,
    ));

    // The arc itself: the plastic under the cut, then its lit edge over the top of it. The
    // highlight goes last so nothing is painted over it.
    for_each_scoop_span(|x, w, depth| {
        let top = SCOOP_Y + depth + RIM_W;
        out.push(band(x, top, w, floor - top, housing(), alpha));
    });
    out.push(band(0.0, floor, w, OUT_H as f32 - floor, housing(), alpha));
    for_each_scoop_span(|x, w, depth| {
        out.push(band(x, SCOOP_Y + depth, w, RIM_W, edge(), alpha));
    });
}

/// The arc, walked by column and merged into spans of equal depth, so the hole and the
/// plastic beside it cannot drift apart.
///
/// Walking it by depth instead is what made it jagged: the trough is deliberately flat, so
/// half the arc's width shares its last pixel or two of depth, and the bottom of the curve
/// came out as one step a hundred pixels wide.
fn for_each_scoop_span(mut span: impl FnMut(f32, f32, f32)) {
    let hw = SCOOP_W / 2.0;
    let depth = |x: f32| SCOOP_D * (1.0 - (x.abs() / hw).powf(SCOOP_FLAT)).max(0.0);
    let mut x = -hw;
    while x < hw {
        let d = depth(x + 0.5).round();
        let start = x;
        while x < hw && depth(x + 0.5).round() == d {
            x += 1.0;
        }
        span(CX + start, x - start, d);
    }
}

/// The slot with nothing going into it. The shelf shows it so the cart you pick has a
/// visible place to go, and so the bottom of the screen is the same object on every screen
/// rather than appearing only during the animation.
pub fn draw_empty_slot(out: &mut Vec<Draw>) {
    // Both halves. The recess is a hole in the front pieces, so a slot drawn from the front
    // alone is a hole onto the backdrop rather than an opening in a device.
    draw_slot_back(1.0, out);
    draw_slot_front(1.0, out);
}

/// The travel, in three parts: the cart falls to the lip, rests on it, then is pushed
/// through and settles. A single ease covers the same ground but arrives seated without ever
/// having met anything, which is what makes it read as a card going down a chute.
fn travel(seat: f32) -> f32 {
    if seat < CATCH_IN {
        CATCH_AT * ease(seat / CATCH_IN)
    } else if seat < CATCH_OUT {
        CATCH_AT + CREEP * (seat - CATCH_IN) / (CATCH_OUT - CATCH_IN)
    } else {
        let caught = CATCH_AT + CREEP;
        caught + (1.0 - caught) * ease((seat - CATCH_OUT) / (1.0 - CATCH_OUT))
    }
}

/// Smootherstep. Zero velocity at both ends, so the two halves of the travel meet the catch
/// without a step in speed.
fn ease(u: f32) -> f32 {
    u * u * u * (u * (u * 6.0 - 15.0) + 10.0)
}

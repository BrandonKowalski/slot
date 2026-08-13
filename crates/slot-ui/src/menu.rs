//! The shelf's menu: the short list of things that have nowhere else to live. Opened with
//! SELECT and START, and only from the shelf.
//!
//! It exists so an affordance does not have to be a chord nobody can discover. Two entries is
//! not much of a list, but the reason for it is the next one.

use slot_gfx::{Draw, TexId, OUT_H, OUT_W};

use crate::hud::{PLATE, PLATE_H};
use crate::plate::{word_width, HINT_H};

/// One row per entry, tall enough that a row is a target rather than a line of type.
const ROW_H: f32 = 34.0;

/// Inside the panel, around the rows.
const PAD: f32 = 16.0;

/// The gutter the caret sits in, held on every row so the labels do not shift sideways as the
/// cursor moves.
const CARET_GUTTER: f32 = 18.0;
const CARET_W: f32 = 5.0;
const CARET_H: f32 = 5.0;

/// The mark on the selected row, in the ink the hints are set in.
const CARET: [f32; 4] = [0.96, 0.95, 0.94, 1.0];

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum MenuItem {
    RelinkAdb,
    About,
}

impl MenuItem {
    pub const ALL: [MenuItem; 2] = [MenuItem::RelinkAdb, MenuItem::About];

    /// Position in `ALL`, which is the order the binary rasterises and uploads faces in.
    pub fn index(self) -> usize {
        self as usize
    }

    /// Lower case, like the hint row: the case band and the hints are the house style, and a
    /// menu in title case would read as a different program.
    pub fn label(self) -> &'static str {
        match self {
            MenuItem::RelinkAdb => "relink adb",
            MenuItem::About => "about",
        }
    }
}

/// The list and where the cursor is on it. The faces belong to the binary, which is the only
/// thing here with a GL context; this holds the handles and decides where they go.
#[derive(Default, Debug)]
pub struct Menu {
    cursor: usize,
    /// In `MenuItem::ALL` order, and empty until the binary has uploaded them.
    faces: Vec<TexId>,
}

impl Menu {
    pub fn new() -> Menu {
        Menu::default()
    }

    /// Clamped rather than wrapped. Two entries are a line to walk along, and rolling from the
    /// bottom to the top of a list this short reads as a slip.
    pub fn up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn down(&mut self) {
        self.cursor = (self.cursor + 1).min(MenuItem::ALL.len() - 1);
    }

    pub fn selected(&self) -> MenuItem {
        MenuItem::ALL[self.cursor]
    }

    pub fn set_faces(&mut self, faces: Vec<TexId>) {
        self.faces = faces;
    }

    /// Wide enough for the longest entry, so adding one never leaves a label clipped.
    fn panel_w() -> f32 {
        let widest = MenuItem::ALL
            .iter()
            .map(|i| word_width(i.label()))
            .max()
            .unwrap_or(0) as f32;
        widest + CARET_GUTTER + PAD * 2.0
    }

    fn panel_h() -> f32 {
        MenuItem::ALL.len() as f32 * ROW_H + PAD * 2.0
    }

    pub fn draw(&self, out: &mut Vec<Draw>) {
        let (w, h) = (Menu::panel_w(), Menu::panel_h());
        let x = (OUT_W as f32 - w) / 2.0;
        // Above centre by half a plate, so the panel sits where the eye already is rather than
        // dead centre over the cart it opened from.
        let y = (OUT_H as f32 - h) / 2.0 - PLATE_H / 2.0;
        out.push(Draw::Rect {
            x,
            y,
            w,
            h,
            colour: PLATE,
        });

        for (n, item) in MenuItem::ALL.iter().enumerate() {
            let row_y = y + PAD + n as f32 * ROW_H;
            if n == self.cursor {
                out.push(Draw::Rect {
                    x: x + PAD,
                    y: row_y + (ROW_H - CARET_H) / 2.0,
                    w: CARET_W,
                    h: CARET_H,
                    colour: CARET,
                });
            }
            // The row holds its place whether or not its face arrived, for the same reason a
            // cart with no face still holds its slot on the shelf.
            let Some(tex) = self.faces.get(item.index()).copied() else {
                continue;
            };
            let label_w = word_width(item.label()) as f32;
            out.push(Draw::Tex {
                x: x + PAD + CARET_GUTTER,
                y: row_y + (ROW_H - HINT_H as f32) / 2.0,
                w: label_w,
                h: HINT_H as f32,
                tex,
                alpha: 1.0,
            });
        }
    }
}

use slot_gfx::{Draw, TexId, OUT_H, OUT_W};
use slot_power::Battery;
use slot_store::{parse_stamp, StateEntry};

use crate::art;
use crate::battery::{draw_gauge, GAUGE_H};
use crate::footer::{draw_printed, Printed};
use crate::hud::{PLATE, PLATE_H};
use crate::plate::{hint_quad, hint_row, hint_width, Hint, HINT_GAP, HINT_H, TITLE_H, TITLE_W};

/// The screenshot is the screen. 240x160 scales to 720x480 at exactly 3x, the same integer
/// scale the game runs at, so the switcher shows the frame that was paused rather than a
/// resample of it.
pub const PHOTO_W: u32 = 240;
pub const PHOTO_H: u32 = 160;

/// Stands in for a picture that was never written. Neutral rather than black, so an entry
/// missing its thumbnail reads as a blank screen instead of a dead one.
const BLANK: [u8; 3] = [0x3a, 0x3a, 0x3e];

pub const DOT: f32 = 6.0;
const DOT_GAP: f32 = 10.0;
/// The unselected dots. Present enough to count, faint enough that the selected one is
/// unmistakable.
const DOT_DIM: f32 = 0.35;

/// Clear of both ends of the bottom plate, and now both ends of the top one too: the gauge
/// sits here on the left and the clock on the right, the same margin the case band uses for
/// the same pair.
const MARGIN: f32 = 16.0;

/// Every action the switcher takes is on it: an unlabelled button is one nobody presses. The
/// ways out of the switcher come first and the things done to the state on screen after, since
/// that is the split the plate is laid out on.
pub const LEGEND: [(&str, &str); 3] = [("B", "Back"), ("Y", "Delete"), ("A", "Load")];
/// How many of `hints` belong to the left end of the plate. The rest go to the right, the undo
/// with them: undoing acts on the entry under the eye, as loading it does.
const WAYS_OUT: usize = 2;
const UNDO_KEY: &str = "X";

pub struct PhotoFace {
    pub rgba: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

/// The screenshot alone, at its own size. The chrome is a draw list, so nothing is baked
/// into the picture and the 3x upscale stays exact.
pub fn photo_face(entry: &StateEntry) -> PhotoFace {
    let rgba = art::cover(&entry.thumb, PHOTO_W, PHOTO_H).unwrap_or_else(|| {
        std::iter::repeat_n(
            [BLANK[0], BLANK[1], BLANK[2], 255],
            (PHOTO_W * PHOTO_H) as usize,
        )
        .flatten()
        .collect()
    });
    PhotoFace {
        rgba,
        w: PHOTO_W,
        h: PHOTO_H,
    }
}

/// The ring, one screenshot at a time. Opening pauses the game; `A` loads the selection and
/// `B` or MENU puts it back.
pub struct Polaroids {
    pub entries: Vec<StateEntry>,
    pub index: usize,
    faces: Vec<TexId>,
    title: Option<TexId>,
    /// One per hint, in `hints` order. Shorter than `hints` while the undo has been offered
    /// but not yet rasterised, which is why they are read by index rather than zipped.
    hint_faces: Vec<TexId>,
    /// What the undo says, and whether it is on offer at all. Pushed from the app, whose
    /// clock the grace period runs on.
    undo: Option<String>,
}

impl Polaroids {
    pub fn new(entries: Vec<StateEntry>) -> Self {
        Polaroids {
            entries,
            index: 0,
            faces: Vec::new(),
            title: None,
            hint_faces: Vec::new(),
            undo: None,
        }
    }

    /// In `entries` order, newest first. The caller uploads them because only the
    /// compositor can mint a `TexId`.
    pub fn set_faces(&mut self, faces: Vec<TexId>) {
        self.faces = faces;
    }

    /// Re-uploaded whenever the selection moves, since the title names the selection.
    pub fn set_title_face(&mut self, face: Option<TexId>) {
        self.title = face;
    }

    /// In `hints` order.
    pub fn set_hint_faces(&mut self, faces: Vec<TexId>) {
        self.hint_faces = faces;
    }

    pub fn set_undo(&mut self, label: Option<&str>) {
        self.undo = label.map(str::to_string);
    }

    /// In plate order, left end then right. The three fixed ones are always there; the undo
    /// only while the grace period is running, and last so the others never move under it.
    pub fn hints(&self) -> Vec<Hint> {
        let mut out = hint_row(&LEGEND);
        if let Some(label) = &self.undo {
            out.push(Hint {
                key: UNDO_KEY,
                label: label.clone(),
            });
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn selected(&self) -> Option<&StateEntry> {
        self.entries.get(self.index)
    }

    /// How many states there are and which one is showing.
    pub fn dots(&self) -> (usize, usize) {
        (self.entries.len(), self.index)
    }

    /// Relative time of the selected entry, which is the only thing the top plate says.
    pub fn title(&self, now: &str) -> String {
        match self.selected() {
            Some(e) => Self::relative_time(&e.stamp, now),
            None => String::new(),
        }
    }

    /// Drops the entry under the eye, and its face with it: the two are indexed together and
    /// a face left behind would put the wrong picture under every dot after this one.
    pub fn remove_selected(&mut self) {
        if self.index >= self.entries.len() {
            return;
        }
        self.entries.remove(self.index);
        if self.index < self.faces.len() {
            self.faces.remove(self.index);
        }
        self.index = self.index.min(self.entries.len().saturating_sub(1));
    }

    pub fn left(&mut self) {
        self.index = self.index.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.index = (self.index + 1).min(self.entries.len().saturating_sub(1));
    }

    /// `battery`, `battery_percent`, `bolt` and `clock` are the same snapshot the case band
    /// reads, pushed in fresh on every call rather than cached on `self`: the switcher stays
    /// open for as long as the device is charged, and a value latched at construction would
    /// freeze the gauge for the whole time it is on screen. The switcher does not own this
    /// data, it is only shown it.
    pub fn draw(
        &self,
        battery: Option<Battery>,
        battery_percent: Printed,
        bolt: Option<TexId>,
        clock: Printed,
        out: &mut Vec<Draw>,
    ) {
        self.draw_photo(out);
        self.draw_top(battery, battery_percent, bolt, clock, out);
        self.draw_bottom(out);
    }

    fn draw_photo(&self, out: &mut Vec<Draw>) {
        let (w, h) = (OUT_W as f32, OUT_H as f32);
        out.push(match self.faces.get(self.index) {
            // Through the game pass, not over it: the shot is a still of the same panel at
            // the same 3x, so it carries the same mask the live frame does.
            Some(tex) => Draw::Shot { tex: *tex },
            // Opaque, and the same colour a missing thumbnail decodes to. The paused game is
            // still underneath, and a screenshot it showed through would read as live.
            None => Draw::Rect {
                x: 0.0,
                y: 0.0,
                w,
                h,
                colour: [
                    BLANK[0] as f32 / 255.0,
                    BLANK[1] as f32 / 255.0,
                    BLANK[2] as f32 / 255.0,
                    1.0,
                ],
            },
        });
    }

    fn draw_top(
        &self,
        battery: Option<Battery>,
        battery_percent: Printed,
        bolt: Option<TexId>,
        clock: Printed,
        out: &mut Vec<Draw>,
    ) {
        out.push(plate(0.0));
        // No placeholder: the title is type, and absent type is absent rather than a grey
        // bar sitting where a sentence should be.
        if let Some(tex) = self.title {
            out.push(Draw::Tex {
                x: (OUT_W as f32 - TITLE_W as f32) / 2.0,
                y: (PLATE_H - TITLE_H as f32) / 2.0,
                w: TITLE_W as f32,
                h: TITLE_H as f32,
                tex,
                alpha: 1.0,
            });
        }

        // The centred title on this screen is itself a timestamp — when the state was taken.
        // The gauge and the clock are live device status, and land at the same two corners
        // the case band already uses them at — battery left, clock right — so the switcher
        // reads as the same status the case showed a moment ago, not a mirrored one.
        //
        // No capsule at all rather than an empty one: a device with no battery node has
        // nothing to say. `draw_gauge` already early-returns on `None`, the same way the
        // footer calls it, so the two screens degrade identically rather than one of them
        // guarding a call the callee already guards.
        draw_gauge(
            MARGIN,
            (PLATE_H - GAUGE_H) / 2.0,
            battery,
            battery_percent,
            bolt,
            out,
        );
        // `draw_printed` rather than a bare `Draw::Tex`: a clock whose face has not arrived
        // yet still has to hold its space at the margin, not leave the corner looking like
        // nothing was ever going to sit there.
        if clock.w > 0 {
            let x = OUT_W as f32 - MARGIN - clock.w as f32;
            draw_printed(x, (PLATE_H - HINT_H as f32) / 2.0, clock, out);
        }
    }

    /// The ways out at the left end, what acts on the state at the right. Two groups rather
    /// than one run, because a legend read left to right offers four equal choices where there
    /// are really two kinds.
    fn draw_bottom(&self, out: &mut Vec<Draw>) {
        let top = OUT_H as f32 - PLATE_H;
        out.push(plate(top));
        let y = top + (PLATE_H - HINT_H as f32) / 2.0;
        let hints = self.hints();
        let widths: Vec<f32> = hints
            .iter()
            .map(|h| hint_width(h.key, &h.label) as f32)
            .collect();
        let tail = &widths[WAYS_OUT..];
        let tail_w = tail.iter().sum::<f32>() + HINT_GAP * (tail.len() - 1) as f32;
        let mut x = MARGIN;
        for (i, w) in widths.iter().enumerate() {
            if i == WAYS_OUT {
                x = OUT_W as f32 - MARGIN - tail_w;
            }
            out.push(hint_quad(x, y, *w, self.hint_faces.get(i).copied()));
            x += w + HINT_GAP;
        }
        self.draw_dots(top, out);
    }

    /// Centred, which is the only span the legend has left now that it holds both ends.
    fn draw_dots(&self, top: f32, out: &mut Vec<Draw>) {
        let n = self.entries.len();
        let row = n as f32 * DOT + (n as f32 - 1.0).max(0.0) * DOT_GAP;
        let mut x = (OUT_W as f32 - row) / 2.0;
        for i in 0..n {
            out.push(Draw::Rect {
                x,
                y: top + (PLATE_H - DOT) / 2.0,
                w: DOT,
                h: DOT,
                colour: [1.0, 1.0, 1.0, if i == self.index { 1.0 } else { DOT_DIM }],
            });
            x += DOT + DOT_GAP;
        }
    }

    /// Read straight off the two filenames. Hours rather than calendar days, so a save made
    /// an hour before midnight does not age a whole day the moment the clock rolls over.
    /// Past the window the relative form stops being useful and starts being vague, and the
    /// date is what a user comparing two old saves actually needs.
    pub fn relative_time(stamp: &str, now: &str) -> String {
        const RELATIVE_WINDOW: i64 = 12 * 3600;
        let (Some(then), Some(parsed)) = (parse_stamp(stamp), parse_stamp(now)) else {
            return stamp.to_string();
        };
        let delta = parsed - then;
        if delta < 60 {
            return "just now".into();
        }
        if delta < 3600 {
            return format!("{} min ago", delta / 60);
        }
        if delta < RELATIVE_WINDOW {
            return format!("{} hr ago", delta / 3600);
        }
        // Sliced rather than reformatted: `parse_stamp` accepted it, so the fields are where
        // the format says they are.
        format!("{} {}:{}", &stamp[..10], &stamp[11..13], &stamp[14..16])
    }
}

fn plate(y: f32) -> Draw {
    Draw::Rect {
        x: 0.0,
        y,
        w: OUT_W as f32,
        h: PLATE_H,
        colour: PLATE,
    }
}

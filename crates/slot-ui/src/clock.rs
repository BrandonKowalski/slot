use slot_gfx::{Draw, TexId, OUT_H, OUT_W};
use slot_store::{
    civil_from_days, days_from_civil, days_in_month, parse_stamp, UTC_OFFSET_MAX, UTC_OFFSET_MIN,
};

use crate::plate::{blit, hint_width, UndoFace, HINT_H};
use crate::text;

const DAY: i64 = 86_400;

/// The only years the picker can express. A dead RTC reads as 1970 and thirty presses to
/// reach the present is not a screen anyone confirms, so a clock outside the range is seeded
/// to the nearest end rather than left where it cannot be moved from.
const YEAR_MIN: i64 = 2000;
const YEAR_MAX: i64 = 2099;

const INK: [u8; 3] = [0xf6, 0xf4, 0xef];
const BACKDROP: [f32; 4] = [0.06, 0.06, 0.07, 1.0];

/// The step the offset moves by. Half an hour rather than a whole one because India, Iran and
/// half of Australia are on the halves, and Nepal and the Chathams are on the quarters they
/// will have to round to.
const OFFSET_STEP: i64 = 30;

/// Cell widths left to right: year, mark, month, mark, day, gap, hour, mark, minute, gap,
/// offset. A fixed
/// grid rather than one measured run of type, because the caret has to sit under the field it
/// is changing and proportional digits would put every date's fields somewhere slightly else.
const CELLS: [u32; 11] = [88, 22, 52, 22, 52, 36, 52, 22, 52, 36, 128];
/// Which cell each field is drawn in, in `Field::ALL` order.
const FIELD_CELL: [usize; 6] = [0, 2, 4, 6, 8, 10];

const PICKER_H: u32 = 44;
const PICKER_PX: f32 = 30.0;
const PICKER_MIN_PX: f32 = 12.0;
/// Under the field being changed, standing in for the sketch's pair of arrows.
const CARET_H: f32 = 4.0;
const CARET_GAP: f32 = 6.0;
/// Below the caret, far enough down that it reads as an instruction rather than as part of
/// the date.
const HINT_DROP: f32 = 48.0;

const SET_CLOCK_KEY: &str = "A";
const SET_CLOCK_LABEL: &str = "set the clock";

/// Hours and minutes off a ring stamp. Never seconds: a clock showing them is a clock being
/// watched rather than glanced at.
pub fn clock_label(stamp: &str) -> String {
    parse_stamp(stamp).map(hhmm).unwrap_or_default()
}

pub fn hhmm(secs: i64) -> String {
    let rem = secs.rem_euclid(DAY);
    format!("{:02}:{:02}", rem / 3600, rem / 60 % 60)
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Field {
    Year,
    Month,
    Day,
    Hour,
    Minute,
    /// Minutes between what the other fields say and the UTC the card keeps.
    Offset,
}

impl Field {
    pub const ALL: [Field; 6] = [
        Field::Year,
        Field::Month,
        Field::Day,
        Field::Hour,
        Field::Minute,
        Field::Offset,
    ];
}

/// Year, month, day, hour, minute, with one of them under the caret. Asked once on slot's
/// first launch and never reachable again, which is what keeps it from being a setting.
#[derive(Clone, Debug)]
pub struct ClockPicker {
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    /// Minutes to add to UTC to get the other fields. Zero until it is asked for, which is
    /// also right for a device that never left Greenwich.
    offset_min: i64,
    cursor: usize,
}

impl ClockPicker {
    /// Seconds are dropped rather than rounded: the picked time is what the screen said, and
    /// a clock that lands half a minute past what was confirmed is one nobody asked for.
    pub fn from_secs(secs: i64) -> Self {
        let (year, month, day) = civil_from_days(secs.div_euclid(DAY));
        let rem = secs.rem_euclid(DAY);
        ClockPicker {
            year: year.clamp(YEAR_MIN, YEAR_MAX),
            month,
            day,
            hour: rem / 3600,
            minute: rem / 60 % 60,
            offset_min: 0,
            cursor: 0,
        }
    }

    pub fn from_ymd(year: i64, month: i64, day: i64) -> Self {
        ClockPicker {
            year: year.clamp(YEAR_MIN, YEAR_MAX),
            month,
            day,
            hour: 0,
            minute: 0,
            offset_min: 0,
            cursor: 0,
        }
    }

    /// UTC, which is what the card keeps. The fields are the time on the wall, so the offset
    /// that turns one into the other comes back off here.
    pub fn secs(&self) -> i64 {
        let local = days_from_civil(self.year, self.month, self.day) * DAY
            + self.hour * 3600
            + self.minute * 60;
        local - self.offset_min * 60
    }

    pub fn offset_min(&self) -> i64 {
        self.offset_min
    }

    pub fn month(&self) -> i64 {
        self.month
    }

    pub fn day(&self) -> i64 {
        self.day
    }

    pub fn cursor(&self) -> Field {
        Field::ALL[self.cursor]
    }

    pub fn field(&mut self, field: Field) {
        self.cursor = Field::ALL.iter().position(|f| *f == field).unwrap_or(0);
    }

    /// Clamped, not wrapped. Five fields are a line to walk along, and only the values on
    /// them are cyclic.
    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(Field::ALL.len() - 1);
    }

    pub fn up(&mut self) {
        self.step(1);
    }

    pub fn down(&mut self) {
        self.step(-1);
    }

    fn step(&mut self, by: i64) {
        match self.cursor() {
            Field::Year => self.year = wrap(self.year + by, YEAR_MIN, YEAR_MAX),
            Field::Month => self.month = wrap(self.month + by, 1, 12),
            Field::Day => self.day = wrap(self.day + by, 1, days_in_month(self.year, self.month)),
            Field::Hour => self.hour = wrap(self.hour + by, 0, 23),
            Field::Minute => self.minute = wrap(self.minute + by, 0, 59),
            // Clamped where every other field wraps: the ends of this one are the ends of the
            // world, and rolling from Kiritimati to Baker Island on one press is never meant.
            Field::Offset => {
                self.offset_min = (self.offset_min + by * OFFSET_STEP)
                    .clamp(i64::from(UTC_OFFSET_MIN), i64::from(UTC_OFFSET_MAX))
            }
        }
        // A 31st carried into February would confirm a date the calendar does not have.
        self.day = self.day.min(days_in_month(self.year, self.month));
    }

    /// What the line of type says. The binary watches it to know when the face has to be
    /// rasterised again.
    pub fn text(&self) -> String {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02} {}",
            self.year,
            self.month,
            self.day,
            self.hour,
            self.minute,
            self.offset_text()
        )
    }

    /// The whole line, cell by cell, so the caret and the digits are laid out by the same
    /// grid.
    pub fn face(&self) -> UndoFace {
        let w = picker_w();
        let mut rgba = vec![0u8; (w * PICKER_H * 4) as usize];
        for (i, cell) in CELLS.iter().enumerate() {
            let text = self.cell_text(i);
            if text.is_empty() {
                continue;
            }
            let mut buf = vec![0u8; (cell * PICKER_H * 4) as usize];
            if let Some(font) = text::label_font() {
                let layout = text::fit(font, &text, *cell as f32, 1, PICKER_PX, PICKER_MIN_PX);
                text::draw_centred(&mut buf, *cell, PICKER_H, &layout, INK);
            }
            blit(&mut rgba, w, &buf, *cell, PICKER_H, cell_x(i), 0);
        }
        UndoFace {
            rgba,
            w,
            h: PICKER_H,
        }
    }

    fn cell_text(&self, cell: usize) -> String {
        match cell {
            0 => format!("{:04}", self.year),
            1 | 3 => "-".into(),
            2 => format!("{:02}", self.month),
            4 => format!("{:02}", self.day),
            6 => format!("{:02}", self.hour),
            7 => ":".into(),
            8 => format!("{:02}", self.minute),
            10 => format!("UTC{}", self.offset_text()),
            _ => String::new(),
        }
    }

    /// Signed always, so the field reads as an offset rather than as another time.
    fn offset_text(&self) -> String {
        let sign = if self.offset_min < 0 { '-' } else { '+' };
        let mins = self.offset_min.abs();
        format!("{sign}{:02}:{:02}", mins / 60, mins % 60)
    }

    pub fn draw(&self, line: Option<TexId>, hint: Option<TexId>, out: &mut Vec<Draw>) {
        out.push(Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: OUT_W as f32,
            h: OUT_H as f32,
            colour: BACKDROP,
        });
        let w = picker_w() as f32;
        let x = (OUT_W as f32 - w) / 2.0;
        let y = (OUT_H as f32 - PICKER_H as f32) / 2.0 - HINT_DROP / 2.0;
        if let Some(tex) = line {
            out.push(Draw::Tex {
                x,
                y,
                w,
                h: PICKER_H as f32,
                tex,
                alpha: 1.0,
            });
        }
        let cell = FIELD_CELL[self.cursor];
        out.push(Draw::Rect {
            x: x + cell_x(cell) as f32,
            y: y + PICKER_H as f32 + CARET_GAP,
            w: CELLS[cell] as f32,
            h: CARET_H,
            colour: [
                INK[0] as f32 / 255.0,
                INK[1] as f32 / 255.0,
                INK[2] as f32 / 255.0,
                1.0,
            ],
        });
        if let Some(tex) = hint {
            let hw = hint_width(SET_CLOCK_KEY, SET_CLOCK_LABEL) as f32;
            out.push(Draw::Tex {
                x: (OUT_W as f32 - hw) / 2.0,
                y: y + PICKER_H as f32 + HINT_DROP,
                w: hw,
                h: HINT_H as f32,
                tex,
                alpha: 1.0,
            });
        }
    }
}

/// The one instruction on the screen, and the only way off it.
pub fn set_clock_hint_face() -> UndoFace {
    crate::plate::hint_face(SET_CLOCK_KEY, SET_CLOCK_LABEL)
}

fn picker_w() -> u32 {
    CELLS.iter().sum()
}

fn cell_x(cell: usize) -> u32 {
    CELLS[..cell].iter().sum()
}

fn wrap(v: i64, lo: i64, hi: i64) -> i64 {
    (v - lo).rem_euclid(hi - lo + 1) + lo
}

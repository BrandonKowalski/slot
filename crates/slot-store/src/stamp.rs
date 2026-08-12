use std::time::{SystemTime, UNIX_EPOCH};

const DAY: i64 = 86_400;

/// `%Y-%m-%d_%H-%M-%S` in UTC. UTC rather than local because the stamp is a filename and a
/// timezone change must not reorder a ring that is already on the card.
pub fn stamp_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    format_stamp(secs)
}

pub fn format_stamp(secs: i64) -> String {
    let (y, m, d) = civil_from_days(secs.div_euclid(DAY));
    let rem = secs.rem_euclid(DAY);
    format!(
        "{y:04}-{m:02}-{d:02}_{:02}-{:02}-{:02}",
        rem / 3600,
        rem / 60 % 60,
        rem % 60
    )
}

/// Seconds since the epoch, or `None` for anything that is not a stamp. The ring's
/// directory is visible to the user, so "not a stamp" is a file they dropped in.
pub fn parse_stamp(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 19
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'_'
        || b[13] != b'-'
        || b[16] != b'-'
    {
        return None;
    }
    let field = |from: usize, to: usize| -> Option<i64> {
        let text = s.get(from..to)?;
        text.bytes().all(|c| c.is_ascii_digit()).then_some(())?;
        text.parse().ok()
    };
    let (y, m, d) = (field(0, 4)?, field(5, 7)?, field(8, 10)?);
    let (hh, mm, ss) = (field(11, 13)?, field(14, 16)?, field(17, 19)?);
    if !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) || hh > 23 || mm > 59 || ss > 59 {
        return None;
    }
    Some(days_from_civil(y, m, d) * DAY + hh * 3600 + mm * 60 + ss)
}

/// Howard Hinnant's civil calendar algorithm, with the era shifted so day 0 is 1970-01-01.
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = y - (m <= 2) as i64;
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (yoe + era * 400 + (m <= 2) as i64, m, d)
}

pub fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        2 if y.rem_euclid(4) == 0 && (y.rem_euclid(100) != 0 || y.rem_euclid(400) == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

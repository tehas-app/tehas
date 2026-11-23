//! `At` as an RFC 3339 string in whole seconds and UTC,
//! `2026-11-22T13:20:00Z`, as the factory API prints times. No clock: it
//! turns seconds into a date and back.

use serde::de::Error as _;
use serde::ser::Error as _;
use serde::{Deserialize, Deserializer, Serializer};

use crate::At;

/// The text of `at`, or None for a year outside 0000 to 9999.
pub fn text(at: At) -> Option<String> {
    let secs = at.secs();
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    let (y, m, d) = civil(days);
    if !(0..=9999).contains(&y) {
        return None;
    }
    Some(format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    ))
}

/// The time a text names, if it is exactly `YYYY-MM-DDTHH:MM:SSZ` and a
/// real date.
pub fn read(s: &str) -> Option<At> {
    let b = s.as_bytes();
    if b.len() != 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' {
        return None;
    }
    if b[16] != b':' || b[19] != b'Z' {
        return None;
    }
    let num = |from: usize, to: usize| -> Option<i64> {
        let part = &s[from..to];
        part.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| part.parse().ok())
            .flatten()
    };
    let (y, m, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, min, sec) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&m) || d < 1 || d > month_days(y, m) || h > 23 || min > 59 || sec > 59 {
        return None;
    }
    Some(At::new(days(y, m, d) * 86_400 + h * 3600 + min * 60 + sec))
}

/// Writes an `At` as its text.
pub fn serialize<S: Serializer>(at: &At, s: S) -> Result<S::Ok, S::Error> {
    match text(*at) {
        Some(t) => s.serialize_str(&t),
        None => Err(S::Error::custom("a time outside the years 0000 to 9999")),
    }
}

/// Reads an `At` from its text.
pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<At, D::Error> {
    let t = String::deserialize(d)?;
    read(&t).ok_or_else(|| {
        D::Error::custom(format!(
            "not an RFC 3339 time in whole seconds and UTC: {t}"
        ))
    })
}

/// The same for an `Option<At>`, left out when None.
pub mod option {
    use serde::{Deserialize, Deserializer, Serializer};

    use crate::At;

    /// Writes the time, which the field's `skip_serializing_if` leaves out
    /// when None.
    pub fn serialize<S: Serializer>(at: &Option<At>, s: S) -> Result<S::Ok, S::Error> {
        match at {
            Some(at) => super::serialize(at, s),
            None => s.serialize_none(),
        }
    }

    /// Reads the time.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<At>, D::Error> {
        #[derive(Deserialize)]
        struct Wrap(#[serde(with = "super")] At);
        Option::<Wrap>::deserialize(d).map(|w| w.map(|Wrap(at)| at))
    }
}

fn leap(y: i64) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

fn month_days(y: i64, m: i64) -> i64 {
    match m {
        2 if leap(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

// Days since 1970-01-01 of a date, and back: the civil calendar algorithms
// of Howard Hinnant.
fn days(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_reads_back_as_it_was_written() {
        for (secs, t) in [
            (0, "1970-01-01T00:00:00Z"),
            (1_767_225_600, "2026-01-01T00:00:00Z"),
            (951_782_400, "2000-02-29T00:00:00Z"),
            (-1, "1969-12-31T23:59:59Z"),
            (253_402_300_799, "9999-12-31T23:59:59Z"),
        ] {
            assert_eq!(text(At::new(secs)).as_deref(), Some(t));
            assert_eq!(read(t), Some(At::new(secs)), "{t}");
        }
        for secs in (-100_000_000..2_000_000_000).step_by(9_999_991) {
            let at = At::new(secs);
            assert_eq!(read(&text(at).unwrap()), Some(at));
        }
    }

    #[test]
    fn a_text_of_another_shape_or_no_date_is_refused() {
        for t in [
            "",
            "2026-11-22T13:20:00",
            "2026-11-22T13:20:00.5Z",
            "2026-11-22T13:20:00+00:00",
            "2026-11-22 13:20:00Z",
            "2026-13-01T00:00:00Z",
            "2026-02-29T00:00:00Z",
            "2026-04-31T00:00:00Z",
            "2026-11-22T24:00:00Z",
            "2026-11-22T13:60:00Z",
            "2026-11-22T13:20:60Z",
            "+026-11-22T13:20:00Z",
        ] {
            assert_eq!(read(t), None, "{t}");
        }
        assert_eq!(text(At::new(253_402_300_800)), None);
    }
}

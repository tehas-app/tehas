//! The clock and the civil calendar in UTC, by Howard Hinnant's algorithms.

use std::time::{SystemTime, UNIX_EPOCH};

/// Unix seconds now; 0 for a clock set before 1970.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Unix milliseconds now; 0 for a clock set before 1970, as [`now`] is.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Unix seconds with the fraction now; 0 for a clock set before 1970, as
/// [`now`] is. The whole seconds and the fraction of the same duration, which
/// is what `Instant` cannot give: the stamp of a poll's row keeps six decimals.
pub fn now_f64() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// Days since 1970-01-01 of a civil date. The caller checks the date: a
/// month or day out of range is counted on, not refused.
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil date (year, month, day) of days since 1970-01-01.
pub fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Days since 1970-01-01 of the Monday of the week `day` is in. Weeks are UTC
/// and begin on Monday, and 1970-01-01 was a Thursday, so four days on from a
/// Thursday is a Monday.
pub fn week_start_of_day(day: i64) -> i64 {
    day - (day - 4).rem_euclid(7)
}

/// Unix seconds as 2026-10-15T18:38:44Z.
pub fn iso(t: i64) -> String {
    let (y, m, d) = civil_from_days(t.div_euclid(86_400));
    let s = t.rem_euclid(86_400);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        s / 3600,
        s % 3600 / 60,
        s % 60
    )
}

/// Unix seconds of 2026-10-15T18:38:44Z, as [`iso`] and `date -u
/// +%Y-%m-%dT%H:%M:%SZ` write it; None for any other shape.
///
/// This is the one strict reader of the form: every byte of the shape is
/// checked, every field is digits and no sign, and each value is in range.
/// A day past the month's last day, hour 24, minute or second 60 are None.
/// The writers never emit a leap second, so second 60 is not a time.
pub fn from_iso(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'Z'
    {
        return None;
    }
    let n = |r: std::ops::Range<usize>| {
        let v = s.get(r)?;
        v.bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| v.parse().ok())?
    };
    let (y, m, d) = (n(0..4)?, n(5..7)?, n(8..10)?);
    let (hh, mm, ss) = (n(11..13)?, n(14..16)?, n(17..19)?);
    if !(1..=12).contains(&m) || !(1..=days_in_month(y, m)).contains(&d) {
        return None;
    }
    if hh > 23 || mm > 59 || ss > 59 {
        return None;
    }
    Some(days_from_civil(y, m, d) * 86_400 + hh * 3600 + mm * 60 + ss)
}

/// The last day of month `m` of year `y` in the proleptic Gregorian calendar.
fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if y.rem_euclid(4) != 0 || (y.rem_euclid(100) == 0 && y.rem_euclid(400) != 0) {
                28
            } else {
                29
            }
        }
    }
}

/// `date -u +%y%m%d-%H%M%S`, the stamp of a log's name.
pub fn stamp(t: i64) -> String {
    let (y, m, d) = civil_from_days(t.div_euclid(86_400));
    let s = t.rem_euclid(86_400);
    format!(
        "{:02}{m:02}{d:02}-{:02}{:02}{:02}",
        y.rem_euclid(100),
        s / 3600,
        s % 3600 / 60,
        s % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_iso_reads_what_iso_writes() {
        for t in [0, 1_793_967_944, 86_399] {
            assert_eq!(from_iso(&iso(t)), Some(t));
        }
    }

    #[test]
    fn from_iso_is_the_one_strict_parser() {
        // A time the writer emits reads back as the second it was written.
        for t in [0, 1_792_485_967, 951_782_400, 1_793_967_944] {
            assert_eq!(from_iso(&iso(t)), Some(t), "{}", iso(t));
        }
        // The shape: every separator of the form is checked where it stands.
        for bad in [
            "2026-10-15T18:38:44",
            "2026-10-15 18:38:44Z",
            "2026-1x-15T18:38:44Z",
            "2026x10-15T18:38:44Z",
            "2026-10x15T18:38:44Z",
            "2026-10-15X18:38:44Z",
            "2026-10-15T18-38:44Z",
            "2026-10-15T18:38-44Z",
            "2026-10-15T18:38:44z",
            "2026-10-15T18:38:44",
            "2026-10-15T18:38:44Z ",
            "",
        ] {
            assert_eq!(from_iso(bad), None, "{bad}");
        }
        // A sign in a field is not a number, and a two-digit field is two digits.
        for bad in [
            "-026-10-15T18:38:44Z",
            "+026-10-15T18:38:44Z",
            "2026--1-15T18:38:44Z",
            "2026-+1-15T18:38:44Z",
            "2026-10--5T18:38:44Z",
            "2026-10-+5T18:38:44Z",
            "2026-10-15T-8:38:44Z",
            "2026-10-15T+8:38:44Z",
            "2026-10-15T18-3:44Z",
            "2026-10-15T18:+3:44Z",
            "2026-10-15T18:38-4Z",
            "2026-10-15T18:38:+4Z",
        ] {
            assert_eq!(from_iso(bad), None, "{bad}");
        }
        // The ranges, including the month's last day and the leap years.
        for bad in [
            "2026-00-15T18:38:44Z",
            "2026-13-15T18:38:44Z",
            "2026-01-00T18:38:44Z",
            "2026-01-32T18:38:44Z",
            "2026-02-30T18:38:44Z",
            "2026-02-29T18:38:44Z",
            "2026-04-31T18:38:44Z",
            "2026-10-15T24:38:44Z",
            "2026-10-15T18:60:44Z",
            "2026-10-15T18:38:60Z",
        ] {
            assert_eq!(from_iso(bad), None, "{bad}");
        }
        // A leap day and a whole month's last day are times.
        assert_eq!(from_iso("2024-02-29T00:00:00Z"), Some(1_709_164_800));
        assert_eq!(from_iso("2000-02-29T00:00:00Z"), Some(951_782_400));
        assert_eq!(from_iso("1900-02-28T00:00:00Z"), Some(-2_203_977_600));
        assert_eq!(from_iso("1900-02-29T00:00:00Z"), None);
        assert_eq!(from_iso("2026-01-31T23:59:59Z"), Some(1_769_903_999));
        assert_eq!(from_iso("2026-04-30T23:59:59Z"), Some(1_777_593_599));
    }

    #[test]
    fn the_epoch_and_known_days() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(days_from_civil(2000, 2, 29), 11_016);
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(days_from_civil(1969, 12, 31), -1);
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(civil_from_days(-719_468), (0, 3, 1));
    }

    #[test]
    fn days_and_dates_agree_both_ways() {
        // From before year 0 to past 2400, over leap and century years.
        for days in -800_000..200_000 {
            let (y, m, d) = civil_from_days(days);
            assert!((1..=12).contains(&m) && (1..=31).contains(&d), "{days}");
            assert_eq!(days_from_civil(y, m, d), days, "{y}-{m}-{d}");
        }
    }

    #[test]
    fn iso_of_unix_seconds() {
        assert_eq!(iso(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso(1_792_485_967), "2026-10-20T08:46:07Z");
        assert_eq!(iso(-1), "1969-12-31T23:59:59Z");
        assert_eq!(iso(951_782_400), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn weeks_begin_on_monday() {
        // 1970-01-01 was a Thursday, so its Monday is four days before it.
        assert_eq!(week_start_of_day(0), -3);
        // 2026 makes a Monday of the 19th and a Tuesday of the 20th.
        let monday = days_from_civil(2026, 10, 19);
        for (day, week) in [
            (days_from_civil(2026, 10, 19), monday),
            (days_from_civil(2026, 10, 20), monday),
            (days_from_civil(2026, 10, 25), monday),
            (days_from_civil(2026, 10, 26), days_from_civil(2026, 10, 26)),
        ] {
            assert_eq!(week_start_of_day(day), week, "{day}");
        }
    }

    #[test]
    fn now_is_after_this_was_written() {
        assert!(now() > 1_792_000_000);
    }

    #[test]
    fn the_clocks_are_the_same_second() {
        // The three clocks read the host one after the other, so each agrees
        // with `now` to within the second a read may cross.
        let at = now();
        assert!((now_ms() / 1000 - at).abs() <= 1, "{}", now_ms());
        assert!((now_f64() as i64 - at).abs() <= 1, "{}", now_f64());
    }

    #[test]
    fn no_clock_of_the_three_is_negative() {
        // A host whose clock stands after 1970, which is every host this runs
        // on: none of the three reads goes below 0, so each of them keeps the
        // refusal `now` makes of a clock set before the epoch.
        assert!(now() >= 0);
        assert!(now_ms() >= 0);
        assert!(now_f64() >= 0.0);
    }
}

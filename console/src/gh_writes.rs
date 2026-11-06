//! `tehasbot github-writes`: what the gh shim's write log says of the last
//! day (#649 part 1), against GitHub's limits for writes.
//!
//!   console tehasbot github-writes
//!
//! The log is $TEHAS_HOME/state/github-writes.log (TEHAS_GITHUB_WRITES_LOG, as
//! host/shim/gh names it), one line per write: see src/gh_shim.rs. The report
//! gives the writes and the refused ones (403 or 429) of the 24 hours to now,
//! the busiest minute and the busiest hour, and then one row per clock hour
//! with its writes, its refused writes and its busiest minute. A line that is
//! not the shim's is passed over. No log is a day of no writes.
//!
//! Exit: 0; 1 no home; 64 usage.

use std::collections::BTreeMap;
use std::env;
use std::fmt::Write as _;
use std::path::PathBuf;
use tehas_core::time;

const DAY: i64 = 86_400;

/// The writes of one minute: how many, and how many GitHub refused.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Count {
    writes: u64,
    refused: u64,
}

/// The minutes of the day before `now` that saw a write, by their first second.
fn minutes(log: &str, now: i64) -> BTreeMap<i64, Count> {
    let mut by = BTreeMap::new();
    for l in log.lines() {
        let f: Vec<&str> = l.split('\t').collect();
        let [at, _, _, _, _, marker] = f[..] else {
            continue;
        };
        let Some(at) = time::from_iso(at) else {
            continue;
        };
        if at <= now - DAY || at > now {
            continue;
        }
        let c: &mut Count = by.entry(at - at.rem_euclid(60)).or_default();
        c.writes += 1;
        if matches!(marker, "403" | "429") {
            c.refused += 1;
        }
    }
    by
}

// 2026-11-06T13:22Z: the minute, without its seconds.
fn minute(t: i64) -> String {
    let s = time::iso(t);
    format!("{}Z", &s[..16])
}

/// The report of the day before `now`.
pub fn report(log: &str, now: i64) -> String {
    let by = minutes(log, now);
    let total: u64 = by.values().map(|c| c.writes).sum();
    let refused: u64 = by.values().map(|c| c.refused).sum();
    let mut hours: BTreeMap<i64, (Count, u64)> = BTreeMap::new();
    let first = (now - DAY + 1).div_euclid(3600) * 3600;
    let mut h = first;
    while h <= now {
        hours.insert(h, (Count::default(), 0));
        h += 3600;
    }
    for (&m, c) in &by {
        let e = hours.entry(m - m.rem_euclid(3600)).or_default();
        e.0.writes += c.writes;
        e.0.refused += c.refused;
        e.1 = e.1.max(c.writes);
    }
    let mut out = format!(
        "GitHub writes in the 24 hours to {}: {total}, {refused} refused (403 or 429)\n",
        minute(now)
    );
    // The first of the busiest wins a tie: the earliest minute or hour.
    let busiest_minute = by.iter().rev().max_by_key(|(_, c)| c.writes);
    let busiest_hour = hours.iter().rev().max_by_key(|(_, (c, _))| c.writes);
    if let (Some((m, mc)), Some((h, (hc, _)))) = (busiest_minute, busiest_hour) {
        let _ = writeln!(
            out,
            "busiest minute {}: {}; busiest hour {}: {}",
            minute(*m),
            mc.writes,
            minute(*h),
            hc.writes
        );
    }
    out.push_str("GitHub's limits for writes: about 80 a minute and 500 an hour\n\n");
    let _ = writeln!(
        out,
        "{:<18} {:>6} {:>7} {:>14}",
        "hour (UTC)", "writes", "refused", "busiest minute"
    );
    for (h, (c, peak)) in &hours {
        let _ = writeln!(
            out,
            "{:<18} {:>6} {:>7} {:>14}",
            minute(*h),
            c.writes,
            c.refused,
            peak
        );
    }
    out
}

fn log_path() -> Option<PathBuf> {
    let set = |name| {
        env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    set("TEHAS_GITHUB_WRITES_LOG").or_else(|| {
        set("TEHAS_HOME")
            .or_else(|| set("HOME").map(|h| h.join("tehas")))
            .map(|h| h.join("state/github-writes.log"))
    })
}

pub fn main(given: &[String]) -> u8 {
    if !given.is_empty() {
        eprintln!("usage: tehasbot github-writes");
        return 64;
    }
    let Some(path) = log_path() else {
        eprintln!("tehasbot github-writes: neither TEHAS_HOME nor HOME is set");
        return 1;
    };
    let bytes = std::fs::read(&path).unwrap_or_default();
    tehas_core::out!("{}", report(&String::from_utf8_lossy(&bytes), time::now()));
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-11-06T14:05:30Z
    const NOW: i64 = 1_793_973_930;

    fn at(t: i64, kind: &str, marker: &str) -> String {
        format!("{}\t{kind}\to/r\t12\t0\t{marker}\n", time::iso(t))
    }

    #[test]
    fn the_day_is_counted_by_minute_and_hour() {
        let mut log = String::new();
        for s in [0, 10, 59] {
            log += &at(NOW - 330 + s, "issue comment", "-");
        }
        log += &at(NOW - 3600, "api POST", "429");
        log += &at(NOW - 3600, "label create", "403");
        let r = report(&log, NOW);
        assert!(
            r.starts_with(
                "GitHub writes in the 24 hours to 2026-11-06T14:05Z: 5, 2 refused (403 or 429)\n\
             busiest minute 2026-11-06T14:00Z: 3; busiest hour 2026-11-06T14:00Z: 3\n"
            ),
            "{r}"
        );
        assert!(
            r.contains("\n2026-11-06T14:00Z       3       0              3\n"),
            "{r}"
        );
        assert!(
            r.contains("\n2026-11-06T13:00Z       2       2              2\n"),
            "{r}"
        );
        assert!(
            r.contains("\n2026-11-05T15:00Z       0       0              0\n"),
            "{r}"
        );
        // One row per clock hour of the day, the first and the last cut short.
        assert_eq!(r.lines().filter(|l| l.starts_with("2026-")).count(), 25);
    }

    #[test]
    fn lines_outside_the_day_or_not_the_shims_are_passed_over() {
        let mut log = at(NOW - DAY, "issue comment", "-");
        log += &at(NOW + 1, "issue comment", "-");
        log += "garbled\n2026-11-06T14:00:00Z\tissue comment\n";
        log += &at(NOW - DAY + 1, "pr edit", "-");
        let r = report(&log, NOW);
        assert!(
            r.starts_with("GitHub writes in the 24 hours to 2026-11-06T14:05Z: 1, 0 refused"),
            "{r}"
        );
        assert!(
            r.contains("\n2026-11-05T14:00Z       1       0              1\n"),
            "{r}"
        );
    }

    #[test]
    fn no_writes_is_a_day_of_zeros_and_no_busiest_line() {
        let r = report("", NOW);
        assert!(r.starts_with("GitHub writes in the 24 hours to 2026-11-06T14:05Z: 0, 0 refused"));
        assert!(!r.contains("busiest minute 2026"));
        assert_eq!(r.lines().filter(|l| l.starts_with("2026-")).count(), 25);
    }

    #[test]
    fn a_tie_goes_to_the_earliest() {
        let log = at(NOW - 7200, "issue comment", "-") + &at(NOW - 60, "issue comment", "-");
        let r = report(&log, NOW);
        assert!(
            r.contains("busiest minute 2026-11-06T12:05Z: 1; busiest hour 2026-11-06T12:00Z: 1"),
            "{r}"
        );
    }
}

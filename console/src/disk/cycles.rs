//! What each running cycle writes, for `tehas disk --cycles`.
//!
//! A cycle's container is named `tehas-work-<n>` after the issue it works on
//! (`console/cycle/src/work.rs`), so the listing names both the cycle and its
//! issue, and no other name has to be guessed at. Two questions are asked of
//! the daemon for the whole host and never once per container: one
//! `docker inspect --size` for the bytes of a container's writable layer, and
//! one `docker stats --no-stream` for the bytes its block device has written.
//! `inspect --size` is asked and not `ps --size` because it prints the bytes
//! and not a rounded size.
//!
//! A figure that could not be read is `null` with the reason in `why`, never
//! 0: a cycle that wrote nothing and a cycle the daemon would not tell us
//! about are not the same fact, which is the rule of the whole command.
//!
//! Both calls go through `run`, so each is bounded by `TEHAS_DISK_WAIT` as
//! every other outside call of this command is. Nothing here writes anything.

use super::run;
use serde_json::{Map, Value};
use std::time::Duration;

/// The most cycles one answer holds. A host runs a handful; a listing over it
/// is not a host's list of cycles.
const MAX_CYCLES: usize = 64;
/// The most bytes of one word of the daemon's answer that is read as a figure.
const WORD_MAX: usize = 32;

/// The prefix of a cycle's container, so the name alone tells which issue it
/// works on. `<n>` is the issue, and nothing else of it.
const CYCLE_PREFIX: &str = "tehas-work-";

/// What the daemon answers for the cycles of one host, before it is turned
/// into the answer: the issue of each cycle in the order the daemon listed
/// them, and what the two calls said of each.
pub struct CycleRead {
    /// The cycles, as the daemon listed them, smallest issue first.
    pub issues: Vec<u64>,
    /// `SizeRw` of each container, by issue. Absent when `inspect --size`
    /// could not be run at all, which is a reason of its own.
    pub writable: Option<Vec<Value>>,
    /// Why the writable bytes of one issue are `null`, by issue.
    pub writable_why: Vec<(u64, String)>,
    /// The written half of `BlockIO` of each container, by issue. Absent when
    /// `stats --no-stream` could not be run at all.
    pub written: Option<Vec<Value>>,
    pub written_why: Vec<(u64, String)>,
}

/// The name of a cycle's container, as `console/cycle/src/work.rs` makes it.
pub fn container_name(issue: u64) -> String {
    format!("{CYCLE_PREFIX}{issue}")
}

/// The issue of a container's name, `Some` only for a name that is exactly
/// `tehas-work-<n>`: `tehas-work-51x`, `tehas-work-5-2` and `tehas-dev-a` are
/// no cycle, and the exact match is what keeps `tehas-work-5` from being read
/// as `tehas-work-51`.
pub fn cycle_issue(name: &str) -> Option<u64> {
    let digits = name.strip_prefix(CYCLE_PREFIX)?;
    // A whole number with no sign and no leading zero, as the cycle names it:
    // a listing that prints `tehas-work-07` is not a cycle of issue 7.
    let plain = !digits.is_empty()
        && digits.len() <= 10
        && digits != "0"
        && !digits.starts_with('0')
        && digits.bytes().all(|b| b.is_ascii_digit());
    if !plain {
        return None;
    }
    digits.parse().ok()
}

/// The bytes of one figure Docker prints, or None when it is no size. Docker
/// prints decimal units (`B`, `kB`, `MB`, `GB`, `TB`, `PB`) and a fraction, so
/// `1.2GB` is 1 200 000 000 bytes and `512kB` is 512 000; the figure is
/// rounded, which is what a byte count of a fraction means. The number and its
/// unit may stand together (`1.2GB`, as `inspect --size` prints them) or
/// divided by blanks (`3 TB`, as `stats` pads a column), and nothing else may
/// stand between them. `SizeRw` carries no unit at all and is a count of
/// bytes, so a bare number is bytes too. A word that is no number, or has a
/// unit Docker does not print (`1.2GiB`), is None: a byte count is never
/// guessed for a figure the daemon did not say.
pub fn docker_bytes(word: &str) -> Option<u64> {
    let word = word.trim();
    if word.is_empty() || word.len() > WORD_MAX {
        return None;
    }
    // The unit is the run of letters that ends the figure: it begins after
    // the last digit, so everything before it is the number and it is the
    // unit, with only blanks allowed between the two.
    let cut = word
        .bytes()
        .rposition(|b| !b.is_ascii_alphabetic())
        .map_or(0, |i| i + 1);
    let (number, unit) = word.split_at(cut);
    let number = number.trim();
    let unit = unit.trim();
    // Docker prints these figures two ways, and both are a figure: `stats`
    // puts a unit on everything, while `SizeRw` of `inspect --size` is a bare
    // count of bytes. A word of neither - `-`, `N/A`, letters alone - is
    // nothing the daemon said about a size.
    if number.is_empty() {
        return None;
    }
    let (whole, part) = match number.split_once('.') {
        // A dot with no digits on one side is no fraction.
        Some(("", _)) | Some((_, "")) => return None,
        Some((whole, part)) => (whole, Some(part)),
        None => (number, None),
    };
    // The figure is a whole number or a decimal fraction of one, and nothing
    // else: no sign, no leading zero, no exponent, so `1e3B` is not read as
    // 1000B on the strength of a spelling Docker never uses.
    if !plain_number(whole) || part.is_some_and(|p| !plain_number(p)) {
        return None;
    }
    let value: f64 = number.parse().ok().filter(|v: &f64| v.is_finite())?;
    if value < 0.0 {
        return None;
    }
    let multiplier = match unit {
        // A bare number is already bytes, which is how `SizeRw` comes.
        "" | "B" => 1.0,
        "kB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        "PB" => 1e15,
        _ => return None,
    };
    Some((value * multiplier).round() as u64)
}

/// A figure Docker prints for a size: a whole number that still fits a
/// 64-bit one, with no sign and no leading zero, as `tehas_core::disk::figure`
/// reads the figures of `df`. An empty half is refused here, and the caller
/// passes it for a size with no fraction.
fn plain_number(word: &str) -> bool {
    !word.is_empty()
        && word.len() <= 18
        && (word == "0" || (!word.starts_with('0') && word.bytes().all(|b| b.is_ascii_digit())))
}

/// The written half of one `BlockIO` figure: `1.2GB / 0B` is `1.2GB`. A figure
/// with no `/` or an empty half on either side is no figure.
pub fn block_written(said: &str) -> Option<u64> {
    let (written, _) = said.split_once('/')?;
    docker_bytes(written.trim())
}

/// The names the daemon listed, one per line, cut to the cycles it holds.
/// The listing is the daemon's own `--format` output, so a line is one name.
fn listed(said: &str) -> Vec<u64> {
    let mut out: Vec<u64> = said
        .lines()
        .filter_map(|line| cycle_issue(line.trim()))
        .collect();
    out.sort_unstable();
    out.dedup();
    out.truncate(MAX_CYCLES);
    out
}

/// The reading, made of three bounded calls: the listing that names the
/// cycles, then one call each for the two figures, each name its own
/// argument. When the listing could not be made there is nothing to ask about
/// and the other two are not asked. A name Docker does not know fails that one
/// call, which leaves its figure `null` with a reason: the command is told
/// only the names of the cycles it listed, so it cannot ask of another.
pub fn read(wait: Duration) -> Result<CycleRead, String> {
    let said = run("docker", &["ps", "--format", "{{.Names}}"], wait)?;
    let issues = listed(&said);
    if issues.is_empty() {
        return Ok(CycleRead {
            issues,
            writable: Some(Vec::new()),
            writable_why: Vec::new(),
            written: Some(Vec::new()),
            written_why: Vec::new(),
        });
    }
    // One call for every container, each container named in its own argument:
    // Docker answers one line per container, in the order they were named, and
    // `inspect` of a name it does not know is the one answer that fails.
    let owned: Vec<String> = issues.iter().map(|n| container_name(*n)).collect();
    let mut args: Vec<&str> = vec!["inspect", "--size", "--format", "{{.SizeRw}}"];
    args.extend(owned.iter().map(String::as_str));
    let mut writable = None;
    let mut writable_why = Vec::new();
    match run("docker", &args, wait) {
        Ok(said) => {
            // A daemon that answered fewer lines than it was asked for has
            // said nothing of the containers past the end; each of those keeps
            // no figure and a reason.
            let said: Vec<&str> = said.lines().collect();
            let mut per_container = Vec::new();
            for (i, issue) in issues.iter().enumerate() {
                match said.get(i).map(|w| docker_bytes(w.trim())) {
                    Some(Some(bytes)) => per_container.push(Value::from(bytes)),
                    _ => {
                        per_container.push(Value::Null);
                        writable_why
                            .push((*issue, "docker inspect --size answered no size".into()));
                    }
                }
            }
            writable = Some(per_container);
        }
        Err(why) => {
            // The one call answered nothing, so no cycle of the host has a
            // writable figure and each says so with the words the call gave.
            writable_why = issues.iter().map(|n| (*n, why.clone())).collect()
        }
    }
    let mut written = None;
    let mut written_why = Vec::new();
    match run(
        "docker",
        &[
            "stats",
            "--no-stream",
            "--format",
            "{{.Name}}\t{{.BlockIO}}",
        ],
        wait,
    ) {
        Ok(said) => {
            // The daemon's own names, tab-separated, so a line is matched to a
            // container by its name and not by its place: `stats` lists the
            // containers it knows in its own order.
            let mut of: Vec<(String, Option<u64>)> = Vec::new();
            for line in said.lines() {
                let mut cols = line.split('\t');
                let name = cols.next().unwrap_or("").trim();
                let Some(issue) = cycle_issue(name) else {
                    continue;
                };
                of.push((
                    container_name(issue),
                    block_written(cols.next().unwrap_or("")),
                ));
            }
            let mut per_container = Vec::new();
            for issue in &issues {
                let name = container_name(*issue);
                match of.iter().find(|(n, _)| *n == name).and_then(|(_, b)| *b) {
                    Some(bytes) => per_container.push(Value::from(bytes)),
                    None => {
                        per_container.push(Value::Null);
                        written_why.push((*issue, "docker stats answered nothing for it".into()));
                    }
                }
            }
            written = Some(per_container);
        }
        Err(why) => written_why = issues.iter().map(|n| (*n, why.clone())).collect(),
    }
    Ok(CycleRead {
        issues,
        writable,
        writable_why,
        written,
        written_why,
    })
}

/// The answer's `cycles`: `{state, list}` with one entry per cycle, or
/// `{state, why}` when the daemon would not list them. The two figures of a
/// cycle are paired by issue with the reasons the reading collected.
pub fn value(read: Result<CycleRead, String>) -> Value {
    let Ok(read) = read else {
        let why = match read {
            Err(why) => why,
            Ok(_) => String::new(),
        };
        return Map::from_iter([
            ("state".to_string(), Value::from("unreadable")),
            ("why".to_string(), Value::from(why)),
        ])
        .into();
    };
    let mut why_w: Map<String, Value> = Map::new();
    for (issue, why) in read.writable_why {
        why_w.insert(issue.to_string(), Value::from(why));
    }
    let mut why_b: Map<String, Value> = Map::new();
    for (issue, why) in read.written_why {
        why_b.insert(issue.to_string(), Value::from(why));
    }
    let list = Value::Array(
        read.issues
            .iter()
            .enumerate()
            .map(|(i, issue)| {
                let mut entry = Map::new();
                entry.insert("issue".into(), Value::from(*issue));
                entry.insert("container".into(), Value::from(container_name(*issue)));
                let writable = read
                    .writable
                    .as_ref()
                    .and_then(|v| v.get(i))
                    .cloned()
                    .unwrap_or(Value::Null);
                let written = read
                    .written
                    .as_ref()
                    .and_then(|v| v.get(i))
                    .cloned()
                    .unwrap_or(Value::Null);
                entry.insert("writableBytes".into(), writable.clone());
                entry.insert("blockWrittenBytes".into(), written.clone());
                // `why` names which figure is missing and why, and is there
                // only when one of them is.
                if writable.is_null() || written.is_null() {
                    let said = match (writable.is_null(), written.is_null()) {
                        (true, true) => "writableBytes and blockWrittenBytes: ",
                        (true, false) => "writableBytes: ",
                        (false, true) => "blockWrittenBytes: ",
                        _ => "",
                    };
                    let key = issue.to_string();
                    let of = |whys: &Map<String, Value>| {
                        whys.get(&key)
                            .and_then(Value::as_str)
                            .unwrap_or("no reading")
                            .to_string()
                    };
                    let reason = match (writable.is_null(), written.is_null()) {
                        (true, true) => format!("{} / {}", of(&why_w), of(&why_b)),
                        _ if writable.is_null() => of(&why_w),
                        _ => of(&why_b),
                    };
                    entry.insert("why".into(), Value::from(format!("{said}{reason}")));
                }
                Value::Object(entry)
            })
            .collect(),
    );
    let mut out = Map::new();
    out.insert("state".into(), Value::from("read"));
    out.insert("list".into(), list);
    Value::Object(out)
}

/// The lines for a person: one per cycle, or the one line that says the
/// reading is not there. A figure that is not there is named as such beside
/// the reason.
pub fn lines(answer: &Value) -> Vec<String> {
    if answer["state"].as_str() == Some("unreadable") {
        return vec![format!(
            "cycles  no reading: {}",
            answer["why"]
                .as_str()
                .unwrap_or("the daemon answered nothing")
        )];
    }
    let list = match answer["list"].as_array() {
        Some(list) => list,
        None => return vec!["cycles  no reading".to_string()],
    };
    if list.is_empty() {
        return vec!["cycles  no cycle is running".to_string()];
    }
    let mut out = Vec::new();
    for cycle in list {
        let issue = cycle["issue"].as_u64().unwrap_or(0);
        let container = cycle["container"].as_str().unwrap_or("?");
        let figure = |key: &str| match cycle[key].as_u64() {
            Some(bytes) => format!("{bytes}"),
            None => "no figure".to_string(),
        };
        let said = match cycle["why"].as_str() {
            Some(why) => format!(" ({why})"),
            None => String::new(),
        };
        out.push(format!(
            "cycles  #{issue} {container}: {} writable, {} written{said}",
            figure("writableBytes"),
            figure("blockWrittenBytes")
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reading a case says the daemon gave: the listing, the sizes and the
    /// stats lines, each as Docker prints them. No case of this module asks
    /// the host anything; each hands over words and reads what was made.
    fn reading(names: &str, sizes: &str, stats: &str) -> CycleRead {
        let issues = listed(names);
        let sizes: Vec<&str> = sizes.lines().collect();
        let writable: Vec<Value> = issues
            .iter()
            .enumerate()
            .map(
                |(i, _)| match sizes.get(i).map(|w| docker_bytes(w.trim())) {
                    Some(Some(b)) => Value::from(b),
                    _ => Value::Null,
                },
            )
            .collect();
        let mut written: Vec<Value> = Vec::new();
        let mut written_why = Vec::new();
        for issue in &issues {
            let name = container_name(*issue);
            let hit = stats.lines().find_map(|line| {
                let mut cols = line.split('\t');
                (cols.next()? == name).then(|| block_written(cols.next().unwrap_or("")))
            });
            match hit.flatten() {
                Some(b) => written.push(Value::from(b)),
                None => {
                    written.push(Value::Null);
                    written_why.push((*issue, "docker stats answered nothing for it".into()));
                }
            }
        }
        CycleRead {
            issues,
            writable: Some(writable),
            writable_why: Vec::new(),
            written: Some(written),
            written_why,
        }
    }

    #[test]
    fn a_size_docker_prints_is_bytes_and_a_word_that_is_no_size_is_none() {
        assert_eq!(docker_bytes("1.2GB"), Some(1_200_000_000));
        assert_eq!(docker_bytes("0B"), Some(0));
        assert_eq!(docker_bytes("512kB"), Some(512_000));
        assert_eq!(docker_bytes("1.5MB"), Some(1_500_000));
        assert_eq!(docker_bytes(" 3 TB "), Some(3_000_000_000_000));
        assert_eq!(docker_bytes("9PB"), Some(9_000_000_000_000_000));
        // `SizeRw` of `inspect --size` carries no unit: a bare number is bytes.
        assert_eq!(docker_bytes("1234567890"), Some(1_234_567_890));
        assert_eq!(docker_bytes("0"), Some(0));
        assert_eq!(docker_bytes(" 42 "), Some(42));
        assert_eq!(docker_bytes("1.5"), Some(2));
        // Nothing Docker prints for a size is no figure: a byte count is not
        // guessed for it, and 0 would be read as a cycle that wrote nothing.
        for word in [
            "", " ", "-", "N/A", "GB", "1.2GiB", "0B0", "1,2GB", "-1B", "1e3B", "1.2.3GB",
            "1..2GB", "+1B", "0B 0B", "00B", "0x10",
        ] {
            assert_eq!(docker_bytes(word), None, "{word}");
        }
    }

    #[test]
    fn a_container_name_names_its_issue_and_nothing_else() {
        assert_eq!(cycle_issue("tehas-work-5"), Some(5));
        assert_eq!(cycle_issue("tehas-work-1872"), Some(1872));
        assert_eq!(cycle_issue("tehas-work-07"), None);
        // The exact match: a name that only starts like a cycle is no cycle.
        assert_eq!(cycle_issue("tehas-work-51x"), None);
        assert_eq!(cycle_issue("tehas-work-5-2"), None);
        assert_eq!(cycle_issue("tehas-dev-a"), None);
        assert_eq!(cycle_issue("tehas-work-"), None);
        assert_eq!(cycle_issue("tehas-preview-5"), None);
        assert_eq!(cycle_issue("my-tehas-work-5"), None);
        assert_eq!(container_name(5), "tehas-work-5");
    }

    #[test]
    fn the_written_half_of_a_block_figure_is_the_first_of_it() {
        assert_eq!(block_written("1.2GB / 0B"), Some(1_200_000_000));
        assert_eq!(block_written("0B / 512kB"), Some(0));
        assert_eq!(block_written("512kB/0B"), Some(512_000));
        assert_eq!(block_written("0B"), None);
        assert_eq!(block_written(" / 0B"), None);
        assert_eq!(block_written("1.2GiB / 0B"), None);
    }

    #[test]
    fn a_stats_line_missing_for_a_listed_cycle_is_no_figure_and_a_reason() {
        let read = reading(
            "tehas-work-5\ntehas-work-51\n",
            "1GB\n2GB\n",
            "tehas-work-5\t1.2GB / 0B\n",
        );
        let answer = value(Ok(read));
        assert_eq!(answer["state"], "read");
        assert_eq!(answer["list"].as_array().map(Vec::len), Some(2));
        assert_eq!(answer["list"][0]["issue"], 5);
        assert_eq!(answer["list"][0]["blockWrittenBytes"], 1_200_000_000u64);
        // The cycle the daemon said nothing about has no figure and a reason,
        // which is not the same answer as 0.
        assert!(answer["list"][1]["blockWrittenBytes"].is_null());
        assert_eq!(answer["list"][1]["writableBytes"], 2_000_000_000u64);
        assert_eq!(
            answer["list"][1]["why"].as_str().unwrap_or_default(),
            "blockWrittenBytes: docker stats answered nothing for it"
        );
        // A cycle with both figures carries no reason at all.
        assert!(answer["list"][0].get("why").is_none());
    }

    #[test]
    fn the_listing_of_no_cycle_is_an_empty_list_and_not_an_absence() {
        let answer = value(Ok(reading("", "", "")));
        assert_eq!(answer["state"], "read");
        assert_eq!(answer["list"], Value::Array(Vec::new()));
        assert_eq!(lines(&answer), ["cycles  no cycle is running"]);
    }

    #[test]
    fn a_listing_that_failed_is_unreadable_with_a_reason_and_no_list() {
        let answer = value(Err("docker did not answer in 10 s".to_string()));
        assert_eq!(answer["state"], "unreadable");
        assert_eq!(answer["why"], "docker did not answer in 10 s");
        assert!(answer.get("list").is_none());
        assert_eq!(
            lines(&answer),
            ["cycles  no reading: docker did not answer in 10 s"]
        );
    }

    #[test]
    fn the_lines_name_each_cycle_once_with_both_figures() {
        let answer = value(Ok(reading(
            "tehas-work-5\ntehas-work-51\n",
            "1GB\n2GB\n",
            "tehas-work-5\t1.2GB / 0B\ntehas-work-51\t0B / 0B\n",
        )));
        assert_eq!(
            lines(&answer),
            [
                "cycles  #5 tehas-work-5: 1000000000 writable, 1200000000 written",
                "cycles  #51 tehas-work-51: 2000000000 writable, 0 written"
            ]
        );
    }

    #[test]
    fn both_figures_missing_says_both_and_keeps_each_reason() {
        let mut read = reading("tehas-work-5\n", "-\n", "");
        read.writable_why
            .push((5, "docker inspect --size answered no size".into()));
        let answer = value(Ok(read));
        assert!(answer["list"][0]["writableBytes"].is_null());
        assert!(answer["list"][0]["blockWrittenBytes"].is_null());
        assert_eq!(
            answer["list"][0]["why"].as_str().unwrap_or_default(),
            "writableBytes and blockWrittenBytes: docker inspect --size answered no size / docker stats answered nothing for it"
        );
    }

    #[test]
    fn a_call_that_answered_nothing_leaves_every_cycle_of_its_figure_without_a_reason() {
        // One container listed, `inspect --size` answered nothing at all: no
        // figure and the reason the call gave, for the one cycle of the host.
        let mut read = reading("tehas-work-7\n", "", "");
        read.writable = None;
        read.writable_why = vec![(7, "docker answered nothing".to_string())];
        let answer = value(Ok(read));
        assert!(answer["list"][0]["writableBytes"].is_null());
        assert!(answer["list"][0]["blockWrittenBytes"].is_null());
        assert_eq!(
            answer["list"][0]["why"].as_str().unwrap_or_default(),
            "writableBytes and blockWrittenBytes: docker answered nothing / docker stats answered nothing for it"
        );
    }

    #[test]
    fn the_listing_keeps_only_cycles_and_names_each_once() {
        let listed = listed(
            "tehas-work-51\ntehas-work-5\ntehas-dev-a\ntehas-work-5\ntehas-work-51x\ntehas-preview-3\n",
        );
        assert_eq!(listed, [5, 51]);
    }

    // `docker ps` lists the containers it considers running, a paused one
    // among them, and a paused cycle still holds its writable layer: it is a
    // cycle a person wants the figures of, so the listing is taken as it comes
    // and no state is asked of the daemon.
    #[test]
    fn a_listed_cycle_counts_whatever_the_daemon_thinks_of_it() {
        assert_eq!(listed("tehas-work-7\n"), [7]);
        // One name per line, as --format prints them, and nothing else in the
        // line is read: the name is the whole of what is asked of the listing.
        assert_eq!(listed("tehas-work-7\ntehas-work-8\n"), [7, 8]);
    }
}

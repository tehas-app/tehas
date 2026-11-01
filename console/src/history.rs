//! `tehas history`: the cycles and planning turns that ended, oldest first,
//! for a program to read. It reads the state store and nothing else; its text
//! is src/usage/history.txt, and host/docs/api.md has every field.
//!
//! `listing` is the read as data, for a caller in this program; `main` is the
//! command, which prints what `Listing` makes of it. A line is a `J`, because
//! jq kept the keys in the order written and a number as the text it read.

use crate::jq::{obj, J};
use crate::snapshot;
use crate::store;
use std::collections::HashMap;
use std::env;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tehas_core::grammar;
use tehas_core::time;

/// A cursor of a shape this command can print: at most 10 digits of seconds,
/// 9 of issue and 15 of place. The shape of v0.13 and before has no place.
#[derive(Clone, Debug, PartialEq)]
pub struct Cursor {
    text: String,
    seconds: i64,
    issue: i64,
    place: Option<i64>,
}

impl Cursor {
    pub fn parse(text: &str) -> Option<Cursor> {
        let parts: Vec<&str> = text.split('-').collect();
        let (seconds, issue, place) = match parts[..] {
            [seconds, issue] => (seconds, issue, None),
            [seconds, issue, place] => (seconds, issue, Some(grammar::digits(place, 15, false)?)),
            _ => return None,
        };
        if !(1..=10).contains(&seconds.len()) || !seconds.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(Cursor {
            text: text.to_string(),
            seconds: seconds.parse().ok()?,
            issue: grammar::digits(issue, 9, false)?,
            place,
        })
    }

    /// Whether a line was written after the line of this cursor. An old cursor
    /// is compared as before, on the seconds and then the issue.
    fn before(&self, line: &J, seconds: i64) -> bool {
        let number = |key| line.get(key).as_f64().unwrap_or(0.0);
        match self.place {
            Some(place) => number("seq") > place as f64,
            None => {
                seconds > self.seconds
                    || (seconds == self.seconds && number("issue") > self.issue as f64)
            }
        }
    }
}

/// What to list: every line, or those after a cursor; every issue, or one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Query {
    pub after: Option<Cursor>,
    pub issue: Option<i64>,
}

/// The lines oldest first, and the cursor to keep: the last line's, or the one
/// asked with when no line came after it.
#[derive(Clone, Debug, PartialEq)]
pub struct Listing {
    pub cycles: Vec<J>,
    pub next: Option<String>,
}

impl Listing {
    /// The object of `--json`.
    pub fn object(&self) -> J {
        obj(vec![
            ("api", J::Num("1".to_string())),
            ("cycles", J::Arr(self.cycles.clone())),
            ("next", self.next.as_ref().map_or(J::Null, J::str)),
        ])
    }

    /// What the command prints without `--json`.
    pub fn text(&self) -> String {
        self.cycles
            .iter()
            .map(|line| text_line(line) + "\n")
            .collect()
    }
}

/// A read that gave no listing: what the caller exits with, and its line for
/// standard error.
#[derive(Debug, PartialEq)]
pub struct Unread {
    pub rc: u8,
    pub said: String,
}

const USAGE: &str = "usage: tehas history [--json] [--after CURSOR] [--issue N]";

/// Whether the arguments ask for `--json`, and what to list; or 64 with the
/// usage.
pub fn words(given: &[String]) -> Result<(bool, Query), u8> {
    let bad = || {
        eprintln!("{USAGE}");
        64
    };
    let (mut json, mut query) = (false, Query::default());
    let mut given = given.iter();
    while let Some(word) = given.next() {
        match word.as_str() {
            "--json" if !json => json = true,
            "--after" if query.after.is_none() => {
                query.after = given.next().and_then(|c| Cursor::parse(c));
                query.after.as_ref().ok_or_else(bad)?;
            }
            "--issue" if query.issue.is_none() => {
                query.issue = given.next().and_then(|n| grammar::digits(n, 9, false));
                query.issue.ok_or_else(bad)?;
            }
            _ => return Err(bad()),
        }
    }
    Ok((json, query))
}

/// The listing of a factory, whose repository names its pull requests. A store
/// that cannot answer is no listing, never one with the cycles missing: `rc` 1
/// when it is busy, 78 when it cannot be used. A line it cannot read is left
/// out.
pub fn listing(home: &Path, repo: &str, query: &Query) -> Result<Listing, Unread> {
    if !store::ready(home) {
        return Err(Unread {
            rc: 78,
            said: "ERROR: the state store is not installed (run install.sh)".to_string(),
        });
    }
    let issue = query.issue.map(|n| n.to_string());
    let mut args = vec!["history-show"];
    args.extend(issue.as_deref());
    let answer = store::line(home, &args).map_err(|no| Unread {
        rc: if no.rc == 1 { 1 } else { 78 },
        said: format!(
            "ERROR: the history could not be read from the state store: {}",
            no.line
        ),
    })?;
    let answer = J::parse(&answer).unwrap_or(J::Null);
    Ok(listed(lines(&answer, repo), query.after.as_ref()))
}

pub fn main(given: &[String]) -> u8 {
    let (json, query) = match words(given) {
        Ok(words) => words,
        Err(rc) => return rc,
    };
    let home = PathBuf::from(env::var_os("TEHAS_HOME").unwrap_or_default());
    let repo = env::var("TEHAS_REPO").unwrap_or_default();
    let listing = match listing(&home, &repo, &query) {
        Ok(listing) => listing,
        Err(no) => {
            eprintln!("{}", no.said);
            return no.rc;
        }
    };
    let printed = if json {
        listing.object().dump() + "\n"
    } else {
        listing.text()
    };
    let mut out = io::stdout();
    u8::from(
        out.write_all(printed.as_bytes())
            .and_then(|()| out.flush())
            .is_err(),
    )
}

/// The fields that hold a value, as jq's `with_entries(select(...))` keeps them.
fn kept(fields: Vec<(&str, J)>, held: impl Fn(&J) -> bool) -> Vec<(String, J)> {
    fields
        .into_iter()
        .filter(|(_, v)| held(v))
        .map(|(k, v)| (k.to_string(), v))
        .collect()
}

fn no_empty(v: &J) -> bool {
    !v.is_null() && *v != J::str("")
}

/// `todate`, of whatever a row holds as `at`.
fn date(v: &J) -> Option<J> {
    Some(J::str(time::iso(v.as_f64()?.floor() as i64)))
}

/// A row of the store in the shape its line had before the store. A row of
/// neither kind is no line, and an `at` that is no time none of the rows is:
/// lib/history-lines.jq stops there. millwright.sh reads that copy; a test
/// below holds the two to the same lines.
fn line_of(row: &J, repo: &str) -> Option<Option<J>> {
    let f = |key: &str| row.get(key).clone();
    let started = ("started", f("started_at"));
    let line = match row.get("kind").as_str() {
        Some("cycle") => {
            let pr = match row.get("pr") {
                J::Null => J::Null,
                pr => J::Str(format!("https://github.com/{repo}/pull/{}", pr.text())),
            };
            let mut line = vec![
                ("issue".to_string(), f("issue")),
                ("kind".to_string(), J::str("work")),
                ("seq".to_string(), f("seq")),
            ];
            line.extend(kept(
                vec![
                    ("at", date(row.get("at"))?),
                    started,
                    ("rc", f("rc")),
                    ("status", f("status")),
                    ("step", f("step")),
                    ("cap", f("cap")),
                    ("park_reason", f("park_reason")),
                    ("failure_kind", f("failure_kind")),
                    ("failure_cause", f("failure_cause")),
                    ("provider", f("provider")),
                    ("parked", f("parked")),
                    ("pr", pr),
                    ("by", f("by")),
                    ("how", f("how")),
                ],
                no_empty,
            ));
            line
        }
        Some("plan") => kept(
            vec![
                ("issue", f("issue")),
                ("kind", J::str("plan")),
                ("seq", f("seq")),
                ("at", date(row.get("at"))?),
                started,
                ("rc", row.get("rc").or(row.get("outcome")).clone()),
                ("status", f("outcome")),
                ("turn", f("turn")),
                ("by", f("by")),
                ("label", f("label")),
                ("filed", f("filed")),
                (
                    "auto",
                    if row.get("auto").is_true() {
                        J::Bool(true)
                    } else {
                        J::Null
                    },
                ),
            ],
            |v| no_empty(v) && *v != J::Arr(Vec::new()),
        ),
        _ => return Some(None),
    };
    Some(Some(J::Obj(line)))
}

fn lines(answer: &J, repo: &str) -> Vec<J> {
    let lines: Option<Vec<Option<J>>> = answer
        .get("rows")
        .items()
        .iter()
        .map(|row| line_of(row, repo))
        .collect();
    lines.unwrap_or_default().into_iter().flatten().collect()
}

/// A whole number as this command wrote it, which is how jq tested for one.
fn whole(v: &J) -> Option<&str> {
    match v {
        J::Num(n) if n.bytes().all(|b| b.is_ascii_digit()) => Some(n),
        _ => None,
    }
}

fn above_zero(v: &J) -> bool {
    whole(v).is_some_and(|n| n.bytes().any(|b| b != b'0'))
}

/// The seconds of a line this release can read. A line it cannot is left out,
/// so an older or a newer row breaks no listing.
fn seconds_of(line: &J) -> Option<i64> {
    let at = line.get("at").as_str()?;
    let rc = matches!(line.get("rc"), J::Num(_) | J::Str(_));
    let readable = above_zero(line.get("issue"))
        && above_zero(line.get("seq"))
        && rc
        && line.get("status").as_str().is_some()
        && at.starts_with(|c: char| c.is_ascii_digit());
    snapshot::epoch(at).filter(|_| readable)
}

fn is_issue_address(s: &str) -> bool {
    // `$` of jq's regular expression also stands before a last newline.
    let path = s.strip_prefix("https://github.com/").unwrap_or_default();
    let path = path.strip_suffix('\n').unwrap_or(path);
    match path.split('/').collect::<Vec<_>>()[..] {
        [owner, name, "issues", number] => {
            !owner.is_empty()
                && !name.is_empty()
                && !number.is_empty()
                && number.bytes().all(|b| b.is_ascii_digit())
        }
        _ => false,
    }
}

/// What a caller reads. A release that adds a field to a line adds it here too;
/// nothing here renames or removes one.
fn publish(line: &J, seconds: i64) -> J {
    let f = |key: &str| line.get(key).clone();
    let number = |key: &str| whole(line.get(key)).map_or(J::Null, |_| f(key));
    let word = |key: &str| match line.get(key) {
        J::Str(s) if !s.is_empty() => f(key),
        _ => J::Null,
    };
    let boolean = |key: &str| match line.get(key) {
        J::Bool(_) => f(key),
        _ => J::Null,
    };
    let started = whole(line.get("started")).and_then(|t| Some(J::str(time::iso(t.parse().ok()?))));
    let filed: Vec<J> = line
        .get("filed")
        .items()
        .iter()
        .filter(|v| v.as_str().is_some_and(is_issue_address))
        .cloned()
        .collect();
    J::Obj(kept(
        vec![
            (
                "cursor",
                J::Str(format!(
                    "{seconds}-{}-{}",
                    line.get("issue").text(),
                    line.get("seq").text()
                )),
            ),
            ("kind", f("kind")),
            ("issue", f("issue")),
            ("ended", f("at")),
            ("started", started.unwrap_or(J::Null)),
            ("status", number("rc")),
            ("outcome", f("status")),
            ("turn", number("turn")),
            ("step", word("step")),
            ("cap", word("cap")),
            ("parkReason", word("park_reason")),
            ("failureKind", word("failure_kind")),
            ("failureCause", word("failure_cause")),
            ("provider", boolean("provider")),
            ("parked", boolean("parked")),
            ("pr", word("pr")),
            ("by", word("by")),
            ("how", word("how")),
            ("label", word("label")),
            (
                "auto",
                if line.get("auto").is_true() {
                    J::Bool(true)
                } else {
                    J::Null
                },
            ),
            (
                "filed",
                if filed.is_empty() {
                    J::Null
                } else {
                    J::Arr(filed)
                },
            ),
        ],
        |v| !v.is_null(),
    ))
}

/// A park, and not the park of a provider outage, which has its own rule of
/// retries: never the park that starts a run and never the one that breaks it.
fn is_park(line: &J) -> bool {
    line.get("parked").is_true() && !line.get("provider").is_true()
}

fn same_park(a: &J, b: &J) -> bool {
    ["step", "cap", "parkReason", "failureKind", "failureCause"]
        .iter()
        .all(|key| a.get(key) == b.get(key))
}

/// The count of the parks in a row that stopped exactly as this one did, on
/// each line that has one. A run is the parks of one issue with step, cap and
/// the three words of the failure all equal, each the end written just after
/// the other, the parks of an outage skipped. The first of them counts 1, the
/// next 2. A merge, a handoff or any other end breaks a run. It is worked out on
/// these lines and none besides: a listing that begins partway through a run
/// gives its first line no count.
fn count_repeats(lines: &mut [J]) {
    let mut counts = vec![0u64; lines.len()];
    // By issue: its end written last, outages skipped, and its park seen last.
    let (mut end, mut park) = (HashMap::new(), HashMap::new());
    for i in 0..lines.len() {
        let line = &lines[i];
        let issue = line.get("issue").text();
        let work = line.get("kind").as_str() == Some("work");
        if is_park(line) {
            let run = end
                .get(&issue)
                .map(|&e: &usize| &lines[e])
                .is_some_and(|before| is_park(before) && same_park(line, before));
            if run {
                counts[i] = park.get(&issue).map_or(0, |&p: &usize| counts[p]) + 1;
            }
            park.insert(issue.clone(), i);
        } else if work && !line.get("parked").is_true() {
            park.remove(&issue);
        }
        if work && !line.get("provider").is_true() {
            end.insert(issue, i);
        }
    }
    for (line, count) in lines.iter_mut().zip(counts) {
        if let (J::Obj(fields), true) = (line, count > 0) {
            fields.push(("repeat".to_string(), J::Num(count.to_string())));
        }
    }
}

/// The lines after the cursor, in the order the store wrote the ends: the order
/// `--after` reads, so a program that keeps `next` and asks again is given each
/// line once.
fn listed(lines: Vec<J>, after: Option<&Cursor>) -> Listing {
    let mut read: Vec<(J, J)> = lines
        .iter()
        .filter_map(|line| Some((line, seconds_of(line)?)))
        .filter(|(line, seconds)| after.is_none_or(|c| c.before(line, *seconds)))
        .map(|(line, seconds)| (line.get("seq").clone(), publish(line, seconds)))
        .collect();
    read.sort_by(|a, b| a.0.cmp(&b.0));
    let mut cycles: Vec<J> = read.into_iter().map(|(_, line)| line).collect();
    count_repeats(&mut cycles);
    let last = cycles.last().map(|line| line.get("cursor").text());
    Listing {
        next: last.or_else(|| after.map(|c| c.text.clone())),
        cycles,
    }
}

fn text_line(line: &J) -> String {
    let unknown = J::str("?");
    let or_unknown = |key| line.get(key).or(&unknown).text();
    let on = |key: &str, word: &str| {
        if line.get(key).truthy() {
            format!("  {word} {}", line.get(key).text())
        } else {
            String::new()
        }
    };
    let rest = if line.get("kind").as_str() == Some("plan") {
        let refine = line.get("label").as_str() == Some("tehas:refine");
        let filed: Vec<&str> = line
            .get("filed")
            .items()
            .iter()
            .filter_map(J::as_str)
            .collect();
        format!(
            "turn {}{}{}{}",
            or_unknown("turn"),
            if refine { "  refine" } else { "" },
            if line.get("auto").truthy() {
                " (auto)"
            } else {
                ""
            },
            if filed.is_empty() {
                String::new()
            } else {
                format!("  filed {}", filed.join(" "))
            },
        )
    } else {
        format!(
            "rc {}{}{}{}{}",
            or_unknown("status"),
            on("step", "step"),
            if line.get("parked").is_true() {
                "  parked"
            } else {
                ""
            },
            on("repeat", "repeat"),
            on("pr", "pr"),
        )
    };
    format!(
        "{}  {}  #{}  {}  {rest}",
        line.get("cursor").text(),
        line.get("kind").text(),
        line.get("issue").text(),
        line.get("outcome").text(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::process::{Command, Stdio};

    // Outputs in this file were captured from the jq of v0.21.0.
    const ROWS: &str = r#"{"api":1,"ok":true,"verb":"history-show","rows":[
{"kind":"plan","id":1,"issue":12,"turn":1,"at":1760699940,"outcome":"finished","session":"abc","started_at":1760699880,"rc":0,"by":"alice","label":"tehas:refine","seq":1,"filed":["https://github.com/acme/widget/issues/41","not an issue",7,"https://github.com/acme/widget/issues/42\n"],"auto":true},
{"kind":"cycle","id":1,"issue":12,"attempt":1,"at":1760700000,"status":"failed","step":"develop","by":"alice","how":"label","pr":9,"started_at":1760696400,"rc":10,"cap":"step-failed:develop","park_reason":"","failure_kind":"verify-failed","failure_cause":"exit-1","provider":false,"parked":true,"seq":2},
{"kind":"cycle","id":2,"issue":7,"attempt":1,"at":1760700060,"status":"merged","step":null,"by":null,"how":"hand","pr":null,"started_at":null,"rc":0,"cap":null,"park_reason":null,"failure_kind":null,"failure_cause":null,"provider":false,"parked":false,"seq":3},
{"kind":"cycle","id":3,"issue":12,"attempt":2,"at":1760700120,"status":"failed","step":"develop","by":"alice","how":"label","pr":9,"started_at":1760700060,"rc":10,"cap":"step-failed:develop","park_reason":"","failure_kind":"verify-failed","failure_cause":"exit-1","provider":false,"parked":true,"seq":4},
{"kind":"cycle","id":4,"issue":12,"attempt":3,"at":1760700180,"status":"failed","step":"develop","by":null,"how":"outage-retry","pr":null,"started_at":1760700120,"rc":10,"cap":"step-failed:develop","park_reason":"","failure_kind":"verify-failed","failure_cause":"exit-1","provider":true,"parked":true,"seq":5},
{"kind":"cycle","id":5,"issue":12,"attempt":4,"at":1760700240,"status":"failed","step":"develop","by":"alice","how":"label","pr":9,"started_at":1760700180,"rc":10,"cap":"step-failed:develop","park_reason":"","failure_kind":"verify-failed","failure_cause":"exit-1","provider":false,"parked":true,"seq":6},
{"kind":"plan","id":2,"issue":7,"turn":2,"at":1760700300,"outcome":"not-started","session":null,"started_at":null,"rc":null,"by":"ä\u007f\"\\\t\u0001","label":"tehas:plan","seq":7,"filed":[],"auto":false},
{"kind":"cycle","id":6,"issue":12,"attempt":5,"at":1760700360,"status":"merged","step":"merge","by":"alice","how":"label","pr":9,"started_at":1760700300,"rc":0,"cap":null,"park_reason":null,"failure_kind":null,"failure_cause":null,"provider":false,"parked":false,"seq":8},
{"kind":"cycle","id":7,"issue":12,"attempt":6,"at":1760700420,"status":"failed","step":"develop","by":null,"how":"hand","pr":null,"started_at":1760700360,"rc":null,"cap":null,"park_reason":null,"failure_kind":null,"failure_cause":null,"provider":false,"parked":true,"seq":9}
]}"#;

    const CYCLES: [&str; 8] = [
        r#"{"cursor":"1760699940-12-1","kind":"plan","issue":12,"ended":"2025-10-17T11:19:00Z","started":"2025-10-17T11:18:00Z","status":0,"outcome":"finished","turn":1,"by":"alice","label":"tehas:refine","auto":true,"filed":["https://github.com/acme/widget/issues/41","https://github.com/acme/widget/issues/42\n"]}"#,
        r#"{"cursor":"1760700000-12-2","kind":"work","issue":12,"ended":"2025-10-17T11:20:00Z","started":"2025-10-17T10:20:00Z","status":10,"outcome":"failed","step":"develop","cap":"step-failed:develop","failureKind":"verify-failed","failureCause":"exit-1","provider":false,"parked":true,"pr":"https://github.com/acme/widget/pull/9","by":"alice","how":"label"}"#,
        r#"{"cursor":"1760700060-7-3","kind":"work","issue":7,"ended":"2025-10-17T11:21:00Z","status":0,"outcome":"merged","provider":false,"parked":false,"how":"hand"}"#,
        r#"{"cursor":"1760700120-12-4","kind":"work","issue":12,"ended":"2025-10-17T11:22:00Z","started":"2025-10-17T11:21:00Z","status":10,"outcome":"failed","step":"develop","cap":"step-failed:develop","failureKind":"verify-failed","failureCause":"exit-1","provider":false,"parked":true,"pr":"https://github.com/acme/widget/pull/9","by":"alice","how":"label","repeat":1}"#,
        r#"{"cursor":"1760700180-12-5","kind":"work","issue":12,"ended":"2025-10-17T11:23:00Z","started":"2025-10-17T11:22:00Z","status":10,"outcome":"failed","step":"develop","cap":"step-failed:develop","failureKind":"verify-failed","failureCause":"exit-1","provider":true,"parked":true,"how":"outage-retry"}"#,
        r#"{"cursor":"1760700240-12-6","kind":"work","issue":12,"ended":"2025-10-17T11:24:00Z","started":"2025-10-17T11:23:00Z","status":10,"outcome":"failed","step":"develop","cap":"step-failed:develop","failureKind":"verify-failed","failureCause":"exit-1","provider":false,"parked":true,"pr":"https://github.com/acme/widget/pull/9","by":"alice","how":"label","repeat":2}"#,
        r#"{"cursor":"1760700300-7-7","kind":"plan","issue":7,"ended":"2025-10-17T11:25:00Z","outcome":"not-started","turn":2,"by":"ä\u007f\"\\\t\u0001","label":"tehas:plan"}"#,
        r#"{"cursor":"1760700360-12-8","kind":"work","issue":12,"ended":"2025-10-17T11:26:00Z","started":"2025-10-17T11:25:00Z","status":0,"outcome":"merged","step":"merge","provider":false,"parked":false,"pr":"https://github.com/acme/widget/pull/9","by":"alice","how":"label"}"#,
    ];

    const TEXT: &str = r#"1760699940-12-1  plan  #12  finished  turn 1  refine (auto)  filed https://github.com/acme/widget/issues/41 https://github.com/acme/widget/issues/42

1760700000-12-2  work  #12  failed  rc 10  step develop  parked  pr https://github.com/acme/widget/pull/9
1760700060-7-3  work  #7  merged  rc 0
1760700120-12-4  work  #12  failed  rc 10  step develop  parked  repeat 1  pr https://github.com/acme/widget/pull/9
1760700180-12-5  work  #12  failed  rc 10  step develop  parked
1760700240-12-6  work  #12  failed  rc 10  step develop  parked  repeat 2  pr https://github.com/acme/widget/pull/9
1760700300-7-7  plan  #7  not-started  turn 2
1760700360-12-8  work  #12  merged  rc 0  step merge  pr https://github.com/acme/widget/pull/9
"#;

    fn listed_after(after: Option<&str>) -> Listing {
        let answer = J::parse(ROWS).unwrap();
        let after = after.map(|c| Cursor::parse(c).unwrap());
        listed(lines(&answer, "acme/widget"), after.as_ref())
    }

    fn cursors(listing: &Listing) -> Vec<String> {
        listing
            .cycles
            .iter()
            .map(|line| line.get("cursor").text())
            .collect()
    }

    fn repeats(listing: &Listing) -> Vec<String> {
        listing
            .cycles
            .iter()
            .map(|line| line.get("repeat").text())
            .collect()
    }

    fn words_of(given: &[&str]) -> Vec<String> {
        given.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn a_cycle_line_and_a_plan_line_keep_their_keys_and_raw_values() {
        let listing = listed_after(None);
        let got: Vec<String> = listing.cycles.iter().map(J::dump).collect();
        assert_eq!(got, CYCLES);
        let want = format!(
            "{{\"api\":1,\"cycles\":[{}],\"next\":\"1760700360-12-8\"}}",
            CYCLES.join(",")
        );
        assert_eq!(listing.object().dump(), want);
    }

    #[test]
    fn the_text_is_one_line_each_as_jq_rendered_it() {
        assert_eq!(listed_after(None).text(), TEXT);
    }

    #[test]
    fn a_park_with_no_exit_status_is_a_line_left_out() {
        assert!(ROWS.contains(r#""seq":9"#));
        assert!(!cursors(&listed_after(None))
            .iter()
            .any(|c| c.ends_with("-9")));
    }

    #[test]
    fn a_repeat_counts_the_run_past_an_outage_and_only_what_is_listed() {
        let all = repeats(&listed_after(None));
        assert_eq!(
            all,
            ["null", "null", "null", "1", "null", "2", "null", "null"]
        );
        assert_eq!(
            repeats(&listed_after(Some("1760700000-12-4"))),
            ["null", "null", "null", "null"]
        );
        let from_two = listed_after(Some("1-1-1"));
        assert_eq!(
            repeats(&from_two),
            ["null", "null", "1", "null", "2", "null", "null"]
        );
    }

    #[test]
    fn a_cursor_of_each_width_lists_what_jq_listed() {
        let all = cursors(&listed_after(None));
        for (cursor, skipped) in [
            ("7-1", 0),
            ("0000000001-1", 0),
            ("1-1-1", 1),
            ("1760700000-12", 2),
            ("1760700060-7", 3),
            ("1760700000-12-4", 4),
        ] {
            let listing = listed_after(Some(cursor));
            assert_eq!(cursors(&listing), all[skipped..], "{cursor}");
            assert_eq!(listing.next.as_deref(), Some("1760700360-12-8"), "{cursor}");
        }
    }

    #[test]
    fn a_cursor_past_everything_comes_back_as_it_was_given() {
        for cursor in [
            "9999999999-999999999",
            "9999999999-999999999-999999999999999",
        ] {
            let listing = listed_after(Some(cursor));
            assert!(listing.cycles.is_empty());
            assert_eq!(listing.next.as_deref(), Some(cursor));
            assert_eq!(
                listing.object().dump(),
                format!("{{\"api\":1,\"cycles\":[],\"next\":\"{cursor}\"}}")
            );
            assert_eq!(listing.text(), "");
        }
    }

    #[test]
    fn nothing_to_list_is_an_empty_list_and_no_cursor() {
        let answer = J::parse(r#"{"rows":[]}"#).unwrap();
        let listing = listed(lines(&answer, "acme/widget"), None);
        assert_eq!(
            listing.object().dump(),
            r#"{"api":1,"cycles":[],"next":null}"#
        );
        assert_eq!(listing.text(), "");
    }

    #[test]
    fn a_row_with_no_time_ends_the_read_as_jq_did_and_a_strange_kind_is_no_line() {
        let answer = J::parse(ROWS.replace(r#""at":1760700060,"#, "").as_str()).unwrap();
        assert!(lines(&answer, "acme/widget").is_empty());
        let answer = J::parse(r#"{"rows":[{"kind":"other"}]}"#).unwrap();
        assert!(lines(&answer, "acme/widget").is_empty());
    }

    #[test]
    fn the_lines_are_those_of_the_jq_copy_millwright_reads() {
        let program = concat!(env!("CARGO_MANIFEST_DIR"), "/../lib/history-lines.jq");
        let mut jq = Command::new("jq")
            .args(["-c", "--arg", "repo", "acme/widget", "--argjson", "kinds"])
            .args([r#"["cycle","plan"]"#, "-f", program])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("the gate has jq");
        jq.stdin.take().unwrap().write_all(ROWS.as_bytes()).unwrap();
        let out = jq.wait_with_output().unwrap();
        assert!(out.status.success());
        let want = String::from_utf8(out.stdout).unwrap();
        let got: Vec<String> = lines(&J::parse(ROWS).unwrap(), "acme/widget")
            .iter()
            .map(|line| line.dump() + "\n")
            .collect();
        assert_eq!(got.concat(), want);
    }

    #[test]
    fn an_address_is_read_as_the_regular_expression_of_jq_read_it() {
        for good in [
            "https://github.com/acme/widget/issues/41",
            "https://github.com/acme/widget/issues/41\n",
            "https://github.com/a b/c\nd/issues/007",
        ] {
            assert!(is_issue_address(good), "{good:?}");
        }
        for bad in [
            "not an issue",
            "\nhttps://github.com/a/b/issues/1",
            "https://github.com/a/b/issues/1\n\n",
            "https://github.com/a/b/issues/",
            "https://github.com//b/issues/1",
            "https://github.com/a/b/c/issues/1",
            "http://github.com/a/b/issues/1",
        ] {
            assert!(!is_issue_address(bad), "{bad:?}");
        }
    }

    #[test]
    fn the_arguments_are_those_of_the_script() {
        for good in [
            &[][..],
            &["--json"],
            &["--after", "1-1"],
            &["--after", "1760700000-12"],
            &["--after", "0000000000-1-1"],
            &["--after", "9999999999-999999999-999999999999999"],
            &["--issue", "999999999", "--json"],
        ] {
            assert!(words(&words_of(good)).is_ok(), "{good:?}");
        }
        for bad in [
            &["--json", "--json"][..],
            &["--after"],
            &["--after", ""],
            &["--after", "12"],
            &["--after", "1760700000-0"],
            &["--after", "1760700000-1-0"],
            &["--after", "1-01"],
            &["--after", "1-1-"],
            &["--after", "1-1-1-1"],
            &["--after", "x-1"],
            &["--after", "-1-1"],
            &["--after", "1-1\n"],
            &["--after", "12345678901-1"],
            &["--after", "1-1234567890"],
            &["--after", "1-1-1234567890123456"],
            &["--after", "1-1", "--after", "1-2"],
            &["--issue"],
            &["--issue", "0"],
            &["--issue", "01"],
            &["--issue", "1234567890"],
            &["--issue", "7", "--issue", "8"],
            &["--jsn"],
            &["7"],
            &[""],
        ] {
            assert_eq!(words(&words_of(bad)), Err(64), "{bad:?}");
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("tehas-history-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("libexec")).unwrap();
        fs::create_dir_all(dir.join("state")).unwrap();
        dir
    }

    fn store_says(home: &Path, script: &str) {
        let store = home.join("libexec/store");
        fs::write(&store, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(&store, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(home.join("state/tehas.db"), "").unwrap();
    }

    #[test]
    fn the_read_asks_the_store_for_the_issue_and_a_refusal_is_no_listing() {
        let home = scratch("read");
        let query = Query::default();
        let no = listing(&home, "acme/widget", &query).unwrap_err();
        assert_eq!(no.rc, 78);
        assert!(no.said.contains("not installed"), "{}", no.said);

        let args = home.join("args");
        store_says(
            &home,
            &format!(
                r#"echo "$@" >{}; echo '{{"api":1,"ok":true,"verb":"history-show","rows":[]}}'"#,
                args.display()
            ),
        );
        let query = Query {
            after: Cursor::parse("5-1"),
            issue: Some(12),
        };
        let read = listing(&home, "acme/widget", &query).unwrap();
        assert_eq!(read.next.as_deref(), Some("5-1"));
        assert_eq!(fs::read_to_string(args).unwrap(), "history-show 12\n");

        store_says(
            &home,
            "echo 'store: busy #0: the database is locked'; exit 1",
        );
        let no = listing(&home, "acme/widget", &query).unwrap_err();
        assert_eq!(no.rc, 1);
        assert_eq!(
            no.said,
            "ERROR: the history could not be read from the state store: store: busy #0: the database is locked"
        );
        store_says(&home, "echo 'store: corrupt: not a database'; exit 78");
        assert_eq!(listing(&home, "acme/widget", &query).unwrap_err().rc, 78);
        store_says(&home, "echo 'store: usage: no'; exit 64");
        assert_eq!(listing(&home, "acme/widget", &query).unwrap_err().rc, 78);
        fs::remove_dir_all(&home).unwrap();
    }
}

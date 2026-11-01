//! The factory's state as the health file says it: the lines of `tehas
//! status`, its `--json` object, and the lines `tehas top` puts in its frame
//! (src/frame.rs), written once so the two cannot drift apart.
//!
//!   console status show <health file> <now> [--poll <line>] [--product <file>]
//!     The lines of `tehas status`: the factory, the poll line given, the
//!     queue, what holds it, each cycle, each planning turn in flight, the
//!     slice, the platform, and the last check of the product when <file>
//!     holds one.
//!   console status json <health file> <now>
//!     The object of `tehas status --json`, one line. host/docs/api.md has
//!     its fields; a field without a value is left out, never null.
//!
//!   console tehas status [--json]
//!     The command a person types (`print`): the lines or the object of the
//!     health file of this home, at this moment, with the poll line the store
//!     gives. No network, no model, no instance settings, and it writes
//!     nothing.
//!
//! <now> is epoch seconds. A health file older than ten minutes is not
//! believed: its first line says stale.
//!
//! Exit: 0 the factory is taking work or is paused (pass, warn); 1 it is not
//! (fail), the file is stale, or there is no health file with a status in it;
//! 2 usage; 64 an argument of `tehas status` other than --json.

use crate::flags::epoch;
use crate::jq::{first, obj, J};
use crate::store;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// The age after which a health file is not believed.
pub const MAX_AGE: i64 = 600;

pub fn main(given: &[String]) -> u8 {
    if given.first().map(String::as_str) == Some("platform") {
        let home = PathBuf::from(env::var_os("TEHAS_HOME").unwrap_or_default());
        crate::platform::write(&home, &home.join("live"));
        return 0;
    }
    let usage = || {
        eprintln!(
            "console status: usage: show <health file> <now> [--poll <line>] [--product <file>] | json <health file> <now>"
        );
        2
    };
    let (verb, file, now, rest) = match given {
        [verb, file, now, rest @ ..] => match now.parse::<i64>() {
            Ok(now) => (verb.as_str(), file, now, rest),
            Err(_) => return usage(),
        },
        _ => return usage(),
    };
    let (mut poll, mut product) = (String::new(), None);
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        match (verb, flag.as_str(), it.next()) {
            ("show", "--poll", Some(v)) => poll = v.clone(),
            ("show", "--product", Some(v)) => product = Some(v.clone()),
            _ => return usage(),
        }
    }
    match verb {
        "show" => answer(file, now, Some((&poll, product.as_deref()))),
        "json" => answer(file, now, None),
        _ => usage(),
    }
}

/// `tehas status [--json]`.
pub fn print(given: &[String]) -> u8 {
    let json = match given {
        [] => false,
        [flag] if flag == "--json" => true,
        _ => {
            eprintln!("usage: tehas status [--json]");
            return 64;
        }
    };
    let Some(home) = home() else {
        eprintln!("tehas status: neither TEHAS_HOME nor HOME is set");
        return 1;
    };
    let file = home.join("live/health.json").display().to_string();
    let now = tehas_core::time::now();
    if json {
        return answer(&file, now, None);
    }
    let poll = poll_line(&home);
    let product = home.join("state/check-product.json");
    answer(&file, now, Some((&poll, product.to_str())))
}

/// The instance's home, as `lib/config.sh` resolves it.
fn home() -> Option<PathBuf> {
    let set = |name| {
        env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    set("TEHAS_HOME").or_else(|| set("HOME").map(|h| h.join("tehas")))
}

/// The store's line for the last poll. No store, or none on record, is no line.
fn poll_line(home: &Path) -> String {
    store::call(home, &["poll-show"])
        .ok()
        .and_then(|v| v["line"].as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Print the health file: the object, or with `lines` (the poll line and the
/// file of the product's check) the lines.
fn answer(file: &str, now: i64, lines: Option<(&str, Option<&str>)>) -> u8 {
    let health = match fs::read_to_string(file).ok().and_then(|t| J::parse(&t)) {
        Some(h) if h.get("status").as_str().is_some() => h,
        _ => {
            eprintln!("tehas status: no health file at {file} yet: the dispatcher has not run");
            return 1;
        }
    };
    let stale = stale(&health, now);
    match lines {
        Some((poll, product)) => {
            let mut lines = state_lines(&health, now, stale, poll);
            lines.extend(cycles(&health).iter().map(|c| cycle_line(c, now)));
            lines.extend(plan_lines(&health, now));
            lines.push(slice_line(&health, now));
            lines.extend(platform_line(&health, now));
            lines.extend(product.and_then(|p| product_line(Path::new(p), now)));
            for line in lines {
                println!("{line}");
            }
        }
        None => println!("{}", status_json(&health, now, stale).dump()),
    }
    u8::from(stale || health.get("status").as_str() == Some("fail"))
}

/// Older than the rule, or written at a time that cannot be read.
pub fn stale(health: &J, now: i64) -> bool {
    let written = health.get("time").as_str().and_then(epoch).unwrap_or(0);
    now - written > MAX_AGE
}

/// An age in the words tehas status has always used.
pub fn ago(then: i64, now: i64) -> String {
    let d = (now - then).max(0);
    if d < 120 {
        format!("{d} s ago")
    } else if d < 7200 {
        format!("{} min ago", d / 60)
    } else {
        format!("{} h ago", d / 3600)
    }
}

/// The age of a time the health file wrote.
pub fn age(t: &J, now: i64) -> String {
    match t.as_str().and_then(epoch) {
        Some(e) => ago(e, now),
        None => "at an unknown time".to_string(),
    }
}

/// A run of control characters, U+2028 or U+2029 is one space, and the
/// spaces at the ends go: a reason is printed as one line.
pub fn oneline(s: &str) -> String {
    let control =
        |c: char| matches!(c, '\u{1}'..='\u{1f}' | '\u{7f}'..='\u{9f}' | '\u{2028}' | '\u{2029}');
    let mut out = String::new();
    let mut run = false;
    for c in s.chars() {
        if control(c) {
            if !run {
                out.push(' ');
            }
            run = true;
        } else {
            out.push(c);
            run = false;
        }
    }
    out.trim_matches(' ').to_string()
}

fn oneline_of(v: &J) -> String {
    match v {
        J::Null | J::Bool(false) => String::new(),
        v => oneline(&v.text()),
    }
}

/// The factory line, the poll line, the queue, each switch that holds the
/// factory, the limit, a held refresh and all-hands.
pub fn state_lines(h: &J, now: i64, stale: bool, poll: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let output = h.get("output").text_or_empty();
    let status = h.get("status").text();
    lines.push(if stale {
        format!(
            "factory   stale: the dispatcher last reported {}, is its timer running? It said {status}: {output}",
            age(h.get("time"), now)
        )
    } else {
        format!("factory   {status}: {output} ({})", age(h.get("time"), now))
    });
    if !poll.is_empty() {
        lines.push(poll.to_string());
    }
    let checks = h.get("checks");
    let q = checks.get("queue");
    lines.push(if !q.truthy() {
        "queue     no snapshot yet".to_string()
    } else {
        format!(
            "queue     {} in line, {} building, {} in review, {} stopped{} ({})",
            q.get("inLine").text(),
            q.get("building").text(),
            q.get("review").text(),
            q.get("stopped").text(),
            if q.get("paused").truthy() {
                ", paused"
            } else {
                ""
            },
            age(q.get("time"), now)
        )
    });
    lines.extend(pause_lines(checks.get("pause"), now));
    let limit = checks.get("limit");
    if limit.truthy() && limit.get("state").as_str() != Some("none") {
        lines.push(limit_line(limit));
    }
    if let Some(held) = checks.at(&["demand", "refreshHold"]).as_str() {
        lines.push(format!("refresh   held: {held}"));
    }
    if checks.get("allHands").is_true() {
        lines.push(
            "all-hands tehas:all-hands is on the control issue, applied by an allowed account"
                .to_string(),
        );
    }
    lines
}

/// What holds the factory paused, one line for each switch that is set:
/// {flag: {at, reason} or null, label, lifted}. lifted is the label taken off
/// by an account that may not lift it, so the host stays paused.
fn pause_lines(p: &J, now: i64) -> Vec<String> {
    let flag = p.get("flag");
    let label = p.get("label").is_true();
    let lifted = p.get("lifted").is_true();
    let mut lines = Vec::new();
    if p.is_null() || (flag.is_null() && !label && !lifted) {
        return lines;
    }
    if !flag.is_null() {
        let reason = oneline_of(flag.get("reason"));
        lines.push(format!(
            "pause     flag, written {}{}",
            age(flag.get("at"), now),
            if reason.is_empty() {
                String::new()
            } else {
                format!(": {reason}")
            }
        ));
    }
    if label {
        lines.push("pause     label: tehas:pause on the control issue".to_string());
    }
    if lifted {
        lines.push("pause     label: tehas:pause is off the control issue, but no account in TEHAS_ALLOWED_LABELERS took it off".to_string());
    }
    lines
}

/// The limit tehas limit sets, as the health file holds it.
fn limit_line(l: &J) -> String {
    let words = match l.get("state").as_str() {
        Some("none") => "none".to_string(),
        Some("unreadable") => {
            "unreadable: nothing starts until it is set again or cleared".to_string()
        }
        Some("expired") => format!(
            "expired at {}: nothing starts until it is set again or cleared",
            l.get("until").text()
        ),
        _ => {
            let cycles = l.get("cycles");
            format!(
                "{} cycle{} at once, planning {}, {}{}",
                cycles.text(),
                if cycles.as_f64() == Some(1.0) {
                    ""
                } else {
                    "s"
                },
                if l.get("plan").truthy() { "on" } else { "off" },
                match l.get("until") {
                    J::Null => "until cleared".to_string(),
                    u => format!("until {}", u.text()),
                },
                match l.get("reason") {
                    J::Null => String::new(),
                    r => format!(": {}", oneline_of(r)),
                }
            )
        }
    };
    format!("limit     {words}")
}

/// The cycles of the health file: each one when it lists more than one,
/// otherwise the current or last one, null for none yet.
fn cycles(h: &J) -> Vec<J> {
    let all = h.at(&["checks", "cycles"]);
    if all.len() > 1 {
        all.items().to_vec()
    } else {
        vec![h.at(&["checks", "cycle"]).or(&J::Null).clone()]
    }
}

/// One cycle, as the health file writes it: issue, running, status, step,
/// startedAt, time. null is a factory with no cycle yet.
pub fn cycle_line(c: &J, now: i64) -> String {
    let step = c.get("step").or(&J::Str("-".to_string())).text();
    if c.is_null() {
        "cycle     none yet".to_string()
    } else if c.get("running").truthy() {
        format!(
            "cycle     #{} running, step {step}, started {}",
            c.get("issue").text(),
            age(c.get("startedAt"), now)
        )
    } else {
        format!(
            "cycle     #{} {} at step {step}, {}",
            c.get("issue").text(),
            c.get("status").or(&J::Str("unknown".to_string())).text(),
            age(c.get("time"), now)
        )
    }
}

/// A line for each planning turn in flight.
fn plan_lines(h: &J, now: i64) -> Vec<String> {
    h.at(&["checks", "plans"])
        .items()
        .iter()
        .map(|p| {
            format!(
                "plan      #{} turn {} running, started {}",
                p.get("issue").text(),
                p.get("turn").text(),
                age(p.get("startedAt"), now)
            )
        })
        .collect()
}

/// The slots of the last poll: the counts a factory whose settings say
/// `auto` derived from the host on that poll, and the one word for what gave
/// them. Empty for the numbers an instance wrote, which need no telling.
fn slots_line(s: &J) -> String {
    let a = s.get("cycle");
    if s.is_null() || s.get("cyclesHow").as_str() != Some("auto") {
        return String::new();
    }
    let cpus = match a.get("cpus") {
        J::Null => "?".to_string(),
        v => v.text(),
    };
    format!(
        "slots     cycles auto: {}, {}-bound{} ({} CPUs over TEHAS_CYCLE_CPUS={}, {} containers of tehas.slice over TEHAS_CONTAINER_MEMORY={}, ceiling {})\n",
        s.get("cycles").text(),
        a.get("bound").or(&J::Str("host".to_string())).text(),
        if a.get("floored").is_true() { ", no reading" } else { "" },
        cpus,
        a.get("cycleCpus").or(&J::Str("?".to_string())).text(),
        s.get("containers").text(),
        a.get("containerMemory").or(&J::Str("?".to_string())).text(),
        a.get("ceiling").or(&J::Str("?".to_string())).text(),
    )
}

/// The slice check of the dispatcher's last poll.
fn slice_line(h: &J, now: i64) -> String {
    let s = h.at(&["checks", "slice"]);
    let slots = slots_line(s.get("slots"));
    let gib = |v: &J| J::num((v.as_f64().unwrap_or(0.0) / 1_073_741_824.0 * 10.0).round() / 10.0);
    let resources = match s.get("why") {
        J::Null | J::Bool(false) => true,
        why => matches!(why.as_str(), Some("" | "resources")),
    };
    if !s.truthy() {
        "slice     unknown: no poll has reported in 10 min".to_string()
    } else if !resources {
        format!(
            "{slots}slice     {} ({})",
            s.get("hold").or(&J::Str("holding".to_string())).text(),
            age(s.get("time"), now)
        )
    } else {
        format!(
            "{slots}slice     room for {} cycles ({} GiB of {} GiB promised, reserve {}{}, {})",
            s.get("room").or(&J::num(0.0)).text(),
            gib(s.get("committed")).text(),
            gib(s.get("max")).text(),
            s.get("reserve").or(&J::Str("unknown".to_string())).text(),
            match s.get("refreshRoom") {
                J::Null => String::new(),
                r => format!(", refresh lane {} free", r.text()),
            },
            age(s.get("time"), now)
        )
    }
}

/// The triple of vX.Y.Z, or None for anything else.
fn triple(v: &J) -> Option<Vec<f64>> {
    let s = v.as_str()?;
    s.strip_prefix('v')
        .unwrap_or(s)
        .split('.')
        .map(|part| {
            let plain = !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'e' | b'E' | b'+'));
            if plain {
                part.parse().ok()
            } else {
                None
            }
        })
        .collect()
}

/// Whether version a is newer than version b, by the rule tehas status has
/// always used: some place of a is greater than the same place of b, and a
/// place b does not have counts as smaller. Either one that is no triple is
/// never newer.
fn newer(a: &J, b: &J) -> bool {
    match (triple(a), triple(b)) {
        (Some(x), Some(y)) => x
            .iter()
            .enumerate()
            .any(|(i, n)| y.get(i).is_none_or(|m| n > m)),
        _ => false,
    }
}

/// The platform the host runs: the release under ~/tehas/current, or the
/// checkout under ~/tehas/platform against origin. Nothing on a host that
/// reported neither.
fn platform_line(h: &J, now: i64) -> Option<String> {
    let p = h.at(&["checks", "platform"]);
    if !p.truthy() {
        return None;
    }
    let fetched = match p.get("fetchedAt") {
        J::Null => ", never fetched".to_string(),
        t => format!(", fetched {}", age(t, now)),
    };
    let release = p.get("release");
    Some(if !release.is_null() {
        let newest = p.get("newest");
        let said = if newest.is_null() {
            "newest unknown".to_string()
        } else if newer(newest, release) {
            format!("{} is out", newest.text())
        } else {
            format!("newest {}", newest.text())
        };
        format!("platform  {}, {said}{fetched}", release.text())
    } else {
        let commit = p.get("commit").or(&J::Str("unknown".to_string())).text();
        let behind = if p.get("behind").above_zero() {
            format!(
                "{} commits behind {} since {}",
                p.get("behind").text(),
                p.get("upstream").text(),
                first(&p.get("behindSince").or(&J::Str(String::new())).text(), 10)
            )
        } else {
            format!("up to date with {}", p.get("upstream").text())
        };
        format!("platform  {}, {behind}{fetched}", first(&commit, 7))
    })
}

/// The last tehas check-product, when one ran and its file can be read.
fn product_line(file: &Path, now: i64) -> Option<String> {
    let p = J::parse(&fs::read_to_string(file).ok()?)?;
    let said = if p.get("ok").truthy() {
        "keeps the contract".to_string()
    } else {
        let failures = match p.get("failures") {
            J::Arr(a) => a,
            _ => return None,
        };
        let joined: Option<Vec<String>> = failures
            .iter()
            .map(|f| match f {
                J::Null => Some(String::new()),
                J::Str(_) | J::Num(_) | J::Bool(_) => Some(f.text()),
                _ => None,
            })
            .collect();
        format!("{} to fix: {}", failures.len(), joined?.join("; "))
    };
    Some(format!(
        "product   {said}, checked {}",
        age(p.get("at"), now)
    ))
}

/// Seconds since a time the health file wrote, null when it cannot be read.
fn since(t: &J, now: i64) -> J {
    t.as_str()
        .and_then(epoch)
        .map_or(J::Null, |e| J::num((now - e).max(0) as f64))
}

/// The object tehas status --json prints.
pub fn status_json(h: &J, now: i64, stale: bool) -> J {
    let checks = h.get("checks");
    let empty = obj(vec![]);
    let p = checks.get("pause").or(&empty);
    let s = checks.get("slice").or(&J::Null);
    let flag = p.get("flag");
    let label = p.get("label").is_true();
    let lifted = p.get("lifted").is_true();
    let status = h.get("status");
    let reasons = if status.as_str() == Some("pass") {
        vec![]
    } else {
        vec![h.get("output").or(&J::Str(String::new())).clone()]
    };
    let limit = match checks.get("limit") {
        J::Null | J::Bool(false) => J::Null,
        J::Obj(o) => J::Obj(o.iter().filter(|(k, _)| k != "reason").cloned().collect()),
        other => other.clone(),
    };
    let running = checks
        .get("cycles")
        .items()
        .iter()
        .map(|c| {
            let mut f = vec![
                ("issue", c.get("issue").clone()),
                ("since", c.get("startedAt").clone()),
            ];
            if !c.get("step").is_null() {
                f.push(("step", c.get("step").clone()));
            }
            obj(f)
        })
        .collect();
    let planning = checks
        .get("plans")
        .items()
        .iter()
        .map(|c| {
            let mut f = vec![
                ("issue", c.get("issue").clone()),
                ("turn", c.get("turn").clone()),
            ];
            if !c.get("startedAt").is_null() {
                f.push(("since", c.get("startedAt").clone()));
            }
            obj(f)
        })
        .collect();
    let demand = match checks.get("demand") {
        J::Null | J::Bool(false) => J::Null,
        d => {
            let mut f = vec![("waiting", d.get("waiting").clone())];
            if !d.get("oldest").is_null() {
                f.push(("oldestWait", since(d.get("oldest"), now)));
            }
            if !d.get("refreshHold").is_null() {
                f.push(("refreshHold", J::Str(oneline_of(d.get("refreshHold")))));
            }
            obj(f)
        }
    };
    let pause = |text: String| obj(vec![("why", J::str("pause")), ("text", J::Str(text))]);
    let hold = if !flag.is_null() {
        let reason = oneline_of(flag.get("reason"));
        pause(format!(
            "paused by the flag tehas pause wrote{}",
            if reason.is_empty() {
                String::new()
            } else {
                format!(": {reason}")
            }
        ))
    } else if label {
        pause("paused by tehas:pause on the control issue".to_string())
    } else if lifted {
        pause("paused: tehas:pause was taken off by an account that may not lift it".to_string())
    } else if !s.is_null() && !s.get("why").or(&J::Str(String::new())).text().is_empty() {
        obj(vec![
            ("why", s.get("why").clone()),
            ("text", s.get("hold").or(&J::Str(String::new())).clone()),
        ])
    } else {
        J::Null
    };
    let room = if s.is_null() {
        J::Null
    } else {
        obj([
            ("cycles", "free"),
            ("refreshes", "refreshRoom"),
            ("maxAwaiting", "awaitRoom"),
            ("slice", "room"),
            ("limit", "limitRoom"),
        ]
        .into_iter()
        .map(|(name, key)| (name, s.get(key).clone()))
        .filter(|(_, v)| !v.is_null())
        // With `auto` the count in force is the poll's, not the setting's,
        // so the object carries it and the word for what bounded it. The
        // numbers a factory wrote add nothing and are absent, as `limit`
        // is without a limit.
        .chain(std::iter::once(("slots", s.get("slots").clone())))
        .filter(|(_, v)| !v.is_null())
        .collect())
    };
    let mut fields = vec![
        ("api", J::num(1.0)),
        ("time", h.get("time").clone()),
        ("age", since(h.get("time"), now)),
        ("stale", J::Bool(stale)),
        ("state", status.clone()),
        ("reasons", J::Arr(reasons)),
        (
            "paused",
            obj(vec![
                ("local", J::Bool(!flag.is_null())),
                ("label", J::Bool(label || lifted)),
            ]),
        ),
        ("allHands", checks.get("allHands").clone()),
        ("limit", limit),
        ("running", J::Arr(running)),
        ("planning", J::Arr(planning)),
        ("demand", demand),
        ("hold", hold),
        ("room", room),
    ];
    fields.retain(|(k, v)| !matches!(*k, "allHands" | "limit") || !v.is_null());
    obj(fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_792_245_620; // 2026-10-17T14:00:20Z

    fn h(text: &str) -> J {
        J::parse(text).unwrap()
    }

    #[test]
    fn ages_in_seconds_minutes_and_hours() {
        assert_eq!(ago(NOW - 40, NOW), "40 s ago");
        assert_eq!(ago(NOW + 5, NOW), "0 s ago");
        assert_eq!(ago(NOW - 600, NOW), "10 min ago");
        assert_eq!(ago(NOW - 7200, NOW), "2 h ago");
        assert_eq!(age(&J::str("not a time"), NOW), "at an unknown time");
        assert_eq!(age(&J::Null, NOW), "at an unknown time");
    }

    #[test]
    fn ten_minutes_is_the_rule() {
        assert!(!stale(&h(r#"{"time":"2026-10-17T13:50:20Z"}"#), NOW));
        assert!(stale(&h(r#"{"time":"2026-10-17T13:50:19Z"}"#), NOW));
        assert!(stale(&h(r#"{"time":"yesterday"}"#), NOW));
    }

    #[test]
    fn a_reason_is_one_line() {
        assert_eq!(oneline("  a\n\n\tb\u{2028}c  "), "a b c");
    }

    #[test]
    fn the_factory_the_queue_and_what_holds_it() {
        let health = h(
            r#"{"status":"warn","output":"Paused.","time":"2026-10-17T14:00:00Z",
          "checks":{"queue":{"inLine":2,"building":0,"review":1,"stopped":0,"paused":true,"time":"2026-10-17T14:00:00Z"},
          "pause":{"flag":{"at":"2026-10-17T13:00:20Z","reason":"a\nb"},"label":true,"lifted":false},
          "limit":{"state":"ok","cycles":1,"plan":false,"reason":"night"},
          "demand":{"refreshHold":"two in review"},"allHands":true}}"#,
        );
        assert_eq!(
            state_lines(&health, NOW, false, "poll      last 1 s"),
            [
                "factory   warn: Paused. (20 s ago)",
                "poll      last 1 s",
                "queue     2 in line, 0 building, 1 in review, 0 stopped, paused (20 s ago)",
                "pause     flag, written 60 min ago: a b",
                "pause     label: tehas:pause on the control issue",
                "limit     1 cycle at once, planning off, until cleared: night",
                "refresh   held: two in review",
                "all-hands tehas:all-hands is on the control issue, applied by an allowed account",
            ]
        );
        assert_eq!(
            state_lines(&health, NOW, true, "")[0],
            "factory   stale: the dispatcher last reported 20 s ago, is its timer running? It said warn: Paused."
        );
    }

    #[test]
    fn a_cycle_running_ended_or_none() {
        let running =
            h(r#"{"issue":12,"running":true,"step":"develop","startedAt":"2026-10-17T13:50:20Z"}"#);
        assert_eq!(
            cycle_line(&running, NOW),
            "cycle     #12 running, step develop, started 10 min ago"
        );
        let ended = h(
            r#"{"issue":12,"running":false,"status":"handoff","step":"review","time":"2026-10-17T13:58:20Z"}"#,
        );
        assert_eq!(
            cycle_line(&ended, NOW),
            "cycle     #12 handoff at step review, 2 min ago"
        );
        assert_eq!(cycle_line(&J::Null, NOW), "cycle     none yet");
    }

    #[test]
    fn the_slice_with_room_holding_or_unknown() {
        let room = h(
            r#"{"checks":{"slice":{"why":"","room":2,"committed":4294967296,"max":21474836480,"reserve":"2g","refreshRoom":1,"time":"2026-10-17T14:00:00Z"}}}"#,
        );
        assert_eq!(
            slice_line(&room, NOW),
            "slice     room for 2 cycles (4 GiB of 20 GiB promised, reserve 2g, refresh lane 1 free, 20 s ago)"
        );
        let held = h(
            r#"{"checks":{"slice":{"why":"count","hold":"holding: two running","time":"2026-10-17T14:00:00Z"}}}"#,
        );
        assert_eq!(
            slice_line(&held, NOW),
            "slice     holding: two running (20 s ago)"
        );
        assert_eq!(
            slice_line(&h("{}"), NOW),
            "slice     unknown: no poll has reported in 10 min"
        );
    }

    #[test]
    fn a_slice_of_auto_slots_says_the_count_and_what_gave_it() {
        let auto = h(
            r#"{"checks":{"slice":{"why":"","room":3,"committed":0,"max":21474836480,"reserve":"2g","time":"2026-10-17T14:00:00Z",
                 "slots":{"cycles":6,"containers":6,"cyclesHow":"auto","containersHow":"auto",
                          "cycle":{"bound":"cpu","slots":6,"cpus":24,"cycleCpus":"4","containerMemory":"8g","ceiling":"16","floored":false},
                          "container":{"bound":"memory","slots":6,"cpus":24,"cycleCpus":"4","containerMemory":"8g","ceiling":"16","floored":false}}}}}"#,
        );
        assert_eq!(
            slice_line(&auto, NOW),
            "slots     cycles auto: 6, cpu-bound (24 CPUs over TEHAS_CYCLE_CPUS=4, 6 containers of tehas.slice over TEHAS_CONTAINER_MEMORY=8g, ceiling 16)\nslice     room for 3 cycles (0 GiB of 20 GiB promised, reserve 2g, 20 s ago)"
        );
        // A poll that derived nothing writes no slots object, and the line is
        // the one of every factory that sets numbers.
        let numbers = h(
            r#"{"checks":{"slice":{"why":"","room":2,"committed":4294967296,"max":21474836480,"reserve":"2g","time":"2026-10-17T14:00:00Z"}}}"#,
        );
        assert!(!slice_line(&numbers, NOW).starts_with("slots"));
        // A host whose readings said nothing is said as it is.
        let held = h(
            r#"{"checks":{"slice":{"why":"","room":0,"committed":0,"max":0,"reserve":"2g","time":"2026-10-17T14:00:00Z",
                 "slots":{"cycles":1,"containers":2,"cyclesHow":"auto","containersHow":"auto",
                          "cycle":{"bound":"host","slots":0,"cpus":null,"cycleCpus":"4","containerMemory":"8g","ceiling":"16","floored":true}}}}}"#,
        );
        assert!(
            slice_line(&held, NOW)
                .starts_with("slots     cycles auto: 1, host-bound, no reading (? CPUs"),
            "{}",
            slice_line(&held, NOW)
        );
    }

    #[test]
    fn the_platform_a_release_or_a_checkout() {
        let out = h(
            r#"{"checks":{"platform":{"release":"v0.16.1","newest":"v0.16.2","fetchedAt":"2026-10-17T13:40:20Z"}}}"#,
        );
        assert_eq!(
            platform_line(&out, NOW).unwrap(),
            "platform  v0.16.1, v0.16.2 is out, fetched 20 min ago"
        );
        let same = h(r#"{"checks":{"platform":{"release":"v0.16.1","newest":"v0.16.1"}}}"#);
        assert_eq!(
            platform_line(&same, NOW).unwrap(),
            "platform  v0.16.1, newest v0.16.1, never fetched"
        );
        let behind = h(
            r#"{"checks":{"platform":{"commit":"cb6912a0","behind":19,"upstream":"origin/main","behindSince":"2026-10-13T00:00:00Z"}}}"#,
        );
        assert_eq!(
            platform_line(&behind, NOW).unwrap(),
            "platform  cb6912a, 19 commits behind origin/main since 2026-10-13, never fetched"
        );
        assert_eq!(platform_line(&h("{}"), NOW), None);
    }

    #[test]
    fn newer_is_the_rule_tehas_status_has_used() {
        assert!(newer(&J::str("v0.16.2"), &J::str("v0.16.1")));
        assert!(!newer(&J::str("v0.16.1"), &J::str("v0.16.1")));
        assert!(!newer(&J::str("main"), &J::str("v0.16.1")));
        assert!(newer(&J::str("v1.0.0.1"), &J::str("v1.0.0")));
    }

    #[test]
    fn the_json_object_leaves_out_what_has_no_value() {
        let health =
            h(r#"{"status":"pass","output":"Idle.","time":"2026-10-17T14:00:00Z","checks":{}}"#);
        assert_eq!(
            status_json(&health, NOW, false).dump(),
            r#"{"api":1,"time":"2026-10-17T14:00:00Z","age":20,"stale":false,"state":"pass","reasons":[],"paused":{"local":false,"label":false},"running":[],"planning":[],"demand":null,"hold":null,"room":null}"#
        );
    }

    #[test]
    fn the_json_limit_keeps_its_order_without_the_reason() {
        let health = h(
            r#"{"status":"warn","checks":{"limit":{"state":"ok","cycles":1,"plan":true,"reason":"x"}}}"#,
        );
        let v = status_json(&health, NOW, true);
        assert_eq!(
            v.get("limit").dump(),
            r#"{"state":"ok","cycles":1,"plan":true}"#
        );
        assert_eq!(v.get("reasons").dump(), r#"[""]"#);
    }
}

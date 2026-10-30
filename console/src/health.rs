//! The factory's health file, live/health.json, as tehasbot health writes it
//! (libexec/health): pass, warn or fail, the one line that says why, and
//! every check under `checks`.
//!
//!   console health report <home> [--sessions <json>] [--units <text>]
//!     [--backup <json>] [--poll-over <json> --poll-budget <spec>]
//!
//! It reads what the factory already writes under <home>/live/ - the queue
//! snapshot, the cycles and their endings, the last poll's slice and demand,
//! the previews' disk and the platform - and under <home>/state/ the pause
//! flag, the limit and the locks of the planning turns. What only the store or
//! systemd know comes in as arguments, as the shell got it: the open planning
//! sessions (`store plan-show --open`), the failed tehas units (`systemctl
//! --user --failed --plain --no-legend`, left out when systemd did not answer
//! with a list, empty or not), the store's newest backup (`store
//! backup --show`, left out when the store did not answer) and the idle polls
//! over their budget (`store poll-idle --budget`, left out when it refused).
//! A file that is not there, or not what it should be, is null. The report
//! goes to standard output; nothing is written.
//!
//! Exit: 0 the report was printed; 2 usage.

use crate::flags::{self, epoch};
use crate::jq::{first, obj, J, NULL};
use std::fs::{self, File};
use std::path::Path;
use tehas_core::time;

/// A queue snapshot older than this is not news.
const QUEUE_MAX_AGE: i64 = 600;
/// The last poll's slice and demand are not believed after this.
const POLL_MAX_AGE: i64 = 600;
/// The previews' measure of the disk is not believed after this.
const DISK_MAX_AGE: i64 = 1800;
/// A platform this long behind is news.
const BEHIND_FOR: i64 = 86_400;
/// The newest backup of the store is old after this.
const BACKUP_MAX_AGE: f64 = 172_800.0;

#[derive(Default)]
struct Given {
    sessions: Option<String>,
    units: Option<String>,
    backup: Option<String>,
    poll_over: Option<String>,
    poll_budget: Option<String>,
}

pub fn main(given: &[String]) -> u8 {
    let usage = || {
        eprintln!(
            "console health: usage: report <home> [--sessions <json>] [--units <text>] [--backup <json>] [--poll-over <json> --poll-budget <spec>]"
        );
        2
    };
    let (home, rest) = match given {
        [verb, home, rest @ ..] if verb == "report" => (Path::new(home), rest),
        _ => return usage(),
    };
    let mut g = Given::default();
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let Some(v) = it.next().cloned() else {
            return usage();
        };
        match flag.as_str() {
            "--sessions" => g.sessions = Some(v),
            "--units" => g.units = Some(v),
            "--backup" => g.backup = Some(v),
            "--poll-over" => g.poll_over = Some(v),
            "--poll-budget" => g.poll_budget = Some(v),
            _ => return usage(),
        }
    }
    println!("{}", report(home, &g, time::now()).pretty());
    0
}

fn read(path: &Path) -> Option<J> {
    J::parse(&fs::read_to_string(path).ok()?)
}

/// A file that is there and not empty, as one JSON value, or null.
fn snapshot(path: &Path) -> J {
    match fs::metadata(path) {
        Ok(m) if m.len() > 0 => read(path).unwrap_or(J::Null),
        _ => J::Null,
    }
}

fn age_of(t: &J, now: i64) -> Option<i64> {
    t.as_str().and_then(epoch).map(|e| now - e)
}

/// `todate` of a number, null for anything else.
fn iso(v: &J) -> J {
    match v.as_f64() {
        Some(n) => J::Str(time::iso(n.floor() as i64)),
        None => J::Null,
    }
}

/// An age in seconds as the output line says it.
fn ago(v: &J) -> String {
    let n = v.as_f64().unwrap_or(0.0);
    if n < 120.0 {
        format!("{} s", v.text())
    } else if n < 7200.0 {
        format!("{} min", J::num((n / 60.0).floor()).text())
    } else {
        format!("{} h", J::num((n / 3600.0).floor()).text())
    }
}

fn get_fields(v: &J, keys: &[&str]) -> Vec<(String, J)> {
    keys.iter()
        .map(|k| (k.to_string(), v.get(k).clone()))
        .collect()
}

/// The free space of the host as tehasbot preview measured it, with the time
/// of the measure.
fn disk(live: &Path, now: i64) -> J {
    let p = read(&live.join("previews.json")).unwrap_or(J::Null);
    let fresh = age_of(p.get("generatedAt"), now).is_some_and(|a| a < DISK_MAX_AGE);
    match p.get("disk") {
        J::Obj(d) if fresh => {
            let mut d = d.clone();
            set(&mut d, "time", p.get("generatedAt").clone());
            J::Obj(d)
        }
        _ => J::Null,
    }
}

/// `. + {key: value}`: a key already there keeps its place.
fn set(o: &mut Vec<(String, J)>, key: &str, v: J) {
    match o.iter_mut().find(|(k, _)| k == key) {
        Some(f) => f.1 = v,
        None => o.push((key.to_string(), v)),
    }
}

/// The platform checkout against origin, as tehasbot status last read it.
fn platform(live: &Path) -> J {
    match read(&live.join("platform.json")) {
        Some(p @ J::Obj(_)) if matches!(p.get("behind"), J::Num(_)) => p,
        _ => J::Null,
    }
}

/// The slice as the last poll left it, when that poll is young enough.
fn slice(live: &Path, now: i64) -> J {
    let d = snapshot(&live.join("dispatch.json"));
    let young = age_of(d.get("time"), now).is_some_and(|a| a < POLL_MAX_AGE);
    if !matches!(d, J::Obj(_)) || d.get("why").as_str().is_none() || !young {
        return J::Null;
    }
    let mut f = get_fields(
        &d,
        &[
            "time",
            "why",
            "hold",
            "count",
            "room",
            "committed",
            "max",
            "reserve",
        ],
    );
    for k in ["free", "awaitRoom", "limitRoom", "refreshRoom"] {
        if !d.get(k).is_null() {
            f.push((k.to_string(), d.get(k).clone()));
        }
    }
    // What the two counts are when an instance says `auto`: the poll derived
    // them from the host on this very poll, and the reason says which reading
    // gave them. Left out, as every other optional field is, when the poll
    // wrote none: a factory with numbers for both settings reports nothing
    // new.
    if !d.get("slots").is_null() {
        f.push(("slots".to_string(), d.get("slots").clone()));
    }
    J::Obj(f)
}

/// The demand as the last poll left it, when that poll is young enough.
fn demand(live: &Path, now: i64) -> J {
    let d = snapshot(&live.join("demand.json"));
    let young = age_of(d.get("time"), now).is_some_and(|a| a < POLL_MAX_AGE);
    if !matches!(d, J::Obj(_)) || !matches!(d.get("waiting"), J::Num(_)) || !young {
        return J::Null;
    }
    let mut f = get_fields(&d, &["time", "waiting"]);
    if let J::Str(_) = d.get("oldest") {
        f.push(("oldest".to_string(), d.get("oldest").clone()));
    }
    if let Some(h) = d.get("refreshHold").as_str() {
        f.push(("refreshHold".to_string(), J::Str(first(h, 300))));
    }
    J::Obj(f)
}

/// Whether some process holds the lock: a lock no one holds is taken for a
/// moment and given back when the file closes, as `flock -n` does.
fn held(lock: &Path) -> bool {
    File::open(lock).is_ok_and(|f| f.try_lock().is_err())
}

/// The planning turns in flight, each {issue, turn, startedAt}: the sessions
/// the store has running whose turn holds its lock and has not left its exit
/// status (lib/planning.sh).
fn plans(home: &Path, sessions: &J) -> J {
    let state = home.join("state");
    let mut out = Vec::new();
    for s in sessions.get("sessions").items() {
        if !s.get("running").is_true() || !matches!(s.get("issue"), J::Num(_)) {
            continue;
        }
        let n = s.get("issue").text();
        let plain = n.len() <= 10
            && n.bytes().all(|b| b.is_ascii_digit())
            && !n.starts_with('0')
            && !n.is_empty();
        if !plain {
            continue;
        }
        let lock = state.join(format!("plan-{n}.json.lock"));
        let rc = state.join(format!("plan-{n}.json.rc"));
        if !lock.is_file() || rc.exists() || !held(&lock) {
            continue;
        }
        let turns = match s.get("turns") {
            t @ J::Num(_) => t.as_f64().unwrap_or(0.0),
            _ => 0.0,
        };
        out.push(obj(vec![
            ("issue", s.get("issue").clone()),
            ("turn", J::num(turns + 1.0)),
            ("startedAt", iso(s.get("started_at"))),
        ]));
    }
    J::Arr(out)
}

/// The pause flag of this host: null with none, {at, reason} with one, and
/// both null for a file that is no flag, which holds the factory all the same.
fn pause_flag(home: &Path) -> J {
    let r = flags::read_pause(&home.join("state/pause.json"));
    let field = |k: &str| r.json_of(k).and_then(J::parse);
    match r.state() {
        "none" => J::Null,
        "ok" => obj(vec![
            ("at", field("at").unwrap_or(J::Null)),
            ("reason", field("reason").unwrap_or(J::str(""))),
        ]),
        _ => obj(vec![("at", J::Null), ("reason", J::Null)]),
    }
}

/// The failed tehas units, from what systemctl printed. No `--units` is null:
/// systemd did not answer, which is not the same as none having failed.
fn units(text: &str) -> J {
    J::Arr(
        text.split('\n')
            .map(|l| l.split(' ').next().unwrap_or(""))
            .filter(|u| u.starts_with("tehas-"))
            .map(J::str)
            .collect(),
    )
}

fn in_list(v: &J, words: &[&str]) -> bool {
    v.as_str().is_some_and(|s| words.contains(&s))
}

fn ended_well(c: &J) -> bool {
    in_list(c.get("status").or(&J::str("")), &["merged", "handoff"])
}

/// One cycle as the health file lists it.
fn brief(c: &J) -> J {
    obj(vec![
        ("issue", c.get("issue").clone()),
        ("running", c.get("running").or(&J::Bool(false)).clone()),
        ("status", c.get("status").clone()),
        ("step", c.get("step").clone()),
        ("startedAt", iso(c.get("startedAt"))),
        ("time", iso(c.get("updatedAt"))),
    ])
}

fn joined(v: &J) -> String {
    v.items()
        .iter()
        .map(|x| if x.is_null() { String::new() } else { x.text() })
        .collect::<Vec<_>>()
        .join(", ")
}

fn report(home: &Path, g: &Given, now: i64) -> J {
    let live = home.join("live");
    let endings = match read(&live.join("endings.json")) {
        Some(e @ J::Arr(_)) => e,
        _ => J::Arr(vec![]),
    };
    let queue = snapshot(&live.join("status.json"));
    let view = snapshot(&live.join("running.json"));
    let disk = disk(&live, now);
    let platform = platform(&live);
    let slice = slice(&live, now);
    let demand = demand(&live, now);
    let sessions = g.sessions.as_deref().and_then(J::parse).unwrap_or(J::Null);
    let plans = plans(home, &sessions);
    let limit =
        J::parse(&flags::read(&home.join("state/limit.json"), now).json()).unwrap_or(J::Null);

    let flag = pause_flag(home);
    let bool_or_false = |v: &J| match v {
        J::Bool(b) => *b,
        _ => false,
    };
    let label = match (queue.at(&["pause", "label"]), queue.get("paused")) {
        (J::Bool(b), _) => *b,
        (_, J::Bool(b)) => *b,
        _ => false,
    };
    let lifted = bool_or_false(queue.at(&["pause", "lifted"]));
    let pause = obj(vec![
        ("flag", flag.clone()),
        ("label", J::Bool(label)),
        ("lifted", J::Bool(lifted)),
    ]);

    // systemd did not answer: no list, and no warning raised from one.
    let units = g.units.as_deref().map(units);
    let (backup, backup_known) = match g.backup.as_deref().and_then(J::parse) {
        Some(b) => match b.get("newest") {
            J::Null => (J::Null, true),
            n => (
                obj(vec![
                    ("at", n.get("at").clone()),
                    ("ageSeconds", n.get("age_seconds").clone()),
                ]),
                true,
            ),
        },
        None => (J::Null, false),
    };
    let poll_cost = match (g.poll_over.as_deref().and_then(J::parse), &g.poll_budget) {
        (Some(over), Some(budget)) => obj(vec![
            ("over", over.or(&J::Arr(vec![])).clone()),
            ("budget", J::str(budget.clone())),
        ]),
        _ => J::Null,
    };

    // The view tehas work writes, or the one object an earlier version wrote.
    let (running, last) = match &view {
        J::Obj(o) if o.iter().any(|(k, _)| k == "cycles") => (
            view.get("cycles").or(&J::Arr(vec![])).clone(),
            view.get("last").clone(),
        ),
        J::Obj(_) if view.get("running").is_true() => (J::Arr(vec![view.clone()]), J::Null),
        J::Obj(_) => (J::Arr(vec![]), view.clone()),
        _ => (J::Arr(vec![]), J::Null),
    };
    let zero = J::num(0.0);
    let failed = !last.is_null()
        && !ended_well(&last)
        && !running.items().iter().any(|c| {
            // A cycle of no known start is running now.
            c.get("startedAt").is_null()
                || c.get("startedAt")
                    .cmp(last.get("updatedAt").or(&zero))
                    .is_ge()
        });
    let behind = !platform.is_null()
        && platform.get("behind").above_zero()
        && now
            - platform
                .get("behindSince")
                .as_str()
                .and_then(epoch)
                .unwrap_or(0)
            > BEHIND_FOR;
    let queue_age = (!queue.is_null()).then(|| {
        now - queue
            .get("generatedAt")
            .as_str()
            .and_then(epoch)
            .unwrap_or(0)
    });
    let no_start = !slice.is_null() && in_list(slice.get("why"), &["slice", "disk", "unreadable"]);
    let no_resources = !slice.is_null() && slice.get("why").as_str() == Some("resources");
    let old_queue = queue_age.is_some_and(|a| a > QUEUE_MAX_AGE);
    let paused = !flag.is_null() || label || lifted;
    let limit_holds = in_list(limit.get("state"), &["expired", "unreadable"]);
    let over_budget = !poll_cost.is_null() && poll_cost.get("over").len() > 0;
    let old_backup = backup_known
        && (backup.is_null()
            || backup
                .get("ageSeconds")
                .cmp(&J::num(BACKUP_MAX_AGE))
                .is_gt());
    let disk_low = !disk.is_null() && disk.get("low").truthy();
    let has_units = units.as_ref().is_some_and(|u| u.len() > 0);

    let status = if queue.is_null() || failed {
        "fail"
    } else if old_queue
        || paused
        || limit_holds
        || no_start
        || no_resources
        || has_units
        || disk_low
        || behind
        || old_backup
        || over_budget
    {
        "warn"
    } else {
        "pass"
    };

    let output = if queue.is_null() {
        "No queue snapshot yet: the dispatcher has not written one.".to_string()
    } else if failed {
        format!(
            "The last cycle failed: #{} ended as {}.",
            last.get("issue").text(),
            last.get("status").text()
        )
    } else if old_queue {
        format!(
            "The queue snapshot is {} old: GitHub did not answer the last polls.",
            ago(&J::num(queue_age.unwrap_or(0) as f64))
        )
    } else if paused {
        "The factory is paused.".to_string()
    } else if limit_holds {
        if limit.get("state").as_str() == Some("expired") {
            format!(
                "Nothing starts: the limit expired at {}. Set it again or clear it (tehas limit).",
                limit.get("until").text()
            )
        } else {
            "Nothing starts: the limit cannot be read. Set it again or clear it (tehas limit)."
                .to_string()
        }
    } else if no_start || no_resources {
        let hold = slice.get("hold");
        if no_resources {
            let h = hold.text_or_empty();
            let h = match h.strip_prefix("holding: no resources: ") {
                Some(rest) => format!("No resources: {rest}"),
                None => h,
            };
            format!("{h}.")
        } else {
            hold.or(&J::str("holding")).text()
        }
    } else if has_units {
        format!(
            "systemd gave up on {}: run systemctl --user reset-failed and start it again.",
            joined(units.as_ref().unwrap_or(&J::Arr(vec![])))
        )
    } else if disk_low {
        if disk.get("freeBytes").is_null() {
            "The host's free disk space could not be read: previews are not being built."
                .to_string()
        } else {
            "The host is low on disk: previews are not being built.".to_string()
        }
    } else if behind {
        format!(
            "The platform is {} commits behind {} since {}: run tehas update.",
            platform.get("behind").text(),
            platform.get("upstream").text(),
            first(&platform.get("behindSince").text_or_empty(), 10)
        )
    } else if old_backup && backup.is_null() {
        "There is no backup of the state store: tehasbot clean makes one a day.".to_string()
    } else if old_backup {
        format!(
            "The newest backup of the state store is {} old.",
            ago(backup.get("ageSeconds"))
        )
    } else if over_budget {
        format!(
            "An idle poll is over its budget on {} (TEHAS_POLL_BUDGET={}).",
            joined(poll_cost.get("over")),
            poll_cost.get("budget").text()
        )
    } else if running.len() > 0 {
        let issues: Vec<String> = running
            .items()
            .iter()
            .map(|c| format!("#{}", c.get("issue").text()))
            .collect();
        format!("Working on {}.", issues.join(", "))
    } else {
        "Idle, taking work.".to_string()
    };

    let count = |k: &str| J::num(queue.get(k).len() as f64);
    let queue_check = if queue.is_null() {
        J::Null
    } else {
        obj(vec![
            ("time", queue.get("generatedAt").clone()),
            ("paused", J::Bool(paused)),
            ("inLine", count("queued")),
            ("building", count("running")),
            ("review", count("awaiting")),
            ("stopped", count("stopped")),
        ])
    };
    let current = running.items().first().unwrap_or(&NULL).or(&last);
    let cycle = if current.is_null() {
        J::Null
    } else {
        brief(current)
    };
    let cycles = J::Arr(running.items().iter().map(brief).collect());

    // The failed endings of the last ten and the last cycle, newest first, one
    // for each cycle.
    let mut ends: Vec<J> = endings.items().to_vec();
    if !last.is_null() {
        ends.push(last.clone());
    }
    let mut ends: Vec<J> = ends
        .into_iter()
        .filter(|e| matches!(e, J::Obj(_)) && !ended_well(e))
        .collect();
    let key = |e: &J| J::Arr(vec![e.get("issue").clone(), e.get("startedAt").clone()]);
    ends.sort_by(|a, b| key(a).cmp(&key(b)));
    ends.dedup_by(|b, a| key(a).cmp(&key(b)).is_eq());
    ends.sort_by(|a, b| a.get("updatedAt").cmp(b.get("updatedAt")));
    ends.reverse();
    let failed_list = J::Arr(
        ends.iter()
            .map(|e| {
                obj(vec![
                    ("issue", e.get("issue").clone()),
                    ("status", e.get("status").clone()),
                    ("step", e.get("step").clone()),
                    ("startedAt", iso(e.get("startedAt"))),
                    ("time", iso(e.get("updatedAt"))),
                ])
            })
            .collect(),
    );
    let disk_check = if disk.is_null() {
        J::Null
    } else {
        J::Obj(get_fields(
            &disk,
            &["time", "freeBytes", "minFreeBytes", "low"],
        ))
    };
    let all_hands = if queue.is_null() || old_queue {
        J::Null
    } else {
        queue.get("allHands").clone()
    };
    let backup_check = if backup.is_null() {
        J::Null
    } else {
        obj(vec![
            ("time", iso(backup.get("at"))),
            ("ageSeconds", backup.get("ageSeconds").clone()),
        ])
    };

    obj(vec![
        ("status", J::str(status)),
        ("time", J::Str(time::iso(now))),
        ("output", J::Str(output)),
        (
            "checks",
            obj(vec![
                ("queue", queue_check),
                ("cycle", cycle),
                ("cycles", cycles),
                ("plans", plans),
                ("failed", failed_list),
                ("disk", disk_check),
                ("slice", slice),
                ("demand", demand),
                ("limit", limit),
                ("allHands", all_hands),
                ("pause", pause),
                ("platform", platform),
                ("units", units.unwrap_or(J::Null)),
                ("backup", backup_check),
                ("poll_cost", poll_cost),
            ]),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const NOW: i64 = 1_792_245_620; // 2026-10-17T14:00:20Z

    struct Home(PathBuf);

    impl Drop for Home {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// A home with a fresh queue snapshot and nothing else.
    fn home(name: &str) -> Home {
        let dir = std::env::temp_dir().join(format!("tehas-health-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("live")).unwrap();
        fs::create_dir_all(dir.join("state")).unwrap();
        let h = Home(dir);
        h.write(
            "live/status.json",
            r#"{"generatedAt":"2026-10-17T14:00:10Z","queued":[1,2],"running":[3],"awaiting":[],"stopped":[],"allHands":false}"#,
        );
        h
    }

    impl Home {
        fn write(&self, path: &str, text: &str) {
            fs::write(self.0.join(path), text).unwrap();
        }

        fn report(&self, g: &Given) -> J {
            report(&self.0, g, NOW)
        }
    }

    fn said(r: &J) -> (String, String) {
        (r.get("status").text(), r.get("output").text())
    }

    #[test]
    fn a_fresh_queue_and_nothing_else_is_idle() {
        let h = home("idle");
        let r = h.report(&Given::default());
        assert_eq!(said(&r), ("pass".into(), "Idle, taking work.".into()));
        assert_eq!(
            r.get("checks").get("queue").dump(),
            r#"{"time":"2026-10-17T14:00:10Z","paused":false,"inLine":2,"building":1,"review":0,"stopped":0}"#
        );
        assert_eq!(r.at(&["checks", "allHands"]).dump(), "false");
        assert_eq!(r.at(&["checks", "limit"]).dump(), r#"{"state":"none"}"#);
    }

    #[test]
    fn no_queue_snapshot_fails() {
        let h = home("noqueue");
        fs::remove_file(h.0.join("live/status.json")).unwrap();
        let r = h.report(&Given::default());
        assert_eq!(r.get("status").text(), "fail");
        assert_eq!(r.at(&["checks", "queue"]), &J::Null);
    }

    #[test]
    fn a_last_cycle_that_failed_fails_until_one_starts_after_it() {
        let h = home("failed");
        h.write(
            "live/running.json",
            r#"{"cycles":[],"last":{"issue":7,"running":false,"status":"aborted","step":"develop","startedAt":100,"updatedAt":200}}"#,
        );
        let r = h.report(&Given::default());
        assert_eq!(
            said(&r),
            (
                "fail".into(),
                "The last cycle failed: #7 ended as aborted.".into()
            )
        );
        assert_eq!(r.at(&["checks", "failed"]).len(), 1);
        h.write(
            "live/running.json",
            r#"{"cycles":[{"issue":8,"running":true,"startedAt":300}],"last":{"issue":7,"status":"aborted","startedAt":100,"updatedAt":200}}"#,
        );
        let r = h.report(&Given::default());
        assert_eq!(said(&r), ("pass".into(), "Working on #8.".into()));
        assert_eq!(
            r.at(&["checks", "cycle", "startedAt"]).text(),
            "1970-01-01T00:05:00Z"
        );
    }

    #[test]
    fn a_running_cycle_of_no_known_start_clears_the_failure() {
        let h = home("nostart");
        let last = r#""last":{"issue":12,"status":"aborted","startedAt":100,"updatedAt":500}"#;
        for (cycle, failed) in [
            (r#"{"issue":14,"running":true,"startedAt":200}"#, true),
            (r#"{"issue":14,"running":true,"startedAt":600}"#, false),
            (r#"{"issue":14,"running":true,"updatedAt":700}"#, false),
            (r#"{"issue":14,"running":true,"startedAt":null}"#, false),
        ] {
            h.write(
                "live/running.json",
                &format!(r#"{{"cycles":[{cycle}],{last}}}"#),
            );
            let r = h.report(&Given::default());
            assert_eq!(r.get("status").text() == "fail", failed, "{cycle}");
        }
    }

    #[test]
    fn an_old_queue_snapshot_warns() {
        let h = home("old");
        h.write(
            "live/status.json",
            r#"{"generatedAt":"2026-10-17T13:40:20Z","allHands":true}"#,
        );
        let r = h.report(&Given::default());
        assert_eq!(
            said(&r),
            (
                "warn".into(),
                "The queue snapshot is 20 min old: GitHub did not answer the last polls.".into()
            )
        );
        assert_eq!(r.at(&["checks", "allHands"]), &J::Null);
    }

    #[test]
    fn a_pause_flag_or_label_warns() {
        let h = home("pause");
        h.write(
            "state/pause.json",
            r#"{"by":"someone","at":"2026-10-17T13:00:00Z","reason":"weekend"}"#,
        );
        let r = h.report(&Given::default());
        assert_eq!(said(&r), ("warn".into(), "The factory is paused.".into()));
        assert_eq!(
            r.at(&["checks", "pause"]).dump(),
            r#"{"flag":{"at":"2026-10-17T13:00:00Z","reason":"weekend"},"label":false,"lifted":false}"#
        );
        h.write("state/pause.json", "not a flag");
        let r = h.report(&Given::default());
        assert_eq!(
            r.at(&["checks", "pause", "flag"]).dump(),
            r#"{"at":null,"reason":null}"#
        );
        fs::remove_file(h.0.join("state/pause.json")).unwrap();
        h.write(
            "live/status.json",
            r#"{"generatedAt":"2026-10-17T14:00:10Z","pause":{"label":false,"lifted":true}}"#,
        );
        assert_eq!(h.report(&Given::default()).get("status").text(), "warn");
    }

    #[test]
    fn an_expired_limit_warns() {
        let h = home("limit");
        h.write(
            "state/limit.json",
            r#"{"v":1,"cycles":2,"plan":true,"until":"2026-10-17T13:00:00Z"}"#,
        );
        let r = h.report(&Given::default());
        assert_eq!(
            said(&r),
            ("warn".into(), "Nothing starts: the limit expired at 2026-10-17T13:00:00Z. Set it again or clear it (tehas limit).".into())
        );
    }

    #[test]
    fn a_slice_that_holds_every_start_warns() {
        let h = home("slice");
        h.write(
            "live/dispatch.json",
            r#"{"time":"2026-10-17T14:00:00Z","why":"resources","hold":"holding: no resources: memory","room":0,"free":0}"#,
        );
        let r = h.report(&Given::default());
        assert_eq!(said(&r), ("warn".into(), "No resources: memory.".into()));
        assert_eq!(
            r.at(&["checks", "slice"]).dump(),
            r#"{"time":"2026-10-17T14:00:00Z","why":"resources","hold":"holding: no resources: memory","count":null,"room":0,"committed":null,"max":null,"reserve":null,"free":0}"#
        );
        h.write(
            "live/dispatch.json",
            r#"{"time":"2026-10-17T14:00:00Z","why":"count","hold":"holding: two running"}"#,
        );
        assert_eq!(said(&h.report(&Given::default())).0, "pass");
        h.write(
            "live/dispatch.json",
            r#"{"time":"2026-10-17T13:00:00Z","why":"disk"}"#,
        );
        assert_eq!(
            h.report(&Given::default()).at(&["checks", "slice"]),
            &J::Null
        );
    }

    #[test]
    fn the_slots_a_poll_derived_are_kept_and_a_number_writes_none() {
        let h = home("slots");
        h.write(
            "live/dispatch.json",
            r#"{"time":"2026-10-17T14:00:00Z","why":"","room":3,"free":2,
                 "slots":{"cycles":6,"containers":6,"cyclesHow":"auto","containersHow":"auto",
                          "cycle":{"bound":"cpu","slots":6,"cpus":24,"cycleCpus":"4","containerMemory":"8g","ceiling":"16","floored":false}}}"#,
        );
        assert_eq!(
            h.report(&Given::default())
                .at(&["checks", "slice", "slots", "cycle", "bound"])
                .as_str(),
            Some("cpu")
        );
        // A poll that sets numbers writes no slots object, and the check keeps
        // the fields it always had: no key at all, not one that is null.
        h.write(
            "live/dispatch.json",
            r#"{"time":"2026-10-17T14:00:00Z","why":"","room":1,"free":1}"#,
        );
        let r = h.report(&Given::default());
        assert!(r.at(&["checks", "slice"]).get("slots").is_null());
        assert_eq!(
            r.at(&["checks", "slice"]).dump(),
            r#"{"time":"2026-10-17T14:00:00Z","why":"","hold":null,"count":null,"room":1,"committed":null,"max":null,"reserve":null,"free":1}"#
        );
    }

    #[test]
    fn failed_units_low_disk_and_a_platform_behind_warn() {
        let h = home("units");
        let g = Given {
            units: Some(
                "tehas-dispatch.path loaded failed failed Dispatch\nother.service x\n".into(),
            ),
            ..Given::default()
        };
        assert_eq!(
            said(&h.report(&g)).1,
            "systemd gave up on tehas-dispatch.path: run systemctl --user reset-failed and start it again."
        );
        assert_eq!(
            h.report(&g).at(&["checks", "units"]).dump(),
            r#"["tehas-dispatch.path"]"#
        );
        // An empty text is a read that found nothing: an empty list, no warning.
        let g = Given {
            units: Some(String::new()),
            ..Given::default()
        };
        assert_eq!(h.report(&g).at(&["checks", "units"]).dump(), "[]");
        h.write("live/previews.json", r#"{"generatedAt":"2026-10-17T14:00:00Z","disk":{"freeBytes":1,"minFreeBytes":2,"low":true}}"#);
        assert_eq!(
            said(&h.report(&Given::default())).1,
            "The host is low on disk: previews are not being built."
        );
        fs::remove_file(h.0.join("live/previews.json")).unwrap();
        h.write(
            "live/platform.json",
            r#"{"commit":"abc","behind":3,"upstream":"origin/main","behindSince":"2026-10-15T00:00:00Z"}"#,
        );
        assert_eq!(
            said(&h.report(&Given::default())).1,
            "The platform is 3 commits behind origin/main since 2026-10-15: run tehas update."
        );
    }

    #[test]
    fn units_systemd_did_not_answer_is_no_check_and_no_warning() {
        let h = home("no-units");
        // No `--units` at all: the read failed, so the file says nothing of the
        // units and an otherwise healthy home still passes.
        let r = h.report(&Given::default());
        assert_eq!(r.at(&["checks", "units"]), &J::Null);
        assert_eq!(said(&r), ("pass".into(), "Idle, taking work.".into()));
    }

    #[test]
    fn an_old_or_missing_backup_and_a_poll_over_budget_warn() {
        let h = home("backup");
        let g = Given {
            backup: Some(r#"{"newest":null}"#.into()),
            ..Given::default()
        };
        assert_eq!(
            said(&h.report(&g)).1,
            "There is no backup of the state store: tehasbot clean makes one a day."
        );
        let g = Given {
            backup: Some(r#"{"newest":{"at":1792000000,"age_seconds":200000}}"#.into()),
            ..Given::default()
        };
        assert_eq!(
            said(&h.report(&g)).1,
            "The newest backup of the state store is 55 h old."
        );
        let g = Given {
            poll_over: Some(r#"["cpu","wall"]"#.into()),
            poll_budget: Some("cpu=5".into()),
            ..Given::default()
        };
        assert_eq!(
            said(&h.report(&g)).1,
            "An idle poll is over its budget on cpu, wall (TEHAS_POLL_BUDGET=cpu=5)."
        );
        // A store that did not answer is no check, never a passed one.
        assert_eq!(
            h.report(&Given::default()).at(&["checks", "poll_cost"]),
            &J::Null
        );
    }

    #[test]
    fn a_planning_turn_counts_while_its_lock_is_held() {
        let h = home("plans");
        h.write("state/plan-7.json.lock", "");
        let g = Given {
            sessions: Some(
                r#"{"sessions":[{"issue":7,"running":true,"turns":1,"started_at":1792245000}]}"#
                    .into(),
            ),
            ..Given::default()
        };
        assert_eq!(h.report(&g).at(&["checks", "plans"]).len(), 0);
        let lock = File::open(h.0.join("state/plan-7.json.lock")).unwrap();
        lock.lock().unwrap();
        assert_eq!(
            h.report(&g).at(&["checks", "plans"]).dump(),
            r#"[{"issue":7,"turn":2,"startedAt":"2026-10-17T13:50:00Z"}]"#
        );
        h.write("state/plan-7.json.rc", "0");
        assert_eq!(h.report(&g).at(&["checks", "plans"]).len(), 0);
    }

    #[test]
    fn the_failed_list_has_each_cycle_once_newest_first() {
        let h = home("endings");
        h.write(
            "live/endings.json",
            r#"[{"issue":1,"status":"aborted","startedAt":10,"updatedAt":20},{"issue":2,"status":"merged","startedAt":5,"updatedAt":30},
               {"issue":1,"status":"aborted","startedAt":10,"updatedAt":20},{"issue":3,"status":"failed","startedAt":40,"updatedAt":50}, "x"]"#,
        );
        let issues: Vec<String> = h
            .report(&Given::default())
            .at(&["checks", "failed"])
            .items()
            .iter()
            .map(|e| e.get("issue").text())
            .collect();
        assert_eq!(issues, ["3", "1"]);
    }
}

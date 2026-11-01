//! `tehasbot status`: write the dashboard's data file, live/status.json.
//!
//! One snapshot of the queue as GitHub sees it (which issues are in line, being
//! worked on, waiting for a human, stopped, completed), whether the last poll
//! found no resources (live/dispatch.json), and the words of the breakroom game.
//! Deterministic, no model, five GitHub calls. The dispatcher runs it at every
//! poll, and a cycle when it ends.
//!
//! The words are counted over the titles and labels of every issue of the
//! repository, the newest thousand, whether the factory was ever given it or
//! not. Only the hundred most used words and their counts are written: no
//! title and no issue number. Without an answer to that call the snapshot is
//! written with no words, and the page counts the tickets it does list.
//!
//! The run also writes live/platform.json (src/platform.rs), live/epics.json
//! (src/epics.rs) and what the analytics program publishes (src/publish.rs),
//! and it refreshes the health file unless the poll ran it: the poll sets
//! TEHAS_FROM_POLL and runs health a moment later. Those last two happen at
//! every run, early ends included. TERM or INT is an early end: the read in hand
//! finishes, or dies with the group the signal went to, and nothing new starts.
//!
//! The snapshot says whether the factory is paused, and which of the two
//! switches holds it: `paused: true` when either is set, and `pause: {flag,
//! label, lifted}`, where `flag` is {at, reason} written by tehas pause or null,
//! `label` is tehas:pause on the control issue, and `lifted` says the label is
//! off but the account that took it off may not authorise work, which holds this
//! host paused just as much. `allHands` is true while tehas:all-hands on the
//! control issue was applied by an allowed account, null when that could not be
//! read. The flag is read from this host, so pausing needs no answer from
//! GitHub. `by`, the account that set the flag, stays out of the file: the page
//! that reads it can be public.
//!
//! `planned` lists the open issues with tehas:plan or tehas:planning; `planning`
//! counts them.
//!
//! `completedCount` is how many closed issues carry tehas:shipped, counted by a
//! search of its own, so it keeps rising past the hundred items the `completed`
//! list holds. It is the whole number the search answered, or no key at all when
//! the search gave no such number, in which case the page counts the list. The
//! one statement of the bound is `count` below; host/docs/tools.md names the
//! same contract for a reader outside this repository.
//!
//! Unless TEHAS_ANALYTICS=off, it records the issue events awaiting, stopped,
//! merged and closed in the factory's analytics by comparing the snapshot with
//! the one it replaces (`events` below).
//!
//! The control issue that carries `tehas:pause` is not work, so it is left out of
//! every count and every list: no label makes it count in line, being worked on,
//! waiting for a human or waiting to be merged.
//!
//! Prints nothing but an ignored all-hands label, once, analytics warnings and a
//! WARN line when the snapshot cannot be written, on standard error. Exit 0 even
//! when GitHub is down: the previous file is then left in place.

use crate::flags::{epoch, oneline};
use crate::jq::{obj, J};
use crate::platform;
use crate::publish::{log, Analytics};
use crate::shipped;
use serde_json::Value;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tehas_core::call;
use tehas_core::events::{label_actor, pause_holds, Event, Events};
use tehas_core::time;

/// A read that has a bound: it hangs this long, then it failed.
const SEARCH_WAIT: Duration = Duration::from_secs(60);
const SEARCH_GRACE: Duration = Duration::from_secs(1);
/// A poll's dispatch.json is believed this long, in seconds.
const POLL_KEEP: i64 = 600;
/// A snapshot older than this is no snapshot to compare with: a change it
/// missed would be dated now.
const BEFORE_KEEP: i64 = 3600;
/// The report's window, in seconds: nothing older is an event.
const EVENT_WINDOW: i64 = 864_000;
const LABELS: &str = "label:\"tehas:ready\",\"tehas:running\",\"tehas:awaiting-merge\",\"needs-human-attention\",\"tehas:plan\",\"tehas:planning\" sort:updated-desc";
const COUNT_QUERY: &str =
    "\n  query($q: String!) { search(type: ISSUE, query: $q, first: 0) { issueCount } }";

fn words(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| a.to_string()).collect()
}

/// What gh printed, None when it failed. The poll bounds these calls.
fn gh(args: &[&str]) -> Option<String> {
    let out = Command::new("gh")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// What gh printed within the bound of a search, None when it failed or hung.
fn search(args: &[&str]) -> Option<String> {
    let ran = call::run_full("gh", &words(args), None, SEARCH_WAIT, SEARCH_GRACE, &[]);
    ran.ok().then_some(ran.out)
}

fn allowed(login: &str) -> bool {
    let list = env::var("TEHAS_ALLOWED_LABELERS").unwrap_or_default();
    !login.is_empty() && list.split(' ').any(|l| l == login)
}

/// `.[]`: the items of a list or the values of an object, None for anything
/// else, which jq calls an error.
fn each(v: &J) -> Option<Vec<&J>> {
    match v {
        J::Arr(a) => Some(a.iter().collect()),
        J::Obj(o) => Some(o.iter().map(|(_, v)| v).collect()),
        _ => None,
    }
}

/// The events of one issue as `gh api --paginate` prints them, one list a
/// page. None when they are not that, which is never the same as no event.
fn events_of(text: &str) -> Option<Vec<Event>> {
    let pages = serde_json::Deserializer::from_str(text)
        .into_iter::<Value>()
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    let mut all = Vec::new();
    for page in pages {
        let Value::Array(items) = page else {
            return None;
        };
        all.extend(items);
    }
    if all.iter().any(Value::is_array) {
        return None;
    }
    match Events::parse(&Value::Array(all)) {
        Events::Read(events) => Some(events),
        _ => None,
    }
}

/// The two switches of the control issue.
#[derive(Debug, PartialEq)]
struct Control {
    label: bool,
    lifted: bool,
    all_hands: Option<bool>,
}

/// `labels` is what the read of its labels printed, joined by spaces, None
/// when it failed. `read_events` asks for its events, the empty text for a
/// call that failed, and it is asked at most once.
///
/// The label of a pause works one way, so its absence is not always a resume:
/// this reports the verdict the poll acts on (rule 5), so that a factory held by
/// a pause a stranger lifted is not published as taking work. An answer that
/// cannot be read reports nothing extra: the poll, which acts on it, ends when
/// it cannot read the events.
///
/// tehas:all-hands holds while it is on the issue and the account that applied
/// it is allowed. It is reported and changes nothing in the factory. A label
/// anyone else applied is said once, on standard error; `ignored` remembers
/// which application was said. None is a label that could not be read.
fn control(
    labels: Option<&str>,
    read_events: &mut dyn FnMut() -> String,
    allowed: &dyn Fn(&str) -> bool,
    ignored: &Path,
    issue: &str,
) -> Control {
    let joined = format!(" {} ", labels.unwrap_or_default().trim_end_matches('\n'));
    let label = joined.contains(" tehas:pause ");
    let mut read: Option<String> = None;
    let mut lifted = false;
    if !label {
        let text = read.get_or_insert_with(&mut *read_events);
        lifted =
            events_of(text).is_some_and(|e| !text.is_empty() && pause_holds(&e, allowed).is_some());
    }
    let all_hands = if labels.is_none() {
        None
    } else if joined.contains(" tehas:all-hands ") {
        let text = read.get_or_insert_with(&mut *read_events);
        let applied = (!text.trim_end_matches('\n').is_empty())
            .then(|| events_of(text))
            .flatten()
            .map(|e| label_actor(&e, "tehas:all-hands"));
        match applied {
            None => None,
            Some((who, _)) if allowed(&who) => {
                let _ = fs::remove_file(ignored);
                Some(true)
            }
            Some((who, at)) => {
                let line = format!("{who}\t{at}");
                let said = fs::read_to_string(ignored).unwrap_or_default();
                if !ignored.exists() || said.trim_end_matches('\n') != line {
                    eprintln!("tehasbot status: tehas:all-hands on #{issue} was applied by '{who}', not an allowed account: ignored");
                    if let Some(dir) = ignored.parent() {
                        let _ = fs::create_dir_all(dir);
                    }
                    let _ = fs::write(ignored, format!("{line}\n"));
                }
                Some(false)
            }
        }
    } else {
        let _ = fs::remove_file(ignored);
        Some(false)
    };
    Control {
        label,
        lifted,
        all_hands,
    }
}

/// The host's own switch, read without asking GitHub anything: the flag and
/// whether it holds the factory. A flag that cannot be read counts as paused
/// and has nothing to say for itself beyond that.
fn pause_flag(file: &Path) -> (J, bool) {
    if !file.exists() {
        return (J::Null, false);
    }
    let unreadable = || obj(vec![("at", J::Null), ("reason", J::Null)]);
    let text = fs::read(file).ok().and_then(|b| String::from_utf8(b).ok());
    let flag = text.as_deref().and_then(J::parse);
    match flag.as_ref().map(|f| (f.get("at"), f.get("reason"))) {
        Some((J::Str(at), J::Str(reason))) if matches!(flag, Some(J::Obj(_))) => (
            obj(vec![
                ("at", J::str(at.clone())),
                ("reason", J::str(oneline(reason))),
            ]),
            true,
        ),
        _ => (unreadable(), true),
    }
}

/// The model endpoint as the last poll found it, for the pages. A file older
/// than ten minutes is from a dispatcher that no longer gets that far.
fn no_resources(file: &Path, now: i64) -> bool {
    let doc = fs::read_to_string(file).ok().and_then(|t| J::parse(&t));
    doc.is_some_and(|d| {
        d.get("why").as_str() == Some("resources")
            && d.get("time")
                .as_str()
                .and_then(epoch)
                .is_some_and(|t| now - t < POLL_KEEP)
    })
}

/// The snapshot this one replaces, for the events: null when it is not one,
/// or too old.
fn before(file: &Path, now: i64) -> J {
    let doc = fs::read_to_string(file).ok().and_then(|t| J::parse(&t));
    match doc {
        Some(d @ J::Obj(_)) => {
            let at = d.get("generatedAt").as_str().and_then(epoch).unwrap_or(0);
            if now - at < BEFORE_KEEP {
                d
            } else {
                J::Null
            }
        }
        _ => J::Null,
    }
}

/// The figure of the Shipped stage, published only as a whole number of
/// issues: nothing negative, fractional or beyond what a JavaScript number
/// holds exactly, 2^53 - 1 the largest. The reader, host/fun/business-view.js,
/// shows what this publishes and rejects the rest, so the bound is stated once.
fn count(answer: &str) -> Option<J> {
    let doc = J::parse(answer)?;
    let n = doc.at(&["data", "search", "issueCount"]).as_f64()?;
    (n >= 0.0 && n == n.floor() && n < 9_007_199_254_740_992.0).then(|| J::num(n))
}

/// `@uri`
fn uri(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The hundred most used words of the titles and labels of every issue,
/// whole words in lower case, a hyphen kept, three letters the fewest.
fn buzzwords(every: &J) -> J {
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for issue in every.items() {
        if !matches!(issue, J::Obj(_)) {
            continue;
        }
        let mut parts: Vec<&str> = issue.get("title").as_str().into_iter().collect();
        parts.extend(
            each(issue.get("labels"))
                .unwrap_or_default()
                .into_iter()
                .filter_map(|l| l.get("name").as_str()),
        );
        scan(&parts.join(" ").to_ascii_lowercase(), &mut |word| {
            *counts.entry(word.to_string()).or_default() += 1;
        });
    }
    let mut list: Vec<(String, u64)> = counts.into_iter().collect();
    list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    J::Arr(
        list.into_iter()
            .take(100)
            .map(|(word, n)| obj(vec![("word", J::str(word)), ("count", J::num(n as f64))]))
            .collect(),
    )
}

/// `scan("[a-z][a-z0-9-]{2,}")`
fn scan(text: &str, found: &mut dyn FnMut(&str)) {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if !b[i].is_ascii_lowercase() {
            i += 1;
            continue;
        }
        let mut end = i + 1;
        while end < b.len()
            && (b[end].is_ascii_lowercase() || b[end].is_ascii_digit() || b[end] == b'-')
        {
            end += 1;
        }
        if end - i >= 3 {
            found(&text[i..end]);
            i = end;
        } else {
            i += 1;
        }
    }
}

/// `{number, title, url, createdAt, updatedAt, closedAt}`
fn slim(issue: &J) -> J {
    let field = |k| issue.get(k).clone();
    obj(vec![
        ("number", field("number")),
        ("title", field("title")),
        ("url", field("url")),
        ("createdAt", field("createdAt")),
        ("updatedAt", field("updatedAt")),
        ("closedAt", field("closedAt")),
    ])
}

/// `sort_by(.key)`, which keeps the order of equal keys.
fn sorted(mut list: Vec<J>, key: &str) -> Vec<J> {
    list.sort_by(|a, b| a.get(key).cmp(b.get(key)));
    list
}

/// An open issue's label names. None when `.labels` is no list of them.
fn names(issue: &J) -> Option<Vec<&str>> {
    each(issue.get("labels"))?
        .into_iter()
        .map(|l| match l {
            J::Obj(_) => Some(l.get("name").as_str().unwrap_or_default()),
            J::Null => Some(""),
            _ => None,
        })
        .collect()
}

/// What the snapshot is made of.
struct Facts<'a> {
    repo: &'a str,
    control: &'a str,
    now: String,
    issues: &'a J,
    every: &'a J,
    shipped: Vec<J>,
    count: Option<J>,
    query: String,
    switches: Control,
    flag: J,
    flag_paused: bool,
    no_resources: bool,
}

/// The file. The control issue holds the pause label and no outcome: it is never
/// counted, whatever label someone gives it, and the dispatcher starts nothing on
/// the same number. Err when the list of issues is not one.
fn snapshot(f: &Facts) -> Result<J, String> {
    let no_list = || "the list of issues is no list of issues".to_string();
    let mut open: Vec<(&J, Vec<&str>)> = Vec::new();
    for issue in each(f.issues).ok_or_else(no_list)? {
        if !matches!(issue, J::Obj(_) | J::Null) {
            return Err(no_list());
        }
        if issue.get("state").as_str() == Some("OPEN") && issue.get("number").text() != f.control {
            open.push((issue, names(issue).ok_or_else(no_list)?));
        }
    }
    let with = |keep: &dyn Fn(&[&str]) -> bool| -> Vec<J> {
        open.iter()
            .filter(|(_, held)| keep(held))
            .map(|(issue, _)| slim(issue))
            .collect()
    };
    let list = |query: &str| {
        J::str(format!(
            "https://github.com/{}/issues?q={}",
            f.repo,
            uri(query)
        ))
    };
    let planned = sorted(
        with(&|h| h.contains(&"tehas:plan") || h.contains(&"tehas:planning")),
        "number",
    );
    let mut completed = sorted(f.shipped.iter().map(slim).collect(), "closedAt");
    completed.reverse();
    let planning = J::num(planned.len() as f64);
    let mut fields = vec![
        ("generatedAt", J::str(f.now.as_str())),
        ("repo", J::str(f.repo)),
        (
            "paused",
            J::Bool(f.switches.label || f.flag_paused || f.switches.lifted),
        ),
        (
            "pause",
            obj(vec![
                ("flag", f.flag.clone()),
                ("label", J::Bool(f.switches.label)),
                ("lifted", J::Bool(f.switches.lifted)),
            ]),
        ),
        ("allHands", f.switches.all_hands.map_or(J::Null, J::Bool)),
        ("noResources", J::Bool(f.no_resources)),
        ("running", J::Arr(with(&|h| h.contains(&"tehas:running")))),
        (
            "queued",
            J::Arr(sorted(
                with(&|h| h.contains(&"tehas:ready") && !h.contains(&"tehas:running")),
                "number",
            )),
        ),
        (
            "awaiting",
            J::Arr(with(&|h| h.contains(&"tehas:awaiting-merge"))),
        ),
        (
            "stopped",
            J::Arr(with(&|h| {
                h.contains(&"needs-human-attention") && !h.contains(&"tehas:running")
            })),
        ),
        ("planned", J::Arr(planned)),
        ("planning", planning),
        ("completed", J::Arr(completed)),
    ];
    if let Some(n) = &f.count {
        fields.push(("completedCount", n.clone()));
    }
    fields.push((
        "links",
        obj(vec![
            ("queued", list("is:issue is:open label:tehas:ready")),
            (
                "awaiting",
                list("is:issue is:open label:tehas:awaiting-merge"),
            ),
            (
                "stopped",
                list("is:issue is:open label:needs-human-attention"),
            ),
            ("completed", list(&f.query)),
            (
                "all",
                J::str(format!("https://github.com/{}/issues", f.repo)),
            ),
        ]),
    ));
    if !f.every.is_null() {
        fields.push(("buzzwords", buzzwords(f.every)));
    }
    Ok(obj(fields))
}

/// One event of the factory's analytics.
#[derive(Debug, PartialEq)]
struct Change {
    issue: String,
    event: &'static str,
    at: Option<i64>,
}

/// `.[$k]`'s numbers, of a snapshot.
fn numbers<'a>(snapshot: &'a J, key: &str) -> Vec<&'a J> {
    match snapshot.get(key) {
        J::Arr(list) => list
            .iter()
            .filter(|i| matches!(i, J::Obj(_)))
            .map(|i| i.get("number"))
            .filter(|n| matches!(n, J::Num(_)))
            .collect(),
        _ => Vec::new(),
    }
}

fn holds(list: &[&J], n: &J) -> bool {
    list.iter().any(|m| m.cmp(n).is_eq())
}

/// The issue events, from what changed since the snapshot before: newly
/// awaiting a merge or stopped, now; newly completed, merged at the time it was
/// closed; closed and not completed, having been in line, running or stopped
/// before, closed at that time. One that was awaiting a merge is not closed: the
/// search for completed work lags, and its merged comes when it catches up. A run
/// with no snapshot before it records only the merged and closed, which carry
/// their own time. Nothing older than ten days, the report's window.
fn events(now_snapshot: &J, was: &J, issues: &J, issue_control: &str, now: i64) -> Vec<Change> {
    let recent = |issue: &J| {
        issue
            .get("closedAt")
            .as_str()
            .and_then(epoch)
            .filter(|t| *t >= now - EVENT_WINDOW)
    };
    let mut out = Vec::new();
    if !was.is_null() {
        for (key, event) in [("awaiting", "awaiting"), ("stopped", "stopped")] {
            let old = numbers(was, key);
            for n in numbers(now_snapshot, key) {
                if !holds(&old, n) {
                    out.push(Change {
                        issue: n.text(),
                        event,
                        at: None,
                    });
                }
            }
        }
    }
    let old_done = numbers(was, "completed");
    for issue in now_snapshot.get("completed").items() {
        let new = was.is_null() || !holds(&old_done, issue.get("number"));
        if let (true, Some(at)) = (new, recent(issue)) {
            out.push(Change {
                issue: issue.get("number").text(),
                event: "merged",
                at: Some(at),
            });
        }
    }
    let done = numbers(now_snapshot, "completed");
    let awaiting = numbers(was, "awaiting");
    let mut in_work = numbers(was, "queued");
    in_work.extend(numbers(was, "running"));
    in_work.extend(numbers(was, "stopped"));
    in_work.retain(|n| !holds(&awaiting, n));
    for issue in each(issues).unwrap_or_default() {
        let n = issue.get("number");
        if issue.get("state").as_str() == Some("CLOSED")
            && n.text() != issue_control
            && !holds(&done, n)
            && (was.is_null() || holds(&in_work, n))
        {
            if let Some(at) = recent(issue) {
                out.push(Change {
                    issue: n.text(),
                    event: "closed",
                    at: Some(at),
                });
            }
        }
    }
    out
}

/// No call starts after three calls' time limits, so a database that hangs
/// delays the run by four at most. An event whose call fails or runs out of time
/// is lost: the next run compares with this snapshot and does not try it again.
fn record(analytics: &Analytics, changes: &[Change]) {
    let stop = Instant::now() + analytics.wait() * 3;
    for c in changes {
        if Instant::now() >= stop {
            log("WARN analytics: out of time, the other events of this run are left out");
            break;
        }
        analytics.event(&c.issue, c.event, c.at);
    }
}

fn put(live: &Path, file: &J) -> Result<(), String> {
    let new = live.join(format!("status.json.{}.new", std::process::id()));
    let written = fs::write(&new, format!("{}\n", file.pretty()))
        .and_then(|()| fs::rename(&new, live.join("status.json")));
    if written.is_err() {
        let _ = fs::remove_file(&new);
    }
    written.map_err(|e| e.to_string())
}

/// This program under a time limit, for a call whose reads have none of their
/// own: live/epics.json is left as it was.
fn epics(repo: &str, live: &Path) {
    let Ok(me) = env::current_exe() else {
        return;
    };
    let _ = Command::new("timeout")
        .args(["-k", "1", &SEARCH_WAIT.as_secs().to_string()])
        .arg(me)
        .args(["epics", repo])
        .arg(live.join("epics.json"))
        .stdin(Stdio::null())
        .status();
}

fn health() {
    let Ok(me) = env::current_exe() else {
        return;
    };
    let _ = Command::new(me)
        .args(["tehasbot", "health"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn queue(home: &Path, live: &Path, analytics: &Analytics, stop: &AtomicBool) {
    let stopped = || stop.load(Ordering::Relaxed);
    let repo = env::var("TEHAS_REPO").unwrap_or_default();
    let repo = repo.as_str();
    let issue = env::var("TEHAS_CONTROL_ISSUE").unwrap_or_default();
    let issue = issue.as_str();
    let controls = [issue.to_string()];
    platform::write(home, live);

    let Some(issues) = gh(&[
        "issue",
        "list",
        "--repo",
        repo,
        "--state",
        "all",
        "--limit",
        "200",
        "--search",
        LABELS,
        "--json",
        "number,title,url,state,labels,createdAt,updatedAt,closedAt",
    ]) else {
        log("WARN status: the list of issues could not be read, no snapshot was written");
        return;
    };
    if stopped() {
        return;
    }
    let every = gh(&[
        "issue",
        "list",
        "--repo",
        repo,
        "--state",
        "all",
        "--limit",
        "1000",
        "--json",
        "title,labels",
    ])
    .and_then(|text| J::parse(&text))
    .filter(|v| matches!(v, J::Arr(_)))
    .unwrap_or(J::Null);
    // The closing pull request is only in the issue's closed event, so this one
    // is GraphQL. The query and the acceptance rule are shipped.rs's, the same
    // ones tehas tidy reads, so the two commands cannot disagree on what a merge
    // closed.
    if stopped() {
        return;
    }
    let closed_query = format!("q=repo:{repo} is:issue is:closed linked:pr sort:updated-desc");
    let the_query = format!("query={}", shipped::QUERY);
    let Some(closers) = search(&["api", "graphql", "-f", &closed_query, "-f", &the_query]) else {
        log("WARN status: the read of what closed these issues could not be read, no snapshot was written");
        return;
    };
    // A read that named issues and found no merged closer is a whole answer and
    // a snapshot of no completed work; only a reply with no list of nodes in it
    // is no snapshot.
    let Some(nodes) = shipped::list(&closers, &controls) else {
        log("WARN status: the answer of what closed these issues was no list, no snapshot was written");
        return;
    };
    let nodes = nodes
        .iter()
        .filter_map(|n| J::parse(&n.to_string()))
        .collect();
    if stopped() {
        return;
    }
    let shipped_query = format!("is:issue is:closed label:{}", shipped::SHIPPED);
    let count_q = format!("q=repo:{repo} {shipped_query}");
    let count_query = format!("query={COUNT_QUERY}");
    let completed = search(&["api", "graphql", "-f", &count_q, "-f", &count_query])
        .and_then(|answer| count(&answer));

    if stopped() {
        return;
    }
    let labels = gh(&[
        "issue",
        "view",
        issue,
        "--repo",
        repo,
        "--json",
        "labels",
        "--jq",
        "[.labels[].name] | join(\" \")",
    ]);
    let events_path = format!("repos/{repo}/issues/{issue}/events?per_page=100");
    let switches = control(
        labels.as_deref(),
        &mut || gh(&["api", &events_path, "--paginate"]).unwrap_or_default(),
        &allowed,
        &home.join("state/all-hands.ignored"),
        issue,
    );
    if stopped() {
        return;
    }
    let (flag, flag_paused) = pause_flag(&home.join("state/pause.json"));
    let now = time::now();
    let on = env::var("TEHAS_ANALYTICS").as_deref() == Ok("on");
    let was = if on {
        before(&live.join("status.json"), now)
    } else {
        J::Null
    };
    let issues = J::parse(&issues);
    let snapshot = issues
        .as_ref()
        .ok_or_else(|| "the list of issues is no JSON".to_string())
        .and_then(|issues| {
            snapshot(&Facts {
                repo,
                control: issue,
                now: time::iso(time::now()),
                issues,
                every: &every,
                shipped: nodes,
                count: completed,
                query: shipped_query,
                switches,
                flag,
                flag_paused,
                no_resources: no_resources(&live.join("dispatch.json"), now),
            })
        });
    let written = snapshot.and_then(|s| put(live, &s).map(|()| s));
    if let Err(why) = &written {
        log(&format!("WARN status.json not written: {why}"));
    }
    if stopped() {
        return;
    }
    epics(repo, live);
    if stopped() {
        return;
    }

    // An issue this read finds shipped carries the label whatever the reason,
    // and the read names its labels, so an issue that already holds it is not
    // asked for it: `gh issue edit` reads the issue and sends a mutation every
    // time. The labels the read did not answer count as missing.
    for n in shipped::lacking(&closers, &controls, shipped::SHIPPED) {
        let n = n.to_string();
        let _ = gh(&[
            "issue",
            "edit",
            &n,
            "--repo",
            repo,
            "--add-label",
            shipped::SHIPPED,
        ]);
    }

    if stopped() {
        return;
    }
    if let (Ok(s), true, Some(issues)) = (&written, on, &issues) {
        record(analytics, &events(s, &was, issues, issue, time::now()));
    }
}

/// `tehasbot status`. It takes no argument and reads none.
pub fn main() -> u8 {
    let from_poll = env::var_os("TEHAS_FROM_POLL").is_some_and(|v| !v.is_empty());
    let home = PathBuf::from(env::var_os("TEHAS_HOME").unwrap_or_default());
    let live = home.join("live");
    let _ = fs::create_dir_all(&live);
    let analytics = Analytics::new(&home);
    let stop = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        let _ = signal_hook::flag::register(signal, Arc::clone(&stop));
    }
    queue(&home, &live, &analytics, &stop);
    analytics.publish(&live);
    if !from_poll {
        health();
    }
    if stop.load(Ordering::Relaxed) {
        143
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const NOW: i64 = 1_790_000_000;

    fn j(text: &str) -> J {
        J::parse(text).unwrap()
    }

    fn temp(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("tehas-queue-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn facts<'a>(issues: &'a J, every: &'a J, shipped: Vec<J>) -> Facts<'a> {
        Facts {
            repo: "acme/widget",
            control: "1",
            now: "2026-11-01T00:00:00Z".to_string(),
            issues,
            every,
            shipped,
            count: None,
            query: "is:issue is:closed label:tehas:shipped".to_string(),
            switches: Control {
                label: false,
                lifted: false,
                all_hands: Some(false),
            },
            flag: J::Null,
            flag_paused: false,
            no_resources: false,
        }
    }

    #[test]
    fn the_words_are_whole_lower_case_and_three_letters_or_more() {
        let mut got = Vec::new();
        scan("a ab abc x1 x-1 side-project v2 1abc -abc- é", &mut |w| {
            got.push(w.to_string());
        });
        assert_eq!(got, ["abc", "x-1", "side-project", "abc", "abc-"]);
    }

    #[test]
    fn the_buzzwords_count_titles_and_labels_most_used_first() {
        let every = j(
            r#"[{"title":"Dashboard shows the Queue","labels":[{"name":"tehas:ready"}]},
          {"title":"queue, QUEUE; dashboard!","labels":[]},
          {"title":null},7,{"title":"zed","labels":{"a":{"name":"dashboard"}}}]"#,
        );
        assert_eq!(
            buzzwords(&every).dump(),
            r#"[{"word":"dashboard","count":3},{"word":"queue","count":3},{"word":"ready","count":1},{"word":"shows","count":1},{"word":"tehas","count":1},{"word":"the","count":1},{"word":"zed","count":1}]"#
        );
    }

    #[test]
    fn at_most_a_hundred_words_are_kept() {
        let list: Vec<String> = (0..150)
            .map(|n| format!(r#"{{"title":"word{n}"}}"#))
            .collect();
        let every = j(&format!("[{}]", list.join(",")));
        assert_eq!(buzzwords(&every).items().len(), 100);
    }

    #[test]
    fn the_count_is_a_whole_number_a_page_can_hold() {
        let of = |n: &str| count(&format!(r#"{{"data":{{"search":{{"issueCount":{n}}}}}}}"#));
        for good in ["0", "1", "138", "9007199254740991"] {
            assert_eq!(of(good).unwrap().dump(), good);
        }
        for bad in [
            "-138",
            "-0.5",
            "1.5",
            "1e400",
            "1E+999",
            "9007199254740992",
            "null",
            "\"138\"",
        ] {
            assert_eq!(of(bad), None, "{bad}");
        }
        for bad in ["", "not json", r#"{"errors":[]}"#, r#"{"data":{}}"#] {
            assert_eq!(count(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_query_goes_in_a_link_as_uri_writes_it() {
        assert_eq!(
            uri("is:issue label:tehas:ready"),
            "is%3Aissue%20label%3Atehas%3Aready"
        );
    }

    #[test]
    fn the_keys_of_the_snapshot_stay_in_their_order() {
        let issues = j("[]");
        let s = snapshot(&facts(&issues, &J::Null, Vec::new())).unwrap();
        let keys: Vec<&str> = match &s {
            J::Obj(o) => o.iter().map(|(k, _)| k.as_str()).collect(),
            _ => panic!(),
        };
        assert_eq!(
            keys,
            [
                "generatedAt",
                "repo",
                "paused",
                "pause",
                "allHands",
                "noResources",
                "running",
                "queued",
                "awaiting",
                "stopped",
                "planned",
                "planning",
                "completed",
                "links"
            ]
        );
        let every = j("[]");
        let mut f = facts(&issues, &every, Vec::new());
        f.count = Some(J::num(3.0));
        let s = snapshot(&f).unwrap();
        let J::Obj(o) = &s else { panic!() };
        assert_eq!(o[13].0, "completedCount");
        assert_eq!(o[14].0, "links");
        assert_eq!(o[15].0, "buzzwords");
        assert_eq!(
            s.at(&["links", "completed"]).as_str().unwrap(),
            "https://github.com/acme/widget/issues?q=is%3Aissue%20is%3Aclosed%20label%3Atehas%3Ashipped"
        );
    }

    #[test]
    fn the_lists_of_a_snapshot() {
        let issue = |n: u32, state: &str, label: &str| {
            format!(
                r#"{{"number":{n},"title":"t","url":"u","state":"{state}","labels":[{{"name":"{label}"}}]}}"#
            )
        };
        let issues = j(&format!(
            "[{},{},{},{},{},{},{}]",
            issue(4, "OPEN", "tehas:ready"),
            issue(3, "OPEN", "tehas:ready"),
            issue(1, "OPEN", "tehas:ready"),
            issue(5, "CLOSED", "tehas:ready"),
            issue(6, "OPEN", "tehas:plan"),
            r#"{"number":7,"state":"OPEN","labels":[{"name":"tehas:running"},{"name":"needs-human-attention"}]}"#,
            issue(8, "OPEN", "needs-human-attention"),
        ));
        let every = J::Null;
        let shipped = vec![
            j(r#"{"number":20,"closedAt":"2026-01-01T00:00:00Z","labels":[]}"#),
            j(r#"{"number":21,"closedAt":"2026-01-02T00:00:00Z"}"#),
            j(r#"{"number":22,"closedAt":"2026-01-02T00:00:00Z"}"#),
        ];
        let s = snapshot(&facts(&issues, &every, shipped)).unwrap();
        let nums = |k: &str| numbers(&s, k).iter().map(|n| n.dump()).collect::<Vec<_>>();
        assert_eq!(nums("queued"), ["3", "4"]);
        assert_eq!(nums("running"), ["7"]);
        assert_eq!(nums("stopped"), ["8"]);
        assert_eq!(nums("planned"), ["6"]);
        assert_eq!(s.get("planning").dump(), "1");
        // Sorted by the close and reversed: equal times come last named, first.
        assert_eq!(nums("completed"), ["22", "21", "20"]);
        assert_eq!(s.get("completed").items()[2].len(), 6);
    }

    #[test]
    fn a_list_that_is_not_one_is_no_snapshot() {
        for bad in [
            "7",
            "null",
            "\"x\"",
            "[7]",
            r#"[{"state":"OPEN"}]"#,
            r#"[{"state":"OPEN","labels":[7]}]"#,
        ] {
            let issues = j(bad);
            assert!(
                snapshot(&facts(&issues, &J::Null, Vec::new())).is_err(),
                "{bad}"
            );
        }
        for fine in ["[]", "{}", "[null]", r#"[{"state":"CLOSED"}]"#] {
            let issues = j(fine);
            assert!(
                snapshot(&facts(&issues, &J::Null, Vec::new())).is_ok(),
                "{fine}"
            );
        }
    }

    fn pause(event: &str, who: &str) -> String {
        format!(
            r#"[{{"event":"{event}","label":{{"name":"tehas:pause"}},"actor":{{"login":"{who}"}},"created_at":"2026-01-02T00:00:00Z"}}]"#
        )
    }

    fn hands(who: &str) -> String {
        format!(
            r#"[{{"event":"labeled","label":{{"name":"tehas:all-hands"}},"actor":{{"login":"{who}"}},"created_at":"2026-10-17T10:00:00Z"}}]"#
        )
    }

    fn switches(labels: Option<&str>, events: &str, ignored: &Path) -> (Control, u32) {
        let asked = Cell::new(0);
        let c = control(
            labels,
            &mut || {
                asked.set(asked.get() + 1);
                events.to_string()
            },
            &|l| l == "alice",
            ignored,
            "1",
        );
        (c, asked.get())
    }

    #[test]
    fn a_pause_only_an_allowed_account_took_off_is_no_pause() {
        let dir = temp("lifted");
        let f = dir.join("ignored");
        let lifted = |events: &str| switches(Some(""), events, &f).0.lifted;
        assert!(!lifted(&pause("unlabeled", "alice")));
        assert!(lifted(&pause("unlabeled", "mallory")));
        assert!(lifted(&pause("unlabeled", "")));
        assert!(lifted(&pause("labeled", "mallory")));
        assert!(!lifted("[]"));
        // Events that cannot be read report nothing extra.
        assert!(!lifted(""));
        assert!(!lifted("not json"));
        assert!(!lifted("{}"));
        assert!(!lifted(r#"{"message":"no"}"#));
        // A label on the issue needs no events.
        let (c, asked) = switches(Some("tehas:pause"), "", &f);
        assert!(c.label && !c.lifted);
        assert_eq!(asked, 0);
        // Two pages are one list, and of two events in one second the
        // application is read last.
        let pages = format!(
            "{}\n{}",
            pause("labeled", "alice"),
            pause("unlabeled", "alice")
        );
        assert!(lifted(&pages));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn all_hands_holds_only_by_an_allowed_account() {
        let dir = temp("hands");
        let f = dir.join("state/ignored");
        let on = Some("tehas:all-hands");
        assert_eq!(switches(on, &hands("alice"), &f).0.all_hands, Some(true));
        assert_eq!(switches(on, &hands("mallory"), &f).0.all_hands, Some(false));
        assert_eq!(
            fs::read_to_string(&f).unwrap(),
            "mallory\t2026-10-17T10:00:00Z\n"
        );
        // An allowed application takes the memory of the ignored one away.
        assert_eq!(switches(on, &hands("alice"), &f).0.all_hands, Some(true));
        assert!(!f.exists());
        switches(on, &hands("mallory"), &f);
        // Off the issue is off, whatever the events say, and the events are read
        // once for the two switches.
        let (c, asked) = switches(Some(""), &hands("alice"), &f);
        assert_eq!(c.all_hands, Some(false));
        assert_eq!(asked, 1);
        assert!(!f.exists());
        // Unknown: events that cannot be read, or labels that could not.
        assert_eq!(switches(on, "", &f).0.all_hands, None);
        assert_eq!(switches(on, "not json", &f).0.all_hands, None);
        assert_eq!(switches(None, &hands("alice"), &f).0.all_hands, None);
        // No event applied it: nobody did.
        assert_eq!(switches(on, "[]", &f).0.all_hands, Some(false));
        assert_eq!(fs::read_to_string(&f).unwrap(), "\t\n");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_flag_never_publishes_who_wrote_it_and_a_damaged_one_holds() {
        let dir = temp("flag");
        let f = dir.join("pause.json");
        assert_eq!(pause_flag(&f), (J::Null, false));
        fs::write(&f, r#"{"by":"boss","at":"T","reason":"a\nb"}"#).unwrap();
        let (flag, held) = pause_flag(&f);
        assert!(held);
        assert_eq!(flag.dump(), r#"{"at":"T","reason":"a b"}"#);
        let damaged = r#"{"at":null,"reason":null}"#;
        for bad in [
            "not a flag",
            "[]",
            r#"{"at":"T"}"#,
            r#"{"at":1,"reason":"r"}"#,
        ] {
            fs::write(&f, bad).unwrap();
            let (flag, held) = pause_flag(&f);
            assert!(held, "{bad}");
            assert_eq!(flag.dump(), damaged, "{bad}");
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn no_resources_is_said_from_a_fresh_poll_alone() {
        let dir = temp("resources");
        let f = dir.join("dispatch.json");
        assert!(!no_resources(&f, NOW));
        let file = |why: &str, age: i64| {
            let at = time::iso(NOW - age);
            format!(r#"{{"why":"{why}","time":"{at}"}}"#)
        };
        let says = |text: &str| {
            fs::write(&f, text).unwrap();
            no_resources(&f, NOW)
        };
        assert!(says(&file("resources", 599)));
        assert!(!says(&file("resources", 600)));
        assert!(!says(&file("room", 5)));
        for bad in [
            "",
            "[]",
            "not json",
            "{} {}",
            r#"{"why":"resources","time":7}"#,
        ] {
            assert!(!says(bad), "{bad}");
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_snapshot_before_is_an_object_of_the_last_hour() {
        let dir = temp("before");
        let f = dir.join("status.json");
        assert_eq!(before(&f, NOW), J::Null);
        let of = |age: i64| format!(r#"{{"generatedAt":"{}"}}"#, time::iso(NOW - age));
        fs::write(&f, of(3599)).unwrap();
        assert!(!before(&f, NOW).is_null());
        fs::write(&f, of(3600)).unwrap();
        assert!(before(&f, NOW).is_null());
        for bad in ["[]", "not json", r#"{"generatedAt":7}"#, "{}"] {
            fs::write(&f, bad).unwrap();
            assert!(before(&f, NOW).is_null(), "{bad}");
        }
        fs::remove_dir_all(dir).unwrap();
    }

    fn at(age: i64) -> String {
        time::iso(NOW - age)
    }

    fn change(issue: &str, event: &'static str, at: Option<i64>) -> Change {
        Change {
            issue: issue.to_string(),
            event,
            at,
        }
    }

    #[test]
    fn a_change_since_the_snapshot_before_is_an_event() {
        let was = j(
            r#"{"queued":[{"number":3}],"running":[{"number":6}],"awaiting":[{"number":5},{"number":7}],"stopped":[],"completed":[{"number":20}]}"#,
        );
        let now_snapshot = j(&format!(
            r#"{{"awaiting":[{{"number":5}},{{"number":8}}],"stopped":[{{"number":9}}],
              "completed":[{{"number":20,"closedAt":"{a}"}},{{"number":21,"closedAt":"{a}"}},{{"number":22,"closedAt":"{old}"}}]}}"#,
            a = at(60),
            old = at(EVENT_WINDOW + 1)
        ));
        let issues = j(&format!(
            r#"[{{"number":3,"state":"CLOSED","closedAt":"{a}"}},
              {{"number":6,"state":"OPEN"}},
              {{"number":7,"state":"CLOSED","closedAt":"{a}"}},
              {{"number":1,"state":"CLOSED","closedAt":"{a}"}},
              {{"number":21,"state":"CLOSED","closedAt":"{a}"}},
              {{"number":30,"state":"CLOSED","closedAt":"{a}"}}]"#,
            a = at(60)
        ));
        assert_eq!(
            events(&now_snapshot, &was, &issues, "1", NOW),
            [
                change("8", "awaiting", None),
                change("9", "stopped", None),
                change("21", "merged", Some(NOW - 60)),
                change("3", "closed", Some(NOW - 60)),
            ]
        );
    }

    #[test]
    fn with_no_snapshot_before_only_what_carries_its_own_time_is_an_event() {
        let now_snapshot = j(&format!(
            r#"{{"awaiting":[{{"number":5}}],"stopped":[{{"number":9}}],"completed":[{{"number":21,"closedAt":"{a}"}}]}}"#,
            a = at(60)
        ));
        let issues = j(&format!(
            r#"[{{"number":30,"state":"CLOSED","closedAt":"{a}"}},{{"number":31,"state":"CLOSED","closedAt":"{old}"}},{{"number":32,"state":"CLOSED","closedAt":"soon"}}]"#,
            a = at(60),
            old = at(EVENT_WINDOW + 1)
        ));
        assert_eq!(
            events(&now_snapshot, &J::Null, &issues, "1", NOW),
            [
                change("21", "merged", Some(NOW - 60)),
                change("30", "closed", Some(NOW - 60)),
            ]
        );
    }

    #[test]
    fn the_events_of_a_page_are_read_one_list_after_another() {
        let e = events_of("[{\"event\":\"labeled\"}]\n[{\"event\":\"unlabeled\"},null]").unwrap();
        assert_eq!(e.len(), 2);
        assert_eq!(events_of("").unwrap().len(), 0);
        assert!(events_of("[[]]").is_none());
        assert!(events_of("[7]").is_none());
        assert!(events_of("{}").is_none());
    }
}

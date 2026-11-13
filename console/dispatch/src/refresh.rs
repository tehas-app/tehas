//! tehasbot refresh: the factory's own pull requests that conflict with the
//! branch, each asked of the dispatcher in a file under state/refresh. What a
//! run does and its statuses are in src/usage/refresh.txt, this command's
//! --help.

use crate::decide::{hex40, iso_now, refresh_why};
use crate::read::github;
use crate::read::run::Calls;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn usage() -> &'static str {
    include_str!("usage/refresh.txt").trim_end_matches('\n')
}

/// Whether a run goes ahead, given what `last-run` holds. A stamp from the
/// future, as after the clock was set back, does not hold runs off for good.
fn due(stamp: &str, now: u64, every_minutes: u64) -> bool {
    let stamp = stamp.trim_end_matches('\n');
    if stamp.is_empty() || stamp.len() > 12 || !stamp.bytes().all(|b| b.is_ascii_digit()) {
        return true;
    }
    match stamp.parse::<u64>() {
        Ok(last) if last <= now => now - last >= every_minutes * 60,
        _ => true,
    }
}

#[derive(Debug, PartialEq)]
struct Conflict {
    issue: u64,
    pr: u64,
    head: String,
    base: String,
}

/// The conflicts of a run, the issues whose requests stay, and the counts the
/// run's line gives beside the conflicts: the pull requests GitHub still has
/// `UNKNOWN`, and the ones that are not ours to refresh.
#[derive(Debug, Default, PartialEq)]
struct Found {
    conflicts: Vec<Conflict>,
    unknown: usize,
    not_ours: usize,
    keep: BTreeSet<u64>,
}

/// UNKNOWN keeps a request and asks for none: GitHub computes the state
/// lazily and the next run reads it again. A pull request the rule rejects,
/// or whose head or base is no sha, is not ours to refresh. What is left of
/// the page is the bot's that GitHub says do not conflict.
fn found(nodes: &[Value], bot: &str, repo: &str, control: u64) -> Found {
    let mut f = Found::default();
    for node in nodes {
        if !refresh_why(node, bot, repo, control).is_empty() {
            f.not_ours += 1;
            continue;
        }
        let (head, base) = (node.get("headRefOid"), node.get("baseRefOid"));
        if !hex40(head) || !hex40(base) {
            f.not_ours += 1;
            continue;
        }
        let number = |v: Option<&Value>| v.and_then(Value::as_u64);
        let (Some(issue), Some(pr)) = (
            number(node.pointer("/closingIssuesReferences/nodes/0/number")),
            number(node.get("number")),
        ) else {
            f.not_ours += 1;
            continue;
        };
        match node.get("mergeable").and_then(Value::as_str) {
            Some("CONFLICTING") => {
                f.keep.insert(issue);
                f.conflicts.push(Conflict {
                    issue,
                    pr,
                    head: head.and_then(Value::as_str).unwrap_or("").to_string(),
                    base: base.and_then(Value::as_str).unwrap_or("").to_string(),
                });
            }
            Some("UNKNOWN") => {
                f.unknown += 1;
                f.keep.insert(issue);
            }
            _ => {}
        }
    }
    f
}

/// The time a request keeps: its own, when it is for the same pull request at
/// the same head.
fn kept_at(request: &str, pr: u64, head: &str) -> Option<String> {
    let v: Value = serde_json::from_str(request).ok()?;
    if v.get("pr").and_then(Value::as_u64) != Some(pr)
        || v.get("head_sha").and_then(Value::as_str) != Some(head)
    {
        return None;
    }
    let at = v.get("at")?.as_str()?;
    (!at.is_empty()).then(|| at.to_string())
}

/// A request, in the field order and layout the dispatcher's readers and a
/// person have always seen.
fn request(at: &str, c: &Conflict) -> String {
    format!(
        "{{\n  \"at\": {},\n  \"pr\": {},\n  \"head_sha\": {},\n  \"base_sha\": {}\n}}\n",
        json!(at),
        c.pr,
        json!(c.head),
        json!(c.base)
    )
}

/// The request `tehasbot refresh --pr` writes: the scan's, with `asked` last.
fn asked_request(at: &str, c: &Conflict) -> String {
    let scan = request(at, c);
    format!(
        "{},\n  \"asked\": \"pr\"\n}}\n",
        scan.trim_end_matches("\n}\n")
    )
}

/// What the scan's rule makes of one pull request it was asked for: the
/// request to write, or why there is none. UNKNOWN is written, as the
/// dispatcher waits on it.
fn asked(node: &Value, bot: &str, repo: &str, control: u64) -> Result<Conflict, String> {
    if node.get("state").and_then(Value::as_str) != Some("OPEN") {
        return Err("it is not open".into());
    }
    let why = refresh_why(node, bot, repo, control);
    if !why.is_empty() {
        return Err(why);
    }
    let f = found(std::slice::from_ref(node), bot, repo, control);
    if let Some(c) = f.conflicts.into_iter().next() {
        return Ok(c);
    }
    if f.not_ours > 0 {
        return Err("its head or base is no sha".into());
    }
    if f.unknown == 0 {
        let state = node.get("mergeable").and_then(Value::as_str);
        return Err(format!(
            "it does not conflict: GitHub says {}",
            state.unwrap_or("nothing")
        ));
    }
    let s = |p: &str| node.pointer(p).and_then(Value::as_str).unwrap_or("");
    let n = |p: &str| node.pointer(p).and_then(Value::as_u64).unwrap_or(0);
    Ok(Conflict {
        issue: n("/closingIssuesReferences/nodes/0/number"),
        pr: n("/number"),
        head: s("/headRefOid").into(),
        base: s("/baseRefOid").into(),
    })
}

/// The pull request of `--pr N` or `--pr N --json`, when that is all the
/// arguments say: N a whole number above 0, written without a leading 0.
pub fn asked_pr(args: &[&str]) -> Option<u64> {
    let (flag, n) = match args {
        [flag, n] | [flag, n, "--json"] => (flag, n),
        _ => return None,
    };
    let number = !n.is_empty() && !n.starts_with('0') && n.bytes().all(|b| b.is_ascii_digit());
    (*flag == "--pr" && number)
        .then(|| n.parse().ok())
        .flatten()
}

/// Write a file whole or not at all.
fn put(path: &Path, text: &str) -> std::io::Result<()> {
    let mut new = path.as_os_str().to_owned();
    new.push(".new");
    let new = PathBuf::from(new);
    let wrote = fs::write(&new, text).and_then(|()| fs::rename(&new, path));
    if wrote.is_err() {
        let _ = fs::remove_file(&new);
    }
    wrote
}

/// A request whose pull request no longer qualifies, or no longer conflicts.
/// Only `<issue>.json` is a request: any other file stays.
fn sweep(dir: &Path, keep: &BTreeSet<u64>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|e| e.path()) {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let Some(n) = name.strip_suffix(".json") else {
            continue;
        };
        let request = !n.starts_with('0') && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit());
        let kept = n.parse().is_ok_and(|n: u64| keep.contains(&n));
        if request && !kept && path.exists() {
            let _ = fs::remove_file(path);
        }
    }
}

/// What `--pr N --json` prints: whether a request was written, and why not
/// when it was not. The issue and head are null when GitHub did not say.
fn said(pr: u64, issue: Option<u64>, head: Option<&str>, why: Option<&str>) -> String {
    format!(
        "{{\"asked\":{},\"pr\":{pr},\"issue\":{},\"head_sha\":{},\"why\":{}}}",
        why.is_none(),
        json!(issue),
        json!(head),
        json!(why)
    )
}

/// The issue and head of a pull request GitHub answered for, as far as it says.
fn named(node: &Value) -> (Option<u64>, Option<&str>) {
    (
        node.pointer("/closingIssuesReferences/nodes/0/number")
            .and_then(Value::as_u64),
        node.get("headRefOid").and_then(Value::as_str),
    )
}

struct Run {
    dir: PathBuf,
    log: PathBuf,
    repo: String,
    control: u64,
    every: u64,
    bot: String,
}

impl Run {
    fn line(&self, text: &str) {
        if let Ok(mut f) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log)
        {
            let _ = writeln!(f, "{} {text}", iso_now());
        }
    }

    fn warn(&self, path: &Path) {
        self.line(&format!("WARN: could not write {}", path.display()));
    }

    fn fail(&self, why: &str) -> i32 {
        eprintln!("{} ERROR: {why}", iso_now());
        self.line(&format!("ERROR: {why}"));
        1
    }

    /// A run that fails has stamped its start, so a GitHub that does not
    /// answer is asked again only after the interval.
    fn go(&self, calls: &mut Calls, now: u64) -> i32 {
        let _ = fs::create_dir_all(&self.dir);
        if let Some(logs) = self.log.parent() {
            let _ = fs::create_dir_all(logs);
        }
        let stamp = self.dir.join("last-run");
        if !due(
            &fs::read_to_string(&stamp).unwrap_or_default(),
            now,
            self.every,
        ) {
            return 0;
        }
        if put(&stamp, &format!("{now}\n")).is_err() {
            self.warn(&stamp);
        }

        let bot = self.bot.clone();
        if bot.is_empty() {
            return self.fail("the bot's own login could not be read");
        }
        let repo = &self.repo;
        let Ok(answer) = github::pulls(calls, repo) else {
            return self.fail(&format!(
                "GitHub did not list the open pull requests of {repo}"
            ));
        };
        let Some(nodes) = answer
            .pointer("/data/repository/pullRequests/nodes")
            .and_then(Value::as_array)
        else {
            return self.fail(&format!(
                "the open pull requests of {repo} could not be read"
            ));
        };

        let f = found(nodes, &bot, repo, self.control);
        let mut wrote = 0;
        for c in &f.conflicts {
            let path = self.dir.join(format!("{}.json", c.issue));
            let at = fs::read_to_string(&path)
                .ok()
                .and_then(|old| kept_at(&old, c.pr, &c.head))
                .unwrap_or_else(iso_now);
            match put(&path, &request(&at, c)) {
                Ok(()) => wrote += 1,
                Err(_) => self.warn(&path),
            }
        }
        sweep(&self.dir, &f.keep);
        self.line(&format!(
            "read {} pull requests: {} conflict, {} unknown, {} not ours to refresh, wrote {wrote} requests",
            nodes.len(),
            f.conflicts.len(),
            f.unknown,
            f.not_ours
        ));
        0
    }
}

impl Run {
    /// `--pr N`: one pull request, now. No `last-run`, no sweep. With
    /// `--json` one object on standard output says how it went, for 0, 3 and 1.
    fn one(&self, calls: &mut Calls, pr: u64, json: bool) -> i32 {
        let print = |text: String| {
            if json {
                tehas_core::outln!("{text}");
            }
        };
        let _ = fs::create_dir_all(&self.dir);
        if let Some(logs) = self.log.parent() {
            let _ = fs::create_dir_all(logs);
        }
        if self.bot.is_empty() {
            let why = "the bot's own login could not be read";
            print(said(pr, None, None, Some(why)));
            return self.fail(why);
        }
        let node = github::pull(calls, &self.repo, pr);
        if node.is_null() || node.get("failed").is_some() {
            let why = format!(
                "GitHub did not answer for pull request #{pr} of {}",
                self.repo
            );
            print(said(pr, None, None, Some(&why)));
            return self.fail(&why);
        }
        let c = match asked(&node, &self.bot, &self.repo, self.control) {
            Ok(c) => c,
            Err(why) => {
                let (issue, head) = named(&node);
                print(said(pr, issue, head, Some(&why)));
                eprintln!("tehasbot refresh: pull request #{pr} is not refreshed: {why}");
                self.line(&format!(
                    "asked for pull request #{pr}: not refreshed: {why}"
                ));
                return 3;
            }
        };
        let path = self.dir.join(format!("{}.json", c.issue));
        let at = fs::read_to_string(&path)
            .ok()
            .and_then(|old| kept_at(&old, c.pr, &c.head))
            .unwrap_or_else(iso_now);
        if put(&path, &asked_request(&at, &c)).is_err() {
            let why = format!("could not write {}", path.display());
            print(said(pr, Some(c.issue), Some(&c.head), Some(&why)));
            return self.fail(&why);
        }
        self.line(&format!(
            "asked for pull request #{pr}: wrote the request of #{}",
            c.issue
        ));
        print(said(pr, Some(c.issue), Some(&c.head), None));
        0
    }
}

/// `tehas-dispatch refresh [argument...]`, with the settings `console run` loads
/// in the environment. Returns the exit status.
pub fn main(args: &[&str]) -> i32 {
    let one = asked_pr(args);
    match args.first() {
        Some(&"-h" | &"--help") => {
            tehas_core::outln!("{}", usage());
            return 0;
        }
        Some(&"--pr") if one.is_none() => {
            eprintln!(
                "tehasbot refresh: --pr takes the number of one pull request\n{}",
                usage()
            );
            return 64;
        }
        Some(_) if one.is_some() => {}
        Some(a) => {
            eprintln!("tehasbot refresh: unknown argument: {a}\n{}", usage());
            return 64;
        }
        None => {}
    }
    let set = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let number = |k: &str| set(k).and_then(|v| v.parse::<u64>().ok());
    // --pr runs whatever the interval says, so it does not need it.
    let every = match one {
        Some(_) => Some(0),
        None => number("TEHAS_REFRESH_EVERY"),
    };
    let (Some(home), Some(log), Some(repo), Some(control), Some(every)) = (
        set("TEHAS_HOME"),
        set("TEHAS_REFRESH_LOG"),
        set("TEHAS_REPO"),
        number("TEHAS_CONTROL_ISSUE"),
        every,
    ) else {
        eprintln!(
            "tehas-dispatch: refresh needs the settings tehasbot refresh loads (lib/config.sh, tehas_load)"
        );
        return 78;
    };
    let home = PathBuf::from(home);
    let run = Run {
        dir: home.join("state/refresh"),
        log: PathBuf::from(log),
        repo,
        control,
        every,
        bot: set("TEHAS_BOT_LOGIN").unwrap_or_default(),
    };
    let store = home.join("libexec/store").to_string_lossy().into_owned();
    let mut calls = Calls::idle(Duration::from_secs(20), store);
    if let Some(pr) = one {
        return run.one(&mut calls, pr, args.last() == Some(&"--json"));
    }
    let now = tehas_core::time::now() as u64;
    run.go(&mut calls, now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tehas_core::testdir::TestDir;

    const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const BASE: &str = "cccccccccccccccccccccccccccccccccccccccc";

    fn scratch(name: &str) -> TestDir {
        TestDir::new(&format!("refresh-{name}"))
    }

    /// One of the bot's pull requests that qualifies and conflicts.
    fn pull(pr: u64, issue: u64) -> Value {
        json!({"number": pr, "isDraft": false, "isCrossRepository": false,
            "headRefOid": HEAD, "baseRefOid": BASE, "mergeable": "CONFLICTING",
            "author": {"login": "bot"}, "headRepository": {"nameWithOwner": "a/b"},
            "closingIssuesReferences": {"totalCount": 1, "nodes": [{
                "number": issue, "state": "OPEN",
                "labels": {"nodes": [{"name": "tehas:awaiting-merge"}]},
                "closedByPullRequestsReferences": {"nodes": [{"number": pr, "state": "OPEN"}]}}]}})
    }

    fn of(nodes: &[Value]) -> Found {
        found(nodes, "bot[bot]", "a/b", 1)
    }

    #[test]
    fn a_run_waits_out_the_interval() {
        let now = 1_000_000;
        assert!(!due(&format!("{}\n", now - 599), now, 10));
        assert!(due(&format!("{}\n", now - 600), now, 10));
        assert!(!due(&format!("{now}\n"), now, 10));
        assert!(due(&format!("{}\n", now - 60), now, 1));
    }

    #[test]
    fn a_stamp_that_cannot_be_read_or_lies_ahead_holds_nothing_off() {
        let now = 1_000_000;
        for stamp in ["", "\n", "soon\n", "1000000 \n", "-1\n", "1000000000000\n"] {
            assert!(due(stamp, now, 10), "{stamp:?}");
        }
        assert!(due("1000001\n", now, 10));
        assert!(due("999999\n999999\n", now, 10));
    }

    #[test]
    fn a_conflict_is_asked_and_kept() {
        let f = of(&[pull(50, 7)]);
        assert_eq!(
            f.conflicts,
            [Conflict {
                issue: 7,
                pr: 50,
                head: HEAD.into(),
                base: BASE.into()
            }]
        );
        assert_eq!(f.keep, BTreeSet::from([7]));
        assert_eq!((f.unknown, f.not_ours), (0, 0));
    }

    #[test]
    fn unknown_keeps_and_asks_nothing_and_mergeable_neither() {
        let mut unknown = pull(50, 7);
        unknown["mergeable"] = json!("UNKNOWN");
        let mut clean = pull(51, 8);
        clean["mergeable"] = json!("MERGEABLE");
        let mut gone = pull(52, 9);
        gone["mergeable"] = Value::Null;
        let f = of(&[unknown, clean, gone]);
        assert!(f.conflicts.is_empty());
        assert_eq!(f.keep, BTreeSet::from([7]));
        // The bot's that do not conflict are neither unknown nor not ours.
        assert_eq!((f.unknown, f.not_ours), (1, 0));
    }

    #[test]
    fn what_the_rule_rejects_is_counted_as_not_ours_to_refresh() {
        let conflict = pull(50, 7);
        let mut unknown = pull(51, 8);
        unknown["mergeable"] = json!("UNKNOWN");
        let mut clean = pull(52, 9);
        clean["mergeable"] = json!("MERGEABLE");
        let mut foreign = pull(53, 10);
        foreign["author"]["login"] = json!("alice");
        let mut fork = pull(54, 11);
        fork["isCrossRepository"] = json!(true);
        let mut no_sha = pull(55, 12);
        no_sha["headRefOid"] = json!("abc");
        let f = of(&[conflict, unknown, clean, foreign, fork, no_sha, json!(null)]);
        assert_eq!(f.conflicts.len(), 1);
        assert_eq!((f.unknown, f.not_ours), (1, 4));
    }

    #[test]
    fn a_head_or_base_that_is_no_sha_keeps_nothing_even_when_unknown() {
        for field in ["headRefOid", "baseRefOid"] {
            for state in ["CONFLICTING", "UNKNOWN"] {
                let mut p = pull(50, 7);
                p["mergeable"] = json!(state);
                p[field] = json!("abc");
                let not_ours = Found {
                    not_ours: 1,
                    ..Found::default()
                };
                assert_eq!(of(&[p]), not_ours, "{field} {state}");
            }
        }
    }

    #[test]
    fn the_rule_decides_what_is_read() {
        let mut other = pull(50, 7);
        other["author"]["login"] = json!("alice");
        assert_eq!(
            of(&[other, json!(null), json!("x"), pull(51, 1)]),
            Found {
                not_ours: 4,
                ..Found::default()
            }
        );
    }

    #[test]
    fn a_request_keeps_its_time_only_for_the_same_pull_request_at_the_same_head() {
        let old = request("2026-01-01T00:00:00Z", &of(&[pull(50, 7)]).conflicts[0]);
        assert_eq!(
            kept_at(&old, 50, HEAD).as_deref(),
            Some("2026-01-01T00:00:00Z")
        );
        assert_eq!(kept_at(&old, 51, HEAD), None);
        assert_eq!(kept_at(&old, 50, BASE), None);
        assert_eq!(kept_at("not json", 50, HEAD), None);
        assert_eq!(kept_at("{}", 50, HEAD), None);
        let empty = json!({"at": "", "pr": 50, "head_sha": HEAD}).to_string();
        assert_eq!(kept_at(&empty, 50, HEAD), None);
        let number = json!({"at": 5, "pr": 50, "head_sha": HEAD}).to_string();
        assert_eq!(kept_at(&number, 50, HEAD), None);
    }

    #[test]
    fn a_request_is_laid_out_as_jq_laid_it_out() {
        let text = request("2026-01-01T00:00:00Z", &of(&[pull(50, 7)]).conflicts[0]);
        assert_eq!(
            text,
            format!(
                "{{\n  \"at\": \"2026-01-01T00:00:00Z\",\n  \"pr\": 50,\n  \"head_sha\": \"{HEAD}\",\n  \"base_sha\": \"{BASE}\"\n}}\n"
            )
        );
    }

    #[test]
    fn a_rewrite_replaces_the_request_and_leaves_no_new_file() {
        let d = scratch("put");
        let path = d.join("7.json");
        fs::write(&path, "old").unwrap();
        put(&path, "new").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(fs::read_dir(&d).unwrap().count(), 1);
        let held = d.join("8.json");
        fs::create_dir(&held).unwrap();
        assert!(put(&held, "new").is_err());
        assert!(!d.join("8.json.new").exists());
    }

    #[test]
    fn the_sweep_takes_requests_not_kept_and_leaves_any_other_file() {
        let d = scratch("sweep");
        for name in [
            "7.json",
            "8.json",
            "07.json",
            "0.json",
            "notes.json",
            ".json",
            "9.said",
            "9.json.new",
            "last-run",
        ] {
            fs::write(d.join(name), "{}").unwrap();
        }
        sweep(&d, &BTreeSet::from([7]));
        let mut left: Vec<String> = fs::read_dir(&d)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(
            left,
            [
                ".json",
                "0.json",
                "07.json",
                "7.json",
                "9.json.new",
                "9.said",
                "last-run",
                "notes.json"
            ]
        );
    }

    #[test]
    fn an_asked_request_keeps_the_scans_field_order_and_adds_asked_last() {
        let c = &of(&[pull(50, 7)]).conflicts[0];
        let text = asked_request("2026-01-01T00:00:00Z", c);
        assert_eq!(
            text,
            format!(
                "{{\n  \"at\": \"2026-01-01T00:00:00Z\",\n  \"pr\": 50,\n  \"head_sha\": \"{HEAD}\",\n  \"base_sha\": \"{BASE}\",\n  \"asked\": \"pr\"\n}}\n"
            )
        );
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["asked"], json!("pr"));
        assert_eq!(
            kept_at(&text, 50, HEAD).as_deref(),
            Some("2026-01-01T00:00:00Z")
        );
    }

    #[test]
    fn one_pull_request_asked_for_follows_the_scans_rule() {
        let open = |mut p: Value| {
            p["state"] = json!("OPEN");
            p
        };
        let ask = |p: Value| asked(&p, "bot[bot]", "a/b", 1);
        let c = ask(open(pull(50, 7))).unwrap();
        assert_eq!((c.issue, c.pr, c.head.as_str()), (7, 50, HEAD));
        let mut unknown = open(pull(50, 7));
        unknown["mergeable"] = json!("UNKNOWN");
        assert_eq!(ask(unknown).unwrap().issue, 7);
        let mut clean = open(pull(50, 7));
        clean["mergeable"] = json!("MERGEABLE");
        assert_eq!(
            ask(clean).unwrap_err(),
            "it does not conflict: GitHub says MERGEABLE"
        );
        let mut foreign = open(pull(50, 7));
        foreign["author"]["login"] = json!("alice");
        assert_eq!(ask(foreign).unwrap_err(), "it is not the bot's");
        let mut no_sha = open(pull(50, 7));
        no_sha["baseRefOid"] = json!("abc");
        assert_eq!(ask(no_sha).unwrap_err(), "its head or base is no sha");
        let mut merged = pull(50, 7);
        merged["state"] = json!("MERGED");
        assert_eq!(ask(merged).unwrap_err(), "it is not open");
    }

    #[test]
    fn only_pr_and_a_whole_number_above_zero_ask_for_one() {
        assert_eq!(asked_pr(&["--pr", "50"]), Some(50));
        assert_eq!(asked_pr(&["--pr", "50", "--json"]), Some(50));
        for bad in [
            &["--pr"][..],
            &["--pr", "50", "--jsn"],
            &["--json", "--pr", "50"],
            &["--pr", "--json"],
            &["--pr", "0", "--json"],
            &["--pr", "50", "--json", "--json"],
            &["--pr", "0"],
            &["--pr", "x"],
            &["--pr", "07"],
            &["--pr", ""],
            &["--pr", "-1"],
            &["--pr", "50", "51"],
            &["--now", "50"],
            &["--pr", "99999999999999999999999"],
            &[],
        ] {
            assert_eq!(asked_pr(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_json_of_one_ask_says_whether_it_was_asked_and_why_not() {
        assert_eq!(
            said(50, Some(7), Some(HEAD), None),
            format!(r#"{{"asked":true,"pr":50,"issue":7,"head_sha":"{HEAD}","why":null}}"#)
        );
        assert_eq!(
            said(50, None, None, Some("it is \"not\" open")),
            r#"{"asked":false,"pr":50,"issue":null,"head_sha":null,"why":"it is \"not\" open"}"#
        );
        let mut p = pull(50, 7);
        assert_eq!(named(&p), (Some(7), Some(HEAD)));
        p["closingIssuesReferences"]["nodes"] = json!([]);
        p["headRefOid"] = json!(null);
        assert_eq!(named(&p), (None, None));
    }

    #[test]
    fn the_help_is_the_wrappers_header() {
        let text = usage();
        assert!(text.starts_with("Find the factory's own pull requests"));
        assert!(text.contains("\n\nUsage:\n  tehasbot refresh "));
        assert!(text.ends_with("removed."));
    }
}

//! tehasbot refresh: the factory's own pull requests that conflict with the
//! branch, each asked of the dispatcher in a file under state/refresh. What a
//! run does and its statuses are in the header of libexec/refresh, which is
//! also this command's --help.

use crate::decide::{hex40, iso_now, refresh_why};
use crate::read::github;
use crate::read::run::Calls;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const WRAPPER: &str = include_str!("../../../libexec/refresh");

fn usage() -> String {
    WRAPPER
        .lines()
        .skip(1)
        .take_while(|l| l.starts_with('#'))
        .map(|l| {
            let text = l.trim_start_matches('#');
            text.strip_prefix(' ').unwrap_or(text)
        })
        .collect::<Vec<_>>()
        .join("\n")
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

/// The conflicts of a run, and the issues whose requests stay.
#[derive(Debug, Default, PartialEq)]
struct Found {
    conflicts: Vec<Conflict>,
    keep: BTreeSet<u64>,
}

/// UNKNOWN keeps a request and asks for none: GitHub computes the state
/// lazily and the next run reads it again.
fn found(nodes: &[Value], bot: &str, repo: &str, control: u64) -> Found {
    let mut f = Found::default();
    for node in nodes {
        if !refresh_why(node, bot, repo, control).is_empty() {
            continue;
        }
        let (head, base) = (node.get("headRefOid"), node.get("baseRefOid"));
        if !hex40(head) || !hex40(base) {
            continue;
        }
        let number = |v: Option<&Value>| v.and_then(Value::as_u64);
        let (Some(issue), Some(pr)) = (
            number(node.pointer("/closingIssuesReferences/nodes/0/number")),
            number(node.get("number")),
        ) else {
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

        // A GitHub App's token cannot ask GET /user, so an app's login is the
        // setting.
        let bot = if self.bot.is_empty() {
            let user = calls.gh(&["api", "user"]).unwrap_or(Value::Null);
            user.get("login")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        } else {
            self.bot.clone()
        };
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
            "read {} pull requests, {} conflict, wrote {wrote} requests",
            nodes.len(),
            f.conflicts.len()
        ));
        0
    }
}

/// `tehas-dispatch refresh [argument...]`, with libexec/refresh's settings in
/// the environment. Returns the exit status.
pub fn main(args: &[&str]) -> i32 {
    match args.first() {
        Some(&"-h" | &"--help") => {
            println!("{}", usage());
            return 0;
        }
        Some(a) => {
            eprintln!("tehasbot refresh: unknown argument: {a}\n{}", usage());
            return 64;
        }
        None => {}
    }
    let set = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let number = |k: &str| set(k).and_then(|v| v.parse::<u64>().ok());
    let (Some(home), Some(log), Some(repo), Some(control), Some(every)) = (
        set("TEHAS_HOME"),
        set("TEHAS_REFRESH_LOG"),
        set("TEHAS_REPO"),
        number("TEHAS_CONTROL_ISSUE"),
        number("TEHAS_REFRESH_EVERY"),
    ) else {
        eprintln!(
            "tehas-dispatch: refresh needs the settings libexec/refresh loads (lib/config.sh, tehas_load)"
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
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    run.go(&mut calls, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const BASE: &str = "cccccccccccccccccccccccccccccccccccccccc";

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "tehas-refresh-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
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
    }

    #[test]
    fn a_head_or_base_that_is_no_sha_keeps_nothing_even_when_unknown() {
        for field in ["headRefOid", "baseRefOid"] {
            for state in ["CONFLICTING", "UNKNOWN"] {
                let mut p = pull(50, 7);
                p["mergeable"] = json!(state);
                p[field] = json!("abc");
                assert_eq!(of(&[p]), Found::default(), "{field} {state}");
            }
        }
    }

    #[test]
    fn the_rule_decides_what_is_read() {
        let mut other = pull(50, 7);
        other["author"]["login"] = json!("alice");
        assert_eq!(
            of(&[other, json!(null), json!("x"), pull(51, 1)]),
            Found::default()
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
        let _ = fs::remove_dir_all(&d);
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
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn the_help_is_the_wrappers_header() {
        let text = usage();
        assert!(text.starts_with("Find the factory's own pull requests"));
        assert!(text.contains("\n\nUsage:\n  tehasbot refresh "));
        assert!(text.ends_with("removed."));
    }
}

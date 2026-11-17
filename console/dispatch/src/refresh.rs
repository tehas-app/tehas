//! tehas refresh --pr N, or tehasbot refresh: one of the factory's own pull
//! requests that conflicts with the branch, asked of the dispatcher in a file
//! under state/refresh. What a run does and its statuses are in
//! src/usage/refresh.txt, this command's --help.

use crate::decide::{hex40, iso_now, refresh_why};
use crate::read::github;
use crate::read::run::Calls;
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn usage() -> &'static str {
    include_str!("usage/refresh.txt").trim_end_matches('\n')
}

#[derive(Debug, PartialEq)]
struct Conflict {
    issue: u64,
    pr: u64,
    head: String,
    base: String,
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

/// Whether `program` names a file that can be run: as a path, or found in a
/// directory of `path`.
fn found(program: &str, path: Option<&std::ffi::OsStr>) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let runs = |p: &Path| {
        fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    if program.contains('/') {
        return runs(Path::new(program));
    }
    path.is_some_and(|path| std::env::split_paths(path).any(|d| runs(&d.join(program))))
}

/// The request `--pr` writes, in the field order and layout the
/// dispatcher's readers and a person have always seen, `asked` last.
fn request(at: &str, c: &Conflict) -> String {
    format!(
        "{{\n  \"at\": {},\n  \"pr\": {},\n  \"head_sha\": {},\n  \"base_sha\": {},\n  \"asked\": \"pr\"\n}}\n",
        json!(at),
        c.pr,
        json!(c.head),
        json!(c.base)
    )
}

/// What the refresh rule makes of the pull request asked for: the request
/// to write, or why there is none. UNKNOWN is written, as the dispatcher
/// waits on it: GitHub computes the state lazily.
fn asked(node: &Value, bot: &str, repo: &str, control: u64) -> Result<Conflict, String> {
    if node.get("state").and_then(Value::as_str) != Some("OPEN") {
        return Err("it is not open".into());
    }
    let why = refresh_why(node, bot, repo, control);
    if !why.is_empty() {
        return Err(why);
    }
    let (head, base) = (node.get("headRefOid"), node.get("baseRefOid"));
    let number = |v: Option<&Value>| v.and_then(Value::as_u64);
    let (Some(issue), Some(pr)) = (
        number(node.pointer("/closingIssuesReferences/nodes/0/number")),
        number(node.get("number")),
    ) else {
        return Err("it names no issue it closes".into());
    };
    if !hex40(head) || !hex40(base) {
        return Err("its head or base is no sha".into());
    }
    match node.get("mergeable").and_then(Value::as_str) {
        Some("CONFLICTING" | "UNKNOWN") => Ok(Conflict {
            issue,
            pr,
            head: head.and_then(Value::as_str).unwrap_or("").to_string(),
            base: base.and_then(Value::as_str).unwrap_or("").to_string(),
        }),
        state => Err(format!(
            "it does not conflict: GitHub says {}",
            state.unwrap_or("nothing")
        )),
    }
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

/// What `--pr N --json` prints: whether a request was written, and why not
/// when it was not. The issue and head are null when GitHub did not say.
fn said(pr: u64, issue: Option<u64>, head: Option<&str>, why: Option<&str>) -> String {
    format!(
        "{{\"api\":1,\"asked\":{},\"pr\":{pr},\"issue\":{},\"headSha\":{},\"why\":{}}}",
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

    fn fail(&self, why: &str) -> i32 {
        eprintln!("{} ERROR: {why}", iso_now());
        self.line(&format!("ERROR: {why}"));
        1
    }

    /// `--pr N`: one pull request, now. With `--json` one object on standard
    /// output says how it went, for 0, 3 and 1.
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
            let why = if found(&calls.gh, std::env::var_os("PATH").as_deref()) {
                format!(
                    "GitHub did not answer for pull request #{pr} of {}",
                    self.repo
                )
            } else {
                format!("gh not found, so pull request #{pr} could not be read")
            };
            print(said(pr, None, None, Some(&why)));
            return self.fail(&why);
        }
        let c = match asked(&node, &self.bot, &self.repo, self.control) {
            Ok(c) => c,
            Err(why) => {
                let (issue, head) = named(&node);
                print(said(pr, issue, head, Some(&why)));
                eprintln!("tehas refresh: pull request #{pr} is not refreshed: {why}");
                self.line(&format!(
                    "asked for pull request #{pr}: not refreshed: {why}"
                ));
                return i32::from(tehas_core::exit::NO);
            }
        };
        let path = self.dir.join(format!("{}.json", c.issue));
        let at = fs::read_to_string(&path)
            .ok()
            .and_then(|old| kept_at(&old, c.pr, &c.head))
            .unwrap_or_else(iso_now);
        if put(&path, &request(&at, &c)).is_err() {
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
    if let Some(&"-h" | &"--help") = args.first() {
        tehas_core::outln!("{}", usage());
        return 0;
    }
    let Some(pr) = asked_pr(args) else {
        let why = match args.first() {
            None | Some(&"--pr") => "--pr takes the number of one pull request".to_string(),
            Some(a) => format!("unknown argument: {a}"),
        };
        eprintln!("tehas refresh: {why}\n{}", usage());
        return 64;
    };
    let set = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let number = |k: &str| set(k).and_then(|v| v.parse::<u64>().ok());
    let (Some(home), Some(log), Some(repo), Some(control)) = (
        set("TEHAS_HOME"),
        set("TEHAS_REFRESH_LOG"),
        set("TEHAS_REPO"),
        number("TEHAS_CONTROL_ISSUE"),
    ) else {
        eprintln!(
            "tehas-dispatch: refresh needs the settings tehas refresh loads (lib/config.sh, tehas_load)"
        );
        return 78;
    };
    let home = PathBuf::from(home);
    let run = Run {
        dir: home.join("state/refresh"),
        log: PathBuf::from(log),
        repo,
        control,
        bot: set("TEHAS_BOT_LOGIN").unwrap_or_default(),
    };
    let store = home.join("libexec/store").to_string_lossy().into_owned();
    let mut calls = Calls::idle(Duration::from_secs(20), store);
    run.one(&mut calls, pr, args.last() == Some(&"--json"))
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

    /// One of the bot's open pull requests that qualifies and conflicts.
    fn pull(pr: u64, issue: u64) -> Value {
        json!({"number": pr, "state": "OPEN", "isDraft": false, "isCrossRepository": false,
            "headRefOid": HEAD, "baseRefOid": BASE, "mergeable": "CONFLICTING",
            "author": {"login": "bot"}, "headRepository": {"nameWithOwner": "a/b"},
            "closingIssuesReferences": {"totalCount": 1, "nodes": [{
                "number": issue, "state": "OPEN",
                "labels": {"nodes": [{"name": "tehas:awaiting-merge"}]},
                "closedByPullRequestsReferences": {"nodes": [{"number": pr, "state": "OPEN"}]}}]}})
    }

    fn ask(p: &Value) -> Result<Conflict, String> {
        asked(p, "bot[bot]", "a/b", 1)
    }

    #[test]
    fn a_program_is_found_by_its_path_or_in_a_directory_of_path() {
        let d = scratch("found");
        let gh = d.join("gh");
        fs::write(&gh, "#!/bin/sh\n").unwrap();
        let mode = |m| {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&gh, fs::Permissions::from_mode(m)).unwrap();
        };
        let path = std::env::join_paths(["/nonexistent", d.path().to_str().unwrap()]).unwrap();
        assert!(!found("gh", Some(&path)), "not executable");
        mode(0o755);
        assert!(found("gh", Some(&path)));
        assert!(found(gh.to_str().unwrap(), None));
        assert!(!found("gh", Some("/nonexistent".as_ref())));
        assert!(!found("gh", None));
        assert!(!found("/nonexistent/gh", Some(&path)));
    }

    #[test]
    fn a_pull_request_read_with_no_gh_to_run_says_gh_not_found() {
        let d = scratch("no-gh");
        let run = Run {
            dir: d.join("state/refresh"),
            log: d.join("logs/refresh.log"),
            repo: "a/b".into(),
            control: 1,
            bot: "bot".into(),
        };
        let store = d.join("store").to_string_lossy().into_owned();
        let mut calls = Calls::idle(Duration::from_secs(1), store);
        calls.gh = d.join("nowhere/gh").to_string_lossy().into_owned();
        assert_eq!(run.one(&mut calls, 50, false), 1);
        let log = fs::read_to_string(d.join("logs/refresh.log")).unwrap();
        assert!(log.contains("ERROR: gh not found"), "{log}");
        assert!(!log.contains("did not answer"), "{log}");
    }

    #[test]
    fn a_request_keeps_its_time_only_for_the_same_pull_request_at_the_same_head() {
        let old = request("2026-01-01T00:00:00Z", &ask(&pull(50, 7)).unwrap());
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
    fn a_request_keeps_its_field_order_with_asked_last() {
        let text = request("2026-01-01T00:00:00Z", &ask(&pull(50, 7)).unwrap());
        assert_eq!(
            text,
            format!(
                "{{\n  \"at\": \"2026-01-01T00:00:00Z\",\n  \"pr\": 50,\n  \"head_sha\": \"{HEAD}\",\n  \"base_sha\": \"{BASE}\",\n  \"asked\": \"pr\"\n}}\n"
            )
        );
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["asked"], json!("pr"));
    }

    #[test]
    fn one_pull_request_asked_for_follows_the_refresh_rule() {
        let c = ask(&pull(50, 7)).unwrap();
        assert_eq!(
            c,
            Conflict {
                issue: 7,
                pr: 50,
                head: HEAD.into(),
                base: BASE.into()
            }
        );
        let mut unknown = pull(50, 7);
        unknown["mergeable"] = json!("UNKNOWN");
        assert_eq!(ask(&unknown).unwrap().issue, 7);
        let mut clean = pull(50, 7);
        clean["mergeable"] = json!("MERGEABLE");
        assert_eq!(
            ask(&clean).unwrap_err(),
            "it does not conflict: GitHub says MERGEABLE"
        );
        let mut gone = pull(50, 7);
        gone["mergeable"] = Value::Null;
        assert_eq!(
            ask(&gone).unwrap_err(),
            "it does not conflict: GitHub says nothing"
        );
        let mut foreign = pull(50, 7);
        foreign["author"]["login"] = json!("alice");
        assert_eq!(ask(&foreign).unwrap_err(), "it is not the bot's");
        let mut fork = pull(50, 7);
        fork["isCrossRepository"] = json!(true);
        assert!(ask(&fork).is_err());
        assert!(ask(&pull(50, 1)).is_err(), "the control issue");
        let mut merged = pull(50, 7);
        merged["state"] = json!("MERGED");
        assert_eq!(ask(&merged).unwrap_err(), "it is not open");
        assert_eq!(ask(&json!(null)).unwrap_err(), "it is not open");
    }

    #[test]
    fn a_head_or_base_that_is_no_sha_is_refused_even_when_unknown() {
        for field in ["headRefOid", "baseRefOid"] {
            for state in ["CONFLICTING", "UNKNOWN"] {
                let mut p = pull(50, 7);
                p["mergeable"] = json!(state);
                p[field] = json!("abc");
                assert_eq!(
                    ask(&p).unwrap_err(),
                    "its head or base is no sha",
                    "{field} {state}"
                );
            }
        }
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
    fn no_argument_and_an_unknown_one_are_refused_before_any_setting() {
        assert_eq!(main(&[]), 64);
        assert_eq!(main(&["--now"]), 64);
        assert_eq!(main(&["--pr", "0"]), 64);
    }

    #[test]
    fn the_json_of_one_ask_says_whether_it_was_asked_and_why_not() {
        assert_eq!(
            said(50, Some(7), Some(HEAD), None),
            format!(r#"{{"api":1,"asked":true,"pr":50,"issue":7,"headSha":"{HEAD}","why":null}}"#)
        );
        assert_eq!(
            said(50, None, None, Some("it is \"not\" open")),
            r#"{"api":1,"asked":false,"pr":50,"issue":null,"headSha":null,"why":"it is \"not\" open"}"#
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
        assert!(text
            .starts_with("Ask the dispatcher to refresh one of the factory's own pull requests"));
        assert!(text.contains("\n\nUsage:\n  tehas refresh "));
        assert!(text.ends_with("written."));
    }
}

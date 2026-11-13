//! `tehas start <n>`: approve one issue for a start from the host (#1967),
//! kept in the state store (store verbs `host-start-set`, `host-start-clear`).
//! src/usage/start.txt is the text a person reads.
//!
//!   tehas start <issue> [--json]
//!   tehas start <issue> --clear [--json]
//!
//! The approval names no GitHub login: the command runs as the factory's user
//! and the host is who approves. Nothing starts here. The next poll starts the
//! issue under its usual rules, and the start spends the approval. Before it
//! writes, the issue is asked of GitHub: one that is not an open issue of
//! `TEHAS_REPO`, or is the control issue, is refused. `--clear` asks GitHub
//! nothing.
//!
//! The clock is the host's, or `TEHAS_STORE_TEST_NOW` when that names whole
//! seconds, as for `tehas order`.
//!
//! Exit: 0 approved, already approved, withdrawn or nothing to withdraw; 1 the
//! issue was refused, GitHub could not be asked, or the store refused or could
//! not be read; 64 a bad argument, which writes nothing.

use crate::forge::{gh_program, repo, repo_shaped, CALL_BOUND};
use crate::store;
use serde_json::{json, Value};
use std::env;
use std::path::{Path, PathBuf};
use tehas_core::forge::{Bounded, GhRunner};
use tehas_core::time;

pub const USAGE: &str = include_str!("usage/start.txt");

/// What was asked.
#[derive(Debug, PartialEq, Eq)]
pub struct Asked {
    pub issue: u64,
    pub clear: bool,
    pub json: bool,
}

/// An issue's number as the factory writes one: no sign, no leading zero.
fn is_issue(w: &str) -> bool {
    (1..=10).contains(&w.len()) && !w.starts_with('0') && w.bytes().all(|b| b.is_ascii_digit())
}

fn bad(why: &str) -> u8 {
    eprintln!("tehas start: {why}");
    eprint!("{USAGE}");
    64
}

/// The words of a run, read left to right. `Ok(None)` is help, printed here.
pub fn words(given: &[String]) -> Result<Option<Asked>, u8> {
    let (mut issue, mut clear, mut json) = (None, false, false);
    for word in given {
        match word.as_str() {
            "-h" | "--help" => {
                tehas_core::out!("{USAGE}");
                return Ok(None);
            }
            "--clear" if !clear => clear = true,
            "--json" if !json => json = true,
            w if w.starts_with('-') => {
                return Err(bad(&format!("unknown argument, or one given twice: {w}")))
            }
            w if issue.is_some() => return Err(bad(&format!("one issue number at most: {w}"))),
            w if is_issue(w) => issue = w.parse().ok(),
            w => return Err(bad(&format!("not an issue number: {w}"))),
        }
    }
    let Some(issue) = issue else {
        return Err(bad("it needs an issue number"));
    };
    Ok(Some(Asked { issue, clear, json }))
}

fn now() -> i64 {
    match env::var("TEHAS_STORE_TEST_NOW") {
        Ok(s) => s.parse().expect("TEHAS_STORE_TEST_NOW is not a Unix time"),
        Err(_) => time::now(),
    }
}

fn home() -> Option<PathBuf> {
    let from = |name: &str| match env::var_os(name) {
        Some(v) if !v.is_empty() => Some(PathBuf::from(v)),
        _ => None,
    };
    from("TEHAS_HOME").or_else(|| from("HOME"))
}

/// Whether `n` may be approved: an open issue of `repo` and not the control
/// issue. The line of the refusal, or Ok.
pub fn may_start(
    gh: &mut dyn GhRunner,
    repo: &str,
    control: Option<u64>,
    n: u64,
) -> Result<(), String> {
    if control == Some(n) {
        return Err(format!(
            "#{n} is the control issue of this factory, which is never worked on"
        ));
    }
    let path = format!("repos/{repo}/issues/{n}");
    let answer = gh.run(&["api", &path]).map_err(|why| {
        format!("#{n} could not be read from {repo}: it is no issue there, or GitHub did not answer ({why})")
    })?;
    let v: Value = serde_json::from_str(&answer)
        .map_err(|_| format!("GitHub's answer for #{n} could not be read"))?;
    if v.get("number").and_then(Value::as_u64) != Some(n) {
        return Err(format!("GitHub's answer for #{n} could not be read"));
    }
    if !v.get("pull_request").unwrap_or(&Value::Null).is_null() {
        return Err(format!(
            "#{n} is a pull request, not an issue the factory works on"
        ));
    }
    match v.get("state").and_then(Value::as_str) {
        Some("open") => Ok(()),
        Some("closed") => Err(format!("#{n} is closed, and a closed issue is never taken")),
        _ => Err(format!("GitHub's answer for #{n} could not be read")),
    }
}

/// The answer of a run: the line a person reads, or the object a program
/// reads with `api` first.
fn answer(a: &Asked, approved: bool, at: Option<i64>, changed: bool) -> String {
    let n = a.issue;
    if a.json {
        let mut v = json!({"api": 1, "issue": n, "approved": approved, "changed": changed});
        if let Some(at) = at {
            v["at"] = json!(time::iso(at));
        }
        return v.to_string();
    }
    let since = at.map(time::iso).unwrap_or_default();
    match (a.clear, changed) {
        (false, true) => format!("#{n}: approved from the host at {since}: the next poll starts it under the usual rules, and the start spends the approval"),
        (false, false) => format!("#{n} is approved from the host already, since {since}"),
        (true, true) => format!("#{n}: the host's approval of {since} is withdrawn"),
        (true, false) => format!("#{n} has no approval from the host, nothing to withdraw"),
    }
}

/// The run in `home`, with `gh` asked about the issue before an approval.
pub fn run_in(
    home: &Path,
    a: &Asked,
    gh: Option<&mut dyn GhRunner>,
    repo: &str,
    control: Option<u64>,
) -> u8 {
    let n = a.issue.to_string();
    if !store::ready(home) {
        eprintln!(
            "tehas start: no state store in {}: run install.sh",
            home.display()
        );
        return 1;
    }
    if !a.clear {
        let Some(gh) = gh else {
            eprintln!("tehas start: no gh to ask GitHub about #{n} with");
            return 1;
        };
        if let Err(why) = may_start(gh, repo, control, a.issue) {
            eprintln!("tehas start: {why}; nothing was approved");
            return 1;
        }
    }
    let at = now().to_string();
    let args: Vec<&str> = if a.clear {
        vec!["host-start-clear", &n]
    } else {
        vec!["host-start-set", &n, "--at", &at]
    };
    match store::call(home, &args) {
        Ok(v) => {
            let row = &v["host_start"];
            let at = row["at"].as_i64();
            let (approved, changed) = if a.clear {
                (false, v["cleared"] == json!(true))
            } else {
                (true, v["new"] == json!(true))
            };
            tehas_core::outln!("{}", answer(a, approved, at, changed));
            0
        }
        Err(refused) => {
            eprintln!("tehas start: the store said: {}", refused.line);
            1
        }
    }
}

pub fn main(given: &[String]) -> u8 {
    let a = match words(given) {
        Ok(Some(a)) => a,
        Ok(None) => return 0,
        Err(rc) => return rc,
    };
    let Some(here) = home() else {
        eprintln!("tehas start: neither TEHAS_HOME nor HOME is set");
        return 64;
    };
    let Some(repository) = repo().filter(|r| repo_shaped(r)) else {
        eprintln!("tehas start: TEHAS_REPO is not owner/name");
        return 1;
    };
    let control = env::var("TEHAS_CONTROL_ISSUE")
        .ok()
        .and_then(|c| c.parse().ok());
    let path = env::var("PATH").unwrap_or_default();
    let tehas_home = env::var("TEHAS_HOME").ok();
    let mut runner = gh_program(tehas_home.as_deref(), &path).map(|p| Bounded::new(p, CALL_BOUND));
    let gh = runner.as_mut().map(|r| r as &mut dyn GhRunner);
    run_in(&here, &a, gh, &repository, control)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tehas_core::testdir::TestDir;

    fn w(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    /// GitHub as one answer for every call, or a failure.
    struct Gh(Result<String, String>, Vec<String>);

    impl GhRunner for Gh {
        fn run(&mut self, args: &[&str]) -> Result<String, String> {
            self.1.push(args.join(" "));
            self.0.clone()
        }
    }

    fn issue(n: u64, state: &str) -> Gh {
        Gh(
            Ok(json!({"number": n, "state": state}).to_string()),
            Vec::new(),
        )
    }

    /// A home with the store program where install.sh puts it and a store
    /// made by the store's own init.
    fn home(name: &str) -> TestDir {
        let home = TestDir::new(&format!("start-{name}"));
        fs::create_dir_all(home.join("libexec")).unwrap();
        tehas_core::testing::write_exe(store::program(&home), "#!/bin/sh\nexit 1\n").unwrap();
        let made = ::store::call(
            &home,
            &[
                "init",
                "--version",
                "v0.0.0",
                "--commit",
                "0123456789abcdef0123456789abcdef01234567",
                "--instance",
                "instance",
                "--repo",
                "acme/widget",
            ],
            &mut std::io::empty(),
        );
        assert!(made.is_ok(), "{made:?}");
        home
    }

    fn listed(home: &Path) -> Value {
        store::call(home, &["host-start-list"]).unwrap()["host_starts"].clone()
    }

    fn asked(issue: u64, clear: bool) -> Asked {
        Asked {
            issue,
            clear,
            json: false,
        }
    }

    #[test]
    fn the_words_are_an_issue_clear_and_json() {
        assert_eq!(
            words(&w(&["12", "--clear", "--json"])),
            Ok(Some(Asked {
                issue: 12,
                clear: true,
                json: true
            }))
        );
        assert_eq!(words(&w(&["12"])), Ok(Some(asked(12, false))));
        for bad in [
            &[][..],
            &["012"],
            &["twelve"],
            &["12", "13"],
            &["12", "--clear", "--clear"],
            &["12", "--force"],
        ] {
            assert_eq!(words(&w(bad)), Err(64), "{bad:?}");
        }
    }

    #[test]
    fn an_open_issue_is_approved_once_and_withdrawn() {
        let home = home("approve");
        let mut gh = issue(12, "open");
        assert_eq!(
            run_in(
                &home,
                &asked(12, false),
                Some(&mut gh),
                "acme/widget",
                Some(1)
            ),
            0
        );
        assert_eq!(gh.1, ["api repos/acme/widget/issues/12"]);
        assert_eq!(listed(&home).as_array().map(Vec::len), Some(1));
        assert_eq!(listed(&home)[0]["issue"], 12);
        // Again: the approval is the one there, and nothing more is written.
        assert_eq!(
            run_in(
                &home,
                &asked(12, false),
                Some(&mut gh),
                "acme/widget",
                Some(1)
            ),
            0
        );
        assert_eq!(listed(&home).as_array().map(Vec::len), Some(1));
        // --clear asks GitHub nothing.
        assert_eq!(
            run_in(&home, &asked(12, true), None, "acme/widget", Some(1)),
            0
        );
        assert_eq!(listed(&home), json!([]));
        assert_eq!(
            run_in(&home, &asked(12, true), None, "acme/widget", Some(1)),
            0
        );
    }

    #[test]
    fn an_issue_that_cannot_start_is_refused_and_nothing_is_written() {
        let home = home("refused");
        let unknown = Gh(Err("exit 1".into()), Vec::new());
        let pr = Gh(
            Ok(json!({"number": 12, "state": "open", "pull_request": {"url": "u"}}).to_string()),
            Vec::new(),
        );
        for mut gh in [unknown, pr, issue(12, "closed"), issue(13, "open")] {
            assert_eq!(
                run_in(
                    &home,
                    &asked(12, false),
                    Some(&mut gh),
                    "acme/widget",
                    Some(1)
                ),
                1
            );
        }
        let mut gh = issue(1, "open");
        assert_eq!(
            run_in(
                &home,
                &asked(1, false),
                Some(&mut gh),
                "acme/widget",
                Some(1)
            ),
            1
        );
        assert!(
            gh.1.is_empty(),
            "the control issue is refused before GitHub is asked"
        );
        assert_eq!(listed(&home), json!([]));
    }

    #[test]
    fn the_refusals_say_why() {
        let mut unknown = Gh(Err("exit 1".into()), Vec::new());
        let why = may_start(&mut unknown, "acme/widget", None, 99).unwrap_err();
        assert!(
            why.contains("#99 could not be read from acme/widget"),
            "{why}"
        );
        let why = may_start(&mut issue(12, "closed"), "acme/widget", None, 12).unwrap_err();
        assert!(why.contains("closed"), "{why}");
    }

    #[test]
    fn json_names_the_approval() {
        let a = Asked {
            issue: 12,
            clear: false,
            json: true,
        };
        let v: Value = serde_json::from_str(&answer(&a, true, Some(0), true)).unwrap();
        assert_eq!(
            v,
            json!({"api": 1, "issue": 12, "approved": true, "changed": true,
                "at": "1970-01-01T00:00:00Z"})
        );
    }
}

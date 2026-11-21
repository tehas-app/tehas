//! The product wiki's state (docs/specs/2026-11-21-wiki.md): the current
//! build, the head of the default branch and how far the build is behind it.
//! A feature still being built; nothing here runs the model.
//!
//!   console tehas wiki [--json]   what the state is
//!   console tehasbot wiki         write it to live/wiki.json for the page
//!
//! A build lives in `<home>/wiki/<id>/` and says what it is in its
//! `build.json`; `<home>/wiki/current` is a link to the current one. The
//! state reads them and the product checkout, and runs no model and no call
//! to GitHub.
//!
//! While the feature is off, `tehas wiki` says so and exits 78, and
//! `tehasbot wiki` exits 0 and writes nothing.
//!
//! Exit: 0 the state was printed or written; 1 live/wiki.json could not be
//! written; 64 an argument other than `--json`; 78 the feature is off, or the
//! settings are missing or malformed.

use serde_json::{json, Value};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tehas_core::file::{write_atomic, Temp};

const USAGE: &str = "usage: tehas wiki [--json]";

/// The most bytes of a build's build.json that are read.
const BUILD_BOUND: u64 = 64 * 1024;

/// From this many days on, the age is said in months.
const MONTHS_FROM: i64 = 60;

/// Whether the arguments ask for `--json`, or 64 with the usage.
pub fn words(given: &[String]) -> Result<bool, u8> {
    match given {
        [] => Ok(false),
        [flag] if flag == "--json" => Ok(true),
        _ => {
            eprintln!("{USAGE}");
            Err(64)
        }
    }
}

/// A build's id: its date and the first eight hex of its commit,
/// `2026-11-21-68ffab1d`. None when either is not of its form.
pub fn id(date: &str, commit: &str) -> Option<String> {
    let d = date.as_bytes();
    let date_ok = d.len() == 10
        && d.iter().enumerate().all(|(i, b)| match i {
            4 | 7 => *b == b'-',
            _ => b.is_ascii_digit(),
        });
    let hex = commit.get(..8)?;
    let hex_ok = commit.len() >= 8
        && commit
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    (date_ok && hex_ok).then(|| format!("{date}-{hex}"))
}

/// Whether a name is a build id, and so a directory a build may be in.
pub fn is_id(name: &str) -> bool {
    name.len() == 19
        && id(&name[..10], &name[11..]).as_deref() == Some(name)
        && &name[10..11] == "-"
}

/// How old a build is against main, in words: days, or months from 60 days.
pub fn age(days: i64) -> String {
    match days {
        d if d < 1 => "less than a day".to_string(),
        1 => "1 day".to_string(),
        d if d < MONTHS_FROM => format!("{d} days"),
        d => format!("{} months", d / 30),
    }
}

/// The line the page and `tehas wiki` say of a build against main.
pub fn behind_line(commits: u64, days: i64) -> String {
    if commits == 0 {
        return "up to date with main".to_string();
    }
    let n = if commits == 1 {
        "1 commit".to_string()
    } else {
        format!("{commits} commits")
    };
    format!("{n} and {} behind main", age(days))
}

/// A commit as the state reads it: the hash, its commit date and the same
/// date in seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct Commit {
    pub hash: String,
    pub date: String,
    pub seconds: i64,
}

/// What the state is made from.
#[derive(Debug, Default)]
pub struct Read {
    /// The current build's build.json, when there is one that reads.
    pub current: Option<Value>,
    pub main: Option<Commit>,
    /// The commits in `<build>..main`, when git could count them.
    pub commits: Option<u64>,
    /// The build's commit as git reads it, for its date.
    pub built: Option<Commit>,
}

/// The object of live/wiki.json and of `tehas wiki --json`.
pub fn object(r: &Read) -> Value {
    let main = r
        .main
        .as_ref()
        .map(|m| json!({"commit": m.hash, "commitDate": m.date}));
    let behind = match (r.commits, &r.built, &r.main) {
        (Some(commits), Some(b), Some(m)) => {
            let days = ((m.seconds - b.seconds) / 86_400).max(0);
            json!({"commits": commits, "days": days, "line": behind_line(commits, days)})
        }
        _ => Value::Null,
    };
    json!({
        "api": 1,
        "state": "idle",
        "current": r.current.clone().unwrap_or(Value::Null),
        "main": main.unwrap_or(Value::Null),
        "behind": behind,
    })
}

/// A build's build.json, when it is an object whose id is a build id and
/// whose commit is full hex.
pub fn build_of(text: &str) -> Option<Value> {
    let v: Value = serde_json::from_str(text).ok()?;
    let id = v.get("id")?.as_str()?;
    let commit = v.get("commit")?.as_str()?;
    let full = commit.len() == 40
        && commit
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    (is_id(id) && full && id[11..] == commit[..8]).then_some(v)
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let line = text.trim();
    (out.status.success() && !line.is_empty()).then(|| line.to_string())
}

fn commit(dir: &Path, rev: &str) -> Option<Commit> {
    let line = git(dir, &["log", "-1", "--format=%H %cI %ct", rev, "--"])?;
    let mut parts = line.split(' ');
    let (hash, date, seconds) = (parts.next()?, parts.next()?, parts.next()?);
    Some(Commit {
        hash: hash.to_string(),
        date: date.to_string(),
        seconds: seconds.parse().ok()?,
    })
}

/// Reads the state from the host: `home` is TEHAS_HOME, `checkout` the
/// product checkout and `branch` its default branch.
pub fn read(home: &Path, checkout: &Path, branch: &str) -> Read {
    let current = fs::File::open(home.join("wiki/current/build.json"))
        .ok()
        .and_then(|f| {
            use std::io::Read as _;
            let mut text = String::new();
            f.take(BUILD_BOUND).read_to_string(&mut text).ok()?;
            build_of(&text)
        });
    let main = commit(checkout, &format!("refs/remotes/origin/{branch}"));
    let built_hash = current
        .as_ref()
        .and_then(|c| c.get("commit")?.as_str().map(str::to_string));
    let built = built_hash.as_deref().and_then(|h| commit(checkout, h));
    let commits = match (&built, &main) {
        (Some(b), Some(m)) => git(
            checkout,
            &["rev-list", "--count", &format!("{}..{}", b.hash, m.hash)],
        )
        .and_then(|n| n.parse().ok()),
        _ => None,
    };
    Read {
        current,
        main,
        commits,
        built,
    }
}

fn here() -> (PathBuf, Read) {
    let home = PathBuf::from(env::var_os("TEHAS_HOME").unwrap_or_default());
    let checkout = PathBuf::from(env::var_os("TEHAS_WORK_REPO").unwrap_or_default());
    let branch = tehas_core::settings::branch().unwrap_or_default();
    let read = read(&home, &checkout, &branch);
    (home, read)
}

/// The lines a person is given.
pub fn lines(state: &Value) -> Vec<String> {
    let mut out = Vec::new();
    match state["current"].as_object() {
        Some(c) => {
            let s = |k: &str| c.get(k).and_then(Value::as_str).unwrap_or("?").to_string();
            let commit = s("commit");
            let short = commit.get(..8).unwrap_or(&commit);
            out.push(format!(
                "wiki: {} from {short} ({}), built {}",
                s("id"),
                s("describe"),
                s("builtAt")
            ));
        }
        None => out.push("wiki: no build yet".to_string()),
    }
    if let Some(line) = state["behind"]["line"].as_str() {
        out.push(format!("wiki: {line}"));
    }
    out
}

pub fn main(given: &[String]) -> u8 {
    let json = match words(given) {
        Ok(j) => j,
        Err(rc) => return rc,
    };
    let get = |name: &str| env::var(name).unwrap_or_default();
    if let Err(why) = tehas_core::features::needs("wiki", "tehas wiki", &get) {
        eprintln!("tehas wiki: {why}");
        return tehas_core::exit::SETTINGS;
    }
    let (_, read) = here();
    let state = object(&read);
    if json {
        tehas_core::outln!("{state}");
    } else {
        for line in lines(&state) {
            tehas_core::outln!("{line}");
        }
    }
    0
}

/// `tehasbot wiki`: one pass of the state into live/wiki.json.
pub fn publish(given: &[String]) -> u8 {
    if !given.is_empty() {
        eprintln!("usage: tehasbot wiki");
        return 64;
    }
    if !tehas_core::features::on("wiki") {
        return 0;
    }
    let (home, read) = here();
    let text = format!("{}\n", object(&read));
    let out = home.join("live/wiki.json");
    let written = fs::create_dir_all(home.join("live"))
        .and_then(|()| write_atomic(&out, text.as_bytes(), 0o644, &Temp::Pid));
    match written {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("tehasbot wiki: {} not written: {e}", out.display());
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn w(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn it_takes_json_alone() {
        assert_eq!(words(&[]), Ok(false));
        assert_eq!(words(&w(&["--json"])), Ok(true));
        for bad in [&["build"][..], &["--json", "--json"], &[""]] {
            assert_eq!(words(&w(bad)), Err(64), "{bad:?}");
        }
    }

    #[test]
    fn an_id_is_the_date_and_eight_hex() {
        let hash = "68ffab1d0123456789abcdef0123456789abcdef";
        assert_eq!(
            id("2026-11-21", hash).as_deref(),
            Some("2026-11-21-68ffab1d")
        );
        assert_eq!(
            id("2026-11-21", "68ffab1d").as_deref(),
            Some("2026-11-21-68ffab1d")
        );
        for (date, commit) in [
            ("2026-1-21", hash),
            ("2026/11/21", hash),
            ("2026-11-21", "68FFAB1D"),
            ("2026-11-21", "68ffab1"),
            ("2026-11-21", "../../etc"),
        ] {
            assert_eq!(id(date, commit), None, "{date} {commit}");
        }
        assert!(is_id("2026-11-21-68ffab1d"));
        for bad in [
            "2026-11-21-68ffab1",
            "2026-11-21_68ffab1d",
            "current",
            "2026-11-21-68ffab1d0",
            "",
        ] {
            assert!(!is_id(bad), "{bad}");
        }
    }

    #[test]
    fn the_age_is_days_then_months_from_sixty() {
        assert_eq!(age(0), "less than a day");
        assert_eq!(age(1), "1 day");
        assert_eq!(age(2), "2 days");
        assert_eq!(age(59), "59 days");
        assert_eq!(age(60), "2 months");
        assert_eq!(age(400), "13 months");
    }

    #[test]
    fn the_line_says_commits_and_age() {
        assert_eq!(behind_line(0, 0), "up to date with main");
        assert_eq!(
            behind_line(1, 0),
            "1 commit and less than a day behind main"
        );
        assert_eq!(behind_line(5, 2), "5 commits and 2 days behind main");
        assert_eq!(behind_line(300, 90), "300 commits and 3 months behind main");
    }

    const A: &str = "68ffab1d0123456789abcdef0123456789abcdef";

    fn build(id: &str, commit: &str) -> String {
        json!({"id": id, "commit": commit, "describe": "v1.2.0-5-g68ffab1d", "builtAt": "2026-11-21T03:04:10Z"}).to_string()
    }

    #[test]
    fn a_build_json_must_name_itself() {
        assert!(build_of(&build("2026-11-21-68ffab1d", A)).is_some());
        assert!(build_of(&build("2026-11-21-00000000", A)).is_none());
        assert!(build_of(&build("2026-11-21-68ffab1d", "68ffab1d")).is_none());
        assert!(build_of(&build("../x", A)).is_none());
        assert!(build_of("[]").is_none());
        assert!(build_of("not json").is_none());
    }

    #[test]
    fn on_with_no_build_says_so() {
        let v = object(&Read::default());
        assert_eq!(v["state"], "idle");
        assert_eq!(v["current"], Value::Null);
        assert_eq!(v["behind"], Value::Null);
        assert_eq!(lines(&v), ["wiki: no build yet"]);
    }

    #[test]
    fn behind_is_counted_from_the_commit_dates() {
        let c = |h: &str, s: i64| Commit {
            hash: h.to_string(),
            date: "d".to_string(),
            seconds: s,
        };
        let r = Read {
            current: build_of(&build("2026-11-21-68ffab1d", A)),
            main: Some(c("b", 10 * 86_400 + 5)),
            commits: Some(5),
            built: Some(c(A, 8 * 86_400)),
        };
        let v = object(&r);
        assert_eq!(
            v["behind"],
            json!({"commits": 5, "days": 2, "line": "5 commits and 2 days behind main"})
        );
        assert_eq!(v["main"]["commit"], "b");
        assert_eq!(
            lines(&v),
            [
                "wiki: 2026-11-21-68ffab1d from 68ffab1d (v1.2.0-5-g68ffab1d), built 2026-11-21T03:04:10Z",
                "wiki: 5 commits and 2 days behind main"
            ]
        );
    }

    fn run(dir: &Path, args: &[&str], at: &str) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_AUTHOR_DATE", at)
            .env("GIT_COMMITTER_DATE", at)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    }

    /// A checkout with `n` commits a day apart and origin/main at the last.
    fn checkout(dir: &Path, n: usize) -> Vec<String> {
        fs::create_dir_all(dir).unwrap();
        run(dir, &["init", "-q", "-b", "main"], "2026-01-01T00:00:00Z");
        let mut hashes = Vec::new();
        for i in 0..n {
            let at = format!("2026-01-{:02}T12:00:00Z", i + 1);
            run(
                dir,
                &["commit", "-q", "--allow-empty", "-m", &format!("c{i}")],
                &at,
            );
            hashes.push(git(dir, &["rev-parse", "HEAD"]).unwrap());
        }
        run(
            dir,
            &["update-ref", "refs/remotes/origin/main", "HEAD"],
            "2026-01-01T00:00:00Z",
        );
        hashes
    }

    fn current(home: &Path, hash: &str) {
        let id = id("2026-01-02", hash).unwrap();
        let dir = home.join("wiki").join(&id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("build.json"), build(&id, hash)).unwrap();
        std::os::unix::fs::symlink(&id, home.join("wiki/current")).unwrap();
    }

    #[test]
    fn the_host_read_counts_commits_and_days() {
        let d = tehas_core::testdir::TestDir::new("wiki-read");
        let repo = d.join("repo");
        let hashes = checkout(&repo, 6);
        current(&d.join("home"), &hashes[1]);
        let v = object(&read(&d.join("home"), &repo, "main"));
        assert_eq!(v["current"]["commit"], hashes[1].as_str());
        assert_eq!(v["main"]["commit"], hashes[5].as_str());
        assert_eq!(v["behind"]["commits"], 4);
        assert_eq!(v["behind"]["days"], 4);
    }

    #[test]
    fn a_build_at_main_is_up_to_date() {
        let d = tehas_core::testdir::TestDir::new("wiki-even");
        let repo = d.join("repo");
        let hashes = checkout(&repo, 2);
        current(&d.join("home"), &hashes[1]);
        let v = object(&read(&d.join("home"), &repo, "main"));
        assert_eq!(v["behind"]["line"], "up to date with main");
    }

    #[test]
    fn a_commit_gone_from_the_checkout_leaves_behind_unknown() {
        let d = tehas_core::testdir::TestDir::new("wiki-gone");
        let repo = d.join("repo");
        checkout(&repo, 2);
        current(&d.join("home"), A);
        let v = object(&read(&d.join("home"), &repo, "main"));
        assert_eq!(v["current"]["commit"], A);
        assert_eq!(v["behind"], Value::Null);
    }

    #[test]
    fn no_checkout_and_no_build_read_as_nothing() {
        let d = tehas_core::testdir::TestDir::new("wiki-none");
        let r = read(&d.join("home"), &d.join("none"), "main");
        assert!(r.current.is_none() && r.main.is_none() && r.commits.is_none());
    }
}

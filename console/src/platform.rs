//! What platform this host runs, in live/platform.json, so a host that needs
//! tehas update says so. tehasbot status writes it at every run, and
//! tehasbot health reads it.
//!
//! A host that runs a release ($TEHAS_HOME/current): the installed version and
//! the newest published one, {release, newest, behind: 0, upstream:
//! "releases"}. A host with the platform checkout ($TEHAS_HOME/platform): the
//! commit it is at and how many commits it is behind origin's default branch.
//! A host with neither has no file.
//!
//! Each half reads the outside world at most once an hour, under a time limit,
//! and state/platform-fetch says when it last tried and last succeeded, so a
//! read that hangs or fails is tried again an hour later and changes nothing
//! else. The last list of releases answered is kept in state/platform-newest.

use crate::jq::{obj, J};
use std::cmp::Ordering;
use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;
use tehas_core::call;
use tehas_core::time;

const FETCH_EVERY: i64 = 3600;
const FETCH_LIMIT: u64 = 60;

/// Write live/platform.json, or take it away on a host with no platform.
pub fn write(home: &Path, live: &Path) {
    match installed(home) {
        Some(version) if !version.is_empty() => release(home, live, &version),
        Some(_) => checkout(home, live),
        None => {}
    }
}

/// The version current points at: the first line of its VERSION, empty when
/// current is no link to a version, and None when that file cannot be read.
fn installed(home: &Path) -> Option<String> {
    let link = home.join("current");
    let version = link.join("VERSION");
    let is_link = fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink());
    if !is_link || !version.is_file() {
        return Some(String::new());
    }
    let text = fs::read(&version).ok()?;
    let text = String::from_utf8_lossy(&text);
    Some(text.lines().next().unwrap_or_default().to_string())
}

/// `vX.Y.Z`, the one shape of a release's tag.
fn tag_ok(tag: &str) -> bool {
    let parts = tag
        .strip_prefix('v')
        .map(|t| t.split('.').collect::<Vec<_>>());
    parts.is_some_and(|p| {
        p.len() == 3
            && p.iter()
                .all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// Two tags of the shape above, by their numbers. Digits of any length.
fn by_version(a: &str, b: &str) -> Ordering {
    let number = |n: &str| n.trim_start_matches('0').to_string();
    let parts = |t: &str| {
        t.trim_start_matches('v')
            .split('.')
            .map(number)
            .collect::<Vec<_>>()
    };
    let (x, y) = (parts(a), parts(b));
    x.iter()
        .zip(&y)
        .map(|(p, q)| p.len().cmp(&q.len()).then_with(|| p.cmp(q)))
        .find(|o| o.is_ne())
        .unwrap_or_else(|| a.cmp(b))
}

/// The release list as GitHub answers it, published `vX.Y.Z` tags only: the
/// newest by version, not by date. None when the answer is not such a list,
/// which ends the read as a list that failed.
fn newest_of(answer: &str) -> Option<String> {
    if answer.trim().is_empty() {
        return None;
    }
    let list = J::parse(answer)?;
    let items: Vec<&J> = match &list {
        J::Arr(a) => a.iter().collect(),
        J::Obj(o) => o.iter().map(|(_, v)| v).collect(),
        _ => return None,
    };
    let mut newest: Option<&str> = None;
    for item in items {
        if !matches!(item, J::Obj(_) | J::Null) {
            return None;
        }
        if item.get("draft").truthy() || item.get("prerelease").truthy() {
            continue;
        }
        let tag = item.get("tag_name").as_str()?;
        if !tag_ok(tag) {
            continue;
        }
        if newest.is_none_or(|n| by_version(tag, n).is_ge()) {
            newest = Some(tag);
        }
    }
    newest.map(str::to_string)
}

/// The instance's setting, else the last line of the installed versions.env,
/// read as text. None when neither names owner/name.
fn release_repo(home: &Path) -> Option<String> {
    let repo = env::var("TEHAS_RELEASE_REPO")
        .ok()
        .filter(|r| !r.is_empty())
        .or_else(|| {
            let text = fs::read_to_string(home.join("versions.env")).ok()?;
            text.lines()
                .filter_map(|l| l.strip_prefix("TEHAS_RELEASE_REPO="))
                .next_back()
                .map(str::to_string)
        })?;
    let word = |w: &str| {
        !w.is_empty()
            && w.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
    };
    let (owner, name) = repo.split_once('/')?;
    (word(owner) && word(name)).then_some(repo)
}

fn run(program: &str, args: &[&str]) -> call::Ran {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    call::run(program, &args, Duration::from_secs(FETCH_LIMIT))
}

/// The newest published release, or nothing when GitHub does not answer or
/// names none. gh when it is signed in, which reaches a private repository
/// with the bot's token; the public address otherwise.
fn newest_read(home: &Path) -> Option<String> {
    let repo = release_repo(home)?;
    let path = format!("repos/{repo}/releases?per_page=100");
    let ran = if run("gh", &["auth", "status"]).ok() {
        run("gh", &["api", &path])
    } else {
        let url = format!("https://api.github.com/{path}");
        let accept = "Accept: application/vnd.github+json";
        run("curl", &["-fsSL", "-H", accept, &url])
    };
    ran.ok().then(|| newest_of(&ran.out)).flatten()
}

/// state/platform-fetch: when a read was last tried and when one last
/// worked, each as seconds. 0 and nothing when the file says neither.
fn stamp_read(stamp: &Path) -> (i64, String) {
    let text = fs::read_to_string(stamp).unwrap_or_default();
    let line = text.lines().next().unwrap_or_default().trim();
    let (first, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    let digits = |w: &str| !w.is_empty() && w.bytes().all(|b| b.is_ascii_digit());
    let last = if digits(first) {
        first.parse().unwrap_or(0)
    } else {
        0
    };
    let rest = rest.trim();
    (last, if digits(rest) { rest } else { "" }.to_string())
}

fn stamp_write(stamp: &Path, now: i64, fetched: &str) {
    if let Some(dir) = stamp.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let _ = fs::write(stamp, format!("{now} {fetched}\n"));
}

fn time_of(seconds: &str) -> J {
    match seconds.parse() {
        Ok(t) => J::str(time::iso(t)),
        Err(_) => J::Null,
    }
}

fn put(live: &Path, file: &J) {
    let new = live.join(format!("platform.json.{}.new", std::process::id()));
    let written = fs::write(&new, format!("{}\n", file.pretty()))
        .and_then(|()| fs::rename(&new, live.join("platform.json")));
    if written.is_err() {
        let _ = fs::remove_file(&new);
    }
}

/// A host that runs a release: the version current points at, the newest
/// there is, and nothing of a checkout it may still hold. A cached newest
/// that is no version, or older than the installed one, or beside an installed
/// version that is none, goes, and the next read asks again.
fn release(home: &Path, live: &Path, installed: &str) {
    let stamp = home.join("state/platform-fetch");
    let cache = home.join("state/platform-newest");
    let now = time::now();
    let (last, mut fetched) = stamp_read(&stamp);
    let cached = fs::read_to_string(&cache).unwrap_or_default();
    let mut newest = cached.lines().next().unwrap_or_default().to_string();
    if !newest.is_empty()
        && (!tag_ok(&newest)
            || !tag_ok(installed)
            || (newest != installed && by_version(&newest, installed).is_lt()))
    {
        let _ = fs::remove_file(&cache);
        newest.clear();
    }
    if now - last >= FETCH_EVERY {
        // The attempt is remembered first, so a read that hangs or fails is
        // not tried at every poll.
        stamp_write(&stamp, now, &fetched);
        if let Some(got) = newest_read(home) {
            newest = got;
            fetched = now.to_string();
            let _ = fs::write(&cache, format!("{newest}\n"));
            stamp_write(&stamp, now, &fetched);
        }
    }
    put(
        live,
        &obj(vec![
            ("generatedAt", J::str(time::iso(time::now()))),
            ("release", J::str(installed)),
            (
                "newest",
                if tag_ok(&newest) {
                    J::str(newest)
                } else {
                    J::Null
                },
            ),
            ("behind", J::num(0.0)),
            ("upstream", J::str("releases")),
            ("fetchedAt", time_of(&fetched)),
        ]),
    );
}

fn git(co: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(co)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status.success().then(|| {
        String::from_utf8_lossy(&out.stdout)
            .trim_end_matches('\n')
            .to_string()
    })
}

fn fetched_from_origin(co: &Path) -> bool {
    Command::new("timeout")
        .arg(FETCH_LIMIT.to_string())
        .args(["git", "-C"])
        .arg(co)
        .args(["fetch", "-q", "origin"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// A host on the platform checkout: its commit, and how far it is behind.
fn checkout(home: &Path, live: &Path) {
    let co = home.join("platform");
    if !co.join(".git").exists() {
        let _ = fs::remove_file(live.join("platform.json"));
        return;
    }
    let stamp = home.join("state/platform-fetch");
    let now = time::now();
    let (last, mut fetched) = stamp_read(&stamp);
    if now - last >= FETCH_EVERY {
        stamp_write(&stamp, now, &fetched);
        if fetched_from_origin(&co) {
            fetched = now.to_string();
            stamp_write(&stamp, now, &fetched);
        }
    }
    let branch = |head: &str| git(&co, &["symbolic-ref", "--quiet", "--short", head]);
    let upstream = branch("refs/remotes/origin/HEAD")
        .unwrap_or_else(|| format!("origin/{}", branch("HEAD").unwrap_or_default()));
    let range = format!("HEAD..{upstream}");
    let head = git(&co, &["rev-parse", "--quiet", "--verify", "HEAD"]).unwrap_or_default();
    let behind = git(&co, &["rev-list", "--count", &range]).unwrap_or_default();
    // When the oldest commit main has and this checkout lacks landed there.
    let since = git(
        &co,
        &["log", "--first-parent", "--reverse", "--format=%ct", &range],
    )
    .unwrap_or_default();
    let since = since.lines().next().unwrap_or_default();
    put(
        live,
        &obj(vec![
            ("generatedAt", J::str(time::iso(time::now()))),
            (
                "commit",
                if head.is_empty() {
                    J::Null
                } else {
                    J::str(head)
                },
            ),
            ("upstream", J::str(upstream)),
            (
                "behind",
                behind.parse::<u64>().map_or(J::Null, |n| J::num(n as f64)),
            ),
            ("behindSince", time_of(since)),
            ("fetchedAt", time_of(&fetched)),
        ]),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_is_v_and_three_numbers() {
        for good in ["v0.9.0", "v10.20.30"] {
            assert!(tag_ok(good), "{good}");
        }
        for bad in [
            "0.9.0", "v0.9", "v0.9.0.1", "v0..1", "v0.9.x", "", "v0.9.0\n",
        ] {
            assert!(!tag_ok(bad), "{bad:?}");
        }
    }

    #[test]
    fn versions_compare_as_numbers() {
        assert!(by_version("v0.9.0", "v0.10.0").is_lt());
        assert!(by_version("v1.0.0", "v0.99.99").is_gt());
        assert!(by_version("v0.9.0", "v0.9.0").is_eq());
        assert!(by_version("v0.0.99999999999999999999", "v0.1.0").is_lt());
    }

    #[test]
    fn the_newest_is_by_version_and_leaves_out_what_is_not_published() {
        let rel = |tag: &str, more: &str| format!(r#"{{"tag_name":"{tag}"{more}}}"#);
        let list = format!(
            "[{},{},{},{},{}]",
            rel("v0.9.0", ""),
            rel("v0.10.0", ""),
            rel("v0.11.0", r#","draft":true"#),
            rel("v0.12.0", r#","prerelease":true"#),
            rel("nightly", ""),
        );
        assert_eq!(newest_of(&list).as_deref(), Some("v0.10.0"));
        assert_eq!(newest_of("[]"), None);
        assert_eq!(newest_of(""), None);
        assert_eq!(newest_of("not json"), None);
        assert_eq!(newest_of("7"), None);
        assert_eq!(newest_of("[7]"), None);
        // A draft is passed over before its tag is read.
        assert_eq!(newest_of(r#"[{"draft":true,"tag_name":7}]"#), None);
        assert_eq!(newest_of(r#"[{"tag_name":7}]"#), None);
    }

    #[test]
    fn the_stamp_holds_two_numbers_or_says_nothing() {
        let dir = std::env::temp_dir().join(format!("tehas-stamp-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("stamp");
        let read = |text: &str| {
            fs::write(&f, text).unwrap();
            stamp_read(&f)
        };
        assert_eq!(read("12 7\n"), (12, "7".to_string()));
        assert_eq!(read("12 \n"), (12, String::new()));
        assert_eq!(read("12"), (12, String::new()));
        assert_eq!(read("soon 7\n"), (0, "7".to_string()));
        assert_eq!(read("12 later\n"), (12, String::new()));
        assert_eq!(read("12 7 3\n"), (12, String::new()));
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(stamp_read(&f), (0, String::new()));
    }

    #[test]
    fn the_time_of_a_stamp() {
        assert_eq!(time_of("1"), J::str("1970-01-01T00:00:01Z"));
        assert_eq!(time_of(""), J::Null);
    }
}

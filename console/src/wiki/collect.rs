//! The raw sources of a build, read from the product checkout before the model
//! runs (docs/specs/2026-11-21-wiki.md, "The raw sources"). They are written
//! to `<id>.raw/` and given to the model read-only. A commit whose text looks
//! like a credential is left out whole and named in what this returns.

use super::check::{self, Pages};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::Command;

/// The most commits one build reads, newest first, and the most bytes of the
/// changelog it copies.
pub const MAX_COMMITS: usize = 20_000;
pub const MAX_CHANGELOG: usize = 1024 * 1024;
const MAX_TAGS: usize = 1000;

pub fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The commit a build reads and what it says of itself.
#[derive(Clone, Debug, PartialEq)]
pub struct Head {
    pub commit: String,
    pub date: String,
    pub describe: String,
}

pub fn head(dir: &Path, rev: &str) -> Option<Head> {
    let line = git(dir, &["log", "-1", "--format=%H%x00%cI", rev, "--"])?;
    let (commit, date) = line.trim().split_once('\0')?;
    let describe = git(dir, &["describe", "--tags", "--always", commit])
        .map(|d| d.trim().to_string())
        .unwrap_or_else(|| commit[..8].to_string());
    Some(Head {
        commit: commit.to_string(),
        date: date.to_string(),
        describe,
    })
}

/// Whether `before` is an ancestor of `head`, so a build may read only what
/// came after it.
pub fn follows(dir: &Path, before: &str, head: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["merge-base", "--is-ancestor", before, head])
        .status()
        .is_ok_and(|s| s.success())
}

/// A commit's body without the trailers of its last paragraph.
pub fn untrailed(body: &str) -> String {
    let body = body.trim_end();
    let (head, last) = body.rsplit_once("\n\n").unwrap_or(("", body));
    let trailer = |l: &str| {
        l.split_once(": ").is_some_and(|(k, _)| {
            !k.is_empty()
                && k.len() <= 40
                && k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
    };
    if !last.is_empty() && last.lines().all(trailer) {
        head.trim_end().to_string()
    } else {
        body.to_string()
    }
}

/// One line of log.jsonl for each commit of the range, and the hashes of the
/// commits left out for their text.
pub fn log(dir: &Path, range: &str) -> (Vec<Value>, Vec<String>) {
    let limit = format!("--max-count={MAX_COMMITS}");
    let text = git(
        dir,
        &[
            "log",
            &limit,
            "--format=%H%x00%cI%x00%s%x00%b%x1e",
            range,
            "--",
        ],
    )
    .unwrap_or_default();
    let mut lines = Vec::new();
    let mut dropped = Vec::new();
    for record in text.split('\x1e') {
        let mut f = record.trim_start_matches('\n').splitn(4, '\0');
        let (Some(hash), Some(date), Some(subject), Some(body)) =
            (f.next(), f.next(), f.next(), f.next())
        else {
            continue;
        };
        let body = untrailed(body);
        if check::secret(subject) || check::secret(&body) {
            dropped.push(hash.to_string());
            continue;
        }
        lines.push(json!({"hash": hash, "date": date, "subject": subject, "body": body}));
    }
    (lines, dropped)
}

/// Every version tag, oldest first, with the commits since the tag before.
pub fn tags(dir: &Path) -> Vec<Value> {
    let text = git(
        dir,
        &[
            "for-each-ref",
            "--sort=creatordate",
            "--format=%(refname:short)%00%(creatordate:iso-strict)%00%(objectname)%00%(*objectname)",
            "refs/tags",
        ],
    )
    .unwrap_or_default();
    let mut out = Vec::new();
    let mut before: Option<String> = None;
    for line in text.lines().take(MAX_TAGS) {
        let f: Vec<&str> = line.split('\0').collect();
        let [name, date, object, peeled] = f[..] else {
            continue;
        };
        let commit = if peeled.is_empty() { object } else { peeled };
        let since = match &before {
            Some(b) => git(dir, &["rev-list", "--count", &format!("{b}..{commit}")]),
            None => git(dir, &["rev-list", "--count", commit]),
        }
        .and_then(|n| n.trim().parse::<u64>().ok());
        out.push(json!({"name": name, "date": date, "commit": commit, "commits": since}));
        before = Some(commit.to_string());
    }
    out
}

/// The paths changed between two commits, and the pages whose `sources` name
/// one of them or a directory above one.
pub fn changed(dir: &Path, before: &str, head: &str, pages: &Pages) -> Value {
    let paths: Vec<String> = git(dir, &["diff", "--name-only", before, head])
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    let mut touched = Vec::new();
    for (name, text) in pages {
        let Ok((front, _)) = check::front(text) else {
            continue;
        };
        let hit = front.sources.iter().any(|s| {
            let s = s.trim_end_matches('/');
            paths
                .iter()
                .any(|p| p == s || p.starts_with(&format!("{s}/")))
        });
        if hit {
            touched.push(name.clone());
        }
    }
    json!({"paths": paths, "pages": touched})
}

/// Writes the raw sources for `head` into `raw`. `before` is the commit of the
/// build before, when this build reads only what came after it. Returns the
/// commits left out for their text.
pub fn write(
    dir: &Path,
    raw: &Path,
    head: &Head,
    before: Option<&str>,
    pages: &Pages,
) -> std::io::Result<Vec<String>> {
    fs::create_dir_all(raw)?;
    fs::write(
        raw.join("head.json"),
        json!({"commit": head.commit, "date": head.date, "describe": head.describe, "after": before}).to_string(),
    )?;
    fs::write(raw.join("tags.json"), Value::Array(tags(dir)).to_string())?;
    let range = match before {
        Some(b) => format!("{b}..{}", head.commit),
        None => head.commit.clone(),
    };
    let (lines, dropped) = log(dir, &range);
    let mut text = String::new();
    for l in &lines {
        text.push_str(&l.to_string());
        text.push('\n');
    }
    fs::write(raw.join("log.jsonl"), text)?;
    let changed = match before {
        Some(b) => changed(dir, b, &head.commit, pages),
        None => json!({"paths": null, "pages": null}),
    };
    fs::write(raw.join("changed.json"), changed.to_string())?;
    if let Some(log) = git(dir, &["show", &format!("{}:CHANGELOG.md", head.commit)]) {
        let mut end = log.len().min(MAX_CHANGELOG);
        while !log.is_char_boundary(end) {
            end -= 1;
        }
        fs::write(raw.join("changelog.md"), &log[..end])?;
    }
    Ok(dropped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tehas_core::testdir::TestDir;

    fn run(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
            .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
            .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    }

    fn repo(d: &TestDir) -> std::path::PathBuf {
        let r = d.join("repo");
        fs::create_dir_all(r.join("console/dispatch")).unwrap();
        run(&r, &["init", "-q", "-b", "main"]);
        fs::write(r.join("console/dispatch/a.rs"), "1").unwrap();
        fs::write(r.join("CHANGELOG.md"), "# Changelog\n").unwrap();
        run(&r, &["add", "."]);
        run(
            &r,
            &[
                "commit",
                "-q",
                "-m",
                "First\n\nWhy it is.\n\nCo-Authored-By: someone <x@example.invalid>",
            ],
        );
        run(&r, &["tag", "v1.0.0"]);
        fs::write(r.join("console/dispatch/a.rs"), "2").unwrap();
        run(&r, &["commit", "-qam", "Second"]);
        let token = format!("ghp_{}", "b".repeat(36));
        fs::write(r.join("README"), "x").unwrap();
        run(&r, &["add", "."]);
        run(&r, &["commit", "-q", "-m", &format!("Third with {token}")]);
        r
    }

    #[test]
    fn trailers_go_and_the_rest_stays() {
        assert_eq!(
            untrailed("Why.\n\nCo-Authored-By: a <b>\nSigned-off-by: c"),
            "Why."
        );
        assert_eq!(untrailed("Why.\n\nMore: words that are prose"), "Why.");
        assert_eq!(
            untrailed("Why.\n\nA paragraph\nof prose."),
            "Why.\n\nA paragraph\nof prose."
        );
        assert_eq!(untrailed(""), "");
    }

    #[test]
    fn the_raw_sources_read_the_range_and_leave_a_credential_out() {
        let d = TestDir::new("wiki-collect");
        let r = repo(&d);
        let h = head(&r, "HEAD").unwrap();
        assert!(h.describe.starts_with("v1.0.0-2-g"), "{}", h.describe);
        let raw = d.join("raw");
        let dropped = write(&r, &raw, &h, None, &Pages::new()).unwrap();
        assert_eq!(dropped.len(), 1);
        let log = fs::read_to_string(raw.join("log.jsonl")).unwrap();
        assert_eq!(log.lines().count(), 2);
        assert!(log.contains("\"body\":\"Why it is.\""), "{log}");
        assert!(
            !log.contains("Co-Authored-By") && !log.contains("ghp_"),
            "{log}"
        );
        let tags: Value =
            serde_json::from_str(&fs::read_to_string(raw.join("tags.json")).unwrap()).unwrap();
        assert_eq!(tags[0]["name"], "v1.0.0");
        assert_eq!(tags[0]["commits"], 1);
        assert_eq!(
            fs::read_to_string(raw.join("changelog.md")).unwrap(),
            "# Changelog\n"
        );
    }

    #[test]
    fn a_build_after_another_reads_what_changed_and_the_pages_it_touches() {
        let d = TestDir::new("wiki-changed");
        let r = repo(&d);
        let first = git(&r, &["rev-list", "--max-parents=0", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        let h = head(&r, "HEAD").unwrap();
        assert!(follows(&r, &first, &h.commit));
        assert!(!follows(&r, &h.commit, &first));
        let mut pages = Pages::new();
        pages.insert(
            "modules/dispatch.md".into(),
            "---\ntitle: D\nsources: [console/dispatch/]\nverified_at: 00000000\n---\n".into(),
        );
        pages.insert(
            "modules/other.md".into(),
            "---\ntitle: O\nsources: [console/other]\nverified_at: 00000000\n---\n".into(),
        );
        let v = changed(&r, &first, &h.commit, &pages);
        assert_eq!(v["pages"], json!(["modules/dispatch.md"]));
        assert_eq!(v["paths"], json!(["README", "console/dispatch/a.rs"]));
        let raw = d.join("raw2");
        write(&r, &raw, &h, Some(&first), &pages).unwrap();
        assert_eq!(
            fs::read_to_string(raw.join("log.jsonl"))
                .unwrap()
                .lines()
                .count(),
            1
        );
    }
}

//! The stamp a cycle's report writes where outsource is on: who built the
//! work, on each comment the report posts, as the last line of the pull
//! request's body, and as the `Built-by:` trailer of a contractor's commits.
//! A contractor's cycle wrote its commits on the contractor, so the factory
//! checks the trailer of each commit the cycle added to the pull request's
//! branch when the cycle ends and rewrites the ones that do not name the
//! contractor. What the cycle added is what the branch holds over its head
//! as the cycle started, which the supervisor notes before the job waiter
//! runs (`state/cycles/<issue>.heads`), or over its base for a new branch.
//! Commits an earlier cycle or a person pushed are left as they are.
//!
//! The rewrite is made in a repository of the report's own, never in a tree a
//! container wrote. Each commit keeps its tree, its author and its committer;
//! only its message changes. The push is a lease on the commit the branch was
//! fetched at, so work pushed since is never overwritten.

use crate::host::{self, Runner, Start};
use serde_json::Value;
use std::path::Path;
use std::time::Duration;
use tehas_core::call::{run_full, Ran, GRACE};
use tehas_core::jq::J;
use tehas_core::stamp::{trailer_ok, with_line, with_trailer, Built};

/// One call of git: a fetch or a push is seconds.
const GIT_BOUND: Duration = Duration::from_secs(120);

/// Where the report keeps the repository it rewrites a branch in.
pub const SCRATCH: &str = "state/outsource/stamp";

/// Who built the cycle `s`, or None where outsource is off or the store could
/// not say which job a contractor's cycle was. A cycle run by `tehas work`
/// ran here; one a job waiter waited on ran on the contractor of the issue's
/// newest job since the cycle's approval. A job waiter's cycle is never
/// stamped in-house: with no approval time or no job in the store's answer
/// it carries no stamp.
pub fn built(home: &Path, s: &Start) -> Option<Built> {
    if !tehas_core::features::on("outsource") {
        return None;
    }
    if s.runner == Runner::Work {
        return Some(Built::InHouse);
    }
    let issue = s.issue.to_string();
    let unknown = |why: &str| {
        crate::after::log(&format!(
            "WARN: #{issue}: the store did not say which job built it ({why}): its comments and pull request carry no stamp"
        ));
        None
    };
    let Some(since) = s.approval_at.parse::<i64>().ok().filter(|t| *t > 0) else {
        return unknown("the cycle's approval has no time");
    };
    let since = since.to_string();
    match host::store(home, &["job-stamp", "--issue", &issue, "--since", &since]) {
        Ok(v) => Built::contractor_of(&v).or_else(|| unknown("its answer names no job")),
        Err(word) => unknown(&word),
    }
}

/// The note of the branch heads on GitHub as a contractor's cycle started,
/// `state/cycles/<issue>.heads`, as `git ls-remote --heads` answers.
pub const HEADS: &str = "heads";

/// The branches of a repository at their commits.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Heads(Vec<(String, String)>);

impl Heads {
    /// `git ls-remote` lines; a line that is not a branch at a commit is left
    /// out.
    pub fn of(text: &str) -> Heads {
        Heads(
            text.lines()
                .filter_map(|l| {
                    let (id, name) = l.split_once('\t')?;
                    let branch = name.trim().strip_prefix("refs/heads/")?;
                    (commit_shaped(id) && branch_shaped(branch))
                        .then(|| (branch.to_string(), id.to_string()))
                })
                .collect(),
        )
    }

    /// The commit `branch` was at, or None when it was not there.
    pub fn at(&self, branch: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(b, _)| b == branch)
            .map(|(_, id)| id.as_str())
    }
}

fn heads_path(home: &Path, issue: u64) -> std::path::PathBuf {
    home.join(format!("state/cycles/{issue}.{HEADS}"))
}

/// The heads the supervisor noted as the cycle started, or None when there is
/// no note. Read before the end of the cycle clears it.
pub fn heads(home: &Path, issue: u64) -> Option<Heads> {
    std::fs::read_to_string(heads_path(home, issue))
        .ok()
        .map(|t| Heads::of(&t))
}

/// Notes the branch heads of `repo` on GitHub before a job waiter runs the
/// cycle `issue` elsewhere: the report checks only the commits the cycle
/// added over them. A read that fails leaves no note, and the report then
/// checks no trailer.
pub fn note_heads(home: &Path, issue: u64, repo: &str) {
    let path = heads_path(home, issue);
    let _ = std::fs::remove_file(&path);
    let remote = format!("https://github.com/{repo}.git");
    let r = git(home, &["ls-remote", "--heads", &remote], None, &[]);
    let written = r.ok() && std::fs::write(&path, &r.out).is_ok();
    if !written {
        crate::after::log(&format!(
            "WARN: #{issue}: {}: the Built-by: trailers of its commits will not be checked",
            said("the branch heads could not be noted", &r)
        ));
    }
}

/// A commit's full id.
fn commit_shaped(id: &str) -> bool {
    matches!(id.len(), 40 | 64) && id.bytes().all(|c| c.is_ascii_hexdigit())
}

/// A comment's body with the stamp's line below it.
pub fn stamped(body: &str, built: Option<&Built>) -> String {
    match built {
        Some(b) => format!("{body}\n\n{}", b.line()),
        None => body.to_string(),
    }
}

/// The facts of the analytics' run with the contractor that ran it. A run
/// with no contractor has the facts as they were.
pub fn with_contractor(facts: J, contractor: &str) -> J {
    match facts {
        J::Obj(mut fields) if !contractor.is_empty() => {
            fields.push(("contractor".into(), J::str(contractor)));
            J::Obj(fields)
        }
        facts => facts,
    }
}

/// The pull request a report stamps, as GitHub's REST API answers
/// `repos/<repo>/pulls/<n>`.
#[derive(Clone, Debug, PartialEq)]
pub struct Pull {
    pub number: u64,
    pub body: String,
    /// The branch, and the branch it goes into.
    pub head: String,
    pub base: String,
    /// The branch is one of the repository's own, not a fork's.
    pub own: bool,
    /// Not merged or closed: only then is its branch worth a rewrite.
    pub open: bool,
}

impl Pull {
    pub fn of(v: &Value, repo: &str) -> Option<Pull> {
        let s = |p: &str| v.pointer(p).and_then(Value::as_str);
        let head_repo = s("/head/repo/full_name").unwrap_or("");
        Some(Pull {
            number: v.get("number").and_then(Value::as_u64)?,
            body: s("/body").unwrap_or("").to_string(),
            head: s("/head/ref").filter(|b| branch_shaped(b))?.to_string(),
            base: s("/base/ref").filter(|b| branch_shaped(b))?.to_string(),
            own: head_repo.eq_ignore_ascii_case(repo),
            open: s("/state") == Some("open"),
        })
    }

    /// The body with the stamp's line, when it does not say it already.
    pub fn footed(&self, built: &Built) -> Option<String> {
        let body = with_line(&self.body, built);
        (body != self.body).then_some(body)
    }
}

/// The number of a pull request's URL, `.../pull/<n>`.
pub fn pull_number(url: &str) -> Option<u64> {
    let (rest, n) = url.trim_end_matches('/').rsplit_once('/')?;
    if !rest.ends_with("/pull") || n.is_empty() || n.starts_with('0') || n.len() > 10 {
        return None;
    }
    n.parse().ok()
}

/// A branch name a call of git may be given: no option, no range, no refspec.
fn branch_shaped(b: &str) -> bool {
    !b.is_empty()
        && b.len() <= 255
        && !b.starts_with(['-', '/', '.'])
        && !b.ends_with(['/', '.'])
        && !b.contains("..")
        && !b.contains("//")
        && b.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._/-".contains(&c))
}

/// What the check of a branch's trailers came to.
#[derive(Debug, PartialEq, Eq)]
pub enum Trailers {
    /// Every commit of the branch over its base names the contractor.
    Kept,
    /// This many commits were written again and the branch pushed.
    Rewritten(usize),
}

fn git(dir: &Path, words: &[&str], input: Option<&str>, env: &[(&str, &str)]) -> Ran {
    let mut args: Vec<String> = [
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.fsmonitor=false",
        "-C",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    args.push(dir.to_string_lossy().into_owned());
    args.extend(words.iter().map(|s| s.to_string()));
    let mut all = vec![("GIT_CONFIG_NOSYSTEM", "1"), ("GIT_TERMINAL_PROMPT", "0")];
    all.extend_from_slice(env);
    run_full("git", &args, input, GIT_BOUND, GRACE, &all)
}

fn said(what: &str, r: &Ran) -> String {
    let first = r
        .err
        .lines()
        .chain(r.out.lines())
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("no answer");
    format!("{what}: {first}")
}

/// One commit as `git log -1 --format` gives it.
struct Commit {
    tree: String,
    parents: Vec<String>,
    author: [String; 3],
    committer: [String; 3],
    message: String,
}

const FORMAT: &str = "--format=%T%x00%P%x00%an%x00%ae%x00%ad%x00%cn%x00%ce%x00%cd%x00%B";

fn read_commit(dir: &Path, id: &str) -> Result<Commit, String> {
    let r = git(dir, &["log", "-1", "--date=raw", FORMAT, id], None, &[]);
    if !r.ok() {
        return Err(said(&format!("commit {id} could not be read"), &r));
    }
    let p: Vec<&str> = r.out.splitn(9, '\0').collect();
    if p.len() != 9 {
        return Err(format!("commit {id} could not be read: not its fields"));
    }
    let own = |a: usize| [p[a].to_string(), p[a + 1].to_string(), p[a + 2].to_string()];
    Ok(Commit {
        tree: p[0].to_string(),
        parents: p[1].split_whitespace().map(str::to_string).collect(),
        author: own(2),
        committer: own(5),
        message: p[8].to_string(),
    })
}

/// Checks the `Built-by:` trailer of every commit of `branch` on `remote`
/// over `base` and over `from`, the branch's head as the cycle started (None:
/// the branch is new), and, when one does not name `name`, writes those
/// commits and the ones after them again with the trailer and pushes the
/// branch, leased on the commit it was fetched at. A `from` the branch no
/// longer holds, as after a rebase, leaves every commit over `base` the
/// cycle's. `scratch` is the report's own directory, made anew and removed
/// after.
pub fn fix_trailers(
    scratch: &Path,
    remote: &str,
    base: &str,
    branch: &str,
    from: Option<&str>,
    name: &str,
) -> Result<Trailers, String> {
    if !branch_shaped(base) || !branch_shaped(branch) {
        return Err("the branch or its base is not a branch name".into());
    }
    if from.is_some_and(|f| !commit_shaped(f)) {
        return Err("the branch's head at the start is not a commit".into());
    }
    if tehas_core::stamp::name(name).is_none() {
        return Err("the contractor's name is not a name".into());
    }
    let _ = std::fs::remove_dir_all(scratch);
    if let Err(e) = std::fs::create_dir_all(scratch) {
        return Err(format!("{} could not be made: {e}", scratch.display()));
    }
    let done = rewrite(scratch, remote, base, branch, from, name);
    let _ = std::fs::remove_dir_all(scratch);
    done
}

fn rewrite(
    dir: &Path,
    remote: &str,
    base: &str,
    branch: &str,
    from: Option<&str>,
    name: &str,
) -> Result<Trailers, String> {
    let r = git(dir, &["init", "--quiet", "--bare"], None, &[]);
    if !r.ok() {
        return Err(said("the repository could not be made", &r));
    }
    let base_ref = format!("+refs/heads/{base}:refs/stamp/base");
    let head_ref = format!("+refs/heads/{branch}:refs/stamp/head");
    let r = git(
        dir,
        &[
            "fetch",
            "--quiet",
            "--no-tags",
            remote,
            &base_ref,
            &head_ref,
        ],
        None,
        &[],
    );
    if !r.ok() {
        return Err(said(&format!("{branch} could not be fetched"), &r));
    }
    let r = git(
        dir,
        &["rev-parse", "--verify", "refs/stamp/head"],
        None,
        &[],
    );
    if !r.ok() {
        return Err(said(&format!("{branch} could not be read"), &r));
    }
    let old = r.out.trim().to_string();
    let mut list = vec![
        "rev-list",
        "--reverse",
        "--topo-order",
        "refs/stamp/head",
        "--not",
        "refs/stamp/base",
    ];
    // The fetch brought what the branch and its base hold: a start head that
    // is not here is one the branch no longer holds.
    let held = |id: &str| {
        let obj = format!("{id}^{{commit}}");
        git(dir, &["cat-file", "-e", &obj], None, &[]).ok()
    };
    if let Some(f) = from.filter(|f| held(f)) {
        list.push(f);
    }
    let r = git(dir, &list, None, &[]);
    if !r.ok() {
        return Err(said(
            &format!("the commits of {branch} could not be listed"),
            &r,
        ));
    }
    let ids: Vec<String> = r.out.split_whitespace().map(str::to_string).collect();
    let mut new: std::collections::HashMap<String, String> = Default::default();
    let mut rewritten = 0;
    for id in &ids {
        let c = read_commit(dir, id)?;
        let parents: Vec<String> = c
            .parents
            .iter()
            .map(|p| new.get(p).cloned().unwrap_or_else(|| p.clone()))
            .collect();
        if trailer_ok(&c.message, name) && parents == c.parents {
            continue;
        }
        let message = with_trailer(&c.message, name);
        let mut words = vec!["commit-tree".to_string(), c.tree.clone()];
        for p in &parents {
            words.extend(["-p".to_string(), p.clone()]);
        }
        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        let env = [
            ("GIT_AUTHOR_NAME", c.author[0].as_str()),
            ("GIT_AUTHOR_EMAIL", c.author[1].as_str()),
            ("GIT_AUTHOR_DATE", c.author[2].as_str()),
            ("GIT_COMMITTER_NAME", c.committer[0].as_str()),
            ("GIT_COMMITTER_EMAIL", c.committer[1].as_str()),
            ("GIT_COMMITTER_DATE", c.committer[2].as_str()),
        ];
        let r = git(dir, &words, Some(&message), &env);
        if !r.ok() {
            return Err(said(&format!("commit {id} could not be written again"), &r));
        }
        new.insert(id.clone(), r.out.trim().to_string());
        rewritten += 1;
    }
    let Some(head) = new.get(&old) else {
        return Ok(Trailers::Kept);
    };
    let lease = format!("--force-with-lease=refs/heads/{branch}:{old}");
    let to = format!("{head}:refs/heads/{branch}");
    let r = git(
        dir,
        &["push", "--quiet", "--no-verify", &lease, remote, &to],
        None,
        &[],
    );
    if !r.ok() {
        return Err(said(&format!("{branch} could not be pushed"), &r));
    }
    Ok(Trailers::Rewritten(rewritten))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tehas_core::testdir::TestDir;

    fn box1() -> Built {
        Built::Contractor {
            name: "box-1".into(),
            job: 4,
        }
    }

    #[test]
    fn a_comment_carries_the_line_only_when_there_is_a_stamp() {
        assert_eq!(
            stamped("Tehas cycle ended.", Some(&box1())),
            "Tehas cycle ended.\n\nBuilt by contractor: box-1 (job 4)"
        );
        assert_eq!(
            stamped("Tehas cycle ended.", Some(&Built::InHouse)),
            "Tehas cycle ended.\n\nBuilt by: in-house"
        );
        assert_eq!(stamped("Tehas cycle ended.", None), "Tehas cycle ended.");
    }

    #[test]
    fn the_facts_name_a_contractor_and_stay_as_they_were_otherwise() {
        let facts = || J::parse(r#"{"line":{},"rukas":null}"#).unwrap();
        assert_eq!(
            with_contractor(facts(), "box-1").dump(),
            r#"{"line":{},"rukas":null,"contractor":"box-1"}"#
        );
        assert_eq!(with_contractor(facts(), "").dump(), facts().dump());
    }

    #[test]
    fn the_pull_request_is_read_and_footed_once() {
        let v = json!({"number": 9, "body": "Closes #7.", "state": "open",
                       "head": {"ref": "feature/issue-7-x", "repo": {"full_name": "Acme/Widget"}},
                       "base": {"ref": "main"}});
        let p = Pull::of(&v, "acme/widget").unwrap();
        assert_eq!(
            (p.number, p.head.as_str(), p.base.as_str(), p.own, p.open),
            (9, "feature/issue-7-x", "main", true, true)
        );
        let footed = p.footed(&box1()).unwrap();
        assert_eq!(footed, "Closes #7.\n\nBuilt by contractor: box-1 (job 4)");
        let again = Pull { body: footed, ..p };
        assert_eq!(again.footed(&box1()), None);
        assert!(!Pull::of(&v, "other/widget").unwrap().own);
        let mut bad = v.clone();
        bad["head"]["ref"] = json!("--upload-pack=x");
        assert_eq!(Pull::of(&bad, "acme/widget"), None);
        assert_eq!(pull_number("https://github.com/a/b/pull/12"), Some(12));
        assert_eq!(pull_number("https://github.com/a/b/issues/12"), None);
        assert_eq!(pull_number("https://github.com/a/b/pull/012"), None);
    }

    fn sh(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Ada")
            .env("GIT_AUTHOR_EMAIL", "ada@example.invalid")
            .env("GIT_COMMITTER_NAME", "Ada")
            .env("GIT_COMMITTER_EMAIL", "ada@example.invalid")
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap()
    }

    /// A remote with `main` and a branch of two commits over it: one with no
    /// trailer and one that names another contractor.
    fn remote(d: &TestDir) -> (std::path::PathBuf, String) {
        let remote = d.join("remote.git");
        let work = d.join("work");
        std::fs::create_dir_all(&work).unwrap();
        sh(d, &["init", "--quiet", "--bare", remote.to_str().unwrap()]);
        sh(&work, &["init", "--quiet", "-b", "main"]);
        std::fs::write(work.join("a"), "1").unwrap();
        sh(&work, &["add", "a"]);
        sh(&work, &["commit", "--quiet", "-m", "Base"]);
        sh(&work, &["checkout", "--quiet", "-b", "feature/issue-7-x"]);
        std::fs::write(work.join("a"), "2").unwrap();
        sh(
            &work,
            &["commit", "--quiet", "-am", "First\n\nIt changes a."],
        );
        std::fs::write(work.join("a"), "3").unwrap();
        sh(
            &work,
            &["commit", "--quiet", "-am", "Second\n\nBuilt-by: other"],
        );
        let r = remote.to_str().unwrap();
        sh(&work, &["push", "--quiet", r, "main", "feature/issue-7-x"]);
        let tree = sh(&work, &["rev-parse", "HEAD^{tree}"]);
        (remote, tree.trim().to_string())
    }

    #[test]
    fn a_contractors_wrong_or_missing_trailer_is_rewritten_and_pushed_once() {
        let d = TestDir::new("stamp-trailers");
        let (remote, tree) = remote(&d);
        let r = remote.to_str().unwrap();
        let scratch = d.join("scratch");
        let base = sh(&remote, &["rev-parse", "main"]);
        assert_eq!(
            fix_trailers(&scratch, r, "main", "feature/issue-7-x", None, "box-1"),
            Ok(Trailers::Rewritten(2))
        );
        assert!(!scratch.exists(), "the scratch is removed");
        let log = sh(
            &remote,
            &["log", "--format=%B%x01%an", "main..feature/issue-7-x"],
        );
        let entries: Vec<&str> = log.split('\x01').collect();
        assert_eq!(log.matches("Built-by: box-1").count(), 2, "{log}");
        assert!(!log.contains("Built-by: other"), "{log}");
        assert!(log.contains("It changes a."), "{log}");
        assert!(entries[1].starts_with("Ada"), "{log}");
        // The work is the same: the tree, and main, did not move.
        assert_eq!(
            sh(&remote, &["rev-parse", "feature/issue-7-x^{tree}"]).trim(),
            tree
        );
        assert_eq!(sh(&remote, &["rev-parse", "main"]), base);
        // A second check finds every commit stamped and pushes nothing.
        let head = sh(&remote, &["rev-parse", "feature/issue-7-x"]);
        assert_eq!(
            fix_trailers(&scratch, r, "main", "feature/issue-7-x", None, "box-1"),
            Ok(Trailers::Kept)
        );
        assert_eq!(sh(&remote, &["rev-parse", "feature/issue-7-x"]), head);
    }

    #[test]
    fn the_commits_on_the_branch_before_the_cycle_are_left_as_they_were() {
        let d = TestDir::new("stamp-from");
        let (remote, _) = remote(&d);
        let r = remote.to_str().unwrap();
        // The two commits were there as the contractor's cycle started: an
        // in-house cycle's, say. The contractor adds one.
        let start = sh(&remote, &["rev-parse", "feature/issue-7-x"]);
        let start = start.trim();
        let work = d.join("work");
        std::fs::write(work.join("a"), "4").unwrap();
        sh(&work, &["commit", "--quiet", "-am", "Third"]);
        sh(&work, &["push", "--quiet", r, "feature/issue-7-x"]);
        let scratch = d.join("scratch");
        assert_eq!(
            fix_trailers(
                &scratch,
                r,
                "main",
                "feature/issue-7-x",
                Some(start),
                "box-1"
            ),
            Ok(Trailers::Rewritten(1))
        );
        assert_eq!(
            sh(&remote, &["rev-parse", "feature/issue-7-x^"]).trim(),
            start
        );
        let last = sh(&remote, &["log", "-1", "--format=%B", "feature/issue-7-x"]);
        assert_eq!(last.trim_end(), "Third\n\nBuilt-by: box-1");
        let before = sh(&remote, &["log", "--format=%B", "main..feature/issue-7-x^"]);
        assert!(!before.contains("box-1"), "{before}");
        assert!(before.contains("Built-by: other"), "{before}");
        // A start head the branch no longer holds, as after a rebase the
        // cycle pushed, leaves every commit over the base the contractor's.
        let gone = "f".repeat(40);
        assert_eq!(
            fix_trailers(
                &scratch,
                r,
                "main",
                "feature/issue-7-x",
                Some(&gone),
                "box-1"
            ),
            Ok(Trailers::Rewritten(3))
        );
    }

    #[test]
    fn the_heads_note_names_each_branch_at_its_commit() {
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        let text = format!(
            "{a}\trefs/heads/main\n{b}\trefs/heads/feature/issue-7-x\n{a}\trefs/tags/v1\nnot a line\n"
        );
        let h = Heads::of(&text);
        assert_eq!(h.at("main"), Some(a.as_str()));
        assert_eq!(h.at("feature/issue-7-x"), Some(b.as_str()));
        assert_eq!(h.at("v1"), None);
        assert_eq!(h.at("feature/issue-8"), None);
        let d = TestDir::new("stamp-heads");
        assert!(heads(d.path(), 7).is_none());
        std::fs::create_dir_all(d.join("state/cycles")).unwrap();
        std::fs::write(d.join("state/cycles/7.heads"), &text).unwrap();
        assert_eq!(heads(d.path(), 7).unwrap().at("main"), Some(a.as_str()));
    }

    #[test]
    fn a_branch_or_name_out_of_shape_is_refused_before_git_is_asked() {
        let d = TestDir::new("stamp-shapes");
        let scratch = d.join("scratch");
        for (base, branch, name) in [
            ("main", "--upload-pack=x", "box-1"),
            ("ma..in", "feature/issue-7", "box-1"),
            ("main", "feature/issue-7", "box 1"),
        ] {
            assert!(fix_trailers(&scratch, "/nowhere", base, branch, None, name).is_err());
            assert!(!scratch.exists());
        }
        assert!(fix_trailers(
            &scratch,
            "/nowhere",
            "main",
            "feature/issue-7",
            None,
            "box-1"
        )
        .is_err());
        assert!(!scratch.exists());
        let from = Some("--upload-pack=x");
        assert!(fix_trailers(
            &scratch,
            "/nowhere",
            "main",
            "feature/issue-7",
            from,
            "box-1"
        )
        .is_err());
        assert!(!scratch.exists());
    }
}

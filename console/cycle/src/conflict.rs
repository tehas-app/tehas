//! A refresh whose pull request conflicts with the branch, read before its
//! container starts (#1727).
//!
//! A refresh runs pi-rukas with --continue-pr: it merges the branch into the
//! pull request's head and gives the developer the conflicts. Its develop step
//! then verifies a tree it builds by putting each commit of the worktree's
//! first-parent range back onto the branch's tip. For a continued pull request
//! that range is the pull request's own commits and the merge: the commits
//! conflict on the tip exactly as the merge did, however the developer
//! resolved it, and the cycle stops on `consolidated-verify-conflict`. Every
//! refresh on tehas-tehas from 2026-10-24 to 2026-11-07 ended that way.
//!
//! So an image whose capabilities answer does not set [`VERIFIES_MERGE`] runs
//! no refresh that conflicts: `tehas work` names the files in
//! state/cycles/<issue>.refresh-conflict and exits 78, and the park says them.
//! A merge that is clean, or one git cannot judge for a reason it cannot name
//! (a head the workspace does not hold, a git too old for `merge-tree`), runs
//! as before. A read that failed for a reason tehas can name - it ran out of
//! time, it could not start git, the head of the request is not a commit -
//! stops the refresh with 78: an unjudged refresh is the run this note exists
//! to prevent (#2227).

use serde_json::{json, Value};
use std::process::{Command, Stdio};
use tehas_core::text::disarm;

/// The feature of the capabilities answer that says the image's pi-rukas
/// verifies a continued pull request's merge as it stands.
pub const VERIFIES_MERGE: &str = "continuePrMergeVerify";

/// The note's name after state/cycles/<issue>.
pub const NOTE: &str = "refresh-conflict";

/// The most paths a note names; the rest are counted.
const MOST: usize = 20;

/// The wait of one `merge-tree`, and what `timeout` answers when it ends the
/// read.
const WAIT: &str = "120";

/// What git answered to `merge-tree`, three states, so a read that failed is
/// not read as a merge that is clean.
#[derive(Debug)]
pub enum Judged {
    /// The two merge cleanly, or `head` conflicts with origin/`branch` in these
    /// paths, in git's order: empty for the clean merge, and empty for a git
    /// that judged nothing. `why` is git's own words for judging nothing, and
    /// `judged` says whether the read wrote a tree object at all.
    Answered {
        paths: Vec<String>,
        why: Option<String>,
        judged: bool,
    },
    /// The read failed for a reason tehas can name: the words say it.
    Unreadable(String),
}

impl Judged {
    /// Whether a refresh may run on this answer, and what it was judged from:
    /// the paths of a conflict, nothing for a clean merge or a git that judged
    /// nothing. None when the read itself failed.
    pub fn paths(&self) -> Option<Vec<String>> {
        match self {
            Judged::Answered { paths, .. } => Some(paths.clone()),
            Judged::Unreadable(_) => None,
        }
    }

    /// What the cycle log says of this answer: the reason of a read that
    /// failed, git's own words for a git that judged nothing, or nothing to
    /// say for a merge it did judge.
    pub fn said(&self) -> Option<String> {
        match self {
            Judged::Answered { why, .. } => why.clone(),
            Judged::Unreadable(why) => Some(why.clone()),
        }
    }

    /// The run as before of a git that knows the read no more: the host has no
    /// such read to make, which is its shape and no fault of the request, so the
    /// refresh runs and git's own words go to the log. A git too old for
    /// `merge-tree --write-tree` says so of the option it does not know.
    fn too_old(err: &[u8]) -> Judged {
        Judged::Answered {
            paths: vec![],
            judged: true,
            why: match first(err) {
                said if said.is_empty() => None,
                said => Some(said),
            },
        }
    }

    /// Whether this answer judged the merge at all: a `merge-tree` that wrote no
    /// tree object judged nothing, whatever it said, and no refresh may run on
    /// an answer nobody judged (#2227).
    pub fn judged(&self) -> bool {
        match self {
            Judged::Answered { judged, .. } => *judged,
            Judged::Unreadable(_) => false,
        }
    }
}

/// The first line of what git wrote on stderr, with what the mask of the host
/// hides replaced. Nothing said it: empty.
fn first(err: &[u8]) -> String {
    String::from_utf8_lossy(err)
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(tehas_core::mask::mask)
        .unwrap_or_default()
}

/// A head that names no commit, as the reason names it.
fn not_a_commit(head: &str) -> Judged {
    let said: String = head
        .chars()
        .filter(|c| !c.is_control() && !c.is_whitespace())
        .take(20)
        .collect();
    Judged::Unreadable(format!("the head of the request is not a commit ({said})"))
}

/// The read that `timeout` ended, with git's own words on the end when it
/// wrote any.
fn too_slow(err: &[u8]) -> Judged {
    let read = format!("git did not answer within {WAIT}s");
    match first(err) {
        said if said.is_empty() => Judged::Unreadable(read),
        said => Judged::Unreadable(format!("{read}: {said}")),
    }
}

/// Whether `head` conflicts with origin/`branch` in the git tree `tree`. Only
/// objects are written: no ref, index or file of the tree changes.
///
/// [`Judged::Unreadable`] is for a read that failed for a reason tehas can
/// name: `timeout` ended git (124 or 137), git could not be started, or the
/// head names no commit. A git that answered another status is [`Judged::
/// Answered`] with no paths and `judged` saying whether it judged at all: one
/// too old for `merge-tree --write-tree` judged the host's shape and the
/// refresh runs as before; one that wrote no tree object judged nothing, and no
/// refresh runs on an answer nobody judged (#2227). The cycle log carries git's
/// first line as the reason it was not judged.
pub fn conflicts(tree: &str, branch: &str, head: &str) -> Judged {
    read("timeout", tree, branch, head)
}

/// Start `wait` in front of the `merge-tree` of `tree`, and give back what it
/// answered, or the fault of the start. A script the loader has not finished
/// taking into page cache is still `Text file busy` (os error 26) to an `exec`
/// of it: on a temp tree other tests keep churning that window is wide enough to
/// hit, and the case would read the fault of its own start as a git that could
/// not be started. The start of a busy program is tried again for a short while;
/// a program that is truly not there fails at once, as it must.
fn started(
    wait: &str,
    tree: &str,
    base: &str,
    head: &str,
) -> std::io::Result<std::process::Output> {
    let mut tries = 0;
    loop {
        let started = Command::new(wait)
            .args(["-k", "10", WAIT, "git", "-C", tree, "merge-tree"])
            .args(["--write-tree", "--name-only", "--no-messages", "-z"])
            .args([base, head])
            .stdin(Stdio::null())
            .output();
        match started {
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy && tries < 200 => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            other => return other,
        }
    }
}

/// The read of [`conflicts`] made with the program `wait` in front of git, so
/// a test can put a `timeout` of its own there and name what a read failed on.
fn read(wait: &str, tree: &str, branch: &str, head: &str) -> Judged {
    if !tehas_core::grammar::commit_id(head) {
        return not_a_commit(head);
    }
    let base = format!("refs/remotes/origin/{branch}");
    let started = match started(wait, tree, &base, head) {
        Ok(started) => started,
        Err(why) => {
            let why = why.to_string();
            return Judged::Unreadable(format!(
                "git merge-tree could not be started: {}",
                tehas_core::mask::mask(&why)
            ));
        }
    };
    if matches!(started.status.code(), Some(124) | Some(137)) {
        return too_slow(&started.stderr);
    }
    let text = String::from_utf8_lossy(&started.stdout);
    let mut parts = text.split('\0');
    let tree_id = parts.next().unwrap_or("");
    let wrote_tree = tree_id.len() >= 40 && tree_id.bytes().all(|b| b.is_ascii_hexdigit());
    let paths: Vec<String> = parts.filter(|p| !p.is_empty()).map(String::from).collect();
    // What git was asked of, and what it said of: the branch of the product and
    // the head of the pull request. A read that named a side answered the
    // question of that side, which is what the host is asked to look at; one that
    // named neither did not answer the question at all.
    let named = named_sides(&started.stderr, branch, head);
    match (wrote_tree, started.status.code(), paths.is_empty()) {
        (true, Some(0), true) | (true, Some(1), false) => Judged::Answered {
            paths,
            why: None,
            judged: true,
        },
        // A git too old for the read is the shape of the host and no fault of
        // the request: the refresh runs, as before, with git's words in the log.
        (false, _, _) if knows_merge_tree(&started.stderr) => Judged::too_old(&started.stderr),
        // A read that wrote no tree object judged nothing, whatever it said, and
        // the words say which part of its answer was missing (#2227).
        (false, _, _) => wrote_nothing(&started.stderr, named),
        (true, _, _) => Judged::Unreadable(format!(
            "git wrote a tree object and answered {} with {} file(s) named",
            started.status.code().unwrap_or(-1),
            paths.len()
        )),
    }
}

/// How many of the two sides of the read the words of git name: the branch of
/// the product or the head of the pull request.
fn named_sides(err: &[u8], branch: &str, head: &str) -> usize {
    let said = String::from_utf8_lossy(err).to_ascii_lowercase();
    usize::from(said.contains(&format!("refs/remotes/origin/{branch}")))
        + usize::from(said.contains(&head.to_ascii_lowercase()))
}

/// Whether what git said names the read itself: the words of a git that has no `merge-tree` of its own, or knows
/// `merge-tree --write-tree` no more, or no `merge-tree` at all.
fn knows_merge_tree(err: &[u8]) -> bool {
    let said = String::from_utf8_lossy(err).to_ascii_lowercase();
    // The words of a git that has no such read at all: the usage of a command it
    // holds, or the words that say it holds none. A read of a working git names
    // the side it could not merge and never answers with the usage of the
    // command, so this never lets a read that judged nothing run (#2227).
    said.contains("usage: git merge-tree")
        || said.contains("is not a git command")
        || said.contains("command not found")
        || said.contains("invalid built-in")
        || said.contains("unknown option")
}

/// A read that wrote no tree object judged nothing: no refresh runs on it
/// (#2227), and git's own first line is the reason the log carries. One that
/// said nothing at all is named as the empty answer it is. A read that wrote no
/// tree and named no side judged less than a read that named one, and the
/// reason says so.
fn wrote_nothing(err: &[u8], named: usize) -> Judged {
    let said = match first(err) {
        said if said.is_empty() => String::new(),
        said => format!(": {said}"),
    };
    let read = format!("git wrote no tree object and said nothing of the merge{said}");
    match named {
        // A read that named a side it was asked about answered the question of
        // that side, and the host is told which one: the checkout does not hold
        // it, and that is what the operator is asked to look at.
        1 => Judged::Unreadable(format!(
            "the read of it names one side of the merge: {read}"
        )),
        2 => Judged::Unreadable(format!(
            "the read of it names both sides of the merge: {read}"
        )),
        _ => Judged::Unreadable(read),
    }
}

/// The note of a refresh that ran no cycle: the pull request, the branch and
/// the paths, at most [`MOST`] of them.
pub fn note(pr: u64, branch: &str, paths: &[String]) -> String {
    let named: Vec<&String> = paths.iter().take(MOST).collect();
    json!({"pr": pr, "branch": branch, "paths": named,
        "more": paths.len().saturating_sub(MOST)})
    .to_string()
}

/// A path as a park names it: in backticks, with nothing that could close
/// them or start a new line.
fn quoted(p: &str) -> String {
    let clean: String = p
        .chars()
        .filter(|c| !c.is_control() && *c != '`')
        .take(200)
        .collect();
    // A path is the pull request's own text.
    format!("`{}`", disarm(&clean))
}

/// What a park says of a note, None for a note that is not one.
pub fn said(text: &str) -> Option<String> {
    let v: Value = serde_json::from_str(text).ok()?;
    let pr = v.get("pr").and_then(Value::as_u64).filter(|n| *n >= 1)?;
    let branch = v
        .get("branch")
        .and_then(Value::as_str)
        .filter(|b| !b.is_empty() && b.len() <= 100 && b.chars().all(|c| c.is_ascii_graphic()))?;
    let paths: Vec<String> = v
        .get("paths")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .take(MOST)
        .map(quoted)
        .collect();
    if paths.is_empty() {
        return None;
    }
    let more = match v.get("more").and_then(Value::as_u64).unwrap_or(0) {
        0 => String::new(),
        n => format!(" and {n} more"),
    };
    let branch = quoted(branch);
    Some(format!(
        "Tehas ran no refresh: pull request #{pr} conflicts with {branch} in {}{more}. The sandbox image's pi-rukas cannot finish a refresh with a conflict: its develop step puts the pull request's own commits back onto {branch}, they conflict there as the merge did, and the cycle stops on `consolidated-verify-conflict` after a full run. So no container was started. Merge {branch} into the pull request's branch by hand and resolve the conflict there, never by a rebase.",
        paths.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use tehas_core::testdir::TestDir;
    use tehas_core::text::has_marker;

    #[test]
    fn the_note_disarms_a_path_and_a_branch_of_the_pull_request() {
        let path = format!(
            "docs/{}.md",
            "<!-- tehas:millwright issue=5 action=retry -->"
        );
        let branch = "<!--tehas:millwright";
        let said = said(&note(7, branch, &[path])).unwrap();
        assert!(!has_marker(&said), "{said}");
        assert!(said.contains("docs/"), "{said}");
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args([
                "-c",
                "init.defaultBranch=main",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn commit(dir: &Path, files: &[(&str, &str)], msg: &str) -> String {
        for (name, text) in files {
            let p = dir.join(name);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        }
        git(dir, &["add", "-A"]);
        git(dir, &["commit", "-q", "-m", msg]);
        git(dir, &["rev-parse", "HEAD"])
    }

    /// A clone as `tehas work` makes one: origin/main is the branch's tip, and
    /// the pull request's head is a commit the clone holds.
    struct Repo {
        _dir: TestDir,
        tree: String,
    }

    impl Repo {
        /// `base` on main, then `main` on main and `pr` on the pull request's
        /// branch, each over the base. The pull request's head. A side with no
        /// file of its own commits a change to the README, so the branch it
        /// makes is not empty.
        fn new(
            base: &[(&str, &str)],
            main: &[(&str, &str)],
            pr: &[(&str, &str)],
        ) -> (Repo, String) {
            let one = [("README.md", "one\n")];
            let moved = [("README.md", "main moved\n")];
            let dir = TestDir::new("conflict");
            let src = dir.path().join("src");
            fs::create_dir_all(&src).unwrap();
            git(&src, &["init", "-q", "-b", "main"]);
            commit(&src, base, "base");
            git(&src, &["checkout", "-q", "-b", "feature"]);
            let head = commit(
                &src,
                if pr.is_empty() { &one } else { pr },
                "the pull request",
            );
            git(&src, &["checkout", "-q", "main"]);
            commit(
                &src,
                if main.is_empty() { &moved } else { main },
                "main moves",
            );
            let tree = dir.path().join("tree");
            fs::create_dir_all(&tree).unwrap();
            git(&tree, &["init", "-q", "-b", "main"]);
            git(
                &tree,
                &[
                    "fetch",
                    "-q",
                    "--no-tags",
                    &src.display().to_string(),
                    "+refs/heads/*:refs/remotes/origin/*",
                ],
            );
            git(&tree, &["checkout", "-q", "-B", "main", "origin/main"]);
            let tree = tree.display().to_string();
            (Repo { _dir: dir, tree }, head)
        }
    }

    /// The paths git answered, panicking on a read that failed.
    fn paths(tree: &str, branch: &str, head: &str) -> Vec<String> {
        conflicts(tree, branch, head)
            .paths()
            .expect("the read failed")
    }

    /// The reason of a read that failed, made with the program `wait`.
    fn reason_at(wait: &Path, tree: &str, branch: &str, head: &str) -> String {
        match read(&wait.display().to_string(), tree, branch, head) {
            Judged::Unreadable(why) => why,
            other => panic!("judged after all: {other:?}"),
        }
    }

    /// The reason of a read that failed.
    fn reason(tree: &str, branch: &str, head: &str) -> String {
        match conflicts(tree, branch, head) {
            Judged::Unreadable(why) => why,
            other => panic!("judged after all: {other:?}"),
        }
    }

    /// What a `timeout` of the case answers: the program at `stub` runs with
    /// `body` as its text, and every line of `said` on its error stream. The
    /// words are written out of the script itself, so no quoting of them stands
    /// between the case and what git would have said. The script is made beside
    /// the stub under a name no start has ever run, and moved into place: a
    /// program that is being replaced when the read comes to it is a `timeout`
    /// that was not there at all, and the case would say the wrong thing.
    fn write(stub: &Path, body: &str, said: &str) {
        let mut text = String::from("#!/bin/sh\n");
        for line in said.lines() {
            // The words are written straight into the script, quoted as one word
            // of the shell: a test of the read says what git said, and nothing of
            // how the stub was made.
            let quoted = line.replace('\'', "'\\''");
            text.push_str(&format!("printf '%s\\n' '{quoted}' >&2\n"));
        }
        text.push_str(body);
        // Written beside the stub and moved into place: a program that is being
        // replaced when the read comes to it is one that could not be started.
        let new = PathBuf::from(format!("{}.new", stub.display()));
        tehas_core::testing::write_exe(&new, text).unwrap();
        fs::rename(&new, stub).unwrap();
    }

    #[test]
    fn a_pull_request_that_conflicts_in_one_file_names_that_file() {
        let base = [("a.txt", "one\n"), ("b.txt", "same\n")];
        let (r, head) = Repo::new(&base, &[("a.txt", "main's\n")], &[("a.txt", "the pr's\n")]);
        assert_eq!(paths(&r.tree, "main", &head), vec!["a.txt".to_string()]);
        // Nothing to say of a merge git did judge.
        assert_eq!(conflicts(&r.tree, "main", &head).said(), None);
        // Only objects were written: the tree is as it was made.
        let root = Path::new(&r.tree);
        assert_eq!(git(root, &["status", "--porcelain"]), "");
        assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "main's\n");
    }

    #[test]
    fn a_clean_merge_names_nothing() {
        let base = [("a.txt", "one\n"), ("b.txt", "two\n")];
        let (r, head) = Repo::new(&base, &[("a.txt", "main's\n")], &[("b.txt", "the pr's\n")]);
        let clean = conflicts(&r.tree, "main", &head);
        assert_eq!(clean.paths(), Some(vec![]));
        assert!(clean.judged(), "a merge git judged is judged");
        assert_eq!(clean.said(), None);
    }

    /// The read of a side the workspace does not hold is a read that wrote no
    /// tree object at all, so git answers 1 with its own name for the side: the
    /// merge is judged nothing of, and no refresh runs on it (#2227). The name of
    /// the side is the reason the log carries, and it says which side the host is
    /// asked to look at.
    #[test]
    fn a_side_the_workspace_does_not_hold_is_a_read_that_judged_nothing() {
        let base = [("a.txt", "one\n"), ("b.txt", "two\n")];
        let (r, head) = Repo::new(&base, &[], &[]);
        for (what, branch, want) in [
            ("head", "main", &"e".repeat(40)),
            ("branch", "trunk", &head),
        ] {
            let judged = conflicts(&r.tree, branch, want);
            let why = match judged {
                Judged::Unreadable(ref why) => why.clone(),
                other => panic!("{what} judged after all: {other:?}"),
            };
            assert!(
                why.starts_with(
                    "the read of it names one side of the merge: git wrote no tree object and said nothing of the merge: "
                ),
                "{what}: {why}"
            );
            assert!(!judged.judged(), "{what}: {judged:?}");
            assert_eq!(judged.paths(), None, "{what}");
            // The line holds no line of its own, and names the side git could
            // not merge, which is what the host is asked to look at.
            assert!(!why.contains('\n'), "{what}: {why}");
            assert!(why.contains("not something we can merge"), "{what}: {why}");
        }
    }

    /// A read that wrote no tree object and named neither side answered the
    /// question not at all, and one that said nothing at all is named as the
    /// empty answer it is: no refresh runs on either (#2227).
    #[test]
    fn a_read_that_named_neither_side_is_no_answer_at_all() {
        let base = [("a.txt", "one\n")];
        let (r, head) = Repo::new(&base, &[], &[]);
        let dir = TestDir::new("conflict-silent");
        let stub = dir.path().join("wait");
        // The words name neither side, so the read answered the question of
        // neither: they are no words of a host whose git has no such command.
        write(
            &stub,
            "exit 128\n",
            "fatal: unable to read the object store",
        );
        let why = reason_at(&stub, &r.tree, "main", &head);
        assert!(
            why.starts_with("git wrote no tree object and said nothing of the merge: "),
            "{why}"
        );
        assert!(why.contains("object store"), "{why}");
        // A read that wrote nothing and said nothing at all.
        write(&stub, "exit 128\n", "");
        assert_eq!(
            reason_at(&stub, &r.tree, "main", &head),
            "git wrote no tree object and said nothing of the merge"
        );
    }

    /// A git too old for the read is the shape of the host and no fault of the
    /// request: the refresh runs, as before, with what git said in the log.
    #[test]
    fn a_git_too_old_for_the_read_runs_the_refresh_as_before() {
        let base = [("a.txt", "one\n")];
        let (r, head) = Repo::new(&base, &[], &[]);
        let dir = TestDir::new("conflict-old");
        let stub = dir.path().join("wait");
        // The words of a git that has no such read: the usage of the read of
        // another release, the words that say the command is not there, and the
        // fatal of the release that knows no tree-writing read. The lines are put
        // in the stub's own file, so no quoting of them stands between the case
        // and what git would have said.
        let cases: [(&str, &str); 3] = [
            // The usage of the read itself, which is what a git of another
            // release says first when an option of it is gone.
            (
                "usage: git merge-tree [--write-tree] [<options>] <branch1> <branch2>",
                "129",
            ),
            (
                "error: unknown option --write-tree for git merge-tree",
                "129",
            ),
            ("git: merge-tree is not a git command. See git --help.", "1"),
        ];
        for (said, rc) in cases {
            write(&stub, &format!("exit {rc}\n"), said);
            let judged = read(&stub.display().to_string(), &r.tree, "main", &head);
            assert!(judged.judged(), "{said}: {judged:?}");
            assert_eq!(judged.paths(), Some(vec![]), "{said}");
            assert!(judged.said().unwrap_or_default().contains("git"), "{said}");
        }
    }

    /// A read that wrote a tree object and answered a status that is not an
    /// answer judged too little to trust, and the words say what it answered.
    #[test]
    fn a_read_that_answered_a_status_that_is_no_answer_is_unreadable() {
        let base = [("a.txt", "one\n")];
        let (r, head) = Repo::new(&base, &[], &[]);
        let dir = TestDir::new("conflict-strange");
        let stub = dir.path().join("wait");
        // The tree object git would have written for a merge of nothing with
        // itself, and a status that is neither 0 nor 1: no answer to a question
        // that was asked. The object is read out of the tree once and held.
        // The tree object of the workspace's own tip, and a status that is
        // neither 0 nor 1: no answer to a question that was asked.
        let tree_id = git(Path::new(&r.tree), &["rev-parse", "HEAD^{tree}"]);
        assert!(tree_id.len() >= 40, "no tree object: {tree_id:?}");
        tehas_core::testing::write_exe(
            &stub,
            format!("#!/bin/sh\nprintf '%s\\0' '{tree_id}'\nprintf '%s\\0' 'a.txt'\nexit 3\n"),
        )
        .unwrap();
        let why = reason_at(&stub, &r.tree, "main", &head);
        assert!(
            why.starts_with("git wrote a tree object and answered 3 with 1 file(s) named"),
            "{why}"
        );
    }

    #[test]
    fn a_head_that_is_no_commit_is_unreadable_and_says_itself() {
        let base = [("a.txt", "one\n")];
        let (r, head) = Repo::new(&base, &[], &[]);
        // A head that is not 40 hexadecimal digits never reaches git: the read
        // failed and the reason says what the head was.
        for bad in ["", "HEAD", "not-a-commit", &"z".repeat(40), &head[..39]] {
            let why = reason(&r.tree, "main", bad);
            assert!(
                why.starts_with("the head of the request is not a commit ("),
                "{bad:?}: {why}"
            );
        }
        // What is named of the head is short and holds no line of its own.
        let why = reason(&r.tree, "main", &format!("{}\nsecond line", "a".repeat(60)));
        assert!(!why.contains('\n'), "{why}");
        assert!(why.contains(&"a".repeat(20)), "{why}");
        assert!(!why.contains(&"a".repeat(21)), "{why}");
    }

    /// A `timeout` that ends git, or a git that cannot be started: the read
    /// failed, and the reason says which and carries what git said. The tree is
    /// a real one, so the only fault left is the read itself.
    #[test]
    fn a_read_that_ran_out_of_time_is_unreadable_with_what_git_said() {
        let base = [("a.txt", "one\n")];
        let (r, head) = Repo::new(&base, &[], &[]);
        let dir = TestDir::new("conflict-timeout");
        let stub = dir.path().join("wait");
        for word in ["124", "137"] {
            tehas_core::testing::write_exe(
                &stub,
                format!("#!/bin/sh\nshift 3\nprintf '%s\\n' 'fatal: early EOF' >&2\nexit {word}\n"),
            )
            .unwrap();
            let why = match read(&stub.display().to_string(), &r.tree, "main", &head) {
                Judged::Unreadable(why) => why,
                other => panic!("judged after all: {other:?}"),
            };
            assert!(
                why.starts_with("git did not answer within 120s: "),
                "{word}: {why}"
            );
            assert!(why.contains("early EOF"), "{word}: {why}");
        }
        // A timeout that says nothing is still a read that failed.
        tehas_core::testing::write_exe(&stub, "#!/bin/sh\nexit 124\n").unwrap();
        assert_eq!(
            reason_at(&stub, &r.tree, "main", &head),
            "git did not answer within 120s"
        );
        // A timeout that is not there at all: git never started.
        fs::remove_file(&stub).unwrap();
        let why = reason_at(&stub, &r.tree, "main", &head);
        assert!(
            why.starts_with("git merge-tree could not be started: "),
            "{why}"
        );
    }

    #[test]
    fn the_conflict_of_a_refresh_that_parked_is_named_from_its_two_sides() {
        let path = "console/dispatch/src/text.rs";
        let module = |added: &str| {
            format!(
                "pub fn promise() {{}}\n\n#[cfg(test)]\nmod tests {{\n    #[test]\n    fn a_promise_is_read() {{\n        assert!(true);\n    }}\n{added}\n    #[test]\n    fn the_hash_drops_carriage_returns() {{\n        assert!(true);\n    }}\n}}\n"
            )
        };
        let base = module("");
        let ours = module("\n    #[test]\n    fn a_body_names_every_issue_its_closing_keywords_name() {\n        assert_eq!(closing_numbers(\"Closes #9\"), [9]);\n    }\n");
        let main = module("\n    #[test]\n    fn a_lean_start_needs_the_one_spec_block_of_valid_json() {\n        assert!(has_spec(\"```pi-spec v1\\n{}\\n```\"));\n    }\n");
        let (r, head) = Repo::new(&[(path, &base)], &[(path, &main)], &[(path, &ours)]);
        let paths = paths(&r.tree, "main", &head);
        assert_eq!(paths, [path]);
        let said = said(&note(1453, "main", &paths)).unwrap();
        assert!(
            said.contains(
                "pull request #1453 conflicts with `main` in `console/dispatch/src/text.rs`."
            ),
            "{said}"
        );
        assert!(!said.contains("more"), "{said}");
    }

    #[test]
    fn a_note_names_twenty_paths_and_counts_the_rest() {
        let paths: Vec<String> = (0..23).map(|i| format!("f{i}.rs")).collect();
        let said = said(&note(7, "main", &paths)).unwrap();
        assert!(said.contains("`f19.rs` and 3 more."), "{said}");
        assert!(!said.contains("f20.rs"), "{said}");
    }

    #[test]
    fn a_path_cannot_break_out_of_its_backticks() {
        let said = said(&note(7, "main", &["a`b\nc.rs".to_string()])).unwrap();
        assert!(said.contains("in `abc.rs`."), "{said}");
    }

    #[test]
    fn a_note_that_is_not_one_says_nothing() {
        for bad in [
            "",
            "{}",
            "not json",
            r#"{"pr":0,"branch":"main","paths":["a"]}"#,
            r#"{"pr":7,"branch":"","paths":["a"]}"#,
            r#"{"pr":7,"branch":"a b","paths":["a"]}"#,
            r#"{"pr":7,"branch":"main","paths":[]}"#,
            r#"{"pr":7,"branch":"main"}"#,
        ] {
            assert_eq!(said(bad), None, "{bad}");
        }
    }
}

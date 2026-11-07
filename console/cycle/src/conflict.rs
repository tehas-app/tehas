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
//! A merge that is clean, or one git cannot judge, runs as before.

use serde_json::{json, Value};
use std::process::{Command, Stdio};

/// The feature of the capabilities answer that says the image's pi-rukas
/// verifies a continued pull request's merge as it stands.
pub const VERIFIES_MERGE: &str = "continuePrMergeVerify";

/// The note's name after state/cycles/<issue>.
pub const NOTE: &str = "refresh-conflict";

/// The most paths a note names; the rest are counted.
const MOST: usize = 20;

/// The paths where `head` conflicts with origin/`branch` in the git tree
/// `tree`, in git's order: empty when the two merge cleanly. None when git
/// cannot say, as for a head the tree does not hold or a git too old for
/// `merge-tree --write-tree`. Only objects are written: no ref, index or file
/// of the tree changes.
pub fn conflicts(tree: &str, branch: &str, head: &str) -> Option<Vec<String>> {
    if head.len() != 40 || !head.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let base = format!("refs/remotes/origin/{branch}");
    let out = Command::new("timeout")
        .args(["-k", "10", "120", "git", "-C", tree, "merge-tree"])
        .args(["--write-tree", "--name-only", "--no-messages", "-z"])
        .args([&base, head])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut parts = text.split('\0');
    let tree_id = parts.next()?;
    if tree_id.len() < 40 || !tree_id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let paths: Vec<String> = parts.filter(|p| !p.is_empty()).map(String::from).collect();
    match out.status.code() {
        Some(0) if paths.is_empty() => Some(paths),
        Some(1) if !paths.is_empty() => Some(paths),
        _ => None,
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
    format!("`{clean}`")
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
    use std::path::Path;
    use tehas_core::testdir::TestDir;

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
        /// branch, each over the base. The pull request's head.
        fn new(
            base: &[(&str, &str)],
            main: &[(&str, &str)],
            pr: &[(&str, &str)],
        ) -> (Repo, String) {
            let dir = TestDir::new("conflict");
            let src = dir.path().join("src");
            fs::create_dir_all(&src).unwrap();
            git(&src, &["init", "-q", "-b", "main"]);
            commit(&src, base, "base");
            git(&src, &["checkout", "-q", "-b", "feature"]);
            let head = commit(&src, pr, "the pull request");
            git(&src, &["checkout", "-q", "main"]);
            commit(&src, main, "main moves");
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

    #[test]
    fn a_pull_request_that_conflicts_in_one_file_names_that_file() {
        let base = [("a.txt", "one\n"), ("b.txt", "same\n")];
        let (r, head) = Repo::new(&base, &[("a.txt", "main's\n")], &[("a.txt", "the pr's\n")]);
        assert_eq!(
            conflicts(&r.tree, "main", &head),
            Some(vec!["a.txt".to_string()])
        );
        // Only objects were written: the tree is as it was made.
        let root = Path::new(&r.tree);
        assert_eq!(git(root, &["status", "--porcelain"]), "");
        assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "main's\n");
    }

    #[test]
    fn a_clean_merge_names_nothing_and_an_unknown_head_is_not_judged() {
        let base = [("a.txt", "one\n"), ("b.txt", "two\n")];
        let (r, head) = Repo::new(&base, &[("a.txt", "main's\n")], &[("b.txt", "the pr's\n")]);
        assert_eq!(conflicts(&r.tree, "main", &head), Some(vec![]));
        assert_eq!(conflicts(&r.tree, "main", &"e".repeat(40)), None);
        assert_eq!(conflicts(&r.tree, "main", "HEAD"), None);
        assert_eq!(conflicts(&r.tree, "trunk", &head), None);
    }

    /// The conflict of tehas#1453, refreshed on 2026-11-04: the pull request
    /// (head 7d3cfa1b) and main (1b22da32) each added a test at the end of the
    /// same test module of console/dispatch/src/text.rs. Shortened to the lines
    /// that met.
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
        let paths = conflicts(&r.tree, "main", &head).unwrap();
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

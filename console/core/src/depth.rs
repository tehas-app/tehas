//! How much history a tree of the product is given: TEHAS_CLONE_DEPTH commits
//! back from the tip of the branch, 0 for all of it. And the one way a shallow
//! tree is deepened: until each tip it must hold reaches a commit as the
//! product checkout has it, so `git merge-base` and `--is-ancestor` answer
//! there as in a full clone. pi-rukas reads exit 1 of `--is-ancestor` as a
//! definite no, and on a shallow tree git says 1 for a commit beyond the cut.

use crate::settings::rules;
use std::env;
use std::process::{Command, Stdio};

pub const SETTING: &str = "TEHAS_CLONE_DEPTH";

/// The bound of one fetch into a tree, in seconds: the clone's own.
pub const FETCH_WAIT: &str = "300";

/// The bound of one read of the product checkout, in seconds.
const READ_WAIT: &str = "60";

/// The depth the load left. Err is the line for standard error, which names
/// the setting: an unset or malformed value never falls back to full history.
pub fn from_env() -> Result<u32, String> {
    let dir = env::var("TEHAS_INSTANCE_DIR").unwrap_or_default();
    let file = format!("{}/instance.env", if dir.is_empty() { "?" } else { &dir });
    let Ok(value) = env::var(SETTING) else {
        return Err(format!("tehas: {SETTING} is not set (see {file})"));
    };
    parse(&value).map_err(|why| format!("tehas: {SETTING} is not valid: {why} (see {file})"))
}

/// A value of the setting as the load's rule reads it.
pub fn parse(value: &str) -> Result<u32, String> {
    rules::check(SETTING, value)?;
    value.parse().map_err(|_| format!("{value} is no number"))
}

/// The words of `git fetch` that cut history at `depth` commits: none for 0.
pub fn words(depth: u32) -> Vec<String> {
    match depth {
        0 => Vec::new(),
        n => vec![format!("--depth={n}")],
    }
}

/// What one git call in `dir` printed, under `timeout`, when it succeeded.
fn said(wait: &str, dir: &str, args: &[&str]) -> Option<String> {
    let out = Command::new("timeout")
        .args(["-k", "10", wait, "git", "-C", dir])
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn ancestor(dir: &str, base: &str, tip: &str) -> bool {
    said(READ_WAIT, dir, &["merge-base", "--is-ancestor", base, tip]).is_some()
}

/// The merge base of `a` and `b` in the product checkout `src`, whose history
/// is whole: the commit a pull request's branch started from.
pub fn merge_base(src: &str, a: &str, b: &str) -> Option<String> {
    said(READ_WAIT, src, &["merge-base", a, b]).filter(|s| !s.is_empty())
}

/// Make <tree>, made from <src> by `sandbox::clone` at `depth`, hold each of
/// `tips` and reach `base` from it as <src> does. A tip is a ref of both, such
/// as refs/remotes/origin/main, or a commit of <src>, which the tree gets with
/// no ref. The depth is worked out from <src>'s whole history: a path from a
/// tip to `base` runs through commits of `base..tip` only, so one fetch of one
/// more than the most of those holds every one of them. Not less than `depth`,
/// and no depth at all for a tree of full history. Err says why, for a refusal
/// that names it; the tree is then whatever the fetch left.
pub fn reach(src: &str, tree: &str, tips: &[&str], base: &str, depth: u32) -> Result<(), String> {
    let short = |c: &str| c.chars().take(12).collect::<String>();
    let mut need = depth;
    for tip in tips {
        if !ancestor(src, base, tip) {
            return Err(format!("{src} does not reach {} from {tip}", short(base)));
        }
        let range = format!("{base}..{tip}");
        let count = said(READ_WAIT, src, &["rev-list", "--count", &range])
            .and_then(|n| n.parse::<u32>().ok())
            .ok_or_else(|| format!("the commits of {range} in {src} could not be counted"))?;
        need = need.max(count.saturating_add(1));
    }
    let deep = if depth == 0 { 0 } else { need };
    for tip in tips {
        let spec = if tip.starts_with("refs/") {
            format!("+{tip}:{tip}")
        } else {
            tip.to_string()
        };
        let mut args = vec!["fetch", "-q", "--no-tags"];
        let cut = words(deep);
        args.extend(cut.iter().map(String::as_str));
        args.extend([src, spec.as_str()]);
        if said(FETCH_WAIT, tree, &args).is_none() {
            return Err(format!("{tip} could not be fetched from {src}"));
        }
    }
    for tip in tips {
        if !ancestor(tree, base, tip) {
            return Err(format!(
                "the workspace does not reach {} from {tip} after a fetch of {deep} commits",
                short(base)
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdir::TestDir;
    use std::path::Path;

    #[test]
    fn a_depth_is_the_rule_s_whole_number_and_0_is_full_history() {
        assert_eq!(parse("0"), Ok(0));
        assert_eq!(parse("100"), Ok(100));
        for bad in ["", "-1", "01", "x", "1000000000"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        assert_eq!(words(0), Vec::<String>::new());
        assert_eq!(words(7), vec!["--depth=7".to_string()]);
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args([
                "-c",
                "commit.gpgsign=false",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn commits(dir: &Path, n: usize, tag: &str) -> String {
        for i in 0..n {
            git(
                dir,
                &["commit", "-q", "--allow-empty", "-m", &format!("{tag} {i}")],
            );
        }
        git(dir, &["rev-parse", "HEAD"])
    }

    /// A product checkout at <dir>/src whose origin/main has `old` commits, a
    /// branch `feature` of `pr` commits off them, then `moved` more on main.
    /// The feature's head and its first commit.
    fn product(dir: &Path, old: usize, pr: usize, moved: usize) -> (String, String, String) {
        let up = dir.join("up");
        std::fs::create_dir_all(&up).unwrap();
        git(&up, &["init", "-q", "-b", "main"]);
        let base = commits(&up, old, "old");
        git(&up, &["checkout", "-q", "-b", "feature"]);
        commits(&up, 1, "pr");
        let first = git(&up, &["rev-parse", "HEAD"]);
        let head = commits(&up, pr - 1, "pr");
        git(&up, &["checkout", "-q", "main"]);
        commits(&up, moved, "moved");
        git(dir, &["clone", "-q", &up.display().to_string(), "src"]);
        (base, first, head)
    }

    fn tree_of(dir: &Path, depth: u32) -> String {
        let src = dir.join("src").display().to_string();
        let tree = dir.join("tree").display().to_string();
        assert!(crate::sandbox::clone(&src, &tree, "main", depth));
        tree
    }

    fn count(tree: &str, rev: &str) -> String {
        git(Path::new(tree), &["rev-list", "--count", rev])
    }

    fn ancestor_rc(tree: &str, a: &str, b: &str) -> Option<i32> {
        Command::new("git")
            .args(["-C", tree, "merge-base", "--is-ancestor", a, b])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .code()
    }

    #[test]
    fn a_tree_holds_the_branch_alone_at_its_depth_and_0_holds_all_of_it() {
        let dir = TestDir::new("depth-clone");
        product(&dir, 5, 2, 10);
        let tree = tree_of(&dir, 3);
        assert_eq!(count(&tree, "HEAD"), "3");
        assert_eq!(
            git(Path::new(&tree), &["rev-parse", "--is-shallow-repository"]),
            "true"
        );
        let refs = git(Path::new(&tree), &["for-each-ref", "--format=%(refname)"]);
        assert_eq!(
            refs.lines().collect::<Vec<_>>(),
            [
                "refs/heads/main",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main"
            ]
        );
        let full = TestDir::new("depth-full");
        product(&full, 5, 2, 10);
        let tree = tree_of(&full, 0);
        assert_eq!(count(&tree, "HEAD"), "15");
        assert_eq!(
            git(Path::new(&tree), &["rev-parse", "--is-shallow-repository"]),
            "false"
        );
    }

    #[test]
    fn a_branch_that_started_beyond_the_depth_is_reached_from_both_tips() {
        let dir = TestDir::new("depth-reach");
        let (base, _, head) = product(&dir, 20, 2, 10);
        let tree = tree_of(&dir, 3);
        // As cloned, the tree does not hold the head, and git says no merge base.
        assert_eq!(ancestor_rc(&tree, &head, &head), Some(128));
        let src = dir.join("src").display().to_string();
        let tips = ["refs/remotes/origin/main", head.as_str()];
        let src_base = merge_base(&src, tips[0], &head).unwrap();
        assert_eq!(src_base, base);
        reach(&src, &tree, &tips, &base, 3).unwrap();
        let t = Path::new(&tree);
        assert_eq!(
            git(t, &["merge-base", "refs/remotes/origin/main", &head]),
            base
        );
        assert_eq!(
            ancestor_rc(&tree, &base, "refs/remotes/origin/main"),
            Some(0)
        );
        assert_eq!(ancestor_rc(&tree, &base, &head), Some(0));
        // Deepened to the base, not to the root: the tree is still shallow.
        assert_eq!(git(t, &["rev-parse", "--is-shallow-repository"]), "true");
    }

    #[test]
    fn a_head_reviewed_before_and_far_back_is_an_ancestor_of_the_new_head() {
        let dir = TestDir::new("depth-reviewed");
        let (base, first, head) = product(&dir, 4, 12, 2);
        let tree = tree_of(&dir, 3);
        let src = dir.join("src").display().to_string();
        // Both heads held, each at the depth alone: the path between them is
        // cut, and git answers a definite no.
        let naive = dir.join("naive").display().to_string();
        assert!(crate::sandbox::clone(&src, &naive, "main", 3));
        for commit in [&first, &head] {
            git(
                Path::new(&naive),
                &["fetch", "-q", "--depth=3", &src, commit],
            );
        }
        assert_eq!(ancestor_rc(&naive, &first, &head), Some(1));
        let tips = ["refs/remotes/origin/main", head.as_str()];
        reach(&src, &tree, &tips, &base, 3).unwrap();
        // The pull request's first head is 11 commits behind its last: beyond
        // the depth, and still an ancestor in the tree.
        assert_eq!(ancestor_rc(&tree, &first, &head), Some(0));
        assert_eq!(ancestor_rc(&tree, &head, &first), Some(1));
    }

    #[test]
    fn a_pull_request_that_merged_the_branch_in_is_reached_from_its_last_merge() {
        let dir = TestDir::new("depth-merged");
        let up = dir.join("up");
        std::fs::create_dir_all(&up).unwrap();
        git(&up, &["init", "-q", "-b", "main"]);
        commits(&up, 10, "old");
        git(&up, &["checkout", "-q", "-b", "feature"]);
        let first = commits(&up, 6, "pr");
        git(&up, &["checkout", "-q", "main"]);
        let merged = commits(&up, 8, "moved");
        git(&up, &["checkout", "-q", "feature"]);
        git(&up, &["merge", "-q", "--no-edit", "main"]);
        let head = commits(&up, 3, "pr after");
        git(&up, &["checkout", "-q", "main"]);
        commits(&up, 9, "moved again");
        git(&dir, &["clone", "-q", &up.display().to_string(), "src"]);
        let src = dir.join("src").display().to_string();
        let tip = "refs/remotes/origin/main";
        let base = merge_base(&src, tip, &head).unwrap();
        assert_eq!(base, merged);
        let tree = tree_of(&dir, 2);
        reach(&src, &tree, &[tip, &head], &base, 2).unwrap();
        let t = Path::new(&tree);
        assert_eq!(git(t, &["merge-base", tip, &head]), base);
        // A head before the merge is an ancestor along the branch's own side.
        assert_eq!(ancestor_rc(&tree, &first, &head), Some(0));
    }

    #[test]
    fn a_tree_of_full_history_takes_the_head_with_no_depth() {
        let dir = TestDir::new("depth-reach-full");
        let (base, first, head) = product(&dir, 5, 4, 10);
        let tree = tree_of(&dir, 0);
        let src = dir.join("src").display().to_string();
        reach(&src, &tree, &["refs/remotes/origin/main", &head], &base, 0).unwrap();
        assert_eq!(ancestor_rc(&tree, &first, &head), Some(0));
        assert_eq!(
            git(Path::new(&tree), &["rev-parse", "--is-shallow-repository"]),
            "false"
        );
    }

    #[test]
    fn a_head_the_checkout_does_not_hold_or_that_does_not_reach_the_base_is_refused() {
        let dir = TestDir::new("depth-refused");
        let (base, _, head) = product(&dir, 5, 2, 10);
        let tree = tree_of(&dir, 3);
        let src = dir.join("src").display().to_string();
        let gone = "0123456789abcdef0123456789abcdef01234567";
        let why = reach(&src, &tree, &[gone], &base, 3).unwrap_err();
        assert!(why.contains("does not reach"), "{why}");
        // The tip of main does not reach the pull request's head.
        let why = reach(&src, &tree, &["refs/remotes/origin/main"], &head, 3).unwrap_err();
        assert!(why.contains("does not reach"), "{why}");
    }
}

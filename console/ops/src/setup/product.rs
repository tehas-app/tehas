//! The product checkout setup finds: fetched and fast-forwarded to origin, or
//! left as it is with the reason named. A checkout `tehas state unpack` brought
//! from another host is as old as that host left it, and the gates would run
//! on that. Nothing here stops the run: setup's status is what it was.

use super::{program, status_of, Vars};
use std::process::{Command, Stdio};

/// Seconds for git fetch from origin, as `tehas update` gives its own.
const FETCH_LIMIT: &str = "120";

fn git(vars: &Vars, work: &str) -> Command {
    let mut cmd = program(vars, "git");
    cmd.args(["-C", work]).env("GIT_TERMINAL_PROMPT", "0");
    cmd
}

/// What git said on standard output when it succeeded, or nothing.
fn asked(vars: &Vars, work: &str, words: &[&str]) -> String {
    match git(vars, work)
        .args(words)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    {
        Ok(done) if done.status.success() => String::from_utf8_lossy(&done.stdout)
            .trim_end_matches('\n')
            .to_string(),
        _ => String::new(),
    }
}

fn quiet(cmd: &mut Command) -> u8 {
    status_of(
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
    )
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(12)]
}

fn left(work: &str, why: &str) -> String {
    format!(
        "no fast-forward of the product checkout {work} ({why}); the checkout was left as it is"
    )
}

/// The line to tell. Fast-forward only, on the branch the checkout is on: local
/// work, a rewritten origin or a fetch that cannot run is for a person.
pub(super) fn fast_forward(vars: &Vars, work: &str) -> String {
    // Not a checkout of its own: git would find the one of a directory above.
    if asked(vars, work, &["rev-parse", "--git-dir"]) != ".git" {
        return left(work, "git cannot read it as a checkout");
    }
    let branch = asked(vars, work, &["symbolic-ref", "--quiet", "--short", "HEAD"]);
    if branch.is_empty() {
        return left(work, "it is not on a branch");
    }
    // Full ids: an abbreviation taken either side of the fetch can differ.
    let head = asked(vars, work, &["rev-parse", "--quiet", "--verify", "HEAD"]);
    if head.is_empty() {
        return left(work, &format!("branch {branch} has no commit yet"));
    }
    if !asked(vars, work, &["status", "--porcelain"]).is_empty() {
        return left(work, "it has local changes");
    }
    let mut fetch = program(vars, "timeout");
    fetch
        .args([
            FETCH_LIMIT,
            "git",
            "-C",
            work,
            "fetch",
            "--quiet",
            "--prune",
            "origin",
        ])
        .env("GIT_TERMINAL_PROMPT", "0");
    match quiet(&mut fetch) {
        0 => {}
        124 => {
            return left(
                work,
                &format!("the fetch from origin gave no answer in {FETCH_LIMIT}s"),
            )
        }
        rc => return left(work, &format!("could not fetch from origin, git exit {rc}")),
    }
    let remote_ref = format!("refs/remotes/origin/{branch}");
    let remote = asked(
        vars,
        work,
        &["rev-parse", "--quiet", "--verify", &remote_ref],
    );
    if remote.is_empty() {
        return left(work, &format!("origin has no branch {branch}"));
    }
    if head == remote {
        return format!(
            "the product checkout {work} is up to date at {}",
            short(&head)
        );
    }
    let ancestor =
        quiet(git(vars, work).args(["merge-base", "--is-ancestor", "HEAD", &remote_ref]));
    if ancestor != 0 {
        return left(
            work,
            &format!(
                "it cannot fast-forward to origin/{branch} at {}: it has commits of its own, or origin/{branch} was rewritten",
                short(&remote)
            ),
        );
    }
    if quiet(git(vars, work).args(["merge", "--quiet", "--ff-only", &remote_ref])) != 0 {
        return left(work, "the fast-forward failed");
    }
    format!(
        "fast-forwarded the product checkout {work} {} -> {}",
        short(&head),
        short(&remote)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn vars() -> Vars {
        let path = std::env::var_os("PATH").unwrap().into_encoded_bytes();
        Vars::new([(b"PATH".to_vec(), path)])
    }

    fn run(dir: &Path, words: &[&str]) {
        let done = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args(words)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(done.status.success(), "{words:?}");
    }

    fn head(work: &Path) -> String {
        asked(&vars(), work.to_str().unwrap(), &["rev-parse", "HEAD"])
    }

    /// A clone at `<home>/repos/widget` of a bare origin one commit ahead, and
    /// the tree that pushes to it.
    fn behind(home: &Path) -> (PathBuf, PathBuf) {
        let src = home.join("src");
        let origin = home.join("origin/acme/widget.git");
        let work = home.join("repos/widget");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(home.join("repos")).unwrap();
        run(&src, &["init", "-q", "--initial-branch=main"]);
        fs::write(src.join("a"), "a\n").unwrap();
        run(&src, &["add", "-A"]);
        run(&src, &["commit", "-qm", "one"]);
        let (o, w) = (origin.to_str().unwrap(), work.to_str().unwrap());
        run(home, &["clone", "-q", "--bare", src.to_str().unwrap(), o]);
        run(home, &["clone", "-q", o, w]);
        fs::write(src.join("a"), "b\n").unwrap();
        run(&src, &["commit", "-qam", "two"]);
        run(&src, &["push", "-q", o, "main"]);
        (work, src)
    }

    fn scratch(name: &str) -> tehas_core::testdir::TestDir {
        tehas_core::testdir::TestDir::new(&format!("setup-product-{name}"))
    }

    #[test]
    fn a_checkout_behind_origin_moves_to_its_head_and_a_second_run_changes_nothing() {
        let d = scratch("behind");
        let (work, src) = behind(d.path());
        let w = work.to_str().unwrap();
        let (from, to) = (head(&work), head(&src));
        assert_ne!(from, to);
        assert_eq!(
            fast_forward(&vars(), w),
            format!(
                "fast-forwarded the product checkout {w} {} -> {}",
                &from[..12],
                &to[..12]
            )
        );
        assert_eq!(head(&work), to);
        let count = asked(&vars(), w, &["rev-list", "--count", "--all"]);
        assert_eq!(
            fast_forward(&vars(), w),
            format!("the product checkout {w} is up to date at {}", &to[..12])
        );
        assert_eq!(head(&work), to);
        assert_eq!(asked(&vars(), w, &["rev-list", "--count", "--all"]), count);
    }

    #[test]
    fn a_checkout_with_commits_of_its_own_is_left_and_named() {
        let d = scratch("own");
        let (work, _) = behind(d.path());
        run(&work, &["commit", "-q", "--allow-empty", "-m", "local"]);
        let before = head(&work);
        let w = work.to_str().unwrap();
        let said = fast_forward(&vars(), w);
        assert!(
            said.starts_with(&format!("no fast-forward of the product checkout {w} (it cannot fast-forward to origin/main at ")),
            "{said}"
        );
        assert!(said.ends_with("; the checkout was left as it is"), "{said}");
        assert_eq!(head(&work), before);
    }

    #[test]
    fn local_changes_a_detached_head_and_a_fetch_that_fails_are_left() {
        let d = scratch("left");
        let (work, _) = behind(d.path());
        let w = work.to_str().unwrap();
        let before = head(&work);
        fs::write(work.join("a"), "edited\n").unwrap();
        assert_eq!(fast_forward(&vars(), w), left(w, "it has local changes"));
        run(&work, &["checkout", "-q", "--", "a"]);
        run(&work, &["checkout", "-q", "--detach"]);
        assert_eq!(fast_forward(&vars(), w), left(w, "it is not on a branch"));
        run(&work, &["checkout", "-q", "main"]);
        run(&work, &["remote", "set-url", "origin", "/no/such/origin"]);
        assert!(fast_forward(&vars(), w).contains("(could not fetch from origin, git exit "));
        assert_eq!(head(&work), before);
        let none = d.join("none");
        fs::create_dir_all(none.join(".git")).unwrap();
        let n = none.to_str().unwrap();
        assert_eq!(
            fast_forward(&vars(), n),
            left(n, "git cannot read it as a checkout")
        );
    }
}

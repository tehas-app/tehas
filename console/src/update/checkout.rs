//! The git half of `tehas update`: fast-forward the platform checkout, for a
//! host that develops tehas. It changes nothing when the checkout has local
//! changes or cannot be fast-forwarded.

use super::{output, say, tell, timed, Host, Mode, Plan};
use crate::store::merged;
use std::process::{Command, Stdio};

/// Seconds for git fetch from origin.
const FETCH_LIMIT: u32 = 120;

/// The rules of the code just pulled, not of the one that was installed, asked
/// before anything of the host is touched. A library too old to answer leaves it
/// to install.sh, which checks the settings itself.
const CHECK_PULLED: &str = r#"
if [ ! -r "$1/lib/config.sh" ]; then
  echo "this checkout has no lib/config.sh; install.sh will report it"
  exit 0
fi
. "$1/lib/config.sh"
if [ "$(type -t tehas_load 2>/dev/null)" != function ]; then
  echo "this checkout does not define tehas_load; install.sh will check the settings itself"
  exit 0
fi
if [ "$(type -t tehas_require_instance_settings 2>/dev/null)" != function ]; then
  echo "this checkout does not define tehas_require_instance_settings; install.sh will check the settings itself"
  exit 0
fi
tehas_load && tehas_require_instance_settings
"#;

fn git(checkout: &str) -> Command {
    let mut cmd = Command::new("git");
    cmd.args(["-C", checkout]);
    cmd
}

/// A question git answers on standard output, or nothing.
fn asked(checkout: &str, words: &[&str]) -> String {
    let (rc, said) = output(git(checkout).args(words).stderr(Stdio::null()));
    if rc == 0 {
        said
    } else {
        String::new()
    }
}

fn indented(said: &str) {
    for line in said.split('\n') {
        tell(&format!("  {line}"));
    }
}

/// Twelve of each for the lines an operator reads: long enough to tell two
/// commits apart, short enough to fit the message.
fn short(sha: &str) -> &str {
    &sha[..sha.len().min(12)]
}

fn branch_note(checkout: &str, branch: &str, default: &str) {
    let default = default.strip_prefix("origin/").unwrap_or(default);
    if !default.is_empty() && default != branch {
        tell(&format!(
            "checkout: {checkout} is on branch {branch}; the default branch of origin is {default}. This update uses {branch}."
        ));
    }
}

/// `checkout` at the remote's commit, or a refusal. 0 goes on to install.sh.
pub(crate) fn pull(host: &Host, plan: &mut Plan) -> u8 {
    let tree = host.checkout().display().to_string();
    let checkout = tree.as_str();
    plan.tree = tree.clone();
    // A branch, and nothing uncommitted: an update that carried on past either one
    // would merge into a tree somebody is editing.
    let branch = asked(checkout, &["symbolic-ref", "--quiet", "--short", "HEAD"]);
    if branch.is_empty() {
        tell(&format!(
            "checkout: {checkout} is not on a branch (detached HEAD); nothing was changed"
        ));
        return 1;
    }
    // Full commit ids: two calls either side of a fetch can abbreviate one commit
    // differently, and a comparison of those would compare two different strings.
    let head = asked(checkout, &["rev-parse", "--quiet", "--verify", "HEAD"]);
    if head.is_empty() {
        tell(&format!(
            "checkout: {checkout} is on branch {branch} with no commit yet; nothing was changed"
        ));
        return 1;
    }
    let default = asked(
        checkout,
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    );
    branch_note(checkout, &branch, &default);

    let (rc, changes) = output(git(checkout).args(["status", "--porcelain"]));
    if rc != 0 {
        tell(&format!(
            "checkout: git status failed in {checkout}; nothing was changed"
        ));
        return 1;
    }
    if !changes.is_empty() {
        tell(&format!(
            "checkout: {checkout} has local changes; nothing was changed"
        ));
        indented(&changes);
        return 1;
    }

    let (rc, said) =
        merged(timed(FETCH_LIMIT, "git").args(["-C", checkout, "fetch", "--prune", "origin"]));
    if rc != 0 {
        tell("checkout: could not fetch from origin; nothing was changed");
        if rc == 124 {
            tell(&format!("  the fetch gave no answer in {FETCH_LIMIT}s"));
        }
        let said = said.trim_end_matches('\n');
        if !said.is_empty() {
            indented(said);
        }
        return 1;
    }

    let remote_ref = format!("refs/remotes/origin/{branch}");
    let remote = asked(checkout, &["rev-parse", "--quiet", "--verify", &remote_ref]);
    if remote.is_empty() {
        tell(&format!(
            "checkout: origin has no branch {branch} in {checkout}; nothing was changed"
        ));
        return 1;
    }
    if head == remote {
        say(&format!(
            "checkout: {branch} is up to date at {}",
            short(&head)
        ));
        return 0;
    }
    // Fast-forward only. Local commits of its own, or a branch that was rewritten
    // on the remote, are a question for a person, not for an update.
    let ancestor = git(checkout)
        .args(["merge-base", "--is-ancestor", "HEAD", &remote_ref])
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if !ancestor {
        tell(&format!(
            "checkout: {checkout} cannot fast-forward to origin/{branch} ({}); nothing was changed",
            short(&remote)
        ));
        tell(&format!(
            "  the checkout has commits of its own, or origin/{branch} was rewritten. Reconcile it by hand."
        ));
        return 1;
    }
    let (_, behind) =
        output(git(checkout).args(["rev-list", "--count", &format!("HEAD..{remote_ref}")]));
    let (from, to) = (short(&head), short(&remote));
    if plan.mode == Mode::Check {
        say(&format!(
            "checkout: would fast-forward {from} -> {to} ({behind} commits)"
        ));
        plan.found = true;
        return 0;
    }
    let (rc, said) = merged(git(checkout).args(["merge", "--ff-only", &remote_ref]));
    if rc != 0 {
        tell("checkout: the fast-forward failed; nothing was installed");
        let said = said.trim_end_matches('\n');
        if !said.is_empty() {
            indented(said);
        }
        return 1;
    }
    say(&format!(
        "checkout: fast-forwarded {from} -> {to} ({behind} commits)"
    ));
    let (rc, said) = merged(Command::new("bash").args(["-c", CHECK_PULLED, "_", checkout]));
    let said = said.trim_end_matches('\n');
    if rc != 0 {
        tell("settings: the checkout this update pulled asks for settings this instance does not have; nothing was installed");
        if !said.is_empty() {
            indented(said);
        }
        tell(&format!(
            "  {checkout} is at the new code; fix the instance settings, or reset the checkout to what was installed before."
        ));
        return 78;
    }
    if !said.is_empty() {
        say(&format!("settings: {said}"));
    }
    0
}

#[cfg(test)]
mod tests {
    use super::super::testing::dir;
    use super::*;
    use std::path::{Path, PathBuf};

    fn host(home: &Path) -> Host {
        Host {
            home: home.to_path_buf(),
            image: String::new(),
        }
    }

    fn run(dir: &Path, words: &[&str]) {
        let done = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args(words)
            .output()
            .unwrap();
        assert!(done.status.success(), "{words:?}");
    }

    // A checkout at <home>/platform with a commit of its own, cloned from a bare
    // origin one commit ahead.
    fn behind(home: &Path) -> PathBuf {
        let work = home.join("work");
        let origin = home.join("origin");
        let checkout = home.join("platform");
        std::fs::create_dir_all(&work).unwrap();
        run(&work, &["init", "-q", "--initial-branch=main"]);
        std::fs::write(work.join("a"), "a\n").unwrap();
        run(&work, &["add", "-A"]);
        run(&work, &["commit", "-qm", "one"]);
        run(
            home,
            &[
                "clone",
                "-q",
                "--bare",
                work.to_str().unwrap(),
                origin.to_str().unwrap(),
            ],
        );
        run(
            home,
            &[
                "clone",
                "-q",
                origin.to_str().unwrap(),
                checkout.to_str().unwrap(),
            ],
        );
        std::fs::write(work.join("a"), "b\n").unwrap();
        run(&work, &["commit", "-qam", "two"]);
        run(&work, &["push", "-q", origin.to_str().unwrap(), "main"]);
        checkout
    }

    fn head(checkout: &Path) -> String {
        asked(checkout.to_str().unwrap(), &["rev-parse", "HEAD"])
    }

    #[test]
    fn check_says_what_would_move_and_moves_nothing() {
        let d = dir("pull-check");
        let checkout = behind(&d.0);
        let before = head(&checkout);
        let mut plan = Plan {
            mode: Mode::Check,
            ..Plan::default()
        };
        assert_eq!(pull(&host(&d.0), &mut plan), 0);
        assert!(plan.found);
        assert_eq!(head(&checkout), before);
        assert_eq!(plan.tree, checkout.display().to_string());
    }

    #[test]
    fn local_changes_stop_it_before_the_fetch() {
        let d = dir("pull-dirty");
        let checkout = behind(&d.0);
        std::fs::write(checkout.join("new"), "x\n").unwrap();
        let before = head(&checkout);
        let mut plan = Plan::default();
        assert_eq!(pull(&host(&d.0), &mut plan), 1);
        assert_eq!(head(&checkout), before);
    }

    #[test]
    fn a_detached_checkout_is_refused() {
        let d = dir("pull-detached");
        let checkout = behind(&d.0);
        run(&checkout, &["checkout", "-q", "--detach", "HEAD"]);
        assert_eq!(pull(&host(&d.0), &mut Plan::default()), 1);
    }

    #[test]
    fn the_short_form_of_a_commit_is_twelve_digits() {
        assert_eq!(short("0123456789abcdef"), "0123456789ab");
        assert_eq!(short("abc"), "abc");
    }
}

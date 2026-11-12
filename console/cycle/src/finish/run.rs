//! The run: what the kept workspace holds, the rebase of its branch in a
//! repository of the run's own, the push of a new branch, and the draft pull
//! request.
//!
//! The rules, in the order they stop a run:
//!
//! 1. the workspace is there and is a repository, and holds the issue's branch;
//! 2. nothing of it is uncommitted, because nobody has checked work that was
//!    never committed;
//! 3. the remote holds the default branch, and the branch has a commit over it;
//! 4. the name the work would be pushed to is not on the remote, and the issue
//!    has no open pull request;
//! 5. the rebase of the branch's commits onto the remote's default branch is
//!    clean and leaves a commit;
//! 6. the push of the new name is accepted, and GitHub answers for the draft.
//!
//! No git runs in the workspace. A cycle's container wrote all of it, its
//! `.git/config` included, and git honours that file's programs (filters, a
//! signing program, a credential helper, an ssh command) and its remote URL in
//! every command run there. The run makes a repository of its own in its
//! scratch and takes the workspace's commits by `git fetch <workspace>`: the
//! serving side of that fetch is `git upload-pack`, which git keeps safe to run
//! on a repository nobody trusts. The working tree is read by a `status` of the
//! run's repository with the workspace as its work tree. The push goes to the
//! plan's remote, never to a URL the workspace names. The workspace is read and
//! never written, so the branch the cycle left stands where the cycle left it.

use super::cmd::{asked, dir_exists, Done, Host, Scratch};
use super::plan::{body, branch_shaped, title, Finished, Plan};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Where the run's repository keeps the workspace's refs: its branches under
/// `refs/kept/heads/` and its `HEAD` as `refs/kept/HEAD`.
const KEPT_HEADS: &str = "refs/kept/heads/";
const KEPT_HEAD: &str = "refs/kept/HEAD";

/// The most of the workspace's `info/exclude` the run copies.
const EXCLUDE_MAX: u64 = 1 << 16;

/// A step the run could not do: the rule's own sentence, which the issue is
/// told, and what the command said, which only the log keeps.
pub struct Why {
    pub(super) why: String,
    pub(super) said: String,
}

impl Why {
    pub(super) fn new(why: impl Into<String>) -> Why {
        Why {
            why: why.into(),
            said: String::new(),
        }
    }

    /// The command's first line, cut to 300 characters, kept apart from the
    /// rule: on a conflict it quotes a commit subject the cycle wrote. The rule
    /// takes only the words of `end_fault::words`, which say whether GitHub
    /// failed.
    pub(super) fn said(mut self, d: &Done) -> Why {
        if let Some(w) = d.said() {
            let words = tehas_core::end_fault::words(&w);
            if !words.is_empty() {
                self.why = format!("{} ({words})", self.why);
            }
            self.said = w.chars().take(300).collect();
        }
        self
    }

    /// A refusal of a rule that needs no command's words.
    fn only(why: impl Into<String>) -> Why {
        Why::new(why)
    }
}

trait Say {
    fn let_say(self, why: &str) -> Result<String, Why>;
}

impl Say for Done {
    fn let_say(self, why: &str) -> Result<String, Why> {
        if !self.ok() {
            return Err(Why::new(why).said(&self));
        }
        Ok(self.out)
    }
}

/// `git -C <repo>` and then `args`: every git call of a run is asked of the run's
/// own repository. The words go through [`super::cmd::asked`], so hooks and
/// fsmonitor are pinned off on every one of them as well.
fn git(h: &mut dyn Host, repo: &Path, args: &[&str]) -> Done {
    let all: Vec<String> = std::iter::once("-C".to_string())
        .chain(std::iter::once(repo.display().to_string()))
        .chain(args.iter().map(|s| s.to_string()))
        .collect();
    h.run("git", &asked(all), repo)
}

/// `gh` with these arguments, asked of the home the run was given.
fn gh(h: &mut dyn Host, home: &Path, args: &[&str]) -> Done {
    let all: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    h.run("gh", &all, home)
}

/// The run: everything the rules of this file decide, and the scratch it takes
/// back with it.
pub fn run(plan: &Plan, h: &mut dyn Host) -> Finished {
    let workspace = plan.workspace();
    if !dir_exists(&workspace) {
        return Finished::refused(format!("{} is not a directory", workspace.display()));
    }
    let scratch = Scratch::new(
        plan.scratch_parent(),
        format!("{}-{}", plan.workspace_name(), plan.issue),
    );
    let dir = match scratch.dir() {
        Ok(d) => d,
        Err(e) => return Finished::refused(e),
    };
    let outcome = in_scratch(plan, h, &workspace, &dir, &scratch);
    // Whatever the run did, its repository goes with it: a tree the rebase left
    // mid flight is the run's own and belongs to no one else.
    h.clean(&dir);
    match outcome {
        Ok(url) => Finished {
            url,
            why: String::new(),
            said: String::new(),
        },
        Err(why) => Finished {
            said: why.said,
            ..Finished::refused(why.why)
        },
    }
}

/// Rules 1 to 6, asked of the run's repository under `dir`.
fn in_scratch(
    plan: &Plan,
    h: &mut dyn Host,
    workspace: &Path,
    dir: &Path,
    scratch: &Scratch,
) -> Result<String, Why> {
    let repo = taken(h, workspace, dir)?;
    let branch = issue_branch_of(
        &git(
            h,
            &repo,
            &["for-each-ref", "--format=%(refname:lstrip=3)", KEPT_HEADS],
        )
        .let_say("its branches could not be read")?,
        plan.issue,
    )
    .ok_or_else(|| {
        Why::only(format!(
            "{} holds no branch of issue #{}",
            workspace.display(),
            plan.issue
        ))
    })?;
    let pushed = plan.branch();
    preflight(plan, h, workspace, &repo, &branch, &pushed)?;
    rebase_push_open(plan, h, &repo, &branch, &pushed, scratch)
}

/// The run's repository, `<dir>/repo`, holding the workspace's branches and
/// `HEAD` under `refs/kept/` and its `info/exclude`, with `origin` the plan's
/// remote. Nothing of the workspace's config comes with them.
fn taken(h: &mut dyn Host, workspace: &Path, dir: &Path) -> Result<PathBuf, Why> {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).map_err(|e| Why::only(format!("{}: {e}", repo.display())))?;
    git(h, &repo, &["init", "--quiet"]).let_say("the run's repository could not be made")?;
    let from = workspace.display().to_string();
    let heads = format!("+refs/heads/*:{KEPT_HEADS}*");
    let head = format!("+HEAD:{KEPT_HEAD}");
    git(
        h,
        &repo,
        &["fetch", "--quiet", "--no-tags", &from, &head, &heads],
    )
    .let_say(&format!("{} is no git repository", workspace.display()))?;
    copy_exclude(workspace, &repo);
    Ok(repo)
}

/// The workspace's `.git/info/exclude` copied as text into the run's
/// repository, so the files the cycle kept out of git are not read as
/// uncommitted work. A link or anything but a plain file is not followed.
fn copy_exclude(workspace: &Path, repo: &Path) {
    let git_dir = workspace.join(".git");
    if !dir_exists(&git_dir) || !dir_exists(&git_dir.join("info")) {
        return;
    }
    let Ok((text, _)) = tehas_core::file::read_regular(&git_dir.join("info/exclude"), EXCLUDE_MAX)
    else {
        return;
    };
    let to = repo.join(".git/info");
    if std::fs::create_dir_all(&to).is_ok() {
        let _ = std::fs::write(to.join("exclude"), text);
    }
}

/// The kept workspace's branch: `feature/issue-<n>`, with the optional slug
/// pi-rukas gives it, as the grammar of src/finish/plan.rs reads it. The first
/// name of a `for-each-ref` answer that is the issue's. A name that is no branch
/// name is not the issue's branch, so a `feature/issue-7-` or a name of another
/// issue's number is not taken for it.
pub fn issue_branch_of(names: &str, n: u64) -> Option<String> {
    let want = format!("feature/issue-{n}");
    let slug = format!("{want}-");
    names
        .lines()
        .map(str::trim_end)
        .find(|l| *l == want || (l.starts_with(&slug) && branch_shaped(l)))
        .map(str::to_string)
}

/// Everything checked before the branch is checked out: rules 2 to 4.
fn preflight(
    plan: &Plan,
    h: &mut dyn Host,
    workspace: &Path,
    repo: &Path,
    branch: &str,
    pushed: &str,
) -> Result<(), Why> {
    let kept = format!("{KEPT_HEADS}{branch}");
    // Nobody has looked at work that was never committed, so it is not finished
    // work and this command does not deliver it. The workspace's files are read
    // against the commit its `HEAD` named, through the run's own repository:
    // its head and index are that commit and the workspace is its work tree.
    // Submodules are left alone, because a status of one would be a git of a
    // repository the container wrote.
    git(h, repo, &["reset", "--quiet", KEPT_HEAD]).let_say("its working tree could not be read")?;
    let tree = format!("--work-tree={}", workspace.display());
    let dirty = git(
        h,
        repo,
        &[
            &tree,
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--ignore-submodules=all",
        ],
    )
    .let_say("its working tree could not be read")?;
    if !dirty.trim().is_empty() {
        return Err(Why::only(
            "its working tree holds work that was never committed",
        ));
    }
    git(h, repo, &["remote", "add", "origin", &plan.remote()])
        .let_say("the remote could not be named")?;
    // The fetch is before the count: the rebase decides against what the remote
    // holds now.
    let fetched = git(h, repo, &["fetch", "--quiet", "origin", &plan.default]);
    if !fetched.ok() {
        return Err(
            Why::new(format!("origin/{} could not be fetched", plan.default)).said(&fetched),
        );
    }
    git(
        h,
        repo,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/remotes/origin/{}", plan.default),
        ],
    )
    .let_say(&format!(
        "origin/{} is not a branch of the remote",
        plan.default
    ))?;
    let ahead = git(
        h,
        repo,
        &[
            "rev-list",
            "--count",
            &format!("refs/remotes/origin/{}..{kept}", plan.default),
        ],
    )
    .let_say("its commits over the default branch could not be counted")?;
    if !has_commits(ahead.trim()) {
        return Err(Why::only(format!(
            "{branch} holds no commit over origin/{}",
            plan.default
        )));
    }
    // The remote's own answer, because pushing over a name that stands there is
    // the one thing this run must never do, and gh's list is no authority on what
    // refs exist. git answers 2 for a name the remote does not have.
    let there = git(
        h,
        repo,
        &[
            "ls-remote",
            "--exit-code",
            "--heads",
            "origin",
            &format!("refs/heads/{pushed}"),
        ],
    );
    if there.ok() {
        return Err(Why::only(format!(
            "{pushed} is already a branch of the remote"
        )));
    }
    if there.status != Some(2) {
        return Err(Why::new(format!("{pushed} could not be asked of the remote")).said(&there));
    }
    if let Some(url) = open_pull(plan, h, pushed)? {
        return Err(Why::only(format!(
            "the issue already has an open pull request, {url}"
        )));
    }
    Ok(())
}

/// Whether a `rev-list --count` answer names at least one commit. A count that
/// is no count is no commit: a run does not push on an answer it cannot read.
pub fn has_commits(count: &str) -> bool {
    !count.is_empty() && count != "0" && count.bytes().all(|c| c.is_ascii_digit())
}

/// Whether the issue, or the name the work would be pushed to, already has an
/// open pull request. A list that could not be read is a refusal and not an
/// absence: nothing is pushed on a guess about what GitHub holds open.
///
/// One list of the repository's open pull requests is read and asked twice.
/// `--head` would be the narrower ask for the branch, but a pull request whose
/// head is named otherwise names the issue just the same, so the heads and the
/// bodies have to be read either way and one call answers both questions.
fn open_pull(plan: &Plan, h: &mut dyn Host, pushed: &str) -> Result<Option<String>, Why> {
    let out = gh(
        h,
        &plan.home,
        &[
            "pr",
            "list",
            "--repo",
            &plan.repo,
            "--state",
            "open",
            "--json",
            "url,headRefName,body",
            "--limit",
            "100",
        ],
    )
    .let_say("the open pull requests could not be read")?;
    Ok(pull_named(&out, pushed).or_else(|| pull_for_issue(&out, plan.issue)))
}

/// The pull requests of a `--json` answer, or none at all when the answer is no
/// list: an answer GitHub did not give is read as nothing open, and the call's
/// own failure is what refuses the run.
fn list(out: &str) -> Vec<Value> {
    serde_json::from_str::<Value>(out.trim())
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
}

/// The URL of one pull request of an answer, when it is an https one.
fn url_of(pull: &Value) -> Option<String> {
    pull.get("url")
        .and_then(Value::as_str)
        .filter(|u| u.starts_with("https://"))
        .map(str::to_string)
}

/// The URL of the open pull request whose head is exactly `branch`.
pub fn pull_named(out: &str, branch: &str) -> Option<String> {
    list(out).into_iter().find_map(|p| {
        (p.get("headRefName").and_then(Value::as_str) == Some(branch)).then(|| url_of(&p))?
    })
}

/// The URL of the open pull request of issue `n`: one whose head is the issue's
/// branch as pi-rukas names it, or whose body closes it. None is no such pull
/// request in the answer.
pub fn pull_for_issue(out: &str, n: u64) -> Option<String> {
    let head = format!("feature/issue-{n}");
    let slug = format!("{head}-");
    let reference = format!("#{n}");
    list(out).into_iter().find_map(|p| {
        let named = match p.get("headRefName").and_then(Value::as_str) {
            Some(h) => h == head || (h.starts_with(&slug) && branch_shaped(h)),
            None => false,
        };
        let closes = p
            .get("body")
            .and_then(Value::as_str)
            .is_some_and(|b| closes_issue(b, &reference));
        (named || closes).then(|| url_of(&p)).flatten()
    })
}

/// Whether a pull request's body closes the issue: a word of closing, the
/// reference, and no longer number after it. The reading the shell gave a
/// jq program, held to the ASCII words the factory's own bodies use. `#70` does
/// not name 7, and a bare `#7` names nothing.
pub fn closes_issue(text: &str, reference: &str) -> bool {
    const WORDS: [&str; 8] = [
        "closes", "close", "closed", "fixes", "fix", "fixed", "resolves", "resolve",
    ];
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let at = reference.as_bytes();
    // What may stand after the reference: white space, or the punctuation a body
    // puts after it, `Fixes #7.` and `Closes #7)` among them. A digit would make
    // the number longer, `#7` of `#70` being no reference to 7 at all.
    let ends = |one: u8| {
        one.is_ascii_whitespace() || matches!(one, b'.' | b',' | b';' | b':' | b')' | b']')
    };
    bytes
        .windows(at.len())
        .enumerate()
        .filter(|(_, w)| *w == at)
        .any(|(i, _)| {
            if bytes.get(i + at.len()).is_some_and(|one| !ends(*one)) {
                return false;
            }
            // The reference is a word of its own: white space, or one of the
            // writer's marks, stands before its `#`. Where the `#` is written
            // against a word, `Fixes#7`, the writer left the number no room and
            // named nothing.
            match lower[..i].chars().last() {
                None => return false,
                Some(one) if one.is_whitespace() || one == ':' || one == '.' => {}
                Some(_) => return false,
            }
            // The body up to the reference, its trailing white space taken off:
            // the words that came before the number.
            let before = lower[..i].trim_end();
            // The words before the one that closes. A body that negates the
            // word names no pull request of the issue: `does not fix #7`,
            // `no test fixes #7` and `never fixed #7` say the pull request does
            // not close the issue, and one that does not close it is no reason
            // to refuse a finish. `half-fixed #7` is refused another way: the
            // word that closes is a word of its own, and what stands before it
            // leaves no room for one.
            if negated(before) {
                return false;
            }
            // Where the last word of the body begins: just after the last white
            // space, or at the body's own beginning.
            let head = match before.rfind(char::is_whitespace) {
                Some(k) => k + 1,
                None => 0,
            };
            // A word of closing is a word of its own: only white space, or the
            // body's own beginning, stands before it. `unfixes #7` is no word.
            if !before[..head].is_empty() && !before[..head].ends_with(char::is_whitespace) {
                return false;
            }
            // The word of closing, and the one mark a writer may leave on its end:
            // the colon that sets the number off, `close: #7`, and the period that
            // ends the body's sentence, `This fixes #7.`. Past the word and that
            // mark the body goes into nothing else — a letter, `fixesx #7`, would
            // make the word something the writer never wrote.
            WORDS.iter().any(|word| {
                let tail = &before[head..];
                if !tail.starts_with(*word) {
                    return false;
                }
                if tail.len() - word.len() > 1 {
                    return false;
                }
                match tail[word.len()..].chars().next() {
                    None => true,
                    Some(one) => one == ':' || one == '.',
                }
            })
        })
}

/// Whether the words a body puts before a closing word deny it. The word of
/// negation is read where the closing word is read: the word before it, the one
/// before that, or the end of the word before it when a writer wrote it with
/// `not` contracted onto it. Two words are read because `no test fixes #7` denies the closing
/// word with a noun of its own: the word of negation stands two back there, and
/// the noun of its own stands between. A negation further back names another
/// clause than the one that closes the issue, and leaves the reference standing
/// — `nothing here, but it fixes #7` is the issue's pull request.
pub fn negated(before: &str) -> bool {
    const NO: [&str; 3] = ["not", "no", "never"];
    before
        .rsplit(char::is_whitespace)
        .take(3)
        .any(|word| NO.contains(&word) || word.ends_with("n't"))
}

/// Rules 5 and 6: the rebase, the count of what it left, the push, the draft.
fn rebase_push_open(
    plan: &Plan,
    h: &mut dyn Host,
    repo: &Path,
    branch: &str,
    pushed: &str,
    scratch: &Scratch,
) -> Result<String, Why> {
    let kept = format!("{KEPT_HEADS}{branch}");
    // `--force`: the index the status made is the workspace's, not this tree's.
    git(
        h,
        repo,
        &["checkout", "--quiet", "--force", "--detach", &kept],
    )
    .let_say("the branch could not be checked out")?;
    let on = format!("refs/remotes/origin/{}", plan.default);
    // `<upstream>` and `<branch>` are named in full because git takes them from
    // the current branch otherwise, and the tree is on none. `--onto` is the
    // remote's default branch, so the commits of the cycle stand on what a
    // reviewer merges against.
    let rebased = git(h, repo, &["rebase", "--onto", &on, &on, "HEAD"]);
    if !rebased.ok() {
        // git left the tree mid rebase. It is the run's own and goes with it.
        return Err(
            Why::new(format!("the rebase onto origin/{} conflicts", plan.default)).said(&rebased),
        );
    }
    let count = git(h, repo, &["rev-list", "--count", &format!("{on}..HEAD")])
        .let_say("the rebased commits could not be counted")?;
    if !has_commits(count.trim()) {
        return Err(Why::only("the rebase left no commit to push"));
    }
    // The push of a new name with no force, of HEAD. The remote refusing it is
    // the same refusal as the name found there before the push, and it is not
    // tried again.
    let push = git(
        h,
        repo,
        &["push", "origin", &format!("HEAD:refs/heads/{pushed}")],
    );
    if !push.ok() {
        return Err(Why::new(format!("{pushed} could not be pushed")).said(&push));
    }
    draft(plan, h, pushed, scratch)
}

/// The draft pull request, and the URL gh prints for it.
fn draft(plan: &Plan, h: &mut dyn Host, pushed: &str, scratch: &Scratch) -> Result<String, Why> {
    let words = body(plan.issue, &plan.reason, &plan.default);
    let file = scratch.write_body(&words).map_err(Why::only)?;
    let made = gh(
        h,
        &plan.home,
        &[
            "pr",
            "create",
            "--repo",
            &plan.repo,
            "--base",
            &plan.default,
            "--head",
            pushed,
            "--draft",
            "--title",
            &title(plan.issue),
            "--body-file",
            &file.display().to_string(),
        ],
    );
    scratch.drop_body();
    let said = made.let_say("the pull request could not be opened")?;
    // gh prints the URL alone on its last line; a wrapper of it may say more.
    said.lines()
        .rev()
        .map(str::trim)
        .find(|l| l.starts_with("https://"))
        .map(str::to_string)
        .ok_or_else(|| Why::only("the pull request said no url"))
}

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
//!    has no open pull request of the factory's own;
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
use super::Ports;
use std::path::{Path, PathBuf};
use tehas_core::accounts::folded;
use tehas_core::issue_pull::names_change;
use tehas_ports::{
    Access, BlueprintNo, Body, Bound, Branch, ChangeDigest, ChangeFilter, ChangeStates, NewChange,
    Outcome, Unread,
};

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

    /// A refusal after the push: the branch stands on the remote, and rule 4
    /// refuses every later run while it does, so the refusal names it and what
    /// clears it. The branch is not deleted: gh may have opened the pull request
    /// even when it said nothing the run could read, and a deleted head would
    /// close it.
    fn stands(mut self, pushed: &str) -> Why {
        self.why = format!(
            "{}; {pushed} was pushed and stands on the remote: open its pull request \
             by hand, or delete the branch before finish is asked again",
            self.why
        );
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

/// The run: everything the rules of this file decide, and the scratch it takes
/// back with it.
pub fn run(plan: &Plan, h: &mut dyn Host, ports: &mut Ports) -> Finished {
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
    let outcome = in_scratch(plan, h, ports, &workspace, &dir);
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
    ports: &mut Ports,
    workspace: &Path,
    dir: &Path,
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
    preflight(plan, h, ports, workspace, &repo, &branch, &pushed)?;
    rebase_push_open(plan, h, ports, &repo, &branch, &pushed)
}

/// The run's repository, `<dir>/repo`, holding the workspace's branches and
/// `HEAD` under `refs/kept/` and its `info/exclude`, with `origin` the plan's
/// remote. Nothing of the workspace's config comes with them. A workspace has
/// TEHAS_CLONE_DEPTH commits of history, and git refuses refs whose shallow
/// roots it would have to take unless asked with `--update-shallow`. The fetch
/// of the remote's default branch then brings what it gained since, down to
/// the commit the cycle started from, which the workspace holds.
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
        &[
            "fetch",
            "--quiet",
            "--no-tags",
            "--update-shallow",
            &from,
            &head,
            &heads,
        ],
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
    ports: &mut Ports,
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
    let remote = remote_of(plan, ports)?;
    git(h, repo, &["remote", "add", "origin", &remote]).let_say("the remote could not be named")?;
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
    if let Some(url) = open_pull(plan, ports, pushed)? {
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
///
/// Only the factory's own count, by the poll's rule: a branch of the repository
/// that the bot or an allowed account opened. GraphQL is asked because that
/// rule reads the author's user id, which `gh pr list` does not give.
///
/// The list is read to its last page. A page that says more follow but names
/// no cursor, or more pages than [`PAGES_MAX`], refuses the run: a pull request
/// beyond the first page names the issue just the same.
fn open_pull(plan: &Plan, ports: &mut Ports, pushed: &str) -> Result<Option<String>, Why> {
    let filter = ChangeFilter {
        states: ChangeStates::Open,
        bodies: true,
        ..Default::default()
    };
    let bound = Bound::new(100 * PAGES_MAX as u32).expect("a bound");
    let all = match ports.code_host.changes(&filter, bound) {
        Ok(all) => all,
        Err(Outcome::Unread(Unread::Cut)) => {
            return Err(Why::only(format!(
                "{UNREAD}: more than {PAGES_MAX} pages of them"
            )))
        }
        Err(e) => {
            return Err(Why {
                why: UNREAD.to_string(),
                said: e.line().to_string(),
            })
        }
    };
    let allowed = |l: &str| plan.allowed.iter().any(|a| folded(a) == folded(l));
    let own: Vec<&ChangeDigest> = all
        .items()
        .iter()
        .filter(|c| ports.accounts.own_change(c, allowed))
        .collect();
    let named = own.iter().find(|c| c.head.as_str() == pushed);
    let of_issue = || {
        own.iter()
            .find(|c| names_change(c.head.as_str(), c.body.as_deref(), plan.issue))
    };
    Ok(named.or_else(of_issue).map(|c| c.link.as_str().to_string()))
}

/// Where the work is pushed: the plan's own remote, or the code host's for a
/// push. A remote the code host cannot give stops the run before anything is
/// pushed: no URL is made up in its place.
fn remote_of(plan: &Plan, ports: &mut Ports) -> Result<String, Why> {
    if let Some(r) = &plan.remote {
        return Ok(r.clone());
    }
    match ports.code_host.remote(Access::Push) {
        Ok(r) => Ok(r.url.as_str().to_string()),
        Err(e) => Err(Why {
            why: "the remote could not be named".to_string(),
            said: e.line().to_string(),
        }),
    }
}

const UNREAD: &str = "the open pull requests could not be read";

/// The most pages of open pull requests one run reads, 100 to a page.
pub const PAGES_MAX: usize = 10;

/// Rules 5 and 6: the rebase, the count of what it left, the push, the draft.
fn rebase_push_open(
    plan: &Plan,
    h: &mut dyn Host,
    ports: &mut Ports,
    repo: &Path,
    branch: &str,
    pushed: &str,
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
    draft(plan, ports, pushed).map_err(|w| w.stands(pushed))
}

/// The draft change, and where a person opens it.
fn draft(plan: &Plan, ports: &mut Ports, pushed: &str) -> Result<String, Why> {
    let not_opened = |said: &str| Why {
        why: "the pull request could not be opened".to_string(),
        said: said.to_string(),
    };
    let (Ok(head), Ok(base)) = (Branch::new(pushed), Branch::new(plan.default.as_str())) else {
        return Err(not_opened("its branches are no branch names"));
    };
    let new = NewChange {
        head,
        base,
        title: title(plan.issue),
        body: Body::Text(body(plan.issue, &plan.reason, &plan.default)),
        draft: true,
        closes: BlueprintNo::new(plan.issue).into_iter().collect(),
    };
    match ports.writes.open_change(&new) {
        Ok(p) => Ok(p.link.as_str().to_string()),
        Err(e) => Err(not_opened(e.line())),
    }
}

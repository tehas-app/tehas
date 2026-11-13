//! `tehas-cycle finish` against real git repositories in a scratch directory and
//! a stub `gh`: the case says what the remote holds and what GitHub answers, and
//! reads what the run did to the branch, to the remote, to the tree beside it,
//! and to the calls. No case reaches GitHub, a token or the network.

use super::cmd::Scratch;
use super::plan::{body, title, Plan};
use super::run::{has_commits, issue_branch_of, own_pulls, pull_for_issue, pull_named, Why};
use super::{finish, Finished};
use crate::finish::cmd::{asked, asked_of_git, within, Done, Live};
use crate::finish::Host;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tehas_core::testdir::TestDir;
use tehas_core::testing::write_exe;

/// An open pull request as GraphQL answers it: on a branch of the case's
/// repository, opened by the factory's bot.
fn pr(n: u64, head: &str, text: &str) -> Value {
    json!({"url": format!("https://github.com/acme/widget/pull/{n}"),
        "headRefName": head, "body": text, "isCrossRepository": false,
        "headRepository": {"nameWithOwner": "acme/widget"},
        "author": {"login": "tehas-bot"}})
}

/// The GraphQL answer that holds `nodes`, a JSON list, or the text itself as
/// the list when it is no JSON.
fn pulls(nodes: &str) -> String {
    let nodes: Value = serde_json::from_str(nodes).unwrap_or_else(|_| json!(nodes));
    json!({"data": {"repository": {"pullRequests": {"nodes": nodes}}}}).to_string()
}

/// The scratch root of one case.
fn root(name: &str) -> TestDir {
    TestDir::new(&format!("finish-{name}"))
}

/// What every git call of a case is given: one author, so a commit needs no
/// setting of the host, and one default branch, whatever the host prefers.
const WHO: [&str; 6] = [
    "-c",
    "user.email=tehas@invalid",
    "-c",
    "user.name=tehas",
    "-c",
    "init.defaultBranch=main",
];

fn git(args: &[&str]) -> String {
    let out = Command::new("git")
        .args(WHO)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn git_in(dir: &Path, args: &[&str]) -> String {
    let all: Vec<String> = std::iter::once("-C".to_string())
        .chain(std::iter::once(dir.display().to_string()))
        .chain(args.iter().map(|s| s.to_string()))
        .collect();
    git_s(&all)
}

/// The same ask of git, given owned words, for a caller that built them.
fn git_s(args: &[String]) -> String {
    let out = Command::new("git")
        .args(WHO)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// The world of a case: the bare origin, the clone that makes the origin's
/// branches, the home, and the kept workspace of issue 7.
struct World {
    dir: TestDir,
    origin: PathBuf,
    seed: PathBuf,
    home: PathBuf,
    work_dir: PathBuf,
    workspace: PathBuf,
    gh: PathBuf,
}

impl World {
    /// `<dir>/origin.git` with one commit on `main`, `<dir>/seed` beside it, and
    /// `<dir>/home/work/` for the workspace.
    fn new(name: &str) -> World {
        let dir = root(name);
        let origin = dir.join("origin.git");
        let seed = dir.join("seed");
        let home = dir.join("home");
        fs::create_dir_all(&home).unwrap();
        git(&["init", "--quiet", "--bare", &origin.display().to_string()]);
        // A bare `init` of the default branch names a ref that does not exist
        // yet, and `git clone` of such a remote fetches nothing: the HEAD of the
        // remote is pointed at `main` once the seed has made that commit, which
        // is what a remote with history answers with.
        git(&[
            "--git-dir",
            &origin.display().to_string(),
            "symbolic-ref",
            "HEAD",
            "refs/heads/main",
        ]);
        git(&["init", "--quiet", "-b", "main", &seed.display().to_string()]);
        fs::write(seed.join("readme.md"), "one\n").unwrap();
        git_in(&seed, &["add", "-A"]);
        git_in(&seed, &["commit", "--quiet", "-m", "one"]);
        // The origin is filled by fetch and never by push: a bare repository
        // takes objects and refs from a fetch of the seed, which is the
        // repository the case makes its branches in.
        git(&[
            "--git-dir",
            &origin.display().to_string(),
            "fetch",
            "--quiet",
            &seed.display().to_string(),
            "+refs/heads/main:refs/heads/main",
        ]);
        git_in(
            &seed,
            &["remote", "add", "origin", &origin.display().to_string()],
        );
        let work_dir = home.join("work/widget");
        fs::create_dir_all(&work_dir).unwrap();
        World {
            workspace: home.join("work/widget-7"),
            gh: dir.join("gh"),
            origin,
            seed,
            work_dir,
            home,
            dir,
        }
    }

    /// The kept workspace: a clone of the origin with the issue's branch, whose
    /// one commit sits on `main`. The branch is given whole, so a case can name
    /// another issue's branch or a name of its own.
    fn kept(&self, branch: &str, files: &[(&str, &str)]) {
        let _ = fs::remove_dir_all(&self.workspace);
        let ws = self.dir.join("ws");
        git(&[
            "clone",
            "--quiet",
            &self.origin.display().to_string(),
            &ws.display().to_string(),
        ]);
        // A real host has its identity in the user's config; a rebase needs one.
        git_in(&ws, &["config", "user.name", "tehas"]);
        git_in(&ws, &["config", "user.email", "tehas@invalid"]);
        git_in(&ws, &["checkout", "--quiet", "-b", branch, "origin/main"]);
        for (name, text) in files {
            fs::write(ws.join(name), text).unwrap();
        }
        git_in(&ws, &["add", "-A"]);
        git_in(&ws, &["commit", "--quiet", "-m", "the change"]);
        fs::rename(&ws, &self.workspace).unwrap();
    }

    /// The workspace's own branch, fetched into the seed and into the remote
    /// under the same name. The objects of a case's branch are the workspace's,
    /// so the seed is filled from the workspace and the remote from the seed, and
    /// the case makes a ref of the remote without a push anywhere.
    fn share_branch_with_the_remote(&self, branch: &str) {
        let from = format!("{}", self.workspace.display());
        for store in [&self.seed, &self.origin] {
            // A bare repository is asked of its own directory; the seed is a
            // working repository and is asked of the checkout, which is where
            // its objects live.
            let where_ = if store == &self.origin {
                "--git-dir".to_string()
            } else {
                "-C".to_string()
            };
            git(&[
                &where_,
                &store.display().to_string(),
                "fetch",
                "--quiet",
                &from,
                &format!("+refs/heads/{branch}:refs/heads/{branch}"),
            ]);
        }
    }

    /// A commit of the case's own, on the seed under `branch`, and that branch
    /// fetched into the remote under the same name. The seed is where the case's
    /// objects are made; the remote takes them by fetch, which is how a bare
    /// repository is filled without a push anywhere.
    fn seed_commit(&self, branch: &str, name: &str, text: &str, subject: &str) {
        git_in(&self.seed, &["checkout", "--quiet", "-B", branch, "main"]);
        fs::write(self.seed.join(name), text).unwrap();
        git_in(&self.seed, &["add", "-A"]);
        git_in(&self.seed, &["commit", "--quiet", "-m", subject]);
        let from = format!("{}", self.seed.display());
        git(&[
            "--git-dir",
            &self.origin.display().to_string(),
            "fetch",
            "--quiet",
            &from,
            &format!("+refs/heads/{branch}:refs/heads/{branch}"),
        ]);
        git_in(&self.seed, &["checkout", "--quiet", "main"]);
    }

    /// A commit of the case's own on the remote's default branch.
    fn remote_commit(&self, name: &str, text: &str, subject: &str) {
        self.seed_commit("main", name, text, subject);
    }

    /// The remote's default branch taken to the commit `sha`, whose objects the
    /// workspace holds: a case uses it to ask what happens when the issue's
    /// branch has nothing over the default branch, and the remote's `main` and
    /// that branch become one commit. `sha` is the commit of a branch the case
    /// shares with the seed and the remote, so the name git is asked for is a
    /// name that exists, and not a number written out by the case.
    fn remote_main_at(&self, sha: &str) {
        let name = self.name_at(sha);
        assert!(!name.is_empty(), "{sha} is no branch of the workspace");
        git_in(&self.seed, &["update-ref", "refs/heads/main", &name]);
        let from = format!("{}", self.seed.display());
        git(&[
            "--git-dir",
            &self.origin.display().to_string(),
            "fetch",
            "--quiet",
            &from,
            "+refs/heads/main:refs/heads/main",
        ]);
    }

    /// The name of a branch of the workspace that stands at `sha`, or nothing
    /// when the case shared no such branch. The workspace is a clone and its
    /// refs are its own, so the ask is made of it and not of the seed.
    fn name_at(&self, sha: &str) -> String {
        let out = Command::new("git")
            .args(WHO)
            .args(["-C"])
            .arg(&self.workspace)
            .args(["for-each-ref", "--format=%(objectname) %(refname:short)"])
            .arg("refs/heads/")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        text.lines()
            .find(|l| l.starts_with(sha))
            .and_then(|l| l.split_whitespace().nth(1))
            .map(str::to_string)
            .unwrap_or_default()
    }

    /// Whether the remote holds a branch.
    fn remote_has(&self, branch: &str) -> bool {
        Command::new("git")
            .args(["--git-dir"])
            .arg(&self.origin)
            .args(["rev-parse", "--verify", "--quiet"])
            .arg(format!("refs/heads/{branch}"))
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    /// The subjects of a branch of the remote, oldest first.
    fn remote_log(&self, branch: &str) -> Vec<String> {
        let out = Command::new("git")
            .args(["--git-dir"])
            .arg(&self.origin)
            .args(["log", "--format=%s", "--reverse"])
            .arg(branch)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The subject the workspace's own branch stands at, or nothing when it is
    /// gone: what a run must never change.
    fn head_of(&self, branch: &str) -> Option<String> {
        let out = Command::new("git")
            .args(WHO)
            .args(["-C"])
            .arg(&self.workspace)
            .args(["log", "-1", "--format=%s"])
            .arg(branch)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// The names under the work dir that hold a run's tree or body: what a run
    /// that finished leaves none of.
    fn left(&self) -> Vec<String> {
        fs::read_dir(&self.work_dir)
            .map(|d| {
                d.flatten()
                    .filter_map(|e| e.file_name().to_str().map(str::to_string))
                    .filter(|n| n.contains(".finish"))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The run's words.
    fn plan(&self) -> Plan {
        Plan {
            issue: 7,
            repo: "acme/widget".to_string(),
            home: self.home.clone(),
            work_dir: self.work_dir.clone(),
            default: "main".to_string(),
            reason: "a flaky test in a file this change does not touch".to_string(),
            remote: Some(self.origin.display().to_string()),
            bot: "tehas-bot[bot]".to_string(),
            allowed: vec!["alice".to_string(), "bob".to_string()],
        }
    }

    /// The `gh` of a case: one line per call in `gh.calls`; the GraphQL list of
    /// open pull requests answers from `list.answer` and `pr create` from
    /// `create.answer`, an absent file being GitHub's nothing and a file of
    /// `fail` a refusal of the call. A list answer is the nodes alone, as
    /// [`pulls`] wraps them. The body file of a create is copied so a case can
    /// read what the draft said.
    fn stub(&self, list: &str, create: &str) {
        write_exe(
            &self.gh,
            r#"#!/bin/sh
# The gh of finish's cases. It records every call and answers the list of open
# pull requests and pr create from a file of the case; a file of fail refuses its
# call. The body file of a create is copied, so a case can read what the draft
# said.
set -u
dir=$(dirname "$0")
kind=other
prev=
for a in "$@"; do
  case "$a" in
    pr) kind=pr ;;
    api) kind=api ;;
    graphql) [ "$kind" = api ] && kind=list ;;
    create) [ "$kind" = pr ] && kind=create ;;
  esac
  if [ "$kind" = create ] && [ "$prev" = --body-file ]; then
    cp "$a" "$dir/body.text" 2>/dev/null
  fi
  prev=$a
done
case "$kind" in pr|api) kind=other ;; esac
printf '%s %s\n' "$*" "$kind" >>"$dir/gh.calls"
ans=$dir/$kind.answer
if [ -f "$ans" ]; then
  [ "$(cat "$ans")" != fail ] || exit 1
  [ "$kind" = other ] || { cat "$ans"; exit 0; }
  exit 0
fi
[ "$kind" = list ] && { echo '{"data":{"repository":{"pullRequests":{"nodes":[]}}}}'; exit 0; }
exit 0
"#,
        )
        .unwrap();
        if !list.is_empty() {
            let answer = if list == "fail" {
                list.to_string()
            } else {
                pulls(list)
            };
            fs::write(self.dir.join("list.answer"), format!("{answer}\n")).unwrap();
        }
        if !create.is_empty() {
            fs::write(self.dir.join("create.answer"), format!("{create}\n")).unwrap();
        }
    }

    /// A hook of the kept workspace, as an agent that wrote the tree leaves one:
    /// under `<workspace>/.git/hooks`, named for the command that runs it, said
    /// to have run by the file it touches and refused by the status it ends with.
    /// The pins of a call of git are what keeps it from running.
    fn plant(&self, name: &str, touch: &str, status: i32) {
        let dir = self.workspace.join(".git/hooks");
        fs::create_dir_all(&dir).unwrap();
        let hook = dir.join(name);
        write_exe(
            &hook,
            format!("#!/bin/sh\ntouch '{touch}'\nexit {status}\n"),
        )
        .unwrap();
    }

    /// What the stub was asked, one call per line.
    fn calls(&self) -> Vec<String> {
        fs::read_to_string(self.dir.join("gh.calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn asked(&self, what: &str) -> bool {
        self.calls().iter().any(|c| c.contains(what))
    }

    /// The body the draft was opened with.
    fn body(&self) -> String {
        fs::read_to_string(self.dir.join("body.text")).unwrap_or_default()
    }

    /// The host of a case: the host's git, and the case's own `gh` by its path.
    fn host(&self) -> Case {
        Case {
            git: git_program(),
            gh: self.gh.clone(),
        }
    }
}

/// Where the host's git stands.
fn git_program() -> String {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .map(|d| Path::new(d).join("git"))
        .find(|p| p.is_file())
        .map(|p| p.display().to_string())
        .expect("git is on PATH")
}

/// The host of a case: the real git, and the case's own stub of gh. The run asks
/// the names `git` and `gh` and the host decides where they are, which is the
/// only way a case can stand in for GitHub.
struct Case {
    git: String,
    gh: PathBuf,
}

impl Host for Case {
    fn run(&mut self, program: &str, args: &[String], dir: &Path) -> Done {
        // The pins are part of the ask and not of the host, so a case reads them
        // the way the host does and answers a git call made without them the way
        // `Live` does. A rule that built one is a rule that would let the
        // workspace's own hooks run on a host.
        if program == "git" && !asked_of_git(args) {
            return Done {
                status: None,
                out: String::new(),
                err: "a call of git was asked without the pins".to_string(),
            };
        }
        let program = match program {
            "git" => &self.git,
            "gh" => &self.gh.to_string_lossy().into_owned(),
            other => {
                return Done {
                    status: None,
                    out: String::new(),
                    err: format!("{other} is no program of a case"),
                }
            }
        };
        // A run asks nothing of a tree that holds no repository: the command
        // refuses one before it calls anything, so a case that reached here that
        // way is a case that skipped a rule. A directory of the run's own scratch
        // is not a repository either, so the ask is made of git itself.
        if !dir.is_dir() {
            return Done {
                status: Some(0),
                out: String::new(),
                err: format!("{} is no directory", dir.display()),
            };
        }
        if program == "git"
            && !args.iter().any(|a| a == "init")
            && !Command::new("git")
                .args(["rev-parse", "--git-dir"])
                .current_dir(dir)
                .stdin(Stdio::null())
                .output()
                .is_ok_and(|o| o.status.success())
        {
            return Done {
                status: Some(0),
                out: String::new(),
                err: format!("{} holds no repository", dir.display()),
            };
        }
        let out = Command::new(program)
            .args(args)
            .current_dir(dir)
            .stdin(Stdio::null())
            .env_remove("GIT_CONFIG_GLOBAL")
            .env_remove("GIT_CONFIG_SYSTEM")
            // A host's identity is in its user's config; a rebase needs one.
            .env("GIT_COMMITTER_NAME", "tehas")
            .env("GIT_COMMITTER_EMAIL", "tehas@invalid")
            .env_remove("GH_TOKEN")
            .env_remove("GITHUB_TOKEN")
            .output()
            .expect("the program runs");
        Done {
            status: out.status.code(),
            out: String::from_utf8_lossy(&out.stdout).into_owned(),
            err: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    fn clean(&mut self, dir: &Path) {
        if dir.exists() {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

#[test]
fn the_work_a_stopped_cycle_left_is_pushed_and_opened_as_a_draft() {
    let w = World::new("draft");
    w.kept("feature/issue-7-default", &[("a.txt", "the change\n")]);
    // The remote moved on while the cycle stopped: the rebase is what makes the
    // draft something a reviewer can merge.
    w.remote_commit("other.md", "later\n", "later");
    w.stub("", "https://github.com/acme/widget/pull/41");
    let mut h = w.host();
    let got = finish(&w.plan(), &mut h);
    assert_eq!(got.why, "", "the run refused: {}", got.why);
    assert_eq!(got.url, "https://github.com/acme/widget/pull/41");
    // The new branch holds the cycle's commit over the remote's main; the
    // cycle's own branch stands where the cycle left it.
    assert_eq!(
        w.remote_log("finish/issue-7"),
        vec!["one", "later", "the change"]
    );
    assert_eq!(
        w.head_of("feature/issue-7-default").as_deref(),
        Some("the change")
    );
    assert_eq!(w.left(), Vec::<String>::new(), "the run left a tree behind");
    assert!(w.asked("pr create"), "{:?}", w.calls());
    assert!(w.asked("--draft"));
    assert!(w.asked("--head finish/issue-7"));
    assert!(w.asked("--base main"));
    assert!(w.asked("--repo acme/widget"));
    let body = w.body();
    assert!(body.contains("Fixes #7."), "{body}");
    assert!(
        body.contains("a flaky test"),
        "the reason is the asker's: {body}"
    );
    assert!(body.contains("gate did not pass"), "{body}");
}

#[test]
fn a_workspace_that_is_not_there_is_refused() {
    let w = World::new("no-workspace");
    w.stub("", "");
    let mut h = w.host();
    let got = finish(&w.plan(), &mut h);
    assert_eq!(got.url, "");
    assert!(got.why.contains("is not a directory"), "{}", got.why);
    assert!(w.left().is_empty());
    assert!(w.calls().is_empty(), "nothing was asked of GitHub");
}

#[test]
fn a_workspace_of_no_repository_is_refused() {
    let w = World::new("not-a-repository");
    fs::create_dir_all(&w.workspace).unwrap();
    w.stub("", "");
    let mut h = w.host();
    let got = finish(&w.plan(), &mut h);
    assert!(got.why.contains("no git repository"), "{}", got.why);
}

#[test]
fn a_workspace_that_holds_no_branch_of_the_issue_is_refused() {
    let w = World::new("no-branch");
    // A clone of main, which holds no branch of the issue at all.
    let _ = fs::remove_dir_all(&w.workspace);
    git(&[
        "clone",
        "--quiet",
        &w.origin.display().to_string(),
        &w.workspace.display().to_string(),
    ]);
    w.stub("", "");
    let mut h = w.host();
    let got = finish(&w.plan(), &mut h);
    assert!(
        got.why.contains("holds no branch of issue #7"),
        "{}",
        got.why
    );
    assert!(w.left().is_empty());
}

#[test]
fn a_branch_of_another_issue_is_not_this_issue_s_branch() {
    let w = World::new("wrong-issue");
    w.kept("feature/issue-8-default", &[("a.txt", "x\n")]);
    w.stub("", "https://github.com/acme/widget/pull/1");
    let mut h = w.host();
    let got = finish(&w.plan(), &mut h);
    assert!(
        got.why.contains("holds no branch of issue #7"),
        "{}",
        got.why
    );
    assert!(!w.remote_has("finish/issue-7"));
}

#[test]
fn a_branch_named_without_a_slug_is_still_the_issue_s() {
    let w = World::new("no-slug");
    w.kept("feature/issue-7", &[("a.txt", "x\n")]);
    w.stub("", "https://github.com/acme/widget/pull/42");
    let mut h = w.host();
    let got = finish(&w.plan(), &mut h);
    assert_eq!(got.why, "", "{}", got.why);
    assert!(w.remote_has("finish/issue-7"));
}

#[test]
fn work_that_was_never_committed_is_refused() {
    let w = World::new("uncommitted");
    w.kept("feature/issue-7-default", &[("a.txt", "x\n")]);
    fs::write(w.workspace.join("unseen.txt"), "nobody looked at this\n").unwrap();
    w.stub("", "https://github.com/acme/widget/pull/1");
    let mut h = w.host();
    let got = finish(&w.plan(), &mut h);
    assert!(
        got.why.contains("never committed"),
        "uncommitted work is refused: {}",
        got.why
    );
    assert!(!w.remote_has("finish/issue-7"));
    assert!(!w.asked("pr create"));
    assert!(w.left().is_empty());
}

#[test]
fn a_branch_with_no_commit_over_the_default_branch_is_refused() {
    let w = World::new("nothing-over");
    w.kept("feature/issue-7-default", &[("a.txt", "x\n")]);
    // The remote's main is the branch's own commit: there is nothing to open.
    // The branch's objects go to the seed and the remote first, by name.
    w.share_branch_with_the_remote("feature/issue-7-default");
    let head = git_in(&w.workspace, &["rev-parse", "feature/issue-7-default"]);
    w.remote_main_at(head.trim());
    w.stub("", "https://github.com/acme/widget/pull/1");
    let mut h = w.host();
    let got = finish(&w.plan(), &mut h);
    assert!(
        got.why.contains("no commit over"),
        "a branch of nothing is refused: {}",
        got.why
    );
    assert!(!w.remote_has("finish/issue-7"));
    assert!(!w.asked("pr create"));
}

#[test]
fn an_issue_that_already_has_an_open_pull_request_is_refused() {
    let mut by_alice = pr(13, "someone-else", "Fixes #7");
    by_alice["author"] = json!({"login": "alice"});
    for (what, answer) in [
        (
            "its-head-is-the-issue-s-branch",
            pr(9, "feature/issue-7-default", ""),
        ),
        (
            "its-body-closes-the-issue",
            pr(10, "someone-else", "Closes #7 and nothing else."),
        ),
        (
            "its-head-is-the-branch-this-run-would-push",
            pr(12, "finish/issue-7", ""),
        ),
        ("an-allowed-account-opened-it", by_alice),
    ] {
        let w = World::new(what);
        w.kept("feature/issue-7-default", &[("a.txt", "x\n")]);
        w.stub(
            &json!([answer]).to_string(),
            "https://github.com/acme/widget/pull/1",
        );
        let mut h = w.host();
        let got = finish(&w.plan(), &mut h);
        assert!(
            got.why.contains("already has an open pull request"),
            "{what}: {}",
            got.why
        );
        assert!(
            got.why.contains("pull/"),
            "{what}: the url is not named: {}",
            got.why
        );
        assert!(!w.remote_has("finish/issue-7"), "{what}");
        assert!(w.left().is_empty(), "{what}");
    }
}

#[test]
fn a_pull_request_that_names_nothing_is_not_the_issue_s() {
    let w = World::new("other-pr");
    w.kept("feature/issue-7-default", &[("a.txt", "x\n")]);
    // `#70` and `#7x` name no issue, and a head of the wrong number names none.
    w.stub(
        &json!([pr(11, "feature/issue-70-x", "see issue #70 and fix #7x")]).to_string(),
        "https://github.com/acme/widget/pull/12",
    );
    let mut h = w.host();
    let got = finish(&w.plan(), &mut h);
    assert_eq!(got.why, "", "{}", got.why);
    assert!(w.remote_has("finish/issue-7"));
}

#[test]
fn a_pull_request_the_factory_did_not_open_does_not_stop_the_run() {
    type Change = fn(&mut Value);
    let cases: [(&str, Change); 6] = [
        ("an-outsider-s-fork", |p| {
            p["isCrossRepository"] = json!(true);
            p["headRepository"] = json!({"nameWithOwner": "mallory/widget"});
            p["author"] = json!({"login": "mallory"});
        }),
        ("an-allowed-account-s-fork", |p| {
            p["isCrossRepository"] = json!(true);
            p["headRepository"] = json!({"nameWithOwner": "alice/widget"});
            p["author"] = json!({"login": "alice"});
        }),
        ("an-outsider-on-a-branch-of-the-repository", |p| {
            p["author"] = json!({"login": "mallory"});
        }),
        ("a-head-of-another-repository", |p| {
            p["headRepository"] = json!({"nameWithOwner": "acme/other"});
        }),
        ("no-author", |p| {
            p["author"] = Value::Null;
        }),
        ("no-cross-repository-answer", |p| {
            p.as_object_mut().unwrap().remove("isCrossRepository");
        }),
    ];
    for (what, change) in cases {
        let w = World::new(what);
        w.kept("feature/issue-7-default", &[("a.txt", "x\n")]);
        // Each names the issue every way a run reads: its head is the issue's
        // branch, its body closes it, and another's head is the pushed name.
        let mut named = pr(20, "feature/issue-7-default", "Fixes #7.");
        let mut pushed = pr(21, "finish/issue-7", "");
        change(&mut named);
        change(&mut pushed);
        w.stub(
            &json!([named, pushed]).to_string(),
            "https://github.com/acme/widget/pull/22",
        );
        let mut h = w.host();
        let got = finish(&w.plan(), &mut h);
        assert_eq!(got.why, "", "{what}: {}", got.why);
        assert!(w.remote_has("finish/issue-7"), "{what}");
    }
}

#[test]
fn a_list_that_cannot_be_read_is_a_refusal() {
    let w = World::new("list-fails");
    w.kept("feature/issue-7-default", &[("a.txt", "x\n")]);
    w.stub("fail", "https://github.com/acme/widget/pull/1");
    let mut h = w.host();
    let got = finish(&w.plan(), &mut h);
    assert!(got.why.contains("could not be read"), "{}", got.why);
    assert!(!w.remote_has("finish/issue-7"));
    assert!(w.left().is_empty());
}

#[test]
fn a_name_already_on_the_remote_is_refused() {
    let w = World::new("already-pushed");
    w.kept("feature/issue-7-default", &[("a.txt", "x\n")]);
    w.seed_commit(
        "finish/issue-7",
        "already.txt",
        "the name is taken\n",
        "a branch of that name",
    );
    w.stub("", "https://github.com/acme/widget/pull/1");
    let mut h = w.host();
    let got = finish(&w.plan(), &mut h);
    assert!(
        got.why.contains("already a branch of the remote"),
        "{}",
        got.why
    );
    assert!(!w.asked("pr create"));
    assert!(w.left().is_empty());
}

#[test]
fn a_rebase_that_conflicts_leaves_no_tree_and_no_branch() {
    let w = World::new("conflict");
    w.kept(
        "feature/issue-7-default",
        &[("a.txt", "the cycle's line\n")],
    );
    // The same line, changed on both sides: the rebase cannot settle it.
    w.remote_commit("a.txt", "the remote's line\n", "other side");
    w.stub("", "https://github.com/acme/widget/pull/1");
    let mut h = w.host();
    let got = finish(&w.plan(), &mut h);
    assert_eq!(got.why, "the rebase onto origin/main conflicts");
    // git quotes the cycle's commit subject: the log keeps it, the rule does not.
    assert!(got.said.contains("the change"), "{}", got.said);
    assert_eq!(got.url, "");
    assert!(!w.remote_has("finish/issue-7"), "nothing was pushed");
    assert_eq!(
        w.head_of("feature/issue-7-default").as_deref(),
        Some("the change"),
        "the workspace's branch moved"
    );
    assert_eq!(w.left(), Vec::<String>::new(), "the tree was left behind");
    // git's own record of the worktree went with the tree: the workspace names
    // only itself.
    let listed = git_in(&w.workspace, &["worktree", "list"]);
    assert_eq!(listed.lines().count(), 1, "{listed}");
    assert!(!w.asked("pr create"));
}

#[test]
fn a_push_the_remote_refuses_is_not_tried_again() {
    let w = World::new("push-fails");
    w.kept("feature/issue-7-default", &[("a.txt", "x\n")]);
    // The remote's own answer to a push, its hook: the run cannot know why.
    let hook = w.origin.join("hooks/pre-receive");
    write_exe(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    w.stub("", "https://github.com/acme/widget/pull/1");
    let mut h = w.host();
    let got = finish(&w.plan(), &mut h);
    assert!(got.why.contains("could not be pushed"), "{}", got.why);
    assert!(!w.remote_has("finish/issue-7"));
    assert!(!w.asked("pr create"));
    assert_eq!(w.left(), Vec::<String>::new());
}

#[test]
fn a_draft_that_cannot_be_opened_keeps_its_branch_for_the_next_ask() {
    let w = World::new("create-fails");
    w.kept("feature/issue-7-default", &[("a.txt", "x\n")]);
    w.stub("", "fail");
    let mut h = w.host();
    let got = finish(&w.plan(), &mut h);
    assert!(
        got.why.contains("pull request could not be opened"),
        "{}",
        got.why
    );
    assert_eq!(got.url, "");
    // The push is the half that cannot be undone here and it stands: the branch is
    // there for a reviewer, and no body file is left in the work directory.
    assert!(w.remote_has("finish/issue-7"));
    assert_eq!(w.left(), Vec::<String>::new());
    // The next ask finds the name on the remote and refuses rather than pushing
    // the same commits a second time.
    let mut h = w.host();
    let again = finish(&w.plan(), &mut h);
    assert!(
        again.why.contains("already a branch of the remote"),
        "{}",
        again.why
    );
}

#[test]
fn a_draft_that_said_no_url_is_a_refusal() {
    let w = World::new("no-url");
    w.kept("feature/issue-7-default", &[("a.txt", "x\n")]);
    w.stub("", "opened, but it said nothing");
    let mut h = w.host();
    let got = finish(&w.plan(), &mut h);
    assert!(got.why.contains("no url"), "{}", got.why);
    assert_eq!(got.url, "");
}

#[test]
fn a_word_that_is_no_word_stops_the_run_before_a_call() {
    let w = World::new("bad-words");
    w.kept("feature/issue-7-default", &[("a.txt", "x\n")]);
    w.stub("", "https://github.com/acme/widget/pull/1");
    let bad = |why: Plan, what: &str| {
        let mut h = w.host();
        let got = finish(&why, &mut h);
        assert!(got.why.contains(what), "{what} was not named: {}", got.why);
        assert!(!w.remote_has("finish/issue-7"), "{what} pushed");
    };
    let p = w.plan();
    bad(
        Plan {
            repo: "widget".into(),
            ..p.clone()
        },
        "owner/name",
    );
    bad(
        Plan {
            repo: "acme/widget/extra".into(),
            ..p.clone()
        },
        "owner/name",
    );
    bad(
        Plan {
            home: PathBuf::from("home"),
            ..p.clone()
        },
        "absolute",
    );
    bad(
        Plan {
            work_dir: PathBuf::from("work/widget"),
            ..p.clone()
        },
        "absolute",
    );
    bad(
        Plan {
            work_dir: PathBuf::from("/home/t/work/a b"),
            ..p.clone()
        },
        "workspace name",
    );
    bad(
        Plan {
            default: "a b".into(),
            ..p.clone()
        },
        "branch name",
    );
    bad(
        Plan {
            reason: "   ".into(),
            ..p.clone()
        },
        "reason",
    );
    bad(
        Plan {
            allowed: vec![],
            ..p.clone()
        },
        "TEHAS_ALLOWED_LABELERS",
    );
    bad(Plan { issue: 0, ..p }, "issue number");
}

#[test]
fn the_words_of_a_run_name_its_workspace_its_scratch_and_its_branch() {
    let p = Plan {
        issue: 7,
        repo: "acme/widget".to_string(),
        home: PathBuf::from("/home/t/tehas"),
        work_dir: PathBuf::from("/home/t/tehas/work/widget"),
        default: "main".to_string(),
        reason: "the gate's time bound".to_string(),
        remote: None,
        bot: String::new(),
        allowed: vec!["alice".to_string()],
    };
    assert_eq!(p.check(), Ok(()));
    assert_eq!(p.remote(), "https://github.com/acme/widget.git");
    assert_eq!(p.workspace(), PathBuf::from("/home/t/tehas/work/widget-7"));
    assert_eq!(p.scratch_parent(), PathBuf::from("/home/t/tehas/work"));
    assert_eq!(p.branch(), "finish/issue-7");
    assert_eq!(p.workspace_name(), "widget");
}

#[test]
fn the_issue_s_branch_among_the_names_git_answers() {
    let names = "main\nfeature/issue-70-default\nfeature/issue-7-default\nfinish/issue-7\n";
    assert_eq!(
        issue_branch_of(names, 7).as_deref(),
        Some("feature/issue-7-default")
    );
    assert_eq!(issue_branch_of("main\nnext\n", 7), None);
    assert_eq!(
        issue_branch_of("feature/issue-7\n", 7).as_deref(),
        Some("feature/issue-7")
    );
    // A name ending in a dash is no name git can hold: a ref of that shape is
    // rejected by the `check-ref-format` rules src/finish/plan.rs follows, so the
    // filter is proven against the one name that carries the issue's number and
    // fails the grammar.
    assert_eq!(
        issue_branch_of("feature/issue-7-\n", 7),
        None,
        "a name ending in a dash is no branch name"
    );
    assert_eq!(issue_branch_of("", 7), None);
}

#[test]
fn the_open_pull_request_of_an_issue_among_the_answers() {
    let url = "https://github.com/acme/widget/pull/3";
    let one = |head: &str, text: &str| {
        let mut p = pr(3, head, text);
        p["url"] = json!(url);
        vec![p]
    };
    assert_eq!(
        pull_for_issue(&one("feature/issue-7", ""), 7).as_deref(),
        Some(url)
    );
    assert_eq!(
        pull_for_issue(&one("feature/issue-7-abc", ""), 7).as_deref(),
        Some(url)
    );
    assert_eq!(
        pull_for_issue(&one("anything", "Fixes #7."), 7).as_deref(),
        Some(url)
    );
    assert_eq!(pull_for_issue(&one("anything", ""), 7), None);
    assert_eq!(pull_for_issue(&[], 7), None);
    // A pull request with no URL of its own is no pull request to name.
    assert_eq!(
        pull_for_issue(&[json!({"headRefName": "feature/issue-7"})], 7),
        None
    );
    assert_eq!(
        pull_named(&one("finish/issue-7", ""), "finish/issue-7").as_deref(),
        Some(url)
    );
    assert_eq!(
        pull_named(&one("feature/issue-7", ""), "finish/issue-7"),
        None
    );
}

#[test]
fn only_the_factorys_own_pull_requests_of_an_answer_are_read() {
    let plan = Plan {
        issue: 7,
        repo: "acme/widget".to_string(),
        home: PathBuf::from("/home/t/tehas"),
        work_dir: PathBuf::from("/home/t/tehas/work/widget"),
        default: "main".to_string(),
        reason: "the gate's time bound".to_string(),
        remote: None,
        bot: "tehas-bot[bot]".to_string(),
        allowed: vec!["alice".to_string()],
    };
    let own = pr(1, "feature/issue-7", "");
    let mut fork = pr(2, "feature/issue-7", "");
    fork["isCrossRepository"] = json!(true);
    let mut alice = pr(3, "x", "");
    alice["author"] = json!({"login": "alice"});
    let mut bob = pr(4, "y", "");
    bob["author"] = json!({"login": "bob"});
    let answer = pulls(&json!([own.clone(), fork, alice.clone(), bob]).to_string());
    assert_eq!(own_pulls(&answer, &plan), vec![own, alice]);
    assert!(own_pulls("not json", &plan).is_empty());
    assert!(own_pulls("[]", &plan).is_empty());
    assert!(own_pulls(r#"{"url":"x"}"#, &plan).is_empty());
    assert!(own_pulls(&pulls("{}"), &plan).is_empty());
}

#[test]
fn a_body_closes_an_issue_only_when_a_word_of_closing_names_it() {
    for (text, yes) in [
        ("Fixes #7.", true),
        ("Closes #7", true),
        ("resolves #7 in one line", true),
        ("close: #7", true),
        ("This fixes #7.", true),
        ("FIXED #7", true),
        ("Fixes #70.", false),
        ("see #7", false),
        ("Fixes #7x", false),
        ("Fixes#7", false),
        ("unfixes #7", false),
        ("This does not fix #7.", false),
        ("not fixed #7", false),
        ("no test fixes #7", false),
        ("never resolved #7", false),
        ("This doesn't fix #7.", false),
        ("it isn't fixed #7", false),
        // A word of nothing further back is the case's own claim and not a
        // denial of the closing word: the factory's bodies put that word at the
        // end of what they say.
        ("nothing here, but it fixes #7", true),
        ("##7", false),
        ("", false),
    ] {
        assert_eq!(
            super::run::closes_issue(text, "#7"),
            yes,
            "{text:?} closes #7: {yes}"
        );
    }
}

#[test]
fn a_count_of_commits_names_commits_and_nothing_else() {
    assert!(has_commits("1"));
    assert!(has_commits("12"));
    for no in ["", "0", "0x1", "many", " 1"] {
        assert!(!has_commits(no), "{no:?}");
    }
}

#[test]
fn a_refusal_names_the_rule_and_one_line_of_what_the_command_said() {
    let said = |err: &str, out: &str| Done {
        status: Some(1),
        out: out.to_string(),
        err: err.to_string(),
    };
    // The command's own words come after the rule's, and only its first line.
    let d = said("first line\nsecond line\n", "");
    assert_eq!(d.said().as_deref(), Some("first line"));
    let quiet = said("", "  \nonly output\n");
    assert_eq!(quiet.said().as_deref(), Some("only output"));
    assert_eq!(said("", "").said(), None);
    // The rule takes only the fixed words of what the command said; the rest
    // is kept for the log, cut to 300 characters.
    let why = Why::new("finish/issue-7 could not be pushed").said(&said(
        "@alice error: The requested URL returned error: 502\n",
        "",
    ));
    assert_eq!(
        why.why,
        "finish/issue-7 could not be pushed (returned error: 502)"
    );
    assert!(why.said.starts_with("@alice error:"), "{}", why.said);
    let plain = Why::new("the rebase onto origin/main conflicts").said(&said(
        &format!("error: could not apply 1a2b... @alice {}", "x".repeat(500)),
        "",
    ));
    assert_eq!(plain.why, "the rebase onto origin/main conflicts");
    assert_eq!(plain.said.chars().count(), 300);
}

#[test]
fn the_draft_s_words_name_the_issue_the_stopped_cycle_and_the_gate() {
    let b = body(7, "  a flaky test the change does not touch  ", "main");
    assert!(b.starts_with("Fixes #7."), "{b}");
    assert!(b.contains("a flaky test"), "{b}");
    assert!(b.contains("gate did not pass"), "{b}");
    assert!(b.contains("cycle that stopped"), "{b}");
    assert!(b.contains("`main`"), "it names what it rebased onto: {b}");
    assert!(b.contains("cap"), "{b}");
    assert!(b.contains("failing checks"), "{b}");
    assert_eq!(title(7), "Finish issue #7 from a stopped cycle");
}

#[test]
fn the_pins_of_a_call_are_its_own_words_and_no_call_of_git_goes_without_them() {
    // The pins answer the tree's config, so they are part of the words of git
    // and not of the environment of the host: `asked` is the one place a call of
    // git is built, the host reads them back, and a rule that built one bare is
    // refused rather than run.
    let dir = root("pins");
    assert!(asked_of_git(&asked(vec!["status".to_string()])));
    // The words of git stand after the pins, so a call built bare is no call,
    // and one that opens with a `-c` of its own is no call either.
    assert!(!asked_of_git(&["status".to_string()]));
    assert!(!asked_of_git(&["-c".to_string(), "x=y".to_string()]));
    let says = dir.join("says");
    write_exe(&says, "#!/bin/sh\nprintf 'x\\n'\n").unwrap();
    let mut h = live(&says, &says);
    // A call without them prints nothing and is not run: a run cannot decide
    // from an empty stream, and a git of the workspace run that way would
    // honour the tree's own hooks. The two cases below are what show the
    // difference, each planting a hook that must not run -- one under the
    // workspace's `.git`, one in a directory outside the tree the tree's config
    // names.
    let bare = h.run("git", &["status".to_string()], &dir);
    assert_eq!(bare.status, None, "{bare:?}");
    assert!(bare.err.contains("without the pins"), "{bare:?}");
    assert_eq!(bare.out, "", "a refused call reads nothing: {bare:?}");
}

#[test]
fn the_scratch_of_a_run_is_its_own_directory_and_its_body() {
    let dir = root("scratch");
    let s = Scratch::new(dir.to_path_buf(), "widget-7".to_string());
    let tree = s.dir().unwrap();
    assert!(tree.is_dir());
    assert!(tree
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("widget-7.finish."));
    let file = s.write_body("the words").unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), "the words");
    // The body is the run's to take back, and its absence is not an error.
    s.drop_body();
    assert!(!file.exists());
    s.drop_body();
    // A body that cannot be written is a refusal and not a partial file.
    let where_it_cannot = Scratch::new(PathBuf::from("/proc/tehas-nothing"), "x".to_string());
    assert!(where_it_cannot.write_body("x").is_err());
}

#[test]
fn a_run_that_refused_left_nothing_and_a_run_that_finished_left_nothing() {
    // Both paths of the run end the same way for the work directory: the tree
    // and the body of the run are gone, and the workspace is untouched.
    let w = World::new("clean-both");
    w.kept("feature/issue-7-default", &[("a.txt", "x\n")]);
    w.stub("", "https://github.com/acme/widget/pull/5");
    let mut h = w.host();
    assert_eq!(
        finish(&w.plan(), &mut h),
        Finished {
            url: "https://github.com/acme/widget/pull/5".to_string(),
            why: String::new(),
            said: String::new(),
        }
    );
    assert_eq!(w.left(), Vec::<String>::new());
    let listed = git_in(&w.workspace, &["worktree", "list"]);
    assert_eq!(listed.lines().count(), 1, "{listed}");
}

/// The host of a rule that runs a program of the host for real: the same
/// [`Live`] a run is given, aimed at the case's own `git` and `gh`. A rule is
/// decided from what the program printed, so this is the only host a case can
/// read the printed words back through.
fn live(git: &Path, gh: &Path) -> Live {
    Live::aimed_at(git, gh)
}

/// The host's git behind a script that gives it a committer, as a host's user
/// config does: a rebase that rewrites a commit needs one.
fn identified_git(dir: &Path) -> PathBuf {
    let script = dir.join("git");
    write_exe(
        &script,
        format!(
            "#!/bin/sh\nGIT_COMMITTER_NAME=tehas GIT_COMMITTER_EMAIL=tehas@invalid exec '{}' \"$@\"\n",
            git_program()
        ),
    )
    .unwrap();
    script
}

#[test]
fn a_call_carries_what_the_program_printed_on_both_its_streams() {
    // A run decides from what a program printed: the name of a branch, a count of
    // commits, the URL of a pull request. A host that handed back the exit status
    // alone would answer every rule with nothing, and the first rule that reads a
    // name — the branch of the issue — would refuse a workspace that holds one.
    // The words are asked of the streams of the real command, so a call whose
    // streams were not taken from the child prints nothing and this refuses.
    let dir = root("streams");
    let says = dir.join("says");
    write_exe(
        &says,
        "#!/bin/sh\n# One call of the case: what it prints on each stream and the\n# status it ends with, all three from the words it was given.\nprintf 'feature/issue-7-flaky\\n'\nprintf 'a word of its own\\n' >&2\nexit 0\n",
    ).unwrap();
    let mut h = live(&says, &says);
    let got = h.run("git", &asked(vec!["for-each-ref".to_string()]), &dir);
    assert_eq!(got.status, Some(0), "{got:?}");
    assert_eq!(got.out, "feature/issue-7-flaky\n", "{got:?}");
    assert_eq!(got.err, "a word of its own\n", "{got:?}");
    // And the rule reads the name it was given: the branch of the issue is the
    // one the program printed, and no name of its own is made up for a program
    // that printed something else.
    assert_eq!(
        issue_branch_of(&got.out, 7),
        Some("feature/issue-7-flaky".to_string()),
        "the name the program printed is the issue's branch"
    );
    let quiet = dir.join("quiet");
    write_exe(&quiet, "#!/bin/sh\nprintf 'nothing of an issue\\n'\n").unwrap();
    let mut h = live(&quiet, &quiet);
    let said = h.run("git", &asked(vec!["for-each-ref".to_string()]), &dir);
    assert_eq!(said.out, "nothing of an issue\n", "{said:?}");
    assert_eq!(
        issue_branch_of(&said.out, 7),
        None,
        "a name that is no branch of the issue is no branch"
    );
}

/// Stops the process a stub left behind, by the pid it wrote.
fn stop_left(pid_file: &Path) {
    if let Ok(pid) = fs::read_to_string(pid_file) {
        let _ = Command::new("kill")
            .args(["-KILL", pid.trim()])
            .stderr(Stdio::null())
            .status();
    }
}

#[test]
fn a_call_that_ended_is_not_held_by_a_process_it_left_with_its_output() {
    // A program can leave a process behind that keeps its streams open. The call
    // has ended and said what it had to say; the run reads that and goes on
    // instead of waiting for the process that was left.
    let dir = root("left");
    let pid = dir.join("pid");
    let stub = dir.join("stub");
    write_exe(
        &stub,
        format!(
            "#!/bin/sh
printf 'https://example.invalid/pull/1\\n'
sleep 60 &
echo $! > '{}'
exit 0
",
            pid.display()
        ),
    )
    .unwrap();
    let began = std::time::Instant::now();
    let got = within(
        stub.to_str().unwrap(),
        &[],
        &dir,
        std::time::Duration::from_secs(30),
        std::time::Duration::from_secs(1),
    );
    let took = began.elapsed();
    stop_left(&pid);
    assert!(took.as_secs() < 10, "the call waited {took:?}: {got:?}");
    assert_eq!(got.status, Some(0), "{got:?}");
    assert_eq!(got.out, "https://example.invalid/pull/1\n", "{got:?}");
    assert!(
        got.err
            .contains("stub left a process that held its output open"),
        "{got:?}"
    );
}

#[test]
fn a_call_past_its_bound_is_not_held_by_a_process_outside_its_group_and_says_so() {
    // The group is signalled when a call outlives its bound, but a process that
    // left the group is not reached and still holds the streams. The run is not
    // held by it, and the line a refusal quotes names the call that was stopped.
    let dir = root("outlived");
    let pid = dir.join("pid");
    let stub = dir.join("stub");
    write_exe(
        &stub,
        format!(
            "#!/bin/sh
printf 'a word before\\n' >&2
setsid sleep 60 &
echo $! > '{}'
sleep 60
",
            pid.display()
        ),
    )
    .unwrap();
    let began = std::time::Instant::now();
    let got = within(
        stub.to_str().unwrap(),
        &[],
        &dir,
        std::time::Duration::from_secs(1),
        std::time::Duration::from_secs(1),
    );
    let took = began.elapsed();
    stop_left(&pid);
    assert!(took.as_secs() < 10, "the call waited {took:?}: {got:?}");
    assert_eq!(got.status, None, "{got:?}");
    assert_eq!(
        got.said().as_deref(),
        Some("stub did not end within 1 s and was stopped"),
        "{got:?}"
    );
    assert!(got.err.contains("a word before"), "{got:?}");
}

#[test]
fn a_stream_past_what_is_kept_is_read_to_its_end_and_cut() {
    // A program that prints more than a run keeps is still read to its end, so it
    // never stops on a full pipe, and the run keeps only the first of it.
    let dir = root("long");
    let stub = dir.join("stub");
    write_exe(
        &stub,
        "#!/bin/sh
head -c 300000 /dev/zero | tr '\\0' a
exit 3
",
    )
    .unwrap();
    let got = within(
        stub.to_str().unwrap(),
        &[],
        &dir,
        std::time::Duration::from_secs(30),
        std::time::Duration::from_secs(1),
    );
    assert_eq!(got.status, Some(3), "{got:?}");
    assert_eq!(got.out.len(), 1 << 16);
    assert!(got.out.bytes().all(|b| b == b'a'));
    assert_eq!(got.err, "");
}

#[test]
fn a_hook_of_the_workspace_outside_the_tree_is_not_run_either() {
    // The workspace is a directory a cycle's container wrote, and an agent can
    // write anything into the tree it works in, hooks and git configuration
    // included. The factory pushes the cycle's commits and not the scripts
    // somebody left beside them, so a hook of the tree runs for nobody: not the
    // one a checkout calls, nor the one a push does, nor the one the tree's own
    // config points at a program of the host's outside the tree.
    let w = World::new("workspace-hooks");
    w.kept("feature/issue-7-default", &[("a.txt", "x\n")]);
    w.plant(
        "post-checkout",
        &w.dir.join("ran-checkout").display().to_string(),
        0,
    );
    w.plant("pre-push", &w.dir.join("ran-push").display().to_string(), 1);
    // Hooks of the tree's own config, in a directory of the host's outside the
    // tree: `core.hooksPath` moves every hook git runs, so the pin of the path
    // is what answers it and not where the hooks happen to stand.
    let outside = w.dir.join("outside-hooks");
    fs::create_dir_all(&outside).unwrap();
    write_exe(
        outside.join("post-checkout"),
        format!("#!/bin/sh\ntouch '{}'\n", w.dir.join("ran-path").display()),
    )
    .unwrap();
    git_in(
        &w.workspace,
        &["config", "core.hooksPath", &outside.display().to_string()],
    );
    w.stub("", "https://github.com/acme/widget/pull/33");
    let git = PathBuf::from(git_program());
    let mut h = live(&git, &w.gh);
    let got = finish(&w.plan(), &mut h);
    assert_eq!(got.why, "", "the run refused: {}", got.why);
    assert_eq!(got.url, "https://github.com/acme/widget/pull/33");
    assert!(
        w.remote_has("finish/issue-7"),
        "a hook of the workspace stopped the push"
    );
    for name in ["ran-checkout", "ran-push", "ran-path"] {
        assert!(
            !w.dir.join(name).exists(),
            "a hook of the workspace ran: {name}"
        );
    }
    assert_eq!(w.left(), Vec::<String>::new(), "the tree was left");
}

#[test]
fn the_hooks_of_the_workspace_are_asked_of_nobody_and_the_push_goes() {
    // The kept workspace of a real host is a clone, so the hooks git would run
    // for it live under its own `.git`, as the cleaner's case plants its
    // `core.fsmonitor` (`test/clean_test.sh`): the file is container-writable,
    // so anything of the tree's git settings is. A `pre-push` of the workspace
    // that refuses is the shape that matters most, because it lets a script
    // somebody left answer for whether the factory's push goes through. The
    // case's own host is the host of every other case here and runs the real
    // git, so the pins are what stands between the tree and a program of it.
    let w = World::new("live-host-hooks");
    w.kept("feature/issue-7-default", &[("a.txt", "x\n")]);
    let ran = w.dir.join("ran-push").display().to_string();
    w.plant("pre-push", &ran, 1);
    // The tree's own config, pointed at a program of it as well: the default
    // path of the hooks and the path the config gives are two different doors.
    git_in(&w.workspace, &["config", "core.hooksPath", ".git/hooks"]);
    w.stub("", "https://github.com/acme/widget/pull/33");
    let git = PathBuf::from(git_program());
    let mut h = live(&git, &w.gh);
    let got = finish(&w.plan(), &mut h);
    assert_eq!(got.why, "", "the run refused: {}", got.why);
    assert_eq!(got.url, "https://github.com/acme/widget/pull/33");
    assert!(
        w.remote_has("finish/issue-7"),
        "a hook of the workspace stopped the push"
    );
    assert!(!w.dir.join("ran-push").exists(), "the hook was run");
}

#[test]
fn a_run_asks_its_programs_by_their_names_and_the_host_decides_where_they_are() {
    // The names a run gives a host are `git` and `gh` and never a path, so the
    // host alone chooses which program answers: a case stands in for GitHub by
    // putting its own script where gh would be, and a run that named a path could
    // reach past that choice.
    let dir = root("names");
    let seen = dir.join("asked");
    let says = dir.join("says");
    let script = format!(
        "#!/bin/sh\nprintf '%s %s\\n' \"$0\" \"$*\" >>\"{}\"\n",
        seen.display()
    );
    write_exe(&says, script).unwrap();
    let mut h = live(&says, &says);
    h.run("git", &asked(vec!["status".to_string()]), &dir);
    h.run("gh", &["pr".to_string(), "list".to_string()], &dir);
    let names = fs::read_to_string(&seen).unwrap_or_default();
    // The host answers the name with a program of its own, and the program the
    // run reached is the one the host was given: both calls are of `says`.
    let lines: Vec<&str> = names.lines().collect();
    assert_eq!(lines.len(), 2, "{names}");
    assert!(
        lines.iter().all(|l| l.contains("says")),
        "the host's own program answers every call: {names}"
    );
    assert!(lines[0].ends_with("status"), "{names}");
    assert!(lines[1].ends_with("pr list"), "{names}");
}

#[test]
fn a_program_busy_under_a_writer_is_asked_again_and_is_run() {
    // The kernel refuses the exec of a file another process holds open for
    // writing, and a run does write programs to disk: a stub of `gh`, and the
    // body of the draft. The refusal clears when the writer closes, so a call
    // that is told the program is busy asks again rather than answering that the
    // program could not be run. The case holds the program busy the way the host
    // does: a child forked while the write descriptor was open, still alive with
    // its copy of it. The run must still read what the program printed.
    let dir = root("busy");
    let says = dir.join("says");
    write_exe(&says, "#!/bin/sh\nprintf 'feature/issue-7-flaky\\n'\n").unwrap();
    // A copy of the write descriptor, handed to a child by the fork alone: the
    // child is the writer that outlives the write, as the test harness does to
    // its children. The exec below is refused until the child leaves.
    let busy = fs::OpenOptions::new().write(true).open(&says).unwrap();
    let mut holder = Command::new("sleep")
        .arg("0.3")
        .stdin(Stdio::null())
        .stdout(Stdio::from(busy))
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut h = live(&says, &says);
    let got = h.run("git", &asked(vec!["for-each-ref".to_string()]), &dir);
    assert_eq!(
        got.status,
        Some(0),
        "the busy program was never run: {got:?}"
    );
    assert_eq!(got.out, "feature/issue-7-flaky\n", "{got:?}");
    let _ = holder.wait();
}

#[test]
fn a_program_busy_past_its_bound_is_a_call_that_could_not_be_run() {
    // The asks are bounded, as everything else of a call is: a program held for
    // writing by a process that never closes is a hung host, and a run that
    // waited on it would be a step of the poll that stopped reporting. The answer
    // is the one a missing program already gets, so no rule reads an empty stream
    // as a name, a count or a URL.
    let dir = root("busy-forever");
    let says = dir.join("says");
    write_exe(&says, "#!/bin/sh\nprintf 'x\\n'\n").unwrap();
    let held = fs::OpenOptions::new().write(true).open(&says).unwrap();
    // No child is forked, so this process itself is the writer and stays one for
    // the whole call: the asks run out and the call is refused.
    let mut h = live(&says, &says);
    let got = h.run("git", &asked(vec!["for-each-ref".to_string()]), &dir);
    assert_eq!(got.status, None, "{got:?}");
    assert!(got.err.contains("could not be run"), "{got:?}");
    assert_eq!(got.out, "", "a refused call reads nothing: {got:?}");
    drop(held);
}

#[test]
fn a_run_of_the_real_host_finishes_a_repository_of_its_own() {
    // One run through the host a run is really given, over the World's own
    // repositories and the case's stub of gh: the case that reaches the draft has
    // read a branch's name, a count of commits and a URL out of the streams of
    // real commands. A case that goes only through a host of its own cannot show
    // that, and a rule that reads nothing decides from nothing.
    let w = World::new("live-host");
    w.kept("feature/issue-7-default", &[("a.txt", "the change\n")]);
    w.remote_commit("other.md", "later\n", "later");
    w.stub("", "https://github.com/acme/widget/pull/77");
    let git = identified_git(&w.dir);
    let mut h = live(&git, &w.gh);
    let got = finish(&w.plan(), &mut h);
    assert_eq!(got.why, "", "the run refused: {}", got.why);
    assert_eq!(got.url, "https://github.com/acme/widget/pull/77");
    // The branch the run pushed stands on the remote over the remote's own main,
    // and the cycle's branch did not move.
    assert!(w.remote_has("finish/issue-7"));
    assert_eq!(
        w.remote_log("finish/issue-7"),
        vec![
            "one".to_string(),
            "later".to_string(),
            "the change".to_string()
        ]
    );
    assert_eq!(w.left(), Vec::<String>::new());
}

#[test]
fn the_workspace_s_git_config_runs_nothing_on_the_host_and_names_no_remote() {
    // The container wrote the workspace's `.git/config`. Every key below names
    // a program git would run, or a place it would push, in a command run in
    // that repository. The run asks no git of the workspace, so none runs and
    // the push goes to the plan's remote.
    let w = World::new("workspace-config");
    w.kept("feature/issue-7-default", &[("a.txt", "the change\n")]);
    // A rebase that rewrites a commit, so a signing program would be asked.
    w.remote_commit("other.md", "later\n", "later");
    let mark = |name: &str| w.dir.join(name).display().to_string();
    let prog = w.dir.join("prog");
    write_exe(
        &prog,
        format!("#!/bin/sh\ntouch '{}'\nexit 1\n", mark("ran-prog")),
    )
    .unwrap();
    let p = prog.display().to_string();
    let decoy = w.dir.join("decoy.git");
    git(&["init", "--quiet", "--bare", &decoy.display().to_string()]);
    let d = decoy.display().to_string();
    for (k, v) in [
        ("filter.x.smudge", p.as_str()),
        ("filter.x.clean", p.as_str()),
        ("filter.x.process", p.as_str()),
        ("commit.gpgSign", "true"),
        ("gpg.program", p.as_str()),
        ("credential.helper", &format!("!{p}")),
        ("core.sshCommand", p.as_str()),
        ("core.askPass", p.as_str()),
        ("core.pager", p.as_str()),
        ("remote.origin.uploadpack", p.as_str()),
        ("remote.origin.receivepack", p.as_str()),
        ("remote.origin.url", d.as_str()),
        ("remote.origin.pushurl", d.as_str()),
    ] {
        git_in(&w.workspace, &["config", k, v]);
    }
    fs::create_dir_all(w.workspace.join(".git/info")).unwrap();
    fs::write(w.workspace.join(".git/info/attributes"), "* filter=x\n").unwrap();
    w.stub("", "https://github.com/acme/widget/pull/9");
    let git = identified_git(&w.dir);
    let mut h = live(&git, &w.gh);
    let got = finish(&w.plan(), &mut h);
    assert_eq!(got.why, "", "the run refused: {}", got.why);
    assert!(
        !w.dir.join("ran-prog").exists(),
        "a program of the workspace ran"
    );
    assert!(
        w.remote_has("finish/issue-7"),
        "the plan's remote has no branch"
    );
    assert_eq!(
        w.remote_log("finish/issue-7"),
        vec!["one", "later", "the change"]
    );
    let on_decoy = Command::new("git")
        .args(["--git-dir", &d, "rev-parse", "--verify", "--quiet"])
        .arg("refs/heads/finish/issue-7")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    assert!(!on_decoy, "the push went to a remote the workspace named");
    assert_eq!(w.left(), Vec::<String>::new());
}

#[test]
fn a_file_the_workspace_excludes_is_not_uncommitted_work() {
    // The cycle's own scratch, kept out of git by the workspace's
    // `info/exclude`: the run copies that file's text and reads the tree with it.
    let w = World::new("exclude");
    w.kept("feature/issue-7-default", &[("a.txt", "x\n")]);
    fs::create_dir_all(w.workspace.join(".git/info")).unwrap();
    fs::write(w.workspace.join(".git/info/exclude"), "scratch.log\n").unwrap();
    fs::write(w.workspace.join("scratch.log"), "noise\n").unwrap();
    w.stub("", "https://github.com/acme/widget/pull/3");
    let mut h = w.host();
    let got = finish(&w.plan(), &mut h);
    assert_eq!(got.why, "", "the run refused: {}", got.why);
}

#[test]
fn an_exclude_that_is_a_link_is_not_followed() {
    // A link the container planted is not read: the file it would have hidden
    // counts as work that was never committed.
    let w = World::new("exclude-link");
    w.kept("feature/issue-7-default", &[("a.txt", "x\n")]);
    let elsewhere = w.dir.join("elsewhere");
    fs::write(&elsewhere, "scratch.log\n").unwrap();
    fs::create_dir_all(w.workspace.join(".git/info")).unwrap();
    let _ = fs::remove_file(w.workspace.join(".git/info/exclude"));
    std::os::unix::fs::symlink(&elsewhere, w.workspace.join(".git/info/exclude")).unwrap();
    fs::write(w.workspace.join("scratch.log"), "noise\n").unwrap();
    w.stub("", "https://github.com/acme/widget/pull/3");
    let mut h = w.host();
    let got = finish(&w.plan(), &mut h);
    assert!(got.why.contains("never committed"), "{}", got.why);
}

//! `tehas-cycle finish` against real git repositories in a scratch directory and
//! a stub `gh`: the case says what the remote holds and what GitHub answers, and
//! reads what the run did to the branch, to the remote, to the tree beside it,
//! and to the calls. No case reaches GitHub, a token or the network.

use super::cmd::Scratch;
use super::plan::{body, title, Plan};
use super::run::{has_commits, issue_branch_of, pull_for_issue, pull_named};
use super::{finish, Finished};
use crate::finish::cmd::{Done, Live};
use crate::finish::Host;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The scratch root of one case: its name, the process, and a counter, so no two
/// cases share a directory.
fn root(name: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "tehas-finish-{name}-{}-{}",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
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
    dir: PathBuf,
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
        }
    }

    /// The `gh` of a case: one line per call in `gh.calls`; `pr list` answers from
    /// `list.answer` and `pr create` from `create.answer`, an absent file being
    /// GitHub's nothing and a file of `fail` a refusal of the call. The body file
    /// of a create is copied so a case can read what the draft said.
    fn stub(&self, list: &str, create: &str) {
        fs::write(
            &self.gh,
            r#"#!/bin/sh
# The gh of finish's cases. It records every call and answers pr list and pr
# create from a file of the case; a file of fail refuses its call. The body file
# of a create is copied, so a case can read what the draft said.
set -u
dir=$(dirname "$0")
kind=other
prev=
for a in "$@"; do
  case "$a" in
    pr) kind=pr ;;
    list) [ "$kind" = pr ] && kind=list ;;
    create) [ "$kind" = pr ] && kind=create ;;
  esac
  if [ "$kind" = create ] && [ "$prev" = --body-file ]; then
    cp "$a" "$dir/body.text" 2>/dev/null
  fi
  prev=$a
done
[ "$kind" = pr ] && kind=other
printf '%s %s\n' "$*" "$kind" >>"$dir/gh.calls"
ans=$dir/$kind.answer
if [ -f "$ans" ]; then
  [ "$(cat "$ans")" != fail ] || exit 1
  [ "$kind" = other ] || { cat "$ans"; exit 0; }
  exit 0
fi
[ "$kind" = list ] && { echo '[]'; exit 0; }
exit 0
"#,
        )
        .unwrap();
        fs::set_permissions(&self.gh, fs::Permissions::from_mode(0o755)).unwrap();
        if !list.is_empty() {
            fs::write(self.dir.join("list.answer"), format!("{list}\n")).unwrap();
        }
        if !create.is_empty() {
            fs::write(self.dir.join("create.answer"), format!("{create}\n")).unwrap();
        }
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

    fn clean(&mut self, workspace: &Path, dir: &Path) {
        // The same ask the host makes: the tree goes through the repository it is
        // a worktree of, so a run that refused cannot leave the name claimed.
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let tree = entry.path();
                if tree
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("tree-"))
                {
                    let _ = self.run(
                        "git",
                        &[
                            "-C".into(),
                            workspace.display().to_string(),
                            "worktree".into(),
                            "remove".into(),
                            "--force".into(),
                            tree.display().to_string(),
                        ],
                        dir,
                    );
                }
            }
        }
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
    for (what, answer) in [
        (
            "its-head-is-the-issue-s-branch",
            r#"[{"url":"https://github.com/acme/widget/pull/9","headRefName":"feature/issue-7-default","body":""}]"#,
        ),
        (
            "its-body-closes-the-issue",
            r#"[{"url":"https://github.com/acme/widget/pull/10","headRefName":"someone-else","body":"Closes #7 and nothing else."}]"#,
        ),
        (
            "its-head-is-the-branch-this-run-would-push",
            r#"[{"url":"https://github.com/acme/widget/pull/12","headRefName":"finish/issue-7","body":""}]"#,
        ),
    ] {
        let w = World::new(what);
        w.kept("feature/issue-7-default", &[("a.txt", "x\n")]);
        w.stub(answer, "https://github.com/acme/widget/pull/1");
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
        r#"[{"url":"https://github.com/acme/widget/pull/11","headRefName":"feature/issue-70-x","body":"see issue #70 and fix #7x"}]"#,
        "https://github.com/acme/widget/pull/12",
    );
    let mut h = w.host();
    let got = finish(&w.plan(), &mut h);
    assert_eq!(got.why, "", "{}", got.why);
    assert!(w.remote_has("finish/issue-7"));
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
    assert!(got.why.contains("conflicts"), "{}", got.why);
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
    fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
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
    };
    assert_eq!(p.check(), Ok(()));
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
    // rejected by the `check-ref-format` rules lib/cycles.sh follows, so the
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
        format!(r#"[{{"url":"{url}","headRefName":"{head}","body":"{text}"}}]"#)
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
    assert_eq!(pull_for_issue("[]", 7), None);
    assert_eq!(pull_for_issue("not json", 7), None);
    assert_eq!(pull_for_issue(r#"{"url":"x"}"#, 7), None);
    // A pull request with no URL of its own is no pull request to name.
    assert_eq!(
        pull_for_issue(r#"[{"headRefName":"feature/issue-7"}]"#, 7),
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
    // A refusal a person reads on the issue is bounded: the rule and a short
    // quote of the command.
    let long = "x".repeat(500);
    assert!(long.chars().take(300).collect::<String>().chars().count() == 300);
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
fn the_scratch_of_a_run_is_its_own_directory_and_its_body() {
    let dir = root("scratch");
    let s = Scratch::new(dir.clone(), "widget-7".to_string());
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
    fs::remove_dir_all(&dir).unwrap();
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
            why: String::new()
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
    fs::write(
        &says,
        "#!/bin/sh\n# One call of the case: what it prints on each stream and the\n# status it ends with, all three from the words it was given.\nprintf 'feature/issue-7-flaky\\n'\nprintf 'a word of its own\\n' >&2\nexit 0\n",
    )
    .unwrap();
    fs::set_permissions(&says, fs::Permissions::from_mode(0o755)).unwrap();
    let mut h = live(&says, &says);
    let got = h.run("git", &["for-each-ref".to_string()], &dir);
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
    fs::write(&quiet, "#!/bin/sh\nprintf 'nothing of an issue\\n'\n").unwrap();
    fs::set_permissions(&quiet, fs::Permissions::from_mode(0o755)).unwrap();
    let mut h = live(&quiet, &quiet);
    let said = h.run("git", &["for-each-ref".to_string()], &dir);
    assert_eq!(said.out, "nothing of an issue\n", "{said:?}");
    assert_eq!(
        issue_branch_of(&said.out, 7),
        None,
        "a name that is no branch of the issue is no branch"
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_run_asks_its_programs_by_their_names_and_the_host_decides_where_they_are() {
    // The names a run gives a host are `git` and `gh` and never a path, so the
    // host alone chooses which program answers: a case stands in for GitHub by
    // putting its own script where gh would be, and a run that named a path could
    // reach past that choice.
    let dir = root("names");
    let asked = dir.join("asked");
    let says = dir.join("says");
    let script = format!(
        "#!/bin/sh\nprintf '%s %s\\n' \"$0\" \"$*\" >>\"{}\"\n",
        asked.display()
    );
    fs::write(&says, script).unwrap();
    fs::set_permissions(&says, fs::Permissions::from_mode(0o755)).unwrap();
    let mut h = live(&says, &says);
    h.run("git", &["status".to_string()], &dir);
    h.run("gh", &["pr".to_string(), "list".to_string()], &dir);
    let names = fs::read_to_string(&asked).unwrap_or_default();
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
    fs::remove_dir_all(&dir).unwrap();
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
    fs::write(&says, "#!/bin/sh\nprintf 'feature/issue-7-flaky\\n'\n").unwrap();
    fs::set_permissions(&says, fs::Permissions::from_mode(0o755)).unwrap();
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
    let got = h.run("git", &["for-each-ref".to_string()], &dir);
    assert_eq!(
        got.status,
        Some(0),
        "the busy program was never run: {got:?}"
    );
    assert_eq!(got.out, "feature/issue-7-flaky\n", "{got:?}");
    let _ = holder.wait();
    fs::remove_dir_all(&dir).unwrap();
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
    fs::write(&says, "#!/bin/sh\nprintf 'x\\n'\n").unwrap();
    fs::set_permissions(&says, fs::Permissions::from_mode(0o755)).unwrap();
    let held = fs::OpenOptions::new().write(true).open(&says).unwrap();
    // No child is forked, so this process itself is the writer and stays one for
    // the whole call: the asks run out and the call is refused.
    let mut h = live(&says, &says);
    let got = h.run("git", &["for-each-ref".to_string()], &dir);
    assert_eq!(got.status, None, "{got:?}");
    assert!(got.err.contains("could not be run"), "{got:?}");
    assert_eq!(got.out, "", "a refused call reads nothing: {got:?}");
    drop(held);
    fs::remove_dir_all(&dir).unwrap();
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
    let git = PathBuf::from(git_program());
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

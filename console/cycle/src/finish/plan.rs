//! What a `finish` run is given, and the rules of those words before anything is
//! asked of the host.
//!
//! The words are the ones part 2's poll step passes down. The command reads no
//! settings of its own, so the instance's settings stay the settings of the
//! command that asks, and a word that is no word is refused here rather than in
//! the middle of a push.

use std::path::{Path, PathBuf};

/// The branch the finished work is pushed to, named for the issue alone.
pub const BRANCH_PREFIX: &str = "finish/issue-";

/// What a run was given: the issue, the repository its work belongs to, the home
/// whose scratch the run works beside, the kept workspace's name and the branch
/// it rebases onto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub issue: u64,
    /// `owner/name`, as the settings give it. Every GitHub call is asked of it.
    pub repo: String,
    pub home: PathBuf,
    /// `TEHAS_WORK_DIR`: `<home>/work/<name>`, so `<home>/work/<name>-<issue>`
    /// is the kept workspace of this issue.
    pub work_dir: PathBuf,
    /// The default branch, `main` on most hosts.
    pub default: String,
    /// Why the failure is outside the change, in the asker's words.
    pub reason: String,
}

impl Plan {
    /// The rules of the words, as far as they reach before anything is asked of
    /// the host. What they cannot decide is decided by the host's answer, in
    /// [`super::run`].
    pub fn check(&self) -> Result<(), String> {
        if !issue_shaped(&self.issue.to_string()) {
            return Err(format!("{} is no issue number", self.issue));
        }
        if !repo_shaped(&self.repo) {
            return Err(format!("{} is not owner/name", self.repo));
        }
        if !self.home.is_absolute() {
            return Err(format!("{} is not an absolute path", self.home.display()));
        }
        if !self.work_dir.is_absolute() {
            return Err(format!(
                "{} is not an absolute path",
                self.work_dir.display()
            ));
        }
        let name = self.workspace_name();
        if !work_name_shaped(&name) {
            return Err(format!("{name} is no workspace name"));
        }
        if !branch_shaped(&self.default) {
            return Err(format!("{} is no branch name", self.default));
        }
        if self.reason.trim().is_empty() {
            return Err("the reason is nothing".to_string());
        }
        Ok(())
    }

    /// The name of the work directory, the first part of a workspace's name.
    pub fn workspace_name(&self) -> String {
        self.work_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string()
    }

    /// The kept workspace of this issue: the name `tehas work` gave the tree the
    /// cycle worked in, which the poll keeps when the cycle stops.
    pub fn workspace(&self) -> PathBuf {
        let name = format!("{}-{}", self.workspace_name(), self.issue);
        match self.work_dir.parent() {
            Some(parent) => parent.join(name),
            None => self.work_dir.join(name),
        }
    }

    /// The directory the run's scratch is made in: beside the workspace.
    pub fn scratch_parent(&self) -> PathBuf {
        self.workspace()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.home.join("tmp"))
    }

    /// The branch the finished work is pushed to.
    pub fn branch(&self) -> String {
        format!("{BRANCH_PREFIX}{}", self.issue)
    }
}

/// An issue number: 1..=10 digits, no leading zero.
fn issue_shaped(word: &str) -> bool {
    let b = word.as_bytes();
    !b.is_empty() && b.len() <= 10 && b[0] != b'0' && b.iter().all(u8::is_ascii_digit)
}

/// Whether a word is `owner/name`: the shape `lib/config.sh` checks the setting
/// for, under this command's ceiling for a part. A name that is no repository is
/// refused before a call rather than aimed at something else.
fn repo_shaped(repo: &str) -> bool {
    let part = |p: &str| {
        !p.is_empty()
            && p.len() <= 100
            && p.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    };
    let Some((owner, name)) = repo.split_once('/') else {
        return false;
    };
    !name.contains('/') && part(owner) && part(name)
}

/// Whether a workspace's name is one a path may be built from: word characters,
/// `-`, `_` and `.`. A name of anything else is refused rather than made into a
/// directory somewhere else.
fn work_name_shaped(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// Whether a name is one git would call a branch: word characters, `/`, `.` and
/// `-`, with no empty element, no `..`, and none of `.` and `/` at either end or
/// beside one another. `main` and `finish/issue-7` are names of it; a name that
/// is an option, or a path that walks elsewhere, is not.
pub fn branch_shaped(name: &str) -> bool {
    let b = name.as_bytes();
    if b.is_empty() || b.len() > 200 {
        return false;
    }
    let ok = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'/' | b'-');
    // The rules of `git check-ref-format --branch`, which lib/cycles.sh follows
    // for the name a cycle is given: no component begins or ends with a dot or a
    // dash, none ends in `.lock`, no `..` and no `//`, and no character outside
    // the set git allows. A name git itself rejects is no branch name, and a run
    // that took one would ask git to refuse it at every turn.
    b.iter().all(|c| ok(*c))
        && !name.starts_with('.')
        && !name.ends_with('.')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("..")
        && !name.contains("//")
        && !name.contains("./")
        && !name.contains("/.")
        && !name.contains("/-")
        && !name.ends_with(".lock")
}

/// What one run did: the pull request it opened, or the one line of why it
/// refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finished {
    /// The pull request's URL, nothing when it opened nothing.
    pub url: String,
    /// Why it is not done, empty when it is.
    pub why: String,
}

impl Finished {
    pub fn refused(why: impl Into<String>) -> Finished {
        Finished {
            url: String::new(),
            why: why.into(),
        }
    }
}

/// The draft's title: what the pull request is, and what it is not.
pub fn title(issue: u64) -> String {
    format!("Finish issue #{issue} from a stopped cycle")
}

/// What the draft says. The reason is the asker's words; the rest is the
/// factory's, and says in one line that the gate did not pass, so no reviewer is
/// told a finished pull request was a green one. The cap and the failing checks
/// are what CI and a reviewer answer for this pull request: what the asker reads
/// from `factory_park` and writes into the reason.
pub fn body(issue: u64, reason: &str, default: &str) -> String {
    format!(
        "Fixes #{issue}.\n\n\
         The factory opened this pull request from a cycle that stopped: {}\n\n\
         Its gate did not pass. What is here is the branch the stopped cycle \
         left, rebased onto `{default}` and pushed as it stood, so the cap, the \
         failing checks and the review decide on it as they would on any other \
         pull request. Nothing here merged on its own.\n\n\
         Opened by tehas-cycle finish for issue #{issue}.\n",
        reason.trim()
    )
}

//! The easy merge of one pull request: merge the branch into the pull
//! request's head, settle the files a rule can settle without a model, ask one
//! bare `pi` session about the rest, and push the one merge commit.
//!
//! The decisions are here and nothing else: `libexec/easy-merge` is the few
//! lines that start this in the sandbox container, and the container is given
//! what a cycle is given (lib/sandbox.sh), so no mount or variable is added
//! here. The pull request's CI is the gate: this command runs none of the
//! product's own.
//!
//! A command is a program and its arguments, run in the tree; `Runner` says
//! what happened and names the command in the words of a refusal, so a test
//! stands in for git, for cargo and for pi without any of them being there.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// How one command ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    /// The exit status, None when the command did not end in its bound.
    pub status: Option<i32>,
    pub out: String,
    pub err: String,
}

impl Done {
    fn ok(&self) -> bool {
        self.status == Some(0)
    }
}

/// What runs a command. `program` is a path the caller was given, never a word
/// the merge makes up; `tree` is the working directory.
pub trait Runner {
    fn run(&mut self, program: &str, args: &[String], tree: &Path) -> Done;

    /// What has been asked of this runner, in order: the program and its
    /// arguments, one line each. A real runner answers nothing; a test's
    /// answers what it was asked, which is how a case says `no model turn`.
    fn calls(&mut self) -> Vec<String> {
        Vec::new()
    }
}

/// A command the merge could not settle, in the words it refuses with: the
/// rule's own sentence and, where the command said something, what it said.
/// A refusal also says whether a session of `pi` had been asked by then, so
/// the answer the dispatcher reads cannot lose that fact on a path that
/// carries no outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Why {
    pub why: String,
    pub asked_model: bool,
}

/// What one easy merge did, in the words its caller reads: the pull request,
/// the outcome, and the commands that ran. One JSON line on standard output,
/// which is what `libexec/easy-merge` and the dispatcher's log keep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub pr: u64,
    pub issue: u64,
    pub outcome: Outcome,
    /// The commands that ran, named as the plan named them: the program's
    /// last part and its arguments. What a host's log holds, and what a case
    /// says `no model turn` from — a session asked is one line beginning
    /// `pi -p`, whatever path the image holds pi at.
    pub calls: Vec<String>,
}

impl Report {
    pub fn json(&self) -> Value {
        let mut v = self.outcome.json();
        v["pr"] = json!(self.pr);
        v["issue"] = json!(self.issue);
        v["calls"] = json!(self.calls);
        v
    }
}

impl Why {
    fn new(why: impl Into<String>) -> Why {
        Why {
            why: why.into(),
            asked_model: false,
        }
    }

    /// A refusal raised after the session was asked: the merge still says a
    /// model was spent on the pull request.
    fn asked(mut self) -> Why {
        self.asked_model = true;
        self
    }

    /// The command's own words, cut to one line and to 300 characters: a
    /// refusal goes to the issue's comment and to the log.
    fn said(mut self, d: Done) -> Why {
        let words = first_line(&d.err).or_else(|| first_line(&d.out));
        if let Some(w) = words.filter(|w| !w.is_empty()) {
            self.why = format!("{}: {}", self.why, cut(&w));
        }
        self
    }
}

fn first_line(s: &str) -> Option<String> {
    s.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

/// A command's words cut to 300 characters: what a refusal and the log hold.
fn cut(s: &str) -> String {
    s.chars().take(300).collect()
}

/// A file the rules settle without a model: what to write into it is decided
/// here, so a conflict in one of them needs no model turn at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// A lockfile: the merge's own answer is no answer, so it is written
    /// again from the manifests of the merged tree, by the tool of the
    /// lockfile's kind. A kind whose tool this command was not given is not
    /// settled: see `lockfile_tool`.
    Regenerate,
    /// The bash ceiling: the branch's number is the one that stands, as it
    /// holds every merge that came before this pull request.
    BranchSide,
}

/// The kind of a lockfile, as the name of the tool that writes it again, or
/// empty for a file that is no lockfile. The name alone decides, so a
/// product's own lockfile of the same name is the same kind of file.
///
/// The two lockfiles the platform keeps are cargo's under `console/` and npm's
/// under the sandbox's npm directory. `npm` is a kind and not a program this
/// command is ever given, so a lockfile of that kind is left for the model
/// rather than guessed at with a tool nobody named for it.
pub fn lockfile_tool(path: &str) -> &'static str {
    match path.rsplit('/').next().unwrap_or(path) {
        "Cargo.lock" => "cargo",
        "package-lock.json" => "npm",
        _ => "",
    }
}

/// Whether one file is settled by a rule, and by which.
pub fn rule_of(path: &str) -> Option<Rule> {
    if !lockfile_tool(path).is_empty() {
        return Some(Rule::Regenerate);
    }
    is_name(path, "bash-lines.max").then_some(Rule::BranchSide)
}

/// The lines of `git diff --name-only` that name one file exactly.
fn is_path(lines: &[String], path: &str) -> bool {
    lines.iter().any(|l| l == path)
}

/// Whether one name is the last part of a path, which is how `rule_of` reads
/// a file. It agrees with `rule_of` by construction: the name a path's last
/// part is a rule's name is the name `rule_of` matched, and neither reads a
/// file whose name only ends with the rule's (`Cargo.lock.old`, or
/// `src/bash-lines.max.rs`) — were they to disagree, the outsider check would
/// refuse the merge for a file its own rule had just written again.
fn is_name(path: &str, name: &str) -> bool {
    path.rsplit('/').next().unwrap_or(path) == name
}

/// The same commit as git would name it, or `None` for anything that is not
/// 40 hex digits. Case is the only thing this settles: git's `rev-parse`
/// answers in lower case and a person or a tool may spell the sha any other
/// way, so the spelling the command accepts and the spelling the merge
/// compares with are one set, decided here rather than a file apart.
pub fn normalise_commit(sha: &str) -> Option<String> {
    let sha = sha.trim();
    if sha.len() != 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(sha.to_ascii_lowercase())
}

/// The merge's verdict, and what it did to reach it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// The merge commit's sha, None when nothing was pushed.
    pub sha: Option<String>,
    /// The files that conflicted, as git named them.
    pub conflicts: Vec<String>,
    /// The files the rules settled.
    pub settled: Vec<String>,
    /// Whether the `pi` session was asked.
    pub model: bool,
    /// Whether the commit was pushed.
    pub pushed: bool,
    /// Why it is not done, empty when it is.
    pub why: String,
}

impl Outcome {
    fn started(conflicts: Vec<String>) -> Outcome {
        Outcome {
            sha: None,
            conflicts,
            settled: Vec::new(),
            model: false,
            pushed: false,
            why: String::new(),
        }
    }

    /// The command's own words, put after the rule's: a refusal goes to the
    /// issue's comment and to the poll's log, and what the command said is the
    /// part a person can act on. One line, cut to 300 characters.
    fn said(mut self, d: Done) -> Outcome {
        if let Some(w) = first_line(&d.err)
            .or_else(|| first_line(&d.out))
            .filter(|w| !w.is_empty())
        {
            self.why = format!("{}: {}", self.why, cut(&w));
        }
        self
    }

    /// What the dispatcher reads: `ok` and, when it is false, the reason.
    pub fn json(&self) -> Value {
        json!({
            "ok": self.ok(),
            "sha": self.sha.clone(),
            "conflicts": self.conflicts,
            "settled": self.settled,
            "model": self.model,
            "pushed": self.pushed,
            "why": self.why,
        })
    }

    pub fn ok(&self) -> bool {
        self.why.is_empty() && self.sha.is_some() && self.pushed
    }
}

/// Everything the merge was given: the two commits, where its tree is, and the
/// programs it may run. A test makes one with stubs in place of git, cargo and
/// pi; a host makes one from the settings.
pub struct Plan<'a> {
    /// The issue whose pull request this is, for the log and the answer.
    pub issue: u64,
    /// The pull request's head, the commit the merge starts from.
    pub head: &'a str,
    /// The branch's commit, `origin/<TEHAS_BRANCH>` as the caller read it.
    pub base: &'a str,
    /// The branch's name, for the merge message.
    pub branch: &'a str,
    /// The pull request's number, for the merge message and the log.
    pub pr: u64,
    /// The tree to work in, already the pull request's head.
    pub tree: PathBuf,
    /// The name the pull request's branch is pushed back to.
    pub branch_ref: &'a str,
    /// git, cargo and pi: paths the caller was given.
    pub git: String,
    pub cargo: String,
    pub pi: String,
    /// Whether the `pi` session may be asked at all: an instance with no
    /// model endpoint asks nothing and a conflict in code is not easy.
    pub model: bool,
}

impl Plan<'_> {
    fn git_of(&mut self, r: &mut dyn Runner, args: &[&str]) -> Done {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        r.run(&self.git, &args, &self.tree)
    }
}

/// The calls a runner holds, named by the last part of the program: what a
/// host's log keeps, and what a case reads to say `no model turn`.
pub fn short_calls(calls: &[String]) -> Vec<String> {
    calls
        .iter()
        .map(|c| {
            let (program, args) = c.split_once(' ').unwrap_or((c.as_str(), ""));
            format!("{} {args}", program.rsplit('/').next().unwrap_or(program))
        })
        .collect()
}

/// The merge and what it did, for the command's answer and its log.
pub fn report(plan: &mut Plan, r: &mut dyn Runner) -> Report {
    let pr = plan.pr;
    let issue = plan.issue;
    // A refusal carries whether a session was asked, so the answer never
    // comes from the calls the runner holds: on a host `Real` records none.
    let outcome = match merge_and_settle(plan, r) {
        Ok(out) if out.why.is_empty() => out,
        Ok(out) => {
            abandon(plan, r);
            out
        }
        Err(why) => {
            abandon(plan, r);
            let mut out = Outcome::started(Vec::new());
            out.why = why.why;
            out.model = why.asked_model;
            out
        }
    };
    let calls = r.calls();
    Report {
        pr,
        issue,
        outcome,
        calls: short_calls(&calls),
    }
}

/// Take the merge in progress back, so a tree left mid-merge is not read as
/// one that finished. A merge that never began refuses this, which is the
/// answer nothing is wrong.
fn abandon(plan: &mut Plan, r: &mut dyn Runner) {
    let _ = plan.git_of(r, &["merge", "--abort"]);
}

fn merge_and_settle(plan: &mut Plan, r: &mut dyn Runner) -> Result<Outcome, Why> {
    // Past the point where a session is asked, every refusal must still say a
    // model was spent, so each read or write below carries its error through
    // `Why::asked`. One of them missing would answer `model: false` for a
    // merge that did spend a session, which is what the dispatcher falls back
    // on a full cycle for.
    //
    // The head the pull request was at when this merge was asked for: the
    // point the outsider check measures from. A tree at any other commit is
    // not the pull request this decision was made about. git's answer is read
    // as a commit and compared in full, so an answer that holds 40 hex digits
    // and anything after them is refused rather than matched as a prefix.
    let now = plan
        .git_of(r, &["rev-parse", "HEAD"])
        .let_say("the tree's commit could not be read")?;
    let now = match normalise_commit(&now) {
        Some(sha) => sha,
        None => {
            return Err(Why::new("the tree's commit is not a commit"));
        }
    };
    if now != plan.head {
        return Err(Why::new(format!(
            "the tree is not the pull request's head {}",
            plan.head
        )));
    }
    let msg = format!(
        "Merge origin/{} into pull request #{}",
        plan.branch, plan.pr
    );
    match plan.git_of(r, &["merge", "--no-ff", "--no-edit", "-m", &msg, plan.base]) {
        // A clean merge is not an easy merge of anything: no file conflicted,
        // so there is nothing for the rules or a model to settle, and nothing
        // is pushed. The full refresh is the answer to a conflict that was
        // never there, and it reads the pull request again as it is.
        d if d.ok() => {
            let mut out = Outcome::started(Vec::new());
            out.why = "it merged clean, so there was nothing to resolve".to_string();
            return Ok(out);
        }
        // A conflict is where this command earns its keep: git left the
        // markers and the MERGE_HEAD, and the files below are the ones it
        // could not settle.
        _ => {}
    }
    let conflicts = conflicted_files(plan, r)?;
    if conflicts.is_empty() {
        // git stopped without saying what it left: no merge is in progress,
        // or the tree is not the one this decision was made about. The pull
        // request is left exactly as it was, which is what a refresh needs.
        return Err(Why::new(
            "the merge stopped with nothing it left to resolve",
        ));
    }
    let mut out = Outcome::started(conflicts.clone());
    for path in &conflicts {
        match rule_of(path) {
            None => {}
            Some(Rule::Regenerate) => regenerate(plan, r, path, &mut out)?,
            Some(Rule::BranchSide) => take_branch_side(plan, r, path, &mut out)?,
        }
    }
    // A model was asked before anything else is decided, so the answer the
    // merge gives always says whether a model was spent on the pull request,
    // even when the turn did not finish or left a marker behind.
    let left: Vec<String> = conflicts
        .iter()
        .filter(|p| !out.settled.iter().any(|s| s == *p))
        .cloned()
        .collect();
    if !left.is_empty() {
        if !plan.model {
            return Err(Why::new(format!(
                "{} file(s) need a model and this host asks none",
                left.len()
            )));
        }
        // Answered before the session is asked: the turn below may fail or
        // leave a marker behind and the answer still says a model was spent.
        out.model = true;
        ask_model(plan, r, &left).map_err(Why::asked)?;
    }
    // The whole tree is read for a marker, not just the files that conflicted:
    // a marker written anywhere by a rule or by the model is one the merge
    // must not push, and the outsider check below has no reason to excuse one.
    let marked = markers(r, &plan.tree).map_err(Why::asked)?;
    if !marked.is_empty() {
        out.why = format!("conflict markers are left in {}", marked.join(", "));
        return Ok(out);
    }
    commit(plan, r).map_err(Why::asked)?;
    let sha = plan
        .git_of(r, &["rev-parse", "HEAD"])
        .let_say("the merge commit could not be read")?;
    // The commit git names for the commit it has just written, read the same
    // way as the head above: what the merge pushes is a commit id and nothing
    // else.
    let sha = match normalise_commit(&sha) {
        Some(sha) => sha,
        None => {
            return Err(Why::asked(Why::new("the merge commit is not a commit")));
        }
    };
    // The merge commit is two parents; what it changed is what it wrote over
    // the pull request's head, which is exactly the files that conflicted plus
    // the lockfiles written again. Anything else it touched is a file nobody
    // asked it to touch, and this is not the place to decide what to do with
    // one. `HEAD^1` is the pull request's head, `HEAD^2` the branch's commit.
    let changed = plan
        .git_of(r, &["diff", "--name-only", &format!("{sha}^1"), &sha])
        .let_say("what the merge changed could not be read")
        .map_err(Why::asked)?;
    let changed: Vec<String> = changed.lines().map(|l| l.trim().to_string()).collect();
    let outsiders: Vec<String> = changed
        .iter()
        .filter(|f| !is_path(&conflicts, f) && rule_of(f).is_none())
        .cloned()
        .collect();
    if !outsiders.is_empty() {
        let mut out = Outcome::started(conflicts.clone());
        out.sha = Some(sha);
        out.why = format!(
            "the merge commit changed a file that did not conflict: {}",
            outsiders.join(", ")
        );
        return Ok(out);
    }
    // The push is the one the preflight of a cycle makes, with the real
    // destination: the same branch, no force, and nothing else.
    let push = plan.git_of(
        r,
        &[
            "push",
            "origin",
            &format!("HEAD:refs/heads/{}", plan.branch_ref),
        ],
    );
    if !push.ok() {
        let mut out = Outcome::started(conflicts.clone());
        out.sha = Some(sha);
        out.why = "the push of the merge commit failed".to_string();
        return Ok(out.said(push));
    }
    out.sha = Some(sha);
    out.pushed = true;
    Ok(out)
}

trait Say {
    fn let_say(self, why: &str) -> Result<String, Why>;
}

impl Say for Done {
    fn let_say(self, why: &str) -> Result<String, Why> {
        if !self.ok() {
            return Err(Why::new(why).said(self));
        }
        Ok(self.out)
    }
}

/// The files git left conflicted, in the order git lists them.
fn conflicted_files(plan: &mut Plan, r: &mut dyn Runner) -> Result<Vec<String>, Why> {
    let out = plan
        .git_of(r, &["diff", "--name-only", "--diff-filter=U"])
        .let_say("the conflicted files could not be read")?;
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

/// Write a lockfile again from the manifests of the merged tree. `cargo` is
/// run in the tree, so the lock is the one the merged manifests describe.
fn regenerate(
    plan: &mut Plan,
    r: &mut dyn Runner,
    path: &str,
    out: &mut Outcome,
) -> Result<(), Why> {
    // A lockfile is written again by the tool of its kind, and this command is
    // given cargo and no other: an npm lockfile is left for the model, which
    // is what the case of that name says.
    if lockfile_tool(path) != "cargo" {
        return Ok(());
    }
    // The lockfile's own directory: cargo reads the manifests of the workspace
    // it is run in, so a lock of a package is written again from that package.
    let dir = path
        .rsplit_once('/')
        .map(|(d, _)| plan.tree.join(d))
        .unwrap_or_else(|| plan.tree.clone());
    let d = r.run(&plan.cargo, &["generate-lockfile".into()], &dir);
    if !d.ok() {
        return Err(Why::new(format!("{path} could not be written again")).said(d));
    }
    let add = plan.git_of(r, &["add", "--", path]);
    if !add.ok() {
        return Err(Why::new(format!("{path} could not be staged")).said(add));
    }
    out.settled.push(path.to_string());
    Ok(())
}

/// Take the branch's version of a file: `--theirs` of a merge is the commit
/// being merged in, which is the branch.
fn take_branch_side(
    plan: &mut Plan,
    r: &mut dyn Runner,
    path: &str,
    out: &mut Outcome,
) -> Result<(), Why> {
    let d = plan.git_of(r, &["checkout", "--theirs", "--", path]);
    if !d.ok() {
        return Err(Why::new(format!("{path} could not be taken from the branch")).said(d));
    }
    let d = plan.git_of(r, &["add", "--", path]);
    if !d.ok() {
        return Err(Why::new(format!("{path} could not be staged")).said(d));
    }
    out.settled.push(path.to_string());
    Ok(())
}

/// Whether any of the files still holds a marker of the merge.
fn markers(r: &mut dyn Runner, tree: &Path) -> Result<Vec<String>, Why> {
    let args: Vec<String> = [
        "-rIn",
        "--binary-files=without-match",
        "-e",
        "<<<<<<< ",
        ".",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let d = r.run("grep", &args, tree);
    // grep's 1 is "no line matched", which is the answer wanted. 0 names a
    // line, and its path is the file to name in the refusal. Anything else is
    // a tree grep could not read.
    match d.status {
        Some(0) => {
            let mut marked: Vec<String> = d
                .out
                .lines()
                .filter_map(|l| l.split(':').next())
                .map(|p| p.trim_start_matches("./").to_string())
                .filter(|p| !p.is_empty())
                .collect();
            marked.sort();
            marked.dedup();
            if marked.is_empty() {
                return Err(Why::new("a marker was found where none could be named"));
            }
            Ok(marked)
        }
        Some(1) => Ok(Vec::new()),
        // A tree grep could not read is no answer: the merge is refused rather
        // than pushed unverified.
        other => Err(
            Why::new("the tree could not be read for its markers").said(Done {
                status: other,
                out: d.out,
                err: d.err,
            }),
        ),
    }
}

/// The one bare `pi` session: no pi-rukas workflow, no gate, no second turn.
/// Its prompt names the files and says change nothing else.
fn ask_model(plan: &mut Plan, r: &mut dyn Runner, files: &[String]) -> Result<(), Why> {
    let prompt = format!(
        "Resolve the conflict markers in these files of the working tree so both sides still \
         do what they did: {}. Change nothing else: no other file, no rebase, no commit, no \
         push. Leave no marker of the merge behind.",
        files.join(", ")
    );
    // `--tools read,grep,find,ls,edit,write` is the whole of what the session
    // may reach: it edits files and runs nothing, which is all an easy merge
    // asks of it. The commit, the check and the push are this command's.
    let args: Vec<String> = [
        "-p",
        "--no-session",
        "--no-extensions",
        "--no-skills",
        "--no-prompt-templates",
        "--no-context-files",
        "--tools",
        "read,grep,find,ls,edit,write",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain(std::iter::once(prompt))
    .collect();
    let d = r.run(&plan.pi, &args, &plan.tree);
    if !d.ok() {
        return Err(Why::new("the model turn did not finish").said(d));
    }
    let add = plan.git_of(r, &["add", "--", "."]);
    if !add.ok() {
        return Err(Why::new("the resolved files could not be staged").said(add));
    }
    Ok(())
}

/// The merge commit: git's own, with the staged resolutions.
fn commit(plan: &mut Plan, r: &mut dyn Runner) -> Result<(), Why> {
    let d = plan.git_of(r, &["commit", "--no-edit"]);
    if !d.ok() {
        return Err(Why::new("the merge commit could not be written").said(d));
    }
    Ok(())
}

#[cfg(test)]
mod tests;

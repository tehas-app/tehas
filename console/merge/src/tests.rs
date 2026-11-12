//! The easy merge's decisions, against a runner that stands in for git, cargo
//! and pi: no repository, no network and no model. Each case says what the
//! tree looked like, what each command was told to answer, and what the merge
//! did in answer to it.

use super::*;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tehas_core::testdir::TestDir;

/// What one command answers: its exit status and what it prints. `prefix` is
/// the call it answers for, named the way a case names it: `pi -p`, `git
/// merge`, `cargo generate-lockfile`.
#[derive(Clone)]
struct Reply {
    prefix: String,
    status: i32,
    out: String,
}

/// The tree of a case: the files git says conflicted, and the files the merge
/// commit is found to have changed. A command a case does not script answers
/// what the real one would. `calls` holds what the merge was asked to do, so a
/// case can say `no model turn` and a scripted answer can be told from the
/// default one.
struct Fake {
    conflicts: Vec<String>,
    changed: Vec<String>,
    replies: Vec<Reply>,
    calls: Vec<String>,
    /// The programs that ran, by their last part, one entry per call: what a
    /// case counts when it asks whether a model was asked at all.
    asked: Vec<String>,
    /// A call that fails once the merge commit is written, for a read the
    /// merge also makes before it.
    fail_after_commit: Option<String>,
}

impl Fake {
    fn new(conflicts: &[&str], changed: &[&str]) -> Fake {
        Fake {
            conflicts: conflicts.iter().map(|s| s.to_string()).collect(),
            changed: changed.iter().map(|s| s.to_string()).collect(),
            replies: Vec::new(),
            calls: Vec::new(),
            asked: Vec::new(),
            fail_after_commit: None,
        }
    }

    /// Tell the commands whose call begins with `prefix` how to answer. The
    /// first answer written for a call wins, so a case overrides the broad
    /// `git merge` of `merged` by naming its own command more narrowly, and
    /// `answers` puts a repeat of a name over the first. `pi -p` is narrower
    /// than `pi`, and a case that wants the session to fail writes `pi -p`
    /// and nothing else.
    fn answers(&mut self, prefix: &str, status: i32, out: &str) {
        let at = self
            .replies
            .iter()
            .rposition(|r| r.prefix == prefix)
            .map(|i| i + 1)
            .unwrap_or(self.replies.len());
        self.replies.insert(
            at,
            Reply {
                prefix: prefix.to_string(),
                status,
                out: out.to_string(),
            },
        );
    }

    /// The calls made of one program, named by its last part: `pi -p ...`,
    /// whatever path the plan gave the program.
    fn of(&self, program: &str) -> Vec<String> {
        self.calls
            .iter()
            .filter(|c| {
                c.split_once(char::is_whitespace)
                    .is_some_and(|(p, _)| p.ends_with(&format!("/{program}")) || p == program)
            })
            .map(|c| {
                let (p, a) = c.split_once(' ').unwrap_or((c.as_str(), ""));
                format!("{} {a}", p.rsplit('/').next().unwrap_or(p))
            })
            .collect()
    }

    /// The model was asked: one call of `pi` was made, whatever path the plan
    /// named it by and whatever it answered — a session that failed is one the
    /// merge asked for. Read from `asked`, which holds every program that ran
    /// and is written whatever the answer was.
    fn asked_model(&self) -> bool {
        self.asked.iter().any(|p| p.ends_with("/pi") || p == "pi")
    }

    /// Whether a case scripted the answer of this command: an answer whose
    /// name the call begins with. Used to tell a command the case asked about
    /// from one the merge ran on its own.
    fn scripted(&self, name: &str) -> bool {
        self.replies.iter().any(|r| r.prefix == name)
    }

    /// Whether any call of any program holds `words` in its arguments.
    fn did(&self, words: &str) -> bool {
        self.calls.iter().any(|c| c.contains(words))
    }

    /// What the merge was told to answer for one call: the scripted reply, or
    /// the answer of the command itself.
    /// What the merge was told to answer for one call: the scripted reply, or
    /// the answer of the command itself. `named` is the call as a case names
    /// it, the program without its path, so `pi -p ...` is scripted by
    /// `pi -p` however the plan spells the program's name.
    fn reply(&self, named: &str) -> Reply {
        let committed = self.calls.iter().any(|c| c.contains("commit --no-edit"));
        if committed
            && self
                .fail_after_commit
                .as_deref()
                .is_some_and(|f| named.starts_with(f))
        {
            return Reply {
                prefix: String::new(),
                status: 1,
                out: "it failed".to_string(),
            };
        }
        // The scripted answers win, in the order a case wrote them, so the
        // narrower name is written first: `pi -p` before `pi`.
        // The scripted answers win, and the broadest name wins among them: a
        // case that means the model to fail writes `pi -p` alone, and the
        // `pi -p` of the same name it wrote first is the one read.
        for reply in self.replies.iter().rev() {
            if named.starts_with(reply.prefix.as_str()) {
                return reply.clone();
            }
        }
        // The reads a case leaves alone answer what the real command would:
        // the conflicted files, the files the merge commit changed, the
        // commit HEAD stands on, and no marker anywhere. The changed files are
        // read by `git diff-tree --cc` and by the older `git diff <sha>^1 <sha>`
        // alike, so the fake answers both spellings with the same list.
        if named.contains("--diff-filter=U") {
            return Reply::yes(&self.conflicts.join("\n"));
        }
        if named.contains("--name-only") && (named.contains("^") || named.contains("diff-tree")) {
            return Reply::yes(&self.changed.join("\n"));
        }
        if named.starts_with("git rev-parse HEAD") {
            // The second read of HEAD is the merge commit's, which is the sha
            // the merge pushes: the first is the pull request's head the tree
            // must stand on, and the commit is written between them.
            let after = self.calls.iter().any(|c| c.contains("commit --no-edit"));
            return Reply::yes(&format!("{}\n", if after { MERGE } else { HEAD }));
        }
        if named.starts_with("grep ") {
            return Reply {
                prefix: String::new(),
                status: 1,
                out: String::new(),
            };
        }
        Reply::yes("")
    }
}

impl Reply {
    fn yes(out: &str) -> Reply {
        Reply {
            prefix: String::new(),
            status: 0,
            out: out.to_string(),
        }
    }
}

impl Runner for Fake {
    fn run(&mut self, program: &str, args: &[String], _tree: &Path) -> Done {
        let call = format!(
            "{program} {}",
            args.iter()
                .map(|a| a.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        );
        self.calls.push(call.clone());
        // The program, by its last part: a case that asks whether a model
        // session ran reads this, and a session that failed is one that was
        // asked, so it is kept whatever the answer was.
        self.asked
            .push(program.rsplit('/').next().unwrap_or(program).to_string());
        let named = format!(
            "{} {}",
            program.rsplit('/').next().unwrap_or(program),
            args.iter()
                .map(|a| a.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        );
        // `named` is what the case scripted its answers against.
        let r = self.reply(&named);
        Done {
            status: Some(r.status),
            out: r.out,
            err: String::new(),
        }
    }

    fn calls(&mut self) -> Vec<String> {
        self.calls.clone()
    }
}

/// A runner that records nothing: what `Real` is on a host, where the calls
/// are said to the log and never kept. A case uses it to show the answer does
/// not depend on a call list.
struct Silent {
    inner: Fake,
}

impl Silent {
    /// The host's runner over a tree whose commands a case can still inspect
    /// through `inner`.
    fn over(inner: Fake) -> Silent {
        Silent { inner }
    }
}

impl Runner for Silent {
    fn run(&mut self, program: &str, args: &[String], tree: &Path) -> Done {
        self.inner.run(program, args, tree)
    }

    // `calls` is not overridden: this is what `Real` is on a host, which
    // answers the trait's default because it keeps the commands in its log.
}

const HEAD: &str = "1111111111111111111111111111111111111111";
/// The same commit in git's own spelling and in the upper case a person or a
/// tool may write `--head` with: one letter apart, so the case that fails is
/// the one that drops the lower-casing rather than the one that trims.
const HEAD_UPPER: &str = "111111111111111111111111111111111111111A";
const HEAD_LOWER: &str = "111111111111111111111111111111111111111a";
const MERGE: &str = "9999999999999999999999999999999999999999";
const BASE: &str = "2222222222222222222222222222222222222222";

fn plan() -> Plan<'static> {
    Plan {
        issue: 7,
        head: HEAD,
        base: BASE,
        branch: "main",
        pr: 50,
        tree: PathBuf::from("/tree"),
        branch_ref: "feature/issue-7",
        git: "/usr/bin/git".into(),
        cargo: "/usr/bin/cargo".into(),
        pi: "/usr/local/bin/pi".into(),
        // A host asks a model unless the case says it does not.
        model: true,
    }
}

/// A tree whose merge conflicts, whose commit is written and whose push goes
/// through. The conflict is what the merge is for, so every case asks for it;
/// the two reads of `git rev-parse HEAD` are the pull request's head, then the
/// merge commit. A case says otherwise by scripting one command, and scripts
/// the narrower name first so it is read before the broader one.
fn outcome(plan: &mut Plan, r: &mut dyn Runner) -> Outcome {
    report(plan, r).outcome
}

fn merged(conflicts: &[&str], changed: &[&str]) -> (Plan<'static>, Fake) {
    let mut f = Fake::new(conflicts, changed);
    // The scripted answers are read in the order they are written, so the
    // broadest name comes first: `git merge` before a case's `git merge`
    // override is never read, and so every case that wants a conflict says
    // so, and every case that wants a clean merge overrides it with its own
    // `git merge` written before this one — which is why `answers` pushes and
    // `reply` reads in order, and the case's own call wins by being first.
    f.answers("git merge", 1, "CONFLICT (content): Merge conflict");
    (plan(), f)
}

#[test]
fn a_conflict_the_rules_settle_is_pushed_without_a_model_turn() {
    let (mut p, mut f) = merged(
        &["console/Cargo.lock", "scripts/bash-lines.max"],
        &["console/Cargo.lock", "scripts/bash-lines.max"],
    );
    let out = outcome(&mut p, &mut f);
    assert!(out.ok(), "{}", out.why);
    assert_eq!(out.sha.as_deref(), Some(MERGE));
    assert_eq!(
        out.conflicts,
        ["console/Cargo.lock", "scripts/bash-lines.max"]
    );
    assert_eq!(out.settled, out.conflicts);
    assert!(!out.model, "no model was asked: {:?}", f.of("pi"));
    assert!(f.of("pi").is_empty(), "a model turn ran: {:?}", f.of("pi"));
    // The lockfile written again, the ceiling taken from the branch, and the
    // one commit pushed to the pull request's own branch.
    assert_eq!(f.of("cargo"), ["cargo generate-lockfile"]);
    assert!(
        f.did("checkout --theirs -- scripts/bash-lines.max"),
        "{:?}",
        f.of("git")
    );
    assert!(
        f.did("push origin HEAD:refs/heads/feature/issue-7"),
        "{:?}",
        f.of("git")
    );
    // No force, no rebase: the pull request keeps its history and gains one
    // commit, which is all the issue allows.
    for c in &f.calls {
        assert!(!c.contains("--force") && !c.contains("rebase"), "{c}");
    }
}

#[test]
fn a_conflict_in_code_goes_to_exactly_one_bare_pi_session() {
    let (mut p, mut f) = merged(&["src/app.rs"], &["src/app.rs"]);
    let out = outcome(&mut p, &mut f);
    assert!(out.ok(), "{}", out.why);
    assert!(out.model);
    let asks = f.of("pi");
    assert_eq!(asks.len(), 1, "{asks:?}");
    let ask = &asks[0];
    // The program is named as the plan names it, which is a path on a host.
    let (_, ask) = ask.split_once(' ').unwrap();
    let ask = ask.trim_start_matches("-p ");
    let ask = format!("-p {ask}");
    assert!(ask.starts_with("-p "), "{ask}");
    assert!(ask.contains("--no-session"), "{ask}");
    assert!(
        ask.contains("--tools read,grep,find,ls,edit,write"),
        "{ask}"
    );
    assert!(
        ask.contains("Resolve the conflict markers in these files"),
        "{ask}"
    );
    assert!(ask.contains("src/app.rs"), "{ask}");
    assert!(ask.contains("Change nothing else"), "{ask}");
    // A bare session, not a workflow: no pi-rukas, no runner, no gate.
    assert!(!ask.contains("rukas"), "{ask}");
    assert!(!ask.contains("verify"), "{ask}");
}

#[test]
fn the_model_being_asked_is_said_even_when_the_merge_is_refused() {
    // A session that left a marker behind: the dispatcher needs to know a
    // model was spent on it, and the pull request must not gain a commit.
    let (mut p, mut f) = merged(&["src/app.rs"], &["src/app.rs"]);
    f.answers("grep", 0, "./src/app.rs:12:<<<<<<< HEAD\n");
    f.answers("pi -p", 0, "done\n");
    let out = outcome(&mut p, &mut f);
    assert!(!out.ok());
    assert!(f.asked_model(), "no session was asked: {:?}", f.asked);
    assert_eq!(out.sha, None, "nothing was committed");
    assert!(!out.pushed);
    assert_eq!(out.why, "conflict markers are left in src/app.rs");
    // The model was asked even though the merge was refused: the dispatcher
    // must know a model was spent before it falls back to a full cycle.
    assert!(out.model, "the model was not said to have run");
    // The merge is taken back, so the tree is not left mid-merge.
    assert!(f.did("merge --abort"), "{:?}", f.of("git"));
    // Nothing was pushed. The prompt that asks the model not to push names
    // the word, so the push itself is what a case looks for.
    assert!(!f.did("push --"), "a push ran: {:?}", f.calls);
}

#[test]
fn a_file_outside_the_conflicts_refuses_the_merge() {
    let (mut p, mut f) = merged(&["src/app.rs"], &["src/app.rs", "README.md"]);
    let out = outcome(&mut p, &mut f);
    assert!(!out.ok());
    assert_eq!(
        out.why,
        "the merge commit changed a file that did not conflict: README.md"
    );
    assert!(!out.pushed);
}

#[test]
fn a_lockfile_written_again_is_not_an_outsider() {
    // The lockfile the model touched is one the rules own: it is not a file
    // outside the conflicts just because nothing listed it as a conflict.
    let (mut p, mut f) = merged(&["src/app.rs"], &["src/app.rs", "Cargo.lock"]);
    let out = outcome(&mut p, &mut f);
    assert!(out.ok(), "{}", out.why);
}

#[test]
fn a_push_that_fails_leaves_the_commit_and_reports_the_push() {
    let (mut p, mut f) = merged(&["src/app.rs"], &["src/app.rs"]);
    f.answers("git push", 1, "remote: Permission denied");
    let out = outcome(&mut p, &mut f);
    assert!(!out.ok());
    assert!(
        out.why
            .starts_with("the push of the merge commit failed: remote: Permission denied"),
        "{}",
        out.why
    );
    assert!(out.sha.is_some());
    assert!(!out.pushed);
}

#[test]
fn a_clean_merge_is_no_easy_merge() {
    let (mut p, mut f) = merged(&["src/app.rs"], &[]);
    f.answers("git merge", 0, "");
    let out = outcome(&mut p, &mut f);
    assert!(!out.ok());
    assert_eq!(out.why, "it merged clean, so there was nothing to resolve");
    assert!(!f.did("push"), "a push ran: {:?}", f.calls);
}

#[test]
fn a_tree_at_another_commit_is_not_the_pull_request() {
    let (mut p, mut f) = merged(&["src/app.rs"], &["src/app.rs"]);
    f.answers(
        "git rev-parse HEAD",
        0,
        "3333333333333333333333333333333333333333\n",
    );
    let out = outcome(&mut p, &mut f);
    assert!(!out.ok());
    assert!(
        out.why
            .starts_with("the tree is not the pull request's head"),
        "{}",
        out.why
    );
    // Nothing was merged at all: the merge was never asked for.
    assert!(!f.did("merge --no-ff"), "a merge ran: {:?}", f.calls);
}

#[test]
fn a_lockfile_rule_that_cannot_run_refuses_the_whole_merge() {
    let (mut p, mut f) = merged(&["Cargo.lock"], &["Cargo.lock"]);
    f.answers("cargo generate-lockfile", 1, "error: manifest not found");
    let out = outcome(&mut p, &mut f);
    assert!(!out.ok());
    assert!(
        out.why
            .starts_with("Cargo.lock could not be written again: error: manifest not found"),
        "{}",
        out.why
    );
    assert!(!out.pushed);
    assert!(f.did("merge --abort"));
}

#[test]
fn a_model_that_does_not_finish_refuses_the_merge() {
    let (mut p, mut f) = merged(&["src/app.rs"], &["src/app.rs"]);
    f.answers("pi -p", 1, "provider did not answer");
    let out = outcome(&mut p, &mut f);
    assert!(!out.ok());
    assert!(f.asked_model(), "no session was asked: {:?}", f.asked);
    assert!(
        out.why
            .starts_with("the model turn did not finish: provider did not answer"),
        "{}",
        out.why
    );
    assert!(out.model, "the model was asked all the same");
    assert!(
        !f.scripted("grep"),
        "the marker check ran too: {:?}",
        f.calls
    );
}

#[test]
fn a_conflict_in_code_with_no_model_asked_refuses_the_merge() {
    let (mut p, mut f) = merged(&["src/app.rs"], &["src/app.rs"]);
    p.model = false;
    let out = outcome(&mut p, &mut f);
    assert!(!out.ok());
    assert_eq!(out.why, "1 file(s) need a model and this host asks none");
    assert!(f.of("pi").is_empty(), "a model turn ran: {:?}", f.of("pi"));
}

#[test]
fn a_merge_that_stops_with_nothing_to_resolve_refuses() {
    // git said conflict and named no file: the tree is not one this command
    // can read a decision out of, so it says so rather than committing.
    let (mut p, mut f) = merged(&[], &[]);
    let out = outcome(&mut p, &mut f);
    assert!(!out.ok());
    assert_eq!(out.why, "the merge stopped with nothing it left to resolve");
}

#[test]
fn a_lockfile_of_another_tool_is_left_to_the_model() {
    // npm's lockfile is npm's to write again, and npm is not a program this
    // command was given: the rules do not claim it.
    let (mut p, mut f) = merged(&["host/sandbox/npm/playwright/package-lock.json"], &[]);
    p.model = false;
    let out = outcome(&mut p, &mut f);
    assert!(!out.ok());
    assert_eq!(out.why, "1 file(s) need a model and this host asks none");
    // It was not claimed as a rule's file: no lockfile was written again for
    // it, because npm is not a program this command was given.
    assert!(f.of("cargo").is_empty(), "{:?}", f.calls);
}

#[test]
fn the_rules_are_exactly_the_files_the_issue_names() {
    assert_eq!(rule_of("console/Cargo.lock"), Some(Rule::Regenerate));
    assert_eq!(rule_of("Cargo.lock"), Some(Rule::Regenerate));
    assert_eq!(
        rule_of("host/sandbox/npm/playwright/package-lock.json"),
        Some(Rule::Regenerate)
    );
    assert_eq!(rule_of("scripts/bash-lines.max"), Some(Rule::BranchSide));
    // A lockfile under a directory of its own is still a lockfile, and
    // nothing else is settled by a rule.
    assert_eq!(rule_of("a/b/c/Cargo.lock"), Some(Rule::Regenerate));
    assert_eq!(rule_of("src/bash-lines.max.rs"), None);
    assert_eq!(rule_of("src/main.rs"), None);
    assert_eq!(rule_of("Cargo.toml"), None);
}

#[test]
fn a_runner_that_records_no_calls_still_says_the_model_ran() {
    // The host's runner keeps no call list, so `model` must come from the
    // merge itself and not from the calls it holds.
    let (mut p, f) = merged(&["src/app.rs"], &["src/app.rs"]);
    let mut silent = Silent::over(f);
    let r = report(&mut p, &mut silent);
    assert!(r.outcome.ok(), "{}", r.outcome.why);
    assert!(
        r.outcome.model,
        "a session ran and the answer said it did not"
    );
    assert!(
        r.calls.is_empty(),
        "the runner was asked to record nothing: {:?}",
        r.calls
    );
    assert_eq!(r.json()["model"], serde_json::json!(true));
    assert_eq!(
        silent.inner.of("pi").len(),
        1,
        "no session ran: {:?}",
        silent.inner.asked
    );
}

#[test]
fn a_merge_with_no_conflict_says_no_model_ran() {
    let (mut p, mut f) = merged(&["src/app.rs"], &[]);
    f.answers("git merge", 0, "");
    let out = outcome(&mut p, &mut f);
    assert!(!out.ok(), "a clean merge is not an easy merge: {}", out.why);
    assert!(
        !out.model,
        "nothing was left to resolve, so nothing was asked"
    );
    assert!(f.of("pi").is_empty(), "a model turn ran: {:?}", f.of("pi"));
}

#[test]
fn a_head_spelled_in_upper_case_is_the_commit_git_names() {
    // The command reads `--head` through `normalise_commit`, so the plan it
    // hands the merge holds git's own spelling, and the merge goes through
    // rather than refusing on how the caller wrote the sha.
    let (mut p, mut f) = merged(&["src/app.rs"], &["src/app.rs"]);
    f.answers("git rev-parse HEAD", 0, &format!("{HEAD_LOWER}\n"));
    let head = normalise_commit(HEAD_UPPER).unwrap();
    p.head = &head;
    let out = outcome(&mut p, &mut f);
    assert_eq!(p.head, HEAD_LOWER, "the plan does not hold git's spelling");
    assert!(
        out.ok(),
        "the head was refused: {}; {:?}",
        out.why,
        f.of("git")
    );
    assert!(out.model);
    assert!(f.did("merge --no-ff"), "no merge ran: {:?}", f.calls);
}

#[test]
fn a_commit_id_is_40_hex_digits_in_git_s_own_spelling() {
    // The accept rule of the command and the compare rule of the merge are the
    // same rule: what does not read as a commit is refused, whatever case it
    // is spelled in.
    let (upper, lower) = (HEAD_UPPER.to_string(), HEAD_LOWER.to_string());
    assert_eq!(normalise_commit(&upper).as_deref(), Some(lower.as_str()));
    assert_eq!(normalise_commit(&lower).as_deref(), Some(lower.as_str()));
    // The two spellings name one commit and the pair really does differ in
    // case, so the case below cannot pass by comparing a sha with itself.
    assert_ne!(upper, lower);
    assert_eq!(upper.to_ascii_lowercase(), lower);
    // A sha with room around it is the commit it names: the command reads the
    // argument and git's answer through here, and both carry a line ending.
    assert_eq!(
        normalise_commit(&format!(" {lower} \n")).as_deref(),
        Some(lower.as_str())
    );
    for not_a_commit in [
        "",
        "1111",
        &format!("{lower}deadbeef"),
        &format!("{lower} {}", "f".repeat(260)),
        "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
        &lower.replacen('a', "-", 1),
    ] {
        assert_eq!(normalise_commit(not_a_commit), None, "{not_a_commit}");
    }
}

#[test]
fn a_git_answer_that_is_more_than_a_commit_is_not_the_head() {
    // The head guard compares whole commits: an answer that begins with the
    // head's 40 hex digits and holds more than them is no commit, and a tree
    // whose commit cannot be named is not merged into.
    for padded in [
        format!("{HEAD}deadbeef\n"),
        format!("{HEAD}{}\n", "f".repeat(260)),
    ] {
        let (mut p, mut f) = merged(&["src/app.rs"], &["src/app.rs"]);
        f.answers("git rev-parse HEAD", 0, &padded);
        let out = outcome(&mut p, &mut f);
        assert!(!out.ok(), "the merge went through: {}", out.why);
        assert_eq!(out.why, "the tree's commit is not a commit");
        assert!(!out.model, "no session was asked");
        assert!(!f.did("merge --no-ff"), "a merge ran: {:?}", f.calls);
    }
}

#[test]
fn a_refusal_after_the_model_still_says_the_model_ran() {
    // The host's condition: a runner that records nothing, a session that was
    // asked, and a merge that stops after it. The answer must say both.
    let (mut p, mut f) = merged(&["src/app.rs"], &["src/app.rs"]);
    f.answers("pi -p", 1, "provider did not answer");
    let mut silent = Silent::over(f);
    let r = report(&mut p, &mut silent);
    assert!(!r.outcome.ok(), "the merge went through");
    assert!(
        r.outcome
            .why
            .starts_with("the model turn did not finish: provider did not answer"),
        "{}",
        r.outcome.why
    );
    assert!(
        r.outcome.model,
        "a session ran and the refusal said it did not"
    );
    assert!(
        r.calls.is_empty(),
        "the runner records nothing: {:?}",
        r.calls
    );
    assert_eq!(r.json()["model"], serde_json::json!(true));
    assert!(
        silent.inner.asked_model(),
        "no session was asked: {:?}",
        silent.inner.asked
    );
}

#[test]
fn a_refusal_at_the_commit_after_the_model_still_says_the_model_ran() {
    // The session answered and the tree went on to stop for a reason of its
    // own: the model was spent all the same, and a refusal raised past the
    // session carries that with it rather than losing it.
    let (mut p, mut f) = merged(&["src/app.rs"], &["src/app.rs"]);
    f.answers("pi -p", 0, "done\n");
    f.answers("git commit", 1, "nothing to commit");
    let mut silent = Silent::over(f);
    let r = report(&mut p, &mut silent);
    assert!(!r.outcome.ok(), "the merge went through");
    assert!(
        r.outcome
            .why
            .starts_with("the merge commit could not be written: nothing to commit"),
        "{}",
        r.outcome.why
    );
    assert!(
        r.outcome.model,
        "a session ran and the refusal said it did not"
    );
    assert!(
        r.calls.is_empty(),
        "the runner records nothing: {:?}",
        r.calls
    );
    assert_eq!(r.json()["model"], serde_json::json!(true));
    assert!(
        silent.inner.asked_model(),
        "no session was asked: {:?}",
        silent.inner.asked
    );
}

/// A session answered and the merge then stopped at `step`, which answered
/// `status` and `said`.
fn stopped_after_the_session(step: &str, status: i32, said: &str) -> Report {
    let (mut p, mut f) = merged(&["src/app.rs"], &["src/app.rs"]);
    f.answers(step, status, said);
    let mut silent = Silent::over(f);
    report(&mut p, &mut silent)
}

#[test]
fn every_stop_after_the_session_says_the_model_ran() {
    for (step, status, said, why) in [
        ("grep ", 0, "src/app.rs\n", "conflict markers"),
        ("grep ", 2, "it failed", "for its markers"),
        ("git commit", 1, "it failed", "could not be written"),
        (
            "git push",
            1,
            "it failed",
            "the push of the merge commit failed",
        ),
    ] {
        let r = stopped_after_the_session(step, status, said);
        assert!(!r.outcome.ok(), "{step} went through");
        assert!(r.outcome.why.contains(why), "{step}: {}", r.outcome.why);
        assert!(r.outcome.model, "{step}: {}", r.outcome.why);
    }
}

#[test]
fn the_reads_of_the_merge_commit_say_the_model_ran() {
    for (step, why) in [
        ("git rev-parse HEAD", "the merge commit could not be read"),
        (
            "git -c core.quotepath=false diff-tree --cc --name-only 9",
            "what the merge changed could not",
        ),
    ] {
        let (mut p, mut f) = merged(&["src/app.rs"], &["src/app.rs"]);
        // The head is read first; only the read after the commit may fail.
        f.fail_after_commit = Some(step.to_string());
        let mut silent = Silent::over(f);
        let r = report(&mut p, &mut silent);
        assert!(r.outcome.why.starts_with(why), "{}", r.outcome.why);
        assert!(r.outcome.model, "{step}: {}", r.outcome.why);
    }
}

#[test]
fn a_file_outside_the_conflict_after_the_session_says_the_model_ran() {
    let (mut p, f) = merged(&["src/app.rs"], &["src/app.rs", "src/other.rs"]);
    let mut silent = Silent::over(f);
    let r = report(&mut p, &mut silent);
    assert!(
        r.outcome.why.contains("did not conflict"),
        "{}",
        r.outcome.why
    );
    assert!(r.outcome.model);
    assert_eq!(r.outcome.sha.as_deref(), Some(MERGE));
}

#[test]
fn a_refusal_of_a_merge_the_rules_settled_says_no_model_ran() {
    for step in ["git commit", "git push"] {
        let (mut p, mut f) = merged(&["Cargo.lock"], &["Cargo.lock"]);
        f.answers(step, 1, "it failed");
        let mut silent = Silent::over(f);
        let r = report(&mut p, &mut silent);
        assert!(!r.outcome.ok(), "{step} went through");
        assert!(!r.outcome.model, "{step}: {}", r.outcome.why);
        assert!(!silent.inner.asked_model());
    }
}

#[test]
fn the_answer_names_the_pull_request_and_what_ran() {
    let (mut p, mut f) = merged(&["src/app.rs"], &["src/app.rs"]);
    let r = report(&mut p, &mut f);
    let v = r.json();
    assert_eq!(v["pr"], 50);
    assert_eq!(v["issue"], 7);
    assert_eq!(v["ok"], true);
    assert_eq!(v["model"], true);
    assert_eq!(v["pushed"], true);
    assert!(v.get("calls").is_none(), "{v}");
    assert!(r.calls.len() >= 5, "{:?}", r.calls);
    assert!(v["conflicts"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("src/app.rs")));
}

// The two cases of #1746 run against a real repository rather than the faked
// `changed` list above: the outsider check is a read of the merge commit, and
// a fake that answers every `--name-only` with a list a case wrote cannot say
// what git itself found. The repository is made here, in a `TestDir`, with the
// files `a.rs` and `b.rs` of the issue inside it. The stubs `pi` and `cargo`
// the merge runs are written by `stubs`, in a directory of the case's own.

use super::stubs::Tools;

/// One git call of a fixture, with an identity of its own so a host with no
/// global `user.name` commits all the same.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn commit(dir: &Path, files: &[(&str, &str)], msg: &str) -> String {
    for (name, text) in files {
        let p = dir.join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", msg]);
    git(dir, &["rev-parse", "HEAD"])
}

/// A tree that holds both sides of the merge: `base` is the branch's commit and
/// `head` is the pull request's, made to conflict with it over `src/a.rs`.
struct Tree {
    _dir: TestDir,
    tree: PathBuf,
    head: String,
    base: String,
}

fn tree(main: &[(&str, &str)], pr: &[(&str, &str)]) -> Tree {
    let dir = TestDir::new("merge-repo");
    let src = dir.join("origin");
    std::fs::create_dir_all(&src).unwrap();
    git(&src, &["init", "-q", "-b", "main"]);
    commit(&src, &[("src/a.rs", "the base\n")], "base");
    // The pull request's branch, named as the merge pushes it: a branch called
    // `feature` would collide with `feature/issue-7` when the push is made.
    git(&src, &["checkout", "-q", "-b", "pull"]);
    let head = commit(&src, pr, "the pull request");
    git(&src, &["checkout", "-q", "main"]);
    commit(&src, main, "the branch moves");
    let work = dir.join("work");
    std::fs::create_dir_all(&work).unwrap();
    git(&work, &["init", "-q", "-b", "main"]);
    // The address the merge pushes to. A fetch by path leaves no `origin`, and
    // the push of the merge commit is the thing these cases are about.
    git(
        &work,
        &["remote", "add", "origin", &src.display().to_string()],
    );
    git(
        &work,
        &[
            "fetch",
            "-q",
            "--no-tags",
            "origin",
            "+refs/heads/*:refs/remotes/origin/*",
        ],
    );
    git(&work, &["checkout", "-q", "-B", "main", "origin/main"]);
    // The tree stands on the pull request's head, as its starter leaves it.
    git(&work, &["checkout", "-q", "-B", "feature/issue-7", &head]);
    let base = git(&work, &["rev-parse", "origin/main"]);
    Tree {
        _dir: dir,
        tree: work,
        head,
        base,
    }
}

/// What the easy merge did to a real tree, with the model's answer stubbed.
/// A session resolves the files it was named; `extra` names a file it writes
/// as well that nothing asked for, which is how a case says the model strayed.
fn easy_merge(t: &Tree, extra: &str) -> Outcome {
    // The stubs go when the value that names them goes, so the case holds it
    // for as long as the merge runs them (`stubs`).
    let tools = Tools::new("merge-tools");
    let mut plan = Plan {
        issue: 7,
        head: &t.head,
        base: &t.base,
        branch: "main",
        pr: 50,
        tree: t.tree.clone(),
        branch_ref: "feature/issue-7",
        git: tools.git.clone(),
        cargo: tools.cargo.display().to_string(),
        pi: tools.pi.display().to_string(),
        model: true,
    };
    let mut r = Git {
        extra: extra.to_string(),
        calls: Vec::new(),
    };
    report(&mut plan, &mut r).outcome
}

/// The host's runner for these cases: each command run in the tree it is
/// given, and the environment the stub `pi` reads to write where the model
/// would have edited.
struct Git {
    extra: String,
    calls: Vec<String>,
}

impl Runner for Git {
    fn run(&mut self, program: &str, args: &[String], tree: &Path) -> Done {
        self.calls.push(format!("{program} {}", args.join(" ")));
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(tree)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .env("GIT_MERGE_AUTOEDIT", "no")
            .env("TEHAS_TEST_TREE", tree)
            .env("TEHAS_TEST_EXTRA", &self.extra)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let out = cmd.output().expect("the command could not be run");
        Done {
            status: out.status.code(),
            out: String::from_utf8_lossy(&out.stdout).into_owned(),
            err: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    fn calls(&mut self) -> Vec<String> {
        self.calls.clone()
    }
}

/// The commit the tree stands on: read before the merge and again after it, so
/// a case can say a commit was written or not.
fn head_of(t: &Tree) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(&t.tree)
        .args(["rev-parse", "-q", "--verify", "HEAD"])
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// The files of one commit of the tree, as git names them.
fn files_of(t: &Tree, commit: &str) -> String {
    git(&t.tree, &["show", "--name-only", "--format=", commit])
}

/// What the outsider check reads of one commit: the very call `finish` makes,
/// including the setting it makes the read with, so a case can say what git
/// printed rather than what the merge made of it. The case quotes the answer
/// back separately, so the two spellings are both seen.
fn changed_of(t: &Tree, commit: &str) -> Vec<String> {
    let sha = git(&t.tree, &["rev-parse", commit]);
    let out = Command::new("git")
        .arg("-C")
        .arg(&t.tree)
        .args([
            "-c",
            "core.quotepath=false",
            "diff-tree",
            "--cc",
            "--name-only",
            &sha,
        ])
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git diff-tree: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        // The commit's own id opens the answer and holds no path, dropped as
        // `finish` drops it.
        .filter(|l| *l != sha && !l.starts_with("commit ") && !l.is_empty())
        .map(str::to_string)
        .collect()
}

#[test]
fn the_branch_changing_other_files_does_not_refuse_an_easy_merge() {
    // The condition of the issue: the pull request conflicts in `src/a.rs`
    // while the branch also moved `README.md`, which git merged clean. The
    // outsider check used to read that clean change as the merge's own.
    let t = tree(
        &[
            ("src/a.rs", "the branch's\n"),
            ("README.md", "the branch's\n"),
        ],
        &[("src/a.rs", "the pull request's\n")],
    );
    let out = easy_merge(&t, "");
    assert!(out.ok(), "{}", out.why);
    assert!(out.model, "the model was asked for src/a.rs");
    assert_eq!(out.conflicts, ["src/a.rs"]);
    assert!(out.pushed);
    assert_eq!(out.sha.as_deref(), Some(head_of(&t).as_str()));
    // The merge commit is the tree's head, holds both sides, and changed only
    // the conflicted file: the branch's own README came in clean and nothing
    // may be refused for it.
    assert_eq!(files_of(&t, "HEAD"), "src/a.rs");
    assert_eq!(
        std::fs::read_to_string(t.tree.join("src/a.rs")).unwrap(),
        "the model resolved it\n"
    );
    assert_eq!(
        std::fs::read_to_string(t.tree.join("README.md")).unwrap(),
        "the branch's\n"
    );
    assert_eq!(git(&t.tree, &["status", "--porcelain"]), "");
    // The commit was pushed to the pull request's own branch, so the branch
    // the tree is on and the remote's hold the same commit.
    assert_eq!(
        git(
            &t.tree,
            &["rev-parse", "refs/remotes/origin/feature/issue-7"]
        ),
        head_of(&t)
    );
}

#[test]
fn a_file_the_model_wrote_besides_the_conflict_refuses_the_merge() {
    // The other half: a model that edited `c.rs` as well is an outsider the
    // check must still catch, with the branch having moved files of its own.
    let t = tree(
        &[
            ("src/a.rs", "the branch's\n"),
            ("README.md", "the branch's\n"),
        ],
        &[("src/a.rs", "the pull request's\n")],
    );
    let out = easy_merge(&t, "c.rs");
    assert!(!out.ok());
    assert_eq!(
        out.why,
        "the merge commit changed a file that did not conflict: c.rs"
    );
    assert!(out.model, "the model was asked all the same");
    assert!(!out.pushed, "a straying model was pushed: {:?}", out.sha);
    // The merge commit was written before the check read it, so it is the
    // tree's head and the commit the answer names. What the check refused is
    // what does not go to the pull request: the remote's branch is still the
    // pull request's head, and the tree is clean.
    assert_eq!(out.sha.as_deref(), Some(head_of(&t).as_str()));
    // Nothing of the pull request's branch is on the remote at all: the push
    // that would have made it there is the one that was refused.
    let pushed = Command::new("git")
        .arg("-C")
        .arg(&t.tree)
        .args([
            "rev-parse",
            "-q",
            "--verify",
            "refs/remotes/origin/feature/issue-7",
        ])
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .unwrap();
    assert!(
        !pushed.status.success(),
        "the branch was pushed: {}",
        String::from_utf8_lossy(&pushed.stdout)
    );
    assert_eq!(git(&t.tree, &["status", "--porcelain"]), "");
}

#[test]
fn a_name_git_would_quote_is_read_as_the_path_it_names() {
    // Two files the merge cares about, both under a directory git spells with
    // escapes (`docs/中文`): the conflict, and a file of the rule's name that
    // only the model writes. Left to the host's `core.quotepath`, git answers
    // `"docs/\344\270\255\346\226\207/bash-lines.max"`, and that string is not
    // the path: read as a path it is the quoted form `is_path` compares with
    // the conflicts, and its last part is `bash-lines.max"`, which `rule_of`
    // does not match. Either way the check calls the merge's own file an
    // outsider and pushes nothing. The read turns quoting off, so the answer
    // holds the paths the merge staged and both are known.
    let t = tree(
        &[
            ("src/a.rs", "the branch's\n"),
            ("README.md", "the branch's\n"),
            ("docs/中文/keep.md", "the branch's\n"),
        ],
        &[("src/a.rs", "the pull request's\n")],
    );
    // The host's setting decides how git spells a non-ASCII name, so the case
    // states it: a host that quotes is the one that shows this.
    git(&t.tree, &["config", "core.quotepath", "true"]);
    let out = easy_merge(&t, "docs/中文/bash-lines.max");
    assert!(out.ok(), "{}", out.why);
    assert!(out.pushed, "the merge's own file was called an outsider");
    assert_eq!(out.conflicts, ["src/a.rs"]);
    assert!(out.model, "the model was asked for src/a.rs");
    // git quotes the name when the check is asked the same way without the
    // setting, so the fix is what makes the difference here and not the fixture.
    let quoted = Command::new("git")
        .arg("-C")
        .arg(&t.tree)
        .args(["diff-tree", "--cc", "--name-only", "HEAD"])
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "core.quotepath")
        .env("GIT_CONFIG_VALUE_0", "true")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let quoted = String::from_utf8_lossy(&quoted.stdout).into_owned();
    assert!(
        quoted.contains(r"\344\270\255\346\226\207"),
        "git did not quote the name, so the case proves nothing: {quoted}"
    );
    // The read the merge made holds both paths themselves, and nothing else.
    assert_eq!(
        changed_of(&t, "HEAD"),
        ["docs/中文/bash-lines.max", "src/a.rs"]
    );
    assert_eq!(
        std::fs::read_to_string(t.tree.join("docs/中文/bash-lines.max")).unwrap(),
        "the model wrote this\n"
    );
    assert_eq!(git(&t.tree, &["status", "--porcelain"]), "");
}

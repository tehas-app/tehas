//! The easy merge's decisions, against a runner that stands in for git, cargo
//! and pi: no repository, no network and no model. Each case says what the
//! tree looked like, what each command was told to answer, and what the merge
//! did in answer to it.

use super::*;
use std::path::PathBuf;

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
}

impl Fake {
    fn new(conflicts: &[&str], changed: &[&str]) -> Fake {
        Fake {
            conflicts: conflicts.iter().map(|s| s.to_string()).collect(),
            changed: changed.iter().map(|s| s.to_string()).collect(),
            replies: Vec::new(),
            calls: Vec::new(),
            asked: Vec::new(),
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
        // commit HEAD stands on, and no marker anywhere.
        if named.contains("--diff-filter=U") {
            return Reply::yes(&self.conflicts.join("\n"));
        }
        if named.contains("--name-only") && named.contains("^") {
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

const HEAD: &str = "1111111111111111111111111111111111111111";
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
fn the_answer_names_the_pull_request_and_what_ran() {
    let (mut p, mut f) = merged(&["src/app.rs"], &["src/app.rs"]);
    let r = report(&mut p, &mut f);
    let v = r.json();
    assert_eq!(v["pr"], 50);
    assert_eq!(v["issue"], 7);
    assert_eq!(v["ok"], true);
    assert_eq!(v["model"], true);
    assert_eq!(v["pushed"], true);
    assert!(v["calls"].as_array().unwrap().len() >= 5, "{}", v["calls"]);
    assert!(v["conflicts"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("src/app.rs")));
}

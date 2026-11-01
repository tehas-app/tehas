//! A factory pull request's title says its state: `WIP: ` while a cycle works
//! on it, `STUCK: ` when it waits for a person, no prefix when it is ok to
//! merge. A pull request whose issue waits for a person loses `STUCK: ` only
//! when it is proven fine to merge, see `fine_to_merge`. Only a pull request
//! from a `feature/issue-<n>` branch by the factory's login is touched, and
//! only when its title differs. A state that cannot be read writes nothing.
//!
//!   console pr-title sync <pr>
//!     Read one pull request and its linked issue and set the title, for the
//!     end of a cycle. Exit 1 with the reason on standard error when a call
//!     failed.
//!   console pr-title poll <file>
//!     The same for what the poll has read already, so an issue the file holds
//!     costs no call: {"bot", "pulls": [the nodes of the poll's page], "issues":
//!     {"<n>": [labels of the open issues]}}. An issue that is not in the file
//!     is read. One line per title set and one WARN per failure on standard
//!     output, for the poll's log; a failed call exits 0.
//!
//! The title is written through REST: `gh pr edit` asks a GraphQL question the
//! bot's token may not answer (#120). The repository and the program are the
//! ones of `console forge`, see src/forge.rs.
//!
//! Exit: 0 done, 1 as above or the file cannot be read, 64 a bad argument, 78
//! no `TEHAS_REPO` or no gh.

use crate::forge;
use serde_json::Value;
use std::env;
use tehas_core::forge::{Bounded, GhRunner};

const RUNNING: &str = "tehas:running";
const REFRESHING: &str = "tehas:refreshing";
const ATTENTION: &str = "needs-human-attention";
const REVIEW_MARK: &str = "<!-- pi-rukas:lens-residuals ";
const COMMENTS_PAGE: usize = 100;
const USAGE: &str = "usage: console pr-title sync <pr> | poll <file>";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Wip,
    Stuck,
    Clear,
}

impl State {
    fn prefix(self) -> &'static str {
        match self {
            State::Wip => "WIP: ",
            State::Stuck => "STUCK: ",
            State::Clear => "",
        }
    }

    fn word(self) -> &'static str {
        match self {
            State::Wip => "WIP",
            State::Stuck => "STUCK",
            State::Clear => "without a prefix",
        }
    }
}

fn state(pull: &[String], issue: &[String]) -> State {
    let on = |labels: &[String], name: &str| labels.iter().any(|l| l == name);
    if on(issue, RUNNING) || on(pull, REFRESHING) {
        State::Wip
    } else if on(issue, ATTENTION) {
        State::Stuck
    } else {
        State::Clear
    }
}

fn read(gh: &mut dyn GhRunner, path: &str) -> Result<Value, String> {
    let out = gh.run(&["api", path])?;
    serde_json::from_str(out.trim()).map_err(|_| format!("the answer for {path} is not JSON"))
}

/// Whether a review comment lists only findings a merge may carry. A severity
/// not known here blocks, so a new one cannot pass unread.
fn nothing_blocks(body: &str) -> bool {
    body.lines()
        .filter_map(|line| line.strip_prefix("- ["))
        .filter_map(|rest| rest.split(']').next())
        .all(|severity| severity == "MEDIUM" || severity == "LOW")
}

/// When the last review of `bot` was written, if it left nothing that blocks.
/// A full page may hide a later review, so it proves nothing.
fn passed_review<'a>(comments: &'a Value, bot: &str) -> Option<&'a str> {
    let comments = comments.as_array().filter(|c| c.len() < COMMENTS_PAGE)?;
    let review = comments.iter().rev().find(|c| {
        let by = c.pointer("/user/login").and_then(Value::as_str);
        let body = c.get("body").and_then(Value::as_str);
        by.is_some_and(|by| account(by) == account(bot))
            && body.is_some_and(|b| b.contains(REVIEW_MARK))
    })?;
    review
        .get("body")
        .and_then(Value::as_str)
        .filter(|body| nothing_blocks(body))?;
    review.get("created_at").and_then(Value::as_str)
}

/// Whether a pull request that waits for a person is proven fine to merge:
/// GitHub calls it clean, and the factory's last review came after its head
/// and left no finding above MEDIUM. Anything short of that is not proof.
/// Err when a read failed, so the title stays as it is.
fn fine_to_merge(gh: &mut dyn GhRunner, repo: &str, n: u64, bot: &str) -> Result<bool, String> {
    let pull = read(gh, &format!("repos/{repo}/pulls/{n}"))?;
    let clean = pull.get("draft") == Some(&Value::Bool(false))
        && pull.get("mergeable") == Some(&Value::Bool(true))
        && pull.get("mergeable_state").and_then(Value::as_str) == Some("clean");
    let Some(head) = pull
        .pointer("/head/sha")
        .and_then(Value::as_str)
        .filter(|_| clean)
    else {
        return Ok(false);
    };
    let comments = read(
        gh,
        &format!("repos/{repo}/issues/{n}/comments?per_page={COMMENTS_PAGE}"),
    )?;
    let Some(reviewed) = passed_review(&comments, bot) else {
        return Ok(false);
    };
    let commit = read(gh, &format!("repos/{repo}/commits/{head}"))?;
    Ok(commit
        .pointer("/commit/committer/date")
        .and_then(Value::as_str)
        .is_some_and(|committed| reviewed >= committed))
}

/// The title wanted: every prefix at the start off, the one of `state` on.
fn titled(title: &str, state: State) -> String {
    let mut bare = title;
    while let Some(rest) = bare
        .strip_prefix(State::Wip.prefix())
        .or_else(|| bare.strip_prefix(State::Stuck.prefix()))
    {
        bare = rest;
    }
    format!("{}{bare}", state.prefix())
}

/// The issue a branch carries: `feature/issue-<n>` or `feature/issue-<n>-...`.
fn linked(head: &str) -> Option<u64> {
    let rest = head.strip_prefix("feature/issue-")?;
    let digits = rest.split('-').next()?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok().filter(|n| *n > 0)
}

/// One account whatever names it: GraphQL gives an app as `<slug>`, REST as
/// `<slug>[bot]` and gh's JSON as `app/<slug>`.
fn account(login: &str) -> &str {
    let login = login.strip_prefix("app/").unwrap_or(login);
    login.strip_suffix("[bot]").unwrap_or(login)
}

struct Pull {
    number: u64,
    title: String,
    head: String,
    author: String,
    labels: Vec<String>,
}

impl Pull {
    /// The linked issue of a pull request of the factory's, or None.
    fn issue(&self, bot: &str) -> Option<u64> {
        (!bot.is_empty() && account(&self.author) == account(bot))
            .then(|| linked(&self.head))
            .flatten()
    }

    /// A node of the poll's GraphQL page.
    fn of_node(v: &Value) -> Option<Pull> {
        Some(Pull {
            number: v.get("number")?.as_u64()?,
            title: v.get("title")?.as_str()?.to_string(),
            head: v.get("headRefName")?.as_str()?.to_string(),
            author: login(v.pointer("/author/login")),
            labels: names(v.pointer("/labels/nodes")?)?,
        })
    }

    /// The REST answer for one pull request.
    fn of_rest(v: &Value) -> Option<Pull> {
        Some(Pull {
            number: v.get("number")?.as_u64()?,
            title: v.get("title")?.as_str()?.to_string(),
            head: v.pointer("/head/ref")?.as_str()?.to_string(),
            author: login(v.pointer("/user/login")),
            labels: names(v.get("labels")?)?,
        })
    }
}

// A deleted account has no author: it is nobody's, so not the factory's.
fn login(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or("").to_string()
}

fn names(v: &Value) -> Option<Vec<String>> {
    v.as_array()?
        .iter()
        .map(|l| l.get("name").and_then(Value::as_str).map(str::to_string))
        .collect()
}

fn issue_labels(gh: &mut dyn GhRunner, repo: &str, n: u64) -> Result<Vec<String>, String> {
    let out = gh.run(&["api", &format!("repos/{repo}/issues/{n}")])?;
    serde_json::from_str::<Value>(out.trim())
        .ok()
        .and_then(|v| names(v.get("labels")?))
        .ok_or_else(|| format!("the answer for #{n} names no labels"))
}

/// The title written, or None when it was right already.
fn keep(
    gh: &mut dyn GhRunner,
    repo: &str,
    pull: &Pull,
    issue: &[String],
    bot: &str,
) -> Result<Option<State>, String> {
    let mut wanted = state(&pull.labels, issue);
    if wanted == State::Stuck && fine_to_merge(gh, repo, pull.number, bot)? {
        wanted = State::Clear;
    }
    let title = titled(&pull.title, wanted);
    if title == pull.title {
        return Ok(None);
    }
    gh.run(&[
        "api",
        "-X",
        "PATCH",
        &format!("repos/{repo}/pulls/{}", pull.number),
        "-f",
        &format!("title={title}"),
    ])?;
    Ok(Some(wanted))
}

fn sync(gh: &mut dyn GhRunner, repo: &str, n: u64, bot: &str) -> Result<(), String> {
    let out = gh.run(&["api", &format!("repos/{repo}/pulls/{n}")])?;
    let v: Value = serde_json::from_str(out.trim()).map_err(|_| "no pull request".to_string())?;
    if v.get("state").and_then(Value::as_str) != Some("open") {
        return Ok(());
    }
    let pull = Pull::of_rest(&v).ok_or("no pull request in the answer")?;
    if linked(&pull.head).is_none() {
        return Ok(());
    }
    let bot = if bot.is_empty() {
        let who = gh.run(&["api", "user"])?;
        let who: Value = serde_json::from_str(who.trim()).unwrap_or(Value::Null);
        login(who.get("login"))
    } else {
        bot.to_string()
    };
    if bot.is_empty() {
        return Err("the bot's login could not be read".to_string());
    }
    let Some(issue) = pull.issue(&bot) else {
        return Ok(());
    };
    let labels = issue_labels(gh, repo, issue)?;
    keep(gh, repo, &pull, &labels, &bot).map(|_| ())
}

fn poll(gh: &mut dyn GhRunner, repo: &str, given: &Value) -> Vec<String> {
    let bot = given.get("bot").and_then(Value::as_str).unwrap_or("");
    let mut lines = Vec::new();
    let nodes = given.get("pulls").and_then(Value::as_array);
    for pull in nodes.into_iter().flatten().filter_map(Pull::of_node) {
        let Some(issue) = pull.issue(bot) else {
            continue;
        };
        let known = given
            .get("issues")
            .and_then(|i| i.get(issue.to_string()))
            .and_then(names_or_plain);
        let kept = match known {
            Some(labels) => Ok(labels),
            None => issue_labels(gh, repo, issue),
        }
        .and_then(|labels| keep(gh, repo, &pull, &labels, bot));
        match kept {
            Ok(None) => {}
            Ok(Some(s)) => lines.push(format!("pull request #{} is now {}", pull.number, s.word())),
            Err(why) => lines.push(format!(
                "WARN: the title of pull request #{} was not kept: {why}",
                pull.number
            )),
        }
    }
    lines
}

fn names_or_plain(v: &Value) -> Option<Vec<String>> {
    v.as_array()?
        .iter()
        .map(|l| l.as_str().map(str::to_string))
        .collect()
}

fn number(word: &str) -> Option<u64> {
    (!word.is_empty() && word.bytes().all(|b| b.is_ascii_digit()))
        .then(|| word.parse().ok())
        .flatten()
        .filter(|n| *n > 0)
}

pub fn main(given: &[String]) -> u8 {
    let words: Vec<&str> = given.iter().map(String::as_str).collect();
    let asked = match words.as_slice() {
        ["sync", n] => number(n).map(Ok),
        ["poll", file] => Some(Err(*file)),
        _ => None,
    };
    let Some(asked) = asked else {
        eprintln!("{USAGE}");
        return 64;
    };
    let repo = forge::repo().filter(|r| forge::repo_shaped(r));
    let program = forge::gh_program(
        env::var("TEHAS_HOME").ok().as_deref(),
        &env::var("PATH").unwrap_or_default(),
    );
    let (Some(repo), Some(program)) = (repo, program) else {
        eprintln!("console pr-title: no TEHAS_REPO or no gh");
        return 78;
    };
    let mut gh = Bounded::new(program, forge::CALL_BOUND);
    match asked {
        Ok(n) => {
            let bot = env::var("TEHAS_BOT_LOGIN").unwrap_or_default();
            match sync(&mut gh, &repo, n, &bot) {
                Ok(()) => 0,
                Err(why) => {
                    eprintln!("{why}");
                    1
                }
            }
        }
        Err(file) => {
            let Some(v) = std::fs::read_to_string(file)
                .ok()
                .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            else {
                eprintln!("console pr-title: {file} cannot be read");
                return 1;
            };
            for line in poll(&mut gh, &repo, &v) {
                println!("{line}");
            }
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::VecDeque;

    const REPO: &str = "acme/widget";
    const WRITE: &str = "api -X PATCH repos/acme/widget/pulls/9 -f";

    fn labels(given: &[&str]) -> Vec<String> {
        given.iter().map(|s| s.to_string()).collect()
    }

    /// Answers in order; every call is kept as one line.
    #[derive(Default)]
    struct Fake {
        answers: VecDeque<Result<String, String>>,
        calls: Vec<String>,
    }

    impl Fake {
        fn of(answers: Vec<Result<&str, &str>>) -> Fake {
            Fake {
                answers: answers
                    .into_iter()
                    .map(|a| a.map(str::to_string).map_err(str::to_string))
                    .collect(),
                calls: Vec::new(),
            }
        }
    }

    impl GhRunner for Fake {
        fn run(&mut self, args: &[&str]) -> Result<String, String> {
            self.calls.push(args.join(" "));
            self.answers.pop_front().unwrap_or(Ok(String::new()))
        }
    }

    // As `gh api repos/.../pulls/<n>` answers for a GitHub App.
    fn rest_pull(title: &str, head: &str, by: &str, state: &str) -> String {
        json!({"number": 9, "state": state, "title": title, "head": {"ref": head},
            "user": {"login": by, "type": "Bot"}, "labels": []})
        .to_string()
    }

    #[test]
    fn each_state_on_a_plain_title() {
        assert_eq!(titled("a", State::Wip), "WIP: a");
        assert_eq!(titled("a", State::Stuck), "STUCK: a");
        assert_eq!(titled("a", State::Clear), "a");
    }

    #[test]
    fn each_state_on_a_title_with_either_prefix() {
        for title in ["WIP: a", "STUCK: a"] {
            assert_eq!(titled(title, State::Wip), "WIP: a");
            assert_eq!(titled(title, State::Stuck), "STUCK: a");
            assert_eq!(titled(title, State::Clear), "a");
        }
    }

    #[test]
    fn a_title_with_the_same_prefix_comes_back_unchanged() {
        assert_eq!(titled("WIP: a", State::Wip), "WIP: a");
        assert_eq!(titled("STUCK: a", State::Stuck), "STUCK: a");
        assert_eq!(
            titled("chore(work): a", State::Clear),
            "chore(work): a",
            "a type is no prefix"
        );
    }

    #[test]
    fn a_prefix_is_never_doubled_and_only_the_start_counts() {
        assert_eq!(titled("WIP: STUCK: WIP: a", State::Stuck), "STUCK: a");
        assert_eq!(
            titled("chore(work): WIP: a", State::Wip),
            "WIP: chore(work): WIP: a"
        );
        assert_eq!(titled("WIP:a", State::Clear), "WIP:a");
    }

    #[test]
    fn the_state_rule() {
        let none = labels(&[]);
        let attention = labels(&[ATTENTION]);
        let running = labels(&[RUNNING]);
        assert_eq!(state(&none, &none), State::Clear);
        assert_eq!(
            state(&none, &labels(&["tehas:awaiting-merge"])),
            State::Clear
        );
        assert_eq!(state(&none, &attention), State::Stuck);
        assert_eq!(state(&none, &running), State::Wip);
        assert_eq!(state(&labels(&[REFRESHING]), &none), State::Wip);
        // WIP wins while a cycle runs.
        assert_eq!(state(&none, &labels(&[ATTENTION, RUNNING])), State::Wip);
        assert_eq!(state(&labels(&[REFRESHING]), &attention), State::Wip);
    }

    const BOT: &str = "tehas-app-bot[bot]";
    const HEAD_AT: &str = "2026-11-01T11:14:57Z";
    const AFTER: &str = "2026-11-01T11:39:51Z";
    const BEFORE: &str = "2026-11-01T10:47:50Z";

    fn merge_read(draft: bool, mergeable: Value, merge_state: &str) -> String {
        json!({"draft": draft, "mergeable": mergeable, "mergeable_state": merge_state,
            "head": {"sha": "abc"}})
        .to_string()
    }

    fn review(by: &str, at: &str, findings: &str) -> Value {
        json!({"user": {"login": by}, "created_at": at, "body": format!(
            "## Six-pass review — residual findings (round disclosure)\n\n{findings}\n\n\
             <!-- pi-rukas:lens-residuals branch=b patch=p -->")})
    }

    fn commit() -> String {
        json!({"commit": {"committer": {"date": HEAD_AT}}}).to_string()
    }

    fn fine(answers: Vec<Result<&str, &str>>) -> Result<bool, String> {
        fine_to_merge(&mut Fake::of(answers), REPO, 9, BOT)
    }

    #[test]
    fn a_clean_pull_reviewed_since_its_head_with_nothing_high_is_fine() {
        let clean = merge_read(false, json!(true), "clean");
        let comments = json!([review(
            BOT,
            AFTER,
            "- [MEDIUM] a.rs:1 — x\n- [LOW] a.rs:2 — y"
        )]);
        let comments = comments.to_string();
        let commit = commit();
        let mut gh = Fake::of(vec![Ok(&clean), Ok(&comments), Ok(&commit)]);
        assert_eq!(fine_to_merge(&mut gh, REPO, 9, BOT), Ok(true));
        assert_eq!(
            gh.calls,
            [
                "api repos/acme/widget/pulls/9",
                "api repos/acme/widget/issues/9/comments?per_page=100",
                "api repos/acme/widget/commits/abc",
            ]
        );
    }

    #[test]
    fn a_pull_git_hub_does_not_call_clean_is_not_fine_and_costs_one_read() {
        for pull in [
            merge_read(true, json!(true), "clean"),
            merge_read(false, json!(false), "clean"),
            merge_read(false, Value::Null, "unknown"),
            merge_read(false, json!(true), "blocked"),
            merge_read(false, json!(true), "unstable"),
            merge_read(false, json!(true), "dirty"),
            "{}".to_string(),
        ] {
            let mut gh = Fake::of(vec![Ok(&pull)]);
            assert_eq!(fine_to_merge(&mut gh, REPO, 9, BOT), Ok(false), "{pull}");
            assert_eq!(gh.calls.len(), 1);
        }
    }

    #[test]
    fn a_review_must_prove_it_and_only_the_last_one_counts() {
        let clean = merge_read(false, json!(true), "clean");
        let low = review(BOT, AFTER, "- [LOW] a.rs:1 — x");
        let high = review(BOT, AFTER, "- [HIGH] a.rs:1 — x");
        let cases = [
            ("no comment", json!([])),
            (
                "a person's words",
                json!([{"user": {"login": "alice"},
                "created_at": AFTER, "body": "lgtm"}]),
            ),
            ("a high", json!([high])),
            (
                "a critical",
                json!([review(BOT, AFTER, "- [CRITICAL] a.rs:1 — x")]),
            ),
            (
                "a severity not known",
                json!([review(BOT, AFTER, "- [SEVERE] a.rs:1 — x")]),
            ),
            ("clean, then a high", json!([low, high])),
            (
                "a marker only another account wrote",
                json!([review("alice", AFTER, "")]),
            ),
            (
                "a full page may hide the last",
                json!(vec![low.clone(); 100]),
            ),
        ];
        for (what, comments) in cases {
            let comments = comments.to_string();
            assert_eq!(fine(vec![Ok(&clean), Ok(&comments)]), Ok(false), "{what}");
        }
        let comments = json!([high, low]).to_string();
        let commit = commit();
        assert_eq!(
            fine(vec![Ok(&clean), Ok(&comments), Ok(&commit)]),
            Ok(true),
            "a high a later round cleared is gone"
        );
    }

    #[test]
    fn a_review_older_than_the_head_proves_nothing() {
        let clean = merge_read(false, json!(true), "clean");
        let comments = json!([review(BOT, BEFORE, "- [LOW] a.rs:1 — x")]).to_string();
        let commit = commit();
        assert_eq!(
            fine(vec![Ok(&clean), Ok(&comments), Ok(&commit)]),
            Ok(false)
        );
    }

    #[test]
    fn a_read_that_fails_is_no_answer() {
        let clean = merge_read(false, json!(true), "clean");
        let comments = json!([review(BOT, AFTER, "- [LOW] a.rs:1 — x")]).to_string();
        assert!(fine(vec![Err("HTTP 502")]).is_err());
        assert!(fine(vec![Ok("not json")]).is_err());
        assert!(fine(vec![Ok(&clean), Err("HTTP 502")]).is_err());
        assert!(fine(vec![Ok(&clean), Ok(&comments), Err("HTTP 502")]).is_err());
    }

    #[test]
    fn poll_clears_stuck_from_a_pull_that_is_fine_and_keeps_the_label_alone() {
        let given = json!({"bot": BOT,
            "pulls": [node(9, "STUCK: a", "feature/issue-7-a", "tehas-app-bot")],
            "issues": {"7": [ATTENTION]}});
        let clean = merge_read(false, json!(true), "clean");
        let comments = json!([review(BOT, AFTER, "- [LOW] a.rs:1 — x")]).to_string();
        let commit = commit();
        let mut gh = Fake::of(vec![Ok(&clean), Ok(&comments), Ok(&commit), Ok("{}")]);
        let lines = poll(&mut gh, REPO, &given);
        assert_eq!(gh.calls[3], format!("{WRITE} title=a"));
        assert_eq!(gh.calls.len(), 4);
        assert_eq!(lines, ["pull request #9 is now without a prefix"]);
    }

    #[test]
    fn poll_keeps_stuck_on_open_high_findings_and_writes_nothing() {
        let given = json!({"bot": BOT,
            "pulls": [node(9, "STUCK: a", "feature/issue-7-a", "tehas-app-bot")],
            "issues": {"7": [ATTENTION]}});
        let clean = merge_read(false, json!(true), "clean");
        let comments = json!([review(BOT, AFTER, "- [HIGH] a.rs:1 — x")]).to_string();
        let mut gh = Fake::of(vec![Ok(&clean), Ok(&comments)]);
        assert!(poll(&mut gh, REPO, &given).is_empty());
        assert_eq!(gh.calls.len(), 2);
    }

    #[test]
    fn a_running_cycle_is_wip_however_fine_the_pull_looks_and_asks_nothing() {
        let given = json!({"bot": BOT,
            "pulls": [node(9, "WIP: a", "feature/issue-7-a", "tehas-app-bot")],
            "issues": {"7": [RUNNING, ATTENTION]}});
        let mut gh = Fake::default();
        assert!(poll(&mut gh, REPO, &given).is_empty());
        assert!(gh.calls.is_empty());
    }

    #[test]
    fn a_read_that_fails_leaves_a_stuck_title_as_it_is() {
        let given = json!({"bot": BOT,
            "pulls": [node(9, "STUCK: a", "feature/issue-7-a", "tehas-app-bot")],
            "issues": {"7": [ATTENTION]}});
        let mut gh = Fake::of(vec![Err("HTTP 502")]);
        assert_eq!(
            poll(&mut gh, REPO, &given),
            ["WARN: the title of pull request #9 was not kept: HTTP 502"]
        );
        assert_eq!(gh.calls.len(), 1);
    }

    #[test]
    fn the_branch_carries_the_issue() {
        assert_eq!(linked("feature/issue-1146-mcp-a-key"), Some(1146));
        assert_eq!(linked("feature/issue-7"), Some(7));
        assert_eq!(linked("feature/issue-"), None);
        assert_eq!(linked("feature/issue-x-1"), None);
        assert_eq!(linked("feature/issue-12x-a"), None);
        assert_eq!(linked("feature/issue-0"), None);
        assert_eq!(linked("takeover/issue-7"), None);
    }

    #[test]
    fn an_account_is_one_under_each_of_its_names() {
        for author in ["tehas-app-bot", "tehas-app-bot[bot]", "app/tehas-app-bot"] {
            for bot in ["tehas-app-bot", "tehas-app-bot[bot]"] {
                assert_eq!(account(author), account(bot));
            }
        }
        assert_ne!(account("alice"), account("tehas-app-bot[bot]"));
    }

    #[test]
    fn sync_reads_the_pull_and_the_issue_and_writes_the_title() {
        let pull = rest_pull(
            "chore(work): a",
            "feature/issue-7-a",
            "tehas-app-bot[bot]",
            "open",
        );
        let issue = json!({"labels": [{"name": ATTENTION}]}).to_string();
        // A pull request GitHub does not call clean: the second read of it.
        let mut gh = Fake::of(vec![Ok(&pull), Ok(&issue), Ok(&pull), Ok("{}")]);
        assert_eq!(sync(&mut gh, REPO, 9, "tehas-app-bot[bot]"), Ok(()));
        assert_eq!(
            gh.calls,
            [
                "api repos/acme/widget/pulls/9".to_string(),
                "api repos/acme/widget/issues/7".to_string(),
                "api repos/acme/widget/pulls/9".to_string(),
                format!("{WRITE} title=STUCK: chore(work): a"),
            ]
        );
    }

    #[test]
    fn sync_asks_who_it_is_only_without_a_login_set() {
        let pull = rest_pull("a", "feature/issue-7-a", "tehas-bot", "open");
        let issue = json!({"labels": []}).to_string();
        let mut gh = Fake::of(vec![Ok(&pull), Ok(r#"{"login":"tehas-bot"}"#), Ok(&issue)]);
        assert_eq!(sync(&mut gh, REPO, 9, ""), Ok(()));
        assert_eq!(gh.calls[1], "api user");
        assert_eq!(gh.calls.len(), 3, "a right title gets no write");
    }

    #[test]
    fn sync_leaves_a_person_a_stranger_branch_and_a_closed_pull() {
        for (head, by, st) in [
            ("feature/issue-7-a", "alice", "open"),
            ("patch-1", "tehas-bot", "open"),
            ("feature/issue-7-a", "tehas-bot", "closed"),
        ] {
            let pull = rest_pull("WIP: a", head, by, st);
            let mut gh = Fake::of(vec![Ok(&pull)]);
            assert_eq!(sync(&mut gh, REPO, 9, "tehas-bot"), Ok(()));
            assert_eq!(gh.calls.len(), 1, "{head} {by} {st}");
        }
    }

    #[test]
    fn sync_writes_nothing_when_the_state_cannot_be_read() {
        let pull = rest_pull("WIP: a", "feature/issue-7-a", "tehas-bot", "open");
        // A failed read, and an answer that names no labels: no guess.
        for answer in [Err("HTTP 502"), Ok("{}"), Ok("not json")] {
            let mut gh = Fake::of(vec![Ok(&pull), answer]);
            assert!(sync(&mut gh, REPO, 9, "tehas-bot").is_err());
            assert_eq!(gh.calls.len(), 2);
        }
        let mut gh = Fake::of(vec![Err("HTTP 502")]);
        assert_eq!(
            sync(&mut gh, REPO, 9, "tehas-bot"),
            Err("HTTP 502".to_string())
        );
        assert_eq!(gh.calls.len(), 1);
    }

    fn node(number: u64, title: &str, head: &str, by: &str) -> Value {
        json!({"number": number, "title": title, "headRefName": head,
            "author": {"login": by}, "labels": {"nodes": []}})
    }

    #[test]
    fn poll_reuses_the_labels_it_was_given() {
        let given = json!({"bot": "tehas-bot[bot]",
            "pulls": [node(9, "a", "feature/issue-7-a", "tehas-bot")],
            "issues": {"7": [RUNNING]}});
        let mut gh = Fake::default();
        let lines = poll(&mut gh, REPO, &given);
        assert_eq!(gh.calls, [format!("{WRITE} title=WIP: a")]);
        assert_eq!(lines, ["pull request #9 is now WIP"]);
    }

    #[test]
    fn poll_reads_an_issue_it_was_not_given() {
        let given = json!({"bot": "tehas-bot",
            "pulls": [node(9, "WIP: a", "feature/issue-7-a", "tehas-bot")],
            "issues": {}});
        let issue = json!({"labels": []}).to_string();
        let mut gh = Fake::of(vec![Ok(&issue), Ok("{}")]);
        poll(&mut gh, REPO, &given);
        assert_eq!(
            gh.calls,
            [
                "api repos/acme/widget/issues/7".to_string(),
                format!("{WRITE} title=a"),
            ]
        );
    }

    #[test]
    fn poll_warns_on_a_failed_read_or_write_and_goes_on() {
        let given = json!({"bot": "tehas-bot",
            "pulls": [node(8, "a", "feature/issue-6-a", "tehas-bot"),
                node(9, "a", "feature/issue-7-a", "tehas-bot")],
            "issues": {"7": [RUNNING]}});
        let mut gh = Fake::of(vec![Err("HTTP 502"), Err("HTTP 403")]);
        let lines = poll(&mut gh, REPO, &given);
        assert_eq!(gh.calls.len(), 2);
        assert_eq!(
            lines,
            [
                "WARN: the title of pull request #8 was not kept: HTTP 502",
                "WARN: the title of pull request #9 was not kept: HTTP 403",
            ]
        );
    }

    #[test]
    fn poll_asks_nothing_for_a_person_a_right_title_or_no_login() {
        let pulls = json!([
            node(8, "a", "feature/issue-7-a", "alice"),
            node(9, "WIP: a", "feature/issue-7-b", "tehas-bot")
        ]);
        let given = json!({"bot": "tehas-bot", "pulls": pulls, "issues": {"7": [RUNNING]}});
        let mut gh = Fake::default();
        assert!(poll(&mut gh, REPO, &given).is_empty());
        assert!(gh.calls.is_empty());
        let given = json!({"bot": "", "pulls": pulls, "issues": {}});
        assert!(poll(&mut gh, REPO, &given).is_empty());
        assert!(gh.calls.is_empty());
    }

    #[test]
    fn the_verbs_refuse_what_they_do_not_know() {
        for words in [vec![], vec!["sync"], vec!["sync", "x"], vec!["sync", "0"]] {
            let given: Vec<String> = words.iter().map(|w| w.to_string()).collect();
            assert_eq!(main(&given), 64);
        }
    }
}

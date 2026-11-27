//! A runner that answers each call from the adapter's recordings of the
//! fixture projects, under `tests/recorded/`: `<item>/<read>.json` is gh's
//! answer and `<item>/<read>.fail` the reason the call failed, with
//! `<item>/<read>.answer.json` beside it what gh printed as it failed. A
//! call with no recording panics. It counts the calls it was asked.
//!
//! The parents' question is answered alias by alias from each asked
//! issue's `parent.json`, the answer of the one question that asked them
//! all. An alias no recording holds panics: an answer is never made from
//! another question's.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tehas_core::forge::{Failed, GhRunner};
use tehas_github::GitHub;

#[path = "../answers/mod.rs"]
mod answers;

/// The fixture project the recordings were made against.
pub const PROJECT: &str = "acme/widget";

pub struct Replay {
    repo: String,
    asked: Arc<Count>,
}

/// The count of the calls a replay was asked.
#[derive(Default)]
pub struct Count(AtomicUsize);

impl Count {
    pub fn get(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

/// The words of `text` between `open` and the next `close`.
fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let rest = &text[text.find(open)? + open.len()..];
    Some(&rest[..rest.find(close)?])
}

/// The parents' question, which asks each number under an alias `n<n>`.
fn is_parents(query: &str) -> bool {
    query.contains(") { number parent {")
}

/// The numbers the parents' question asked, in the order of its aliases.
fn aliases(query: &str) -> Vec<u64> {
    let open = ": issue(number: ";
    let mut rest = query;
    let mut out = Vec::new();
    while let Some(i) = rest.find(open) {
        let token = rest[..i].rsplit([' ', '{']).next().unwrap_or_default();
        if let Some(no) = token.strip_prefix('n') {
            if let Ok(no) = no.parse::<u64>() {
                out.push(no);
            }
        }
        rest = &rest[i + open.len()..];
    }
    out
}

/// The recording a question is answered from: its directory and its name.
fn question(repo: &str, args: &[&str]) -> (String, String) {
    let field = |name: &str| args.iter().find_map(|a| a.strip_prefix(name));
    let query = field("query=").unwrap_or_default();
    let back = query.contains("last: 100, before");
    // The parents' question never comes here: `run` routes it to
    // `parents()` before reaching `question`.
    let (dir, mut name) = if let Some(first) = between(query, ": issue(number: ", ")") {
        ("closers".to_string(), format!("several-{first}"))
    } else if query.contains("since: $since") {
        let day = field("since=")
            .unwrap_or_default()
            .get(..10)
            .unwrap_or_default();
        ("closed".to_string(), format!("since-{day}"))
    } else if query.contains("issues(first:") {
        let mut name = "list".to_string();
        if query.contains("states: [OPEN]") {
            name.push_str("-open");
        }
        if query.contains("states: [CLOSED]") {
            name.push_str("-closed");
        }
        for label in between(query, "labels: [", "]")
            .unwrap_or_default()
            .split(", ")
            .filter(|l| !l.is_empty())
        {
            name.push('-');
            name.push_str(&label.trim_matches('"').replace(':', "_"));
        }
        if let Some(no) = between(query, "milestoneNumber: \"", "\"") {
            name.push_str(&format!("-m{no}"));
        }
        let dir = match repo {
            PROJECT => "list",
            other => other.split('/').next_back().unwrap_or_default(),
        };
        (dir.to_string(), name)
    } else {
        let dir = format!("issue-{}", field("number=").unwrap_or_default());
        let name = if query.contains("closes:") {
            match (query.contains("events:"), back) {
                (false, _) => "ticket-cut".to_string(),
                (true, false) => "ticket".to_string(),
                (true, true) => "ticket-back".to_string(),
            }
        } else if query.contains("events:") {
            let types = between(query, "itemTypes: [", "]").unwrap_or_default();
            let mut name = "history".to_string();
            if types.split(", ").count() != 14 {
                name.push('-');
                name.push_str(&types.to_ascii_lowercase().replace(", ", "+"));
            }
            if back {
                name.push_str("-back");
            }
            name
        } else if query.contains("mentions:") {
            "mentions".to_string()
        } else if query.contains("blockedBy(") {
            "blockers".to_string()
        } else if query.contains("subIssues") {
            "parts".to_string()
        } else if query.contains("parent {") {
            "parent".to_string()
        } else {
            "exists".to_string()
        };
        (dir, name)
    };
    if let Some(cursor) = field("cursor=") {
        name.push('-');
        name.push_str(cursor);
    }
    (dir, name)
}

impl Replay {
    /// The parents' question: the answer recorded for each asked number,
    /// alias by alias. An alias whose recording holds no node for it
    /// panics: an answer is never made from another question's.
    fn parents(&self, query: &str) -> Result<String, String> {
        let asked = aliases(query);
        if asked.is_empty() {
            panic!("the parents' question asks no number: {query}");
        }
        let mut whole = None;
        let mut held: Vec<u64> = Vec::new();
        for no in &asked {
            let at = answers::find(&format!("issue-{no}"), "parent.json")
                .unwrap_or_else(|| panic!("no recording of the parents' question of {no}"));
            let text =
                std::fs::read_to_string(&at).unwrap_or_else(|e| panic!("{}: {e}", at.display()));
            held.extend(
                asked
                    .iter()
                    .copied()
                    .filter(|n| text.contains(&format!("\"n{n}\""))),
            );
            whole = Some(text);
        }
        // Every asked alias must be held by some recording. The kit holds
        // the whole question in each, so nothing legitimate changes.
        if let Some(no) = asked.iter().find(|n| !held.contains(n)) {
            panic!("no recording of the parents' question of {no}");
        }
        Ok(whole.unwrap_or_default())
    }
}

impl GhRunner for Replay {
    /// `run`, and beside a `.fail` what gh printed as it failed, from
    /// `<read>.answer.json` where there is one.
    fn run_answer(&mut self, args: &[&str]) -> Result<String, Failed> {
        self.run(args).map_err(|why| {
            let answer = match args {
                ["api", "graphql", ..] => {
                    let (dir, name) = question(&self.repo, args);
                    let at = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/recorded")
                        .join(dir)
                        .join(format!("{name}.answer.json"));
                    std::fs::read_to_string(at).ok()
                }
                _ => None,
            };
            Failed { why, answer }
        })
    }

    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.asked.0.store(self.asked.get() + 1, Ordering::SeqCst);
        let none = || -> String { panic!("no recording of {args:?}") };
        let query = args
            .iter()
            .find_map(|a| a.strip_prefix("query="))
            .unwrap_or_default();
        if is_parents(query) {
            return self.parents(query);
        }
        let (dir, name) = match args {
            ["api", "graphql", ..] => question(&self.repo, args),
            ["api", "-X", "DELETE", path] => {
                let rest = path
                    .strip_prefix(&format!("repos/{}/issues/", self.repo))
                    .ok_or_else(none)?;
                let issue = rest.split('/').next().unwrap_or_default();
                (format!("issue-{issue}"), "unmark".to_string())
            }
            ["api", path, ..] => {
                let rest = path
                    .strip_prefix(&format!("repos/{}/issues/", self.repo))
                    .ok_or_else(none)?;
                match rest.split_once('/') {
                    Some((n, "events?per_page=100")) => {
                        (format!("issue-{n}"), "events".to_string())
                    }
                    None => (format!("issue-{rest}"), "issue".to_string()),
                    Some(_) => return Err(none()),
                }
            }
            _ => return Err(none()),
        };
        if let Some(at) = answers::find(&dir, &format!("{name}.json")) {
            let out =
                std::fs::read_to_string(&at).unwrap_or_else(|e| panic!("{}: {e}", at.display()));
            return Ok(out);
        }
        match answers::find(&dir, &format!("{name}.fail")) {
            Some(at) => {
                let why = std::fs::read_to_string(&at)
                    .unwrap_or_else(|e| panic!("{}: {e}", at.display()));
                Err(why.trim_end().to_string())
            }
            None => panic!("no recording {name} of {args:?}"),
        }
    }
}

/// The time the replay's clock reads: a spent budget answers it as `until`.
fn replay_now() -> i64 {
    1_767_225_600
}

/// The adapter of a fixture project over its recordings, and the count of
/// the calls it made.
pub fn adapter_of(repo: &str) -> (GitHub, Arc<Count>) {
    let asked = Arc::new(Count::default());
    let replay = Replay {
        repo: repo.to_string(),
        asked: Arc::clone(&asked),
    };
    (
        GitHub::new(repo, Box::new(replay)).on_clock(replay_now),
        asked,
    )
}

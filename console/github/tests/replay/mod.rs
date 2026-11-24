//! A runner that answers each call from the adapter's recordings of the
//! fixture projects, under `tests/recorded/`: `<item>/<read>.json` is gh's
//! answer and `<item>/<read>.fail` the reason the call failed. A call with
//! no recording fails. It counts the calls it was asked.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tehas_core::forge::GhRunner;
use tehas_github::GitHub;

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

/// The recording a question is answered from: its directory and its name.
fn question(repo: &str, args: &[&str]) -> (String, String) {
    let field = |name: &str| args.iter().find_map(|a| a.strip_prefix(name));
    let query = field("query=").unwrap_or_default();
    let back = query.contains("last: 100, before");
    let (dir, mut name) = if query.contains("issues(first:") {
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

impl GhRunner for Replay {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.asked.0.store(self.asked.get() + 1, Ordering::SeqCst);
        let none = || format!("no recording of {args:?}");
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
        let at = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/recorded")
            .join(dir);
        if let Ok(out) = std::fs::read_to_string(at.join(format!("{name}.json"))) {
            return Ok(out);
        }
        match std::fs::read_to_string(at.join(format!("{name}.fail"))) {
            Ok(why) => Err(why.trim_end().to_string()),
            Err(_) => Err(format!("no recording {name} of {args:?}")),
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

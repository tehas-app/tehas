//! The notice tehasbot preview keeps at the top of a pull request's
//! description: where the preview is, or why there is none.
//!
//! `tell` is given the time GitHub gave for the pull request in the list the
//! run read, a root that holds <number>/state.json, and the address where a
//! serving preview answers. What is said goes to standard output, one line
//! each. A failed call is tried at the next run.
//!
//! What this command said about a commit and state is on record in
//! <root>/<number>/.notice, as "<commit> <state> <outcome> <attempts> <time>":
//! none (nothing to say), said, refused (the block at the top is not this
//! command's), failed with how many writes GitHub did not take, or gaveup
//! after three. A record stands while GitHub gives the same time for the pull
//! request; a later time is a description someone changed, and it is read
//! again. Only a read that failed leaves nothing on record.
//!
//! The description is read at the moment of the edit. Only a block this
//! command wrote is changed: one that opens the description between the two
//! marker lines and holds nothing but one of its notices. What GitHub had is
//! kept in <root>/<number>/.description, mode 0600, before every write.

use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const MARKER: &str = "<!-- tehas-preview -->";
const MARKER_END: &str = "<!-- /tehas-preview -->";
// How the three notice lines begin, and the sentence two of them end with.
const STARTS: [&str; 3] = [
    "Preview of `",
    "Unable to create a preview",
    "There is no preview",
];
const TAIL: &str =
    "It is rebuilt when the branch changes and removed when this pull request is closed.";

pub fn tell(n: &str, stamp: &str, repo: &str, root: &Path, address: &str) {
    Tell {
        n,
        repo,
        dir: root.join(n),
        address,
    }
    .tell(stamp);
}

/// What to do with a description, decided from what GitHub has now.
#[derive(Debug, PartialEq)]
enum Plan {
    Write { old: String, new: String },
    Nothing,
    Closed,
    Refuse,
}

/// One short reason for a call that failed, from what it said last. A token
/// that lost access gets 404 for every pull request, which an operator can
/// tell from a rate limit or a call that never came back.
fn why_failed(said: &str) -> String {
    let line: String = said
        .lines()
        .last()
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    if line.contains("HTTP 404") {
        "no such pull request (HTTP 404)".to_string()
    } else if line.contains("HTTP 403") || line.contains("HTTP 429") || line.contains("rate limit")
    {
        "GitHub refused the call (HTTP 403)".to_string()
    } else if line.is_empty() {
        "no answer".to_string()
    } else {
        line.chars().take(80).collect()
    }
}

fn bare(line: &str) -> &str {
    line.strip_suffix('\r').unwrap_or(line)
}

fn mine(line: Option<&&str>) -> bool {
    let text = bare(line.copied().unwrap_or(""));
    STARTS.iter().any(|s| text.starts_with(s))
}

/// Everything between the two marker lines is one of this command's notices:
/// its line, and for a preview made or attempted, a blank line and the tail.
/// One blank line under the opening marker is how the notice is written.
fn ours(lines: &[&str]) -> bool {
    let r = match lines.first() {
        Some(first) if bare(first).is_empty() => &lines[1..],
        _ => lines,
    };
    mine(r.first())
        && (r.len() == 1 || (r.len() == 3 && bare(r[1]).is_empty() && bare(r[2]) == TAIL))
}

/// The decision about one pull request's description, and the time GitHub
/// gives for it. Err is why the answer could not be used.
fn plan(answer: &Value, n: &str, block: &str, create: bool) -> Result<(Plan, String), String> {
    if answer
        .get("number")
        .and_then(Value::as_u64)
        .map(|x| x.to_string())
        != Some(n.to_string())
    {
        return Err("not this pull request".to_string());
    }
    let seen = answer
        .get("updated_at")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let old = match answer.get("body") {
        Some(Value::String(body)) => body.clone(),
        Some(Value::Null) => String::new(),
        _ => return Err("no description".to_string()),
    };
    // A merged or closed pull request keeps its description.
    if answer.get("state").and_then(Value::as_str) != Some("open") {
        return Ok((Plan::Closed, seen));
    }
    let lines: Vec<&str> = if old.is_empty() {
        Vec::new()
    } else {
        old.split('\n').collect()
    };
    let at =
        |m: &str| -> Vec<usize> { (0..lines.len()).filter(|&i| bare(lines[i]) == m).collect() };
    let (opens, closes) = (at(MARKER), at(MARKER_END));
    // A closing marker on the first line is under no block of ours.
    let under: Vec<usize> = closes.iter().copied().filter(|&i| i > 0).collect();
    let decided = if opens.is_empty() && closes.is_empty() {
        if create {
            let new = if old.is_empty() {
                block.to_string()
            } else {
                format!("{block}\n\n{old}")
            };
            Plan::Write { old, new }
        } else {
            Plan::Nothing
        }
    } else if opens.first() == Some(&0) && !under.is_empty() && ours(&lines[1..under[0]]) {
        // The block, then everything after the closing line as GitHub gave
        // it. With nothing under that line the block ends with the newline
        // it ended with, so the description keeps its shape.
        let rest = &lines[under[0] + 1..];
        let new = if rest.is_empty() {
            format!("{block}\n")
        } else {
            format!("{block}\n{}", rest.join("\n"))
        };
        Plan::Write { old, new }
    } else {
        Plan::Refuse
    };
    Ok((decided, seen))
}

struct Tell<'a> {
    n: &'a str,
    repo: &'a str,
    dir: PathBuf,
    address: &'a str,
}

impl Tell<'_> {
    fn say(&self, what: &str) {
        let _ = writeln!(std::io::stdout(), "#{}: {what}", self.n);
    }

    fn field(state: &Value, key: &str) -> String {
        match state.get(key) {
            Some(Value::String(s)) => s.clone(),
            None | Some(Value::Null) => String::new(),
            Some(other) => other.to_string(),
        }
    }

    /// The record for this key: (outcome, attempts, time), or nothing.
    fn said(&self, key: &str) -> Option<(String, u32, String)> {
        let line = fs::read_to_string(self.dir.join(".notice")).ok()?;
        let line = line.lines().next().unwrap_or("");
        let f: Vec<&str> = line.split_whitespace().collect();
        let get = |i: usize| f.get(i).copied().unwrap_or("");
        if format!("{} {}", get(0), get(1)) != key {
            return None;
        }
        let tries = match get(3) {
            t if t.len() == 1 && t.as_bytes()[0].is_ascii_digit() => t.parse().unwrap_or(0),
            _ => 0,
        };
        Some((get(2).to_string(), tries, get(4).to_string()))
    }

    /// Puts that on record. Without the record the same notice would be
    /// written at every run, so a record not written is said.
    fn record(&self, key: &str, outcome: &str, tries: u32, seen: &str) {
        let mut line = format!("{key} {outcome} {tries}");
        if !seen.is_empty() {
            line = format!("{line} {seen}");
        }
        if fs::write(self.dir.join(".notice"), format!("{line}\n")).is_err() {
            self.say("the notice was not recorded on the host, tried again at the next run");
        }
    }

    fn gh(&self, args: &[&str]) -> Result<String, String> {
        let out = Command::new("gh")
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).into_owned())
        }
    }

    fn tell(&self, stamp: &str) {
        let state: Value = fs::read_to_string(self.dir.join("state.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(Value::Null);
        let (sha, now, reason) = (
            Self::field(&state, "sha"),
            Self::field(&state, "state"),
            Self::field(&state, "reason"),
        );
        let key = format!("{sha} {now}");
        let (outcome, mut tries, was) = self.said(&key).unwrap_or_default();
        match outcome.as_str() {
            // The record stands while GitHub gives the same time.
            "said" | "none" | "refused" | "gaveup" if was == stamp => return,
            "failed" if tries >= 3 => {
                self.record(&key, "gaveup", tries, &was);
                return;
            }
            _ => {}
        }
        let short: String = sha.chars().take(7).collect();
        let (text, create) = match now.as_str() {
            "serving" => (format!("{}{short}`: {}", STARTS[0], self.address), true),
            "build-failed" | "no-space" => (format!("{} of `{short}`. {reason}", STARTS[1]), true),
            // Nothing to say to a description with no notice; one that has a
            // notice has it changed, or it would give an address that 404s.
            "over-limit" | "no-recipe" => (format!("{} of `{short}`. {reason}", STARTS[2]), false),
            _ => return,
        };
        // The blank line under the opening marker keeps the notice out of
        // the HTML comment.
        let mut block = format!("{MARKER}\n\n{text}");
        if create {
            block = format!("{block}\n\n{TAIL}");
        }
        block = format!("{block}\n{MARKER_END}");

        let path = format!("repos/{}/pulls/{}", self.repo, self.n);
        let answer = match self.gh(&["api", &path, "--jq", "{number, body, state, updated_at}"]) {
            Ok(answer) => answer,
            Err(said) => {
                self.say(&format!(
                    "the description could not be read ({}), tried again at the next run",
                    why_failed(&said)
                ));
                return;
            }
        };
        let decided = serde_json::from_str::<Value>(&answer)
            .map_err(|e| e.to_string())
            .and_then(|answer| plan(&answer, self.n, &block, create));
        let (decided, seen) = match decided {
            Ok(decided) => decided,
            Err(why) => {
                self.say(&format!(
                    "the answer about this pull request was not understood ({}), tried again at the next run",
                    why_failed(&why)
                ));
                return;
            }
        };
        let (old, new) = match decided {
            Plan::Nothing => return self.record(&key, "none", 0, &seen),
            Plan::Closed => {
                // Closed since the list was read: the next run removes it.
                self.record(&key, "none", 0, &seen);
                return self.say("the pull request is no longer open, nothing was written");
            }
            Plan::Refuse => {
                // Said once for this commit and state, not at every look.
                self.record(&key, "refused", 0, &seen);
                if outcome != "refused" {
                    self.say(
                        "the description has a preview notice that is not this one, left alone",
                    );
                }
                return;
            }
            Plan::Write { old, new } => (old, new),
        };
        if new == old {
            return self.record(&key, "none", 0, &seen);
        }
        // An edit written over by mistake can be read back from here.
        let kept = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(self.dir.join(".description"))
            .and_then(|mut f| f.write_all(old.as_bytes()));
        if kept.is_err() {
            return self.say(
                "the description was not backed up, the notice was not written, tried again at the next run",
            );
        }
        // GitHub answers a write with the pull request as it has it then,
        // and the time of that answer goes on record, so the notice is not
        // read back at the next run only to be found unchanged.
        let body = format!("body={new}");
        match self.gh(&["api", "-X", "PATCH", &path, "-f", &body]) {
            Ok(answer) => {
                let after = serde_json::from_str::<Value>(&answer)
                    .ok()
                    .and_then(|a| {
                        a.get("updated_at")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .unwrap_or_default();
                let at = if after.is_empty() { seen } else { after };
                self.record(&key, "said", 0, &at);
                self.say("the notice was written");
            }
            Err(_) => {
                tries += 1;
                if tries >= 3 {
                    // Said once, with the third write GitHub did not take.
                    self.record(&key, "gaveup", tries, &seen);
                    if outcome != "gaveup" {
                        self.say("the notice was not written three times, left to the next commit or state");
                    }
                } else {
                    self.record(&key, "failed", tries, &seen);
                    self.say("the notice was not written, tried again at the next run");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const BLOCK: &str = "<!-- tehas-preview -->\n\nPreview of `abc1234`: https://x/\n\nIt is rebuilt when the branch changes and removed when this pull request is closed.\n<!-- /tehas-preview -->";

    fn answer(body: Value) -> Value {
        json!({"number": 12, "body": body, "state": "open", "updated_at": "t1"})
    }

    #[test]
    fn a_description_without_markers_gets_the_notice_above_it() {
        let (p, seen) = plan(&answer(json!("Hello")), "12", BLOCK, true).unwrap();
        assert_eq!(seen, "t1");
        assert_eq!(
            p,
            Plan::Write {
                old: "Hello".into(),
                new: format!("{BLOCK}\n\nHello")
            }
        );
        let (p, _) = plan(&answer(Value::Null), "12", BLOCK, true).unwrap();
        assert_eq!(
            p,
            Plan::Write {
                old: String::new(),
                new: BLOCK.into()
            }
        );
        let (p, _) = plan(&answer(json!("Hello")), "12", BLOCK, false).unwrap();
        assert_eq!(p, Plan::Nothing);
    }

    #[test]
    fn our_block_is_replaced_and_the_rest_kept() {
        let old = "<!-- tehas-preview -->\r\n\r\nUnable to create a preview of `x`. no\r\n<!-- /tehas-preview -->\r\nText\r\n";
        let (p, _) = plan(&answer(json!(old)), "12", BLOCK, true).unwrap();
        assert_eq!(
            p,
            Plan::Write {
                old: old.into(),
                new: format!("{BLOCK}\nText\r\n")
            }
        );
        let only = format!("{BLOCK}\n");
        let (p, _) = plan(&answer(json!(only)), "12", BLOCK, true).unwrap();
        assert_eq!(
            p,
            Plan::Write {
                old: only.clone(),
                new: only
            }
        );
    }

    #[test]
    fn a_block_that_is_not_ours_is_refused() {
        for body in [
            "<!-- tehas-preview -->\nsomething else\n<!-- /tehas-preview -->",
            "Hi\n<!-- tehas-preview -->\nPreview of `x`\n<!-- /tehas-preview -->",
            "<!-- tehas-preview -->\nPreview of `x`\nmore\n<!-- /tehas-preview -->",
            "<!-- /tehas-preview -->",
        ] {
            let (p, _) = plan(&answer(json!(body)), "12", BLOCK, true).unwrap();
            assert_eq!(p, Plan::Refuse, "{body}");
        }
    }

    #[test]
    fn a_closed_pull_request_and_an_answer_of_another_shape() {
        let closed = json!({"number": 12, "body": "x", "state": "closed"});
        assert_eq!(
            plan(&closed, "12", BLOCK, true).unwrap(),
            (Plan::Closed, String::new())
        );
        assert!(plan(&json!({"number": 13, "body": ""}), "12", BLOCK, true).is_err());
        assert!(plan(&json!({"number": 12}), "12", BLOCK, true).is_err());
        assert!(plan(&json!({"number": 12, "body": 5}), "12", BLOCK, true).is_err());
    }

    #[test]
    fn why_a_call_failed() {
        assert_eq!(
            why_failed("x\ngh: Not Found (HTTP 404)\n"),
            "no such pull request (HTTP 404)"
        );
        assert_eq!(
            why_failed("API rate limit exceeded"),
            "GitHub refused the call (HTTP 403)"
        );
        assert_eq!(why_failed(""), "no answer");
        assert_eq!(why_failed(&"a".repeat(100)).len(), 80);
    }
}

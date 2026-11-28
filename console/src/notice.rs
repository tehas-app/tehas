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
use tehas_core::time;
use tehas_ports::{
    Bound, ChangeDigest, ChangeFilter, ChangeNo, ChangeState, ChangeStates, CodeHost, Outcome,
    Unread,
};

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

/// The notice for pull request `n`, its description read on the sweep's
/// code-host handle.
pub fn tell(
    n: &str,
    stamp: &str,
    repo: &str,
    root: &Path,
    address: &str,
    host: &mut Option<Box<dyn CodeHost + Send>>,
) {
    Tell {
        n,
        repo,
        dir: root.join(n),
        address,
    }
    .tell(stamp, host);
}

/// What to do with a description, decided from what GitHub has now.
#[derive(Debug, PartialEq)]
enum Plan {
    Write { old: String, new: String },
    Nothing,
    Closed,
    Refuse,
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

/// The pull request read on the sweep's code-host handle. Ok(None): the
/// site answered and names no such pull request. Err: the read failed, and
/// the outcome says why.
fn read(
    host: &mut Option<Box<dyn CodeHost + Send>>,
    n: &str,
) -> Result<Option<ChangeDigest>, Outcome> {
    let Some(no) = n.parse::<u64>().ok().and_then(ChangeNo::new) else {
        return Ok(None);
    };
    let Some(bound) = Bound::new(1) else {
        return Err(Outcome::Unread(Unread::Malformed));
    };
    // The handle is None when the wiring failed before the run: no answer,
    // as a call that never came back was before this read was a port call.
    let Some(host) = host.as_mut() else {
        return Err(Outcome::Unread(Unread::Malformed));
    };
    let filter = ChangeFilter {
        states: ChangeStates::All,
        numbers: vec![no],
        bodies: true,
        ..Default::default()
    };
    host.changes(&filter, bound)
        .map(|digests| digests.into_items().into_iter().find(|d| d.no == no))
}

/// The decision about one pull request's description, and the time GitHub
/// gives for it. Err is why the answer could not be used.
fn plan(answer: &ChangeDigest, block: &str, create: bool) -> Result<(Plan, String), String> {
    let seen = time::iso(answer.updated.secs());
    let old = match &answer.body {
        Some(body) => body.clone(),
        None => return Err("no description".to_string()),
    };
    // A merged or closed pull request keeps its description.
    if answer.state != ChangeState::Open {
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
    /// The repository, for the one write this command still makes with gh.
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
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        let out = tehas_core::call::run("gh", &args, tehas_core::call::GH_BOUND);
        if out.ok() {
            Ok(out.out)
        } else {
            Err(out.err)
        }
    }

    fn tell(&self, stamp: &str, host: &mut Option<Box<dyn CodeHost + Send>>) {
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

        let answer = match read(host, self.n) {
            Ok(Some(answer)) => answer,
            // The site answered and names no such pull request: what gh's
            // 404 said before this read was a port call.
            Ok(None) => {
                self.say(&format!(
                    "the description could not be read ({}), tried again at the next run",
                    Outcome::NotFound.line()
                ));
                return;
            }
            Err(outcome) => {
                // An answer that came but is no pull request keeps the
                // notice's own words, as it did when the command parsed
                // gh's answer itself; a call that failed says the outcome.
                let words = if matches!(outcome, Outcome::Unread(_)) {
                    "the answer about this pull request was not understood"
                } else {
                    "the description could not be read"
                };
                self.say(&format!(
                    "{words} ({}), tried again at the next run",
                    outcome.line()
                ));
                return;
            }
        };
        let decided = plan(&answer, &block, create);
        let (decided, seen) = match decided {
            Ok(decided) => decided,
            Err(why) => {
                self.say(&format!(
                    "the answer about this pull request was not understood ({why}), tried again at the next run",
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
        let path = format!("repos/{}/pulls/{}", self.repo, self.n);
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
    use tehas_ports::fake::{FakeSite, World};
    use tehas_ports::{Actor, At, Branch, Caller, CommitId, Scope, Site, WebLink};

    const BLOCK: &str = "<!-- tehas-preview -->\n\nPreview of `abc1234`: https://x/\n\nIt is rebuilt when the branch changes and removed when this pull request is closed.\n<!-- /tehas-preview -->";

    /// A pull request of ours with this description. The time it gives is
    /// the second after the epoch, so a seen is `1970-01-01T00:00:01Z`.
    fn answer(body: Option<&str>) -> ChangeDigest {
        let mut d = ChangeDigest::new(
            ChangeNo::new(12).unwrap(),
            ChangeState::Open,
            Branch::new("feature/x").unwrap(),
            CommitId::new("a".repeat(40)).unwrap(),
            Branch::new("main").unwrap(),
            CommitId::new("b".repeat(40)).unwrap(),
            At::new(1),
            At::new(1),
            WebLink::new("https://example.test/c/12").unwrap(),
        );
        d.body = body.map(str::to_string);
        d
    }

    fn closed(body: &str) -> ChangeDigest {
        let mut d = answer(Some(body));
        d.state = ChangeState::Closed { at: At::new(2) };
        d
    }

    #[test]
    fn a_description_without_markers_gets_the_notice_above_it() {
        let (p, seen) = plan(&answer(Some("Hello")), BLOCK, true).unwrap();
        assert_eq!(seen, "1970-01-01T00:00:01Z");
        assert_eq!(
            p,
            Plan::Write {
                old: "Hello".into(),
                new: format!("{BLOCK}\n\nHello")
            }
        );
        let (p, _) = plan(&answer(Some("")), BLOCK, true).unwrap();
        assert_eq!(
            p,
            Plan::Write {
                old: String::new(),
                new: BLOCK.into()
            }
        );
        let (p, _) = plan(&answer(Some("Hello")), BLOCK, false).unwrap();
        assert_eq!(p, Plan::Nothing);
    }

    #[test]
    fn our_block_is_replaced_and_the_rest_kept() {
        let old = "<!-- tehas-preview -->\r\n\r\nUnable to create a preview of `x`. no\r\n<!-- /tehas-preview -->\r\nText\r\n";
        let (p, _) = plan(&answer(Some(old)), BLOCK, true).unwrap();
        assert_eq!(
            p,
            Plan::Write {
                old: old.into(),
                new: format!("{BLOCK}\nText\r\n")
            }
        );
        let only = format!("{BLOCK}\n");
        let (p, _) = plan(&answer(Some(&only)), BLOCK, true).unwrap();
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
            let (p, _) = plan(&answer(Some(body)), BLOCK, true).unwrap();
            assert_eq!(p, Plan::Refuse, "{body}");
        }
    }

    #[test]
    fn a_closed_pull_request_and_an_answer_that_cannot_be_used() {
        let (p, seen) = plan(&closed("x"), BLOCK, true).unwrap();
        assert_eq!(p, Plan::Closed);
        assert_eq!(seen, "1970-01-01T00:00:01Z");
        assert_eq!(
            plan(&answer(None), BLOCK, true).unwrap_err(),
            "no description"
        );
    }

    /// The read on the ports' fake: the number the site names answers, any
    /// other number or a name that is no number is no answer, and no
    /// handle at all is an answer that cannot be read.
    #[test]
    fn the_read_answers_only_the_number_the_site_names() {
        let world = World::new().change(12, "feature/x", |c| c.body("Hello"));
        let ask = |n: &str| -> Result<Option<ChangeDigest>, Outcome> {
            let mut host = Some(
                FakeSite::new(&world)
                    .code_host(&Actor::Ours(Scope::Full), Caller::new("notice"))
                    .unwrap(),
            );
            read(&mut host, n)
        };
        let got = ask("12").unwrap().expect("the digest");
        assert_eq!(got.body.as_deref(), Some("Hello"));
        assert_eq!(ask("13").unwrap(), None);
        assert_eq!(ask("x").unwrap(), None);
        let line = read(&mut None, "12").unwrap_err().line().to_string();
        assert_eq!(line, "the answer does not read");
    }
}

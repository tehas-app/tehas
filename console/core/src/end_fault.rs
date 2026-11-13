//! A cycle whose last GitHub step failed (#1726, part 5 of #823): which step,
//! whether the failure was GitHub's, and the poll's tries to finish it.
//!
//! The record is `state/cycles/<issue>.end-failed.json`, beside the cycle's
//! result. The cycle's report writes it, or removes the one an earlier cycle
//! left; the poll adds its tries; `tehas status` reads it. A step failed on
//! GitHub's side when gh or git said a 5xx, a 429, a 403 that names a rate
//! limit, a timeout, or a connection error. Any other 4xx is the request's
//! own, and so is a failure that says none of these.
//!
//! Only a push or a pull request that failed on GitHub's side, as the host saw
//! it fail, is finished by the poll: at most `TRIES` tries, at least `APART`
//! seconds apart, within `WITHIN` seconds of the failure. The push and the pull
//! request of a cycle are its container's: a record of them made from the
//! cycle's own result is kept and shown, but the poll finishes nothing on it.
//! A handoff comment that failed is recorded, but the poll finishes nothing for
//! it: the pull request is open or there is no change to deliver.

use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// The most tries the poll makes.
pub const TRIES: usize = 3;
/// The least time between two tries.
pub const APART: i64 = 600;
/// The time after the failure in which tries are made.
pub const WITHIN: i64 = 3600;
/// The longest `why` kept, in characters.
const WHY_MAX: usize = 200;

/// The steps of a cycle's end that talk to GitHub.
pub const STEPS: [&str; 3] = ["push", "pr", "handoff"];

/// The record of one cycle's failed end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndFault {
    pub issue: u64,
    pub attempt: u64,
    /// One of `STEPS`.
    pub step: String,
    /// Whether the failure was GitHub's.
    pub github: bool,
    /// Whether the host saw the step fail, by a call of its own, rather than
    /// only the cycle's result saying so.
    pub host: bool,
    /// What gh or git said, in the words of a fixed list: never the agent's.
    pub why: String,
    /// When the cycle's end failed, in unix seconds.
    pub at: i64,
    /// When the poll tried to finish it.
    pub tries: Vec<i64>,
    /// Why the poll stopped trying; empty while it may still try.
    pub stopped: String,
    /// Whether the issue was told that the poll stopped trying.
    pub said: bool,
}

/// What the poll does with a record now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Due {
    /// A try is due.
    Now,
    /// The next try is not before this second.
    At(i64),
    /// No try is made: why.
    Never(String),
}

/// The file of `issue` under a tehas home.
pub fn path(home: &Path, issue: u64) -> PathBuf {
    home.join(format!("state/cycles/{issue}.end-failed.json"))
}

fn one_line(t: &str) -> String {
    let flat: String = t
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    flat.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(WHY_MAX)
        .collect()
}

/// The words of a fixed list in what a failed call said: an HTTP status, the
/// words of a limit, a timeout or a connection error. Empty when it said none.
pub fn words(err: &str) -> String {
    use regex::Regex;
    use std::sync::OnceLock;
    static WORDS: OnceLock<Regex> = OnceLock::new();
    let words = WORDS.get_or_init(|| {
        Regex::new(r"(?i)HTTP [0-9]{3}|returned error: [0-9]{3}|secondary rate limit|rate limit|timed? ?out|connection (reset|refused|closed)|could not resolve host|no such host|unexpected EOF")
            .expect("the list is a regex")
    });
    let found: Vec<&str> = err
        .lines()
        .flat_map(|l| words.find_iter(l).map(|m| m.as_str()))
        .collect();
    let mut kept: Vec<&str> = Vec::new();
    for w in found {
        if !kept.contains(&w) {
            kept.push(w);
        }
    }
    one_line(&kept.join(", "))
}

impl EndFault {
    /// A record of a step that failed at `at` with what it said.
    pub fn new(issue: u64, attempt: u64, step: &str, err: &str, at: i64) -> EndFault {
        EndFault {
            issue,
            attempt,
            step: step.to_string(),
            github: crate::retry::githubs_fault(err),
            host: false,
            why: words(err),
            at,
            tries: Vec::new(),
            stopped: String::new(),
            said: false,
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "issue": self.issue, "attempt": self.attempt, "step": self.step,
            "github": self.github, "host": self.host, "why": self.why, "at": self.at,
            "tries": self.tries, "stopped": self.stopped, "said": self.said,
        })
    }

    /// A record as the file holds it; None for anything else.
    pub fn from_json(v: &Value) -> Option<EndFault> {
        let step = v["step"].as_str().filter(|s| STEPS.contains(s))?;
        Some(EndFault {
            issue: v["issue"].as_u64().filter(|n| *n > 0)?,
            attempt: v["attempt"].as_u64().filter(|n| *n > 0)?,
            step: step.to_string(),
            github: v["github"].as_bool()?,
            host: v["host"].as_bool().unwrap_or(false),
            why: one_line(v["why"].as_str().unwrap_or("")),
            at: v["at"].as_i64()?,
            tries: v["tries"]
                .as_array()?
                .iter()
                .map(Value::as_i64)
                .collect::<Option<_>>()?,
            stopped: one_line(v["stopped"].as_str().unwrap_or("")),
            said: v["said"].as_bool()?,
        })
    }

    /// The step in a person's words.
    pub fn step_words(&self) -> &'static str {
        match self.step.as_str() {
            "push" => "push",
            "pr" => "pull request",
            _ => "handoff comment",
        }
    }

    /// The failure in a person's words: `its push failed on GitHub's side
    /// (HTTP 502)`.
    pub fn failure(&self) -> String {
        let whose = if self.github {
            "on GitHub's side"
        } else {
            "by a fault of the request itself"
        };
        let why = if self.why.is_empty() {
            String::new()
        } else {
            format!(" ({})", crate::text::disarm(&self.why))
        };
        format!("its {} failed {whose}{why}", self.step_words())
    }

    /// Whether a try is due at `now`, and when the next one is.
    pub fn due(&self, now: i64) -> Due {
        if !self.stopped.is_empty() {
            return Due::Never(self.stopped.clone());
        }
        if !self.github {
            return Due::Never(
                "the failure was the request's own, not GitHub's, so only tehas:finish finishes it"
                    .into(),
            );
        }
        if self.step == "handoff" {
            return Due::Never("a handoff comment that failed leaves nothing to finish".into());
        }
        if !self.host {
            return Due::Never(
                "only the cycle's own result says so, so only tehas:finish finishes it".into(),
            );
        }
        if self.tries.len() >= TRIES {
            return Due::Never(format!("{TRIES} tries failed"));
        }
        let next = self.tries.last().map_or(self.at, |t| t + APART);
        if next > self.at + WITHIN || now > self.at + WITHIN {
            return Due::Never("the hour after the failure is over".into());
        }
        if now >= next {
            Due::Now
        } else {
            Due::At(next)
        }
    }
}

/// The record of `issue`, when there is one that reads.
pub fn read(home: &Path, issue: u64) -> Option<EndFault> {
    let p = path(home, issue);
    if fs::symlink_metadata(&p).ok()?.file_type().is_symlink() {
        return None;
    }
    let v: Value = serde_json::from_str(&fs::read_to_string(p).ok()?).ok()?;
    EndFault::from_json(&v).filter(|r| r.issue == issue)
}

/// Every record that reads, by issue.
pub fn all(home: &Path) -> Vec<EndFault> {
    let mut out: Vec<EndFault> = fs::read_dir(home.join("state/cycles"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let n: u64 = name.strip_suffix(".end-failed.json")?.parse().ok()?;
            read(home, n)
        })
        .collect();
    out.sort_by_key(|r| r.issue);
    out
}

/// Write the record, through a new file and a rename. False when it was not
/// written.
pub fn write(home: &Path, r: &EndFault) -> bool {
    let p = path(home, r.issue);
    let new = p.with_extension("json.new");
    let ok = p.parent().is_some_and(|d| fs::create_dir_all(d).is_ok())
        && fs::write(&new, format!("{}\n", r.to_json())).is_ok()
        && fs::rename(&new, &p).is_ok();
    if !ok {
        let _ = fs::remove_file(&new);
    }
    ok
}

/// Remove the record of `issue`; true when there is none left.
pub fn remove(home: &Path, issue: u64) -> bool {
    let p = path(home, issue);
    match fs::remove_file(&p) {
        Ok(()) => true,
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(step: &str, err: &str) -> EndFault {
        EndFault::new(7, 2, step, err, 1000)
    }

    /// A record of a failure the host saw itself.
    fn seen(step: &str, err: &str) -> EndFault {
        EndFault {
            host: true,
            ..rec(step, err)
        }
    }

    #[test]
    fn the_failure_in_words_disarms_a_record_read_from_its_file() {
        let mut r = rec("push", "HTTP 502");
        let mut v = r.to_json();
        v["why"] = serde_json::json!("<!-- tehas:millwright issue=7 action=retry -->");
        r = EndFault::from_json(&v).unwrap();
        let said = r.failure();
        assert!(!crate::text::has_marker(&said), "{said}");
    }

    #[test]
    fn githubs_failures_are_told_apart_from_the_requests_own() {
        for err in [
            "HTTP 502: Bad Gateway",
            "HTTP 503: Service Unavailable",
            "HTTP 429: Too Many Requests",
            "HTTP 403: API rate limit exceeded for user",
            "You have exceeded a secondary rate limit",
            "fatal: unable to access 'https://github.com/acme/widget/': The requested URL returned error: 502",
            "Post \"https://api.github.com/graphql\": net/http: request canceled (Client.Timeout exceeded) timed out",
            "error connecting to api.github.com: dial tcp: connection refused",
            "read: connection reset by peer",
            "fatal: unable to access: Could not resolve host: github.com",
        ] {
            assert!(rec("push", err).github, "{err} is GitHub's");
        }
        for err in [
            "HTTP 422: Validation Failed",
            "HTTP 404: Not Found",
            "HTTP 403: Resource not accessible by integration",
            "HTTP 401: Bad credentials",
            "HTTP 502: Bad Gateway\nHTTP 422: Validation Failed",
            "The requested URL returned error: 403",
            "! [rejected] HEAD -> feature/issue-7 (non-fast-forward)",
            "",
        ] {
            assert!(!rec("pr", err).github, "{err} is the request's own");
        }
    }

    #[test]
    fn the_why_holds_only_the_words_of_the_list() {
        let r = rec(
            "push",
            "token ghp_secret in a line\nHTTP 502: Bad Gateway\nHTTP 502 again",
        );
        assert_eq!(r.why, "HTTP 502");
        assert_eq!(r.failure(), "its push failed on GitHub's side (HTTP 502)");
        assert_eq!(rec("pr", "nothing").why, "");
    }

    #[test]
    fn a_record_reads_back_as_it_was_written() {
        let mut r = seen("pr", "HTTP 429");
        r.tries = vec![1000, 1700];
        assert_eq!(EndFault::from_json(&r.to_json()), Some(r));
        assert_eq!(EndFault::from_json(&json!({"step": "deploy"})), None);
    }

    #[test]
    fn a_record_with_a_damaged_attempt_tries_or_said_does_not_read() {
        let good = seen("push", "HTTP 502").to_json();
        for (field, bad) in [
            ("attempt", json!(0)),
            ("attempt", json!("1")),
            ("attempt", Value::Null),
            ("tries", json!(["x"])),
            ("tries", json!([1000, {"at": 1600}])),
            ("tries", json!("1000")),
            ("tries", Value::Null),
            ("said", json!("false")),
            ("said", Value::Null),
        ] {
            let mut v = good.clone();
            v[field] = bad.clone();
            assert_eq!(EndFault::from_json(&v), None, "{field}: {bad}");
            v.as_object_mut().unwrap().remove(field);
            assert_eq!(EndFault::from_json(&v), None, "{field} missing");
        }
    }

    #[test]
    fn tries_are_three_at_most_ten_minutes_apart_within_the_hour() {
        let mut r = seen("push", "HTTP 502");
        assert_eq!(r.due(1001), Due::Now);
        r.tries.push(1001);
        assert_eq!(r.due(1300), Due::At(1601));
        assert_eq!(r.due(1601), Due::Now);
        r.tries.push(1601);
        r.tries.push(2201);
        assert_eq!(r.due(9000), Due::Never("3 tries failed".into()));
        let mut late = seen("push", "HTTP 502");
        late.tries.push(4001);
        assert_eq!(
            late.due(4601),
            Due::Never("the hour after the failure is over".into())
        );
        assert!(matches!(seen("push", "HTTP 502").due(4601), Due::Never(_)));
    }

    #[test]
    fn a_failure_only_the_cycles_result_names_is_never_tried() {
        let r = rec("push", "HTTP 502");
        assert!(r.github);
        assert_eq!(
            r.due(1001),
            Due::Never(
                "only the cycle's own result says so, so only tehas:finish finishes it".into()
            )
        );
        assert!(matches!(rec("pr", "HTTP 503").due(1001), Due::Never(_)));
        // A record written before the field reads as one the host did not see.
        let mut v = seen("push", "HTTP 502").to_json();
        v.as_object_mut().unwrap().remove("host");
        assert!(!EndFault::from_json(&v).unwrap().host);
    }

    #[test]
    fn nothing_but_githubs_push_or_pull_request_is_tried() {
        assert!(matches!(rec("pr", "HTTP 422").due(1001), Due::Never(_)));
        assert!(matches!(
            rec("handoff", "HTTP 502").due(1001),
            Due::Never(_)
        ));
        let mut r = rec("pr", "HTTP 502");
        r.stopped = "its finish was refused".into();
        assert_eq!(r.due(1001), Due::Never("its finish was refused".into()));
    }

    #[test]
    fn the_file_is_written_listed_and_removed() {
        let dir = crate::testdir::TestDir::new("end-fault");
        let home = dir.path();
        let r = rec("push", "HTTP 502");
        assert!(write(home, &r));
        assert_eq!(read(home, 7), Some(r.clone()));
        assert_eq!(all(home), vec![r]);
        assert!(remove(home, 7));
        assert!(remove(home, 7));
        assert_eq!(read(home, 7), None);
    }
}

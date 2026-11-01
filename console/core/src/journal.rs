//! One write to GitHub through the action journal of the state store (section
//! 5 of its spec): intended, attempted, then settled with its typed result. A
//! write the journal could not record is not made. A poll and a cycle write
//! through this one path. Each brings its own store call: a poll's next read
//! decides a lost write again, and nothing decides a cycle's report again, so
//! a cycle asks a busy store again.

use crate::call::{gh_why, run_full};
use serde_json::{json, Value};
use std::time::Duration;

/// How a journaled call ended.
#[derive(Debug, PartialEq)]
pub enum Wrote {
    /// What gh printed: a comment posted names its id there.
    Done(String),
    /// What gh gave as its reason, as `gh_why` has it.
    Failed(String),
    /// Not made: the journal could not record it.
    Unrecorded,
}

/// The store's answer to one call, or its failure.
pub type Store<'a> = &'a mut dyn FnMut(&[&str]) -> Result<Value, Value>;

/// What one write goes by.
pub struct Journal<'a> {
    pub repo: &'a str,
    pub gh: &'a str,
    /// The read the action rests on.
    pub before: i64,
    /// A write's bound, and the grace of TERM after it.
    pub bound: Duration,
    pub grace: Duration,
    /// The token the poll took, so the shim asks for none.
    pub token: Option<&'a str>,
    /// The row every action names, from poll-begin; 0 until the first write
    /// begins it.
    pub poll_at: &'a mut i64,
    /// The store was asked for the row and gave no time for it.
    pub row_not_begun: &'a mut bool,
    /// The cycle whose action this is, its issue and attempt: a poll's settle
    /// leaves the action of a cycle whose lock is held.
    pub cycle: Option<(u64, u64)>,
}

impl Journal<'_> {
    /// A write to GitHub, journaled: recorded before it is made, settled
    /// after. `input` goes to gh on standard input. `done_if`: words in a
    /// failed call's answer that mean the change is there already, settled
    /// empty.
    pub fn write(
        &mut self,
        store: Store,
        issue: u64,
        kind: &str,
        args: Vec<String>,
        input: Option<&str>,
        done_if: Option<&str>,
    ) -> Wrote {
        // The body by its sum and length: the settling poll finds a comment
        // by them, and the store's arguments stay short and hold no text.
        // Given as JSON (--input -), the body is its field: the text GitHub
        // keeps.
        let body = input;
        let json = args.iter().any(|a| a == "--input");
        let field = input
            .filter(|_| json)
            .and_then(|b| serde_json::from_str::<Value>(b).ok())
            .and_then(|v| v.get("body").and_then(Value::as_str).map(str::to_string));
        let input = field.as_deref().or(input).map(|b| {
            use sha2::{Digest, Sha256};
            let sum: String = Sha256::digest(b.as_bytes())
                .iter()
                .map(|x| format!("{x:02x}"))
                .collect();
            json!({"sha256": sum, "bytes": b.len()})
        });
        let mut call = json!({"repo": self.repo, "gh": args, "input": input});
        if let Some((n, attempt)) = self.cycle {
            call["cycle"] = json!({"issue": n, "attempt": attempt});
        }
        // The first write asks for the row. A store that cannot begin it
        // records, and so makes, no write: an answer with no time of its own
        // is refused the same way, since 0 is no row. Every write asks again
        // while no row stands.
        if *self.poll_at == 0 {
            match store(&["poll-begin"]) {
                Ok(v) => match v.get("at").and_then(Value::as_i64) {
                    Some(at) if at > 0 => *self.poll_at = at,
                    _ => {
                        *self.row_not_begun = true;
                        return Wrote::Unrecorded;
                    }
                },
                Err(_) => return Wrote::Unrecorded,
            }
        }
        let recorded = store(&[
            "journal-record",
            "--poll",
            &self.poll_at.to_string(),
            "--kind",
            kind,
            "--issue",
            &issue.to_string(),
            "--args",
            &call.to_string(),
            "--before",
            &self.before.to_string(),
        ]);
        let Some(id) = recorded
            .ok()
            .and_then(|v| v.get("action").and_then(Value::as_i64))
        else {
            return Wrote::Unrecorded;
        };
        let id = id.to_string();
        if store(&["journal-attempt", "--action", &id]).is_err() {
            return Wrote::Unrecorded;
        }
        let extra: Vec<(&str, &str)> = match self.token {
            Some(t) => vec![("TEHAS_GH_TOKEN", t)],
            None => Vec::new(),
        };
        let r = run_full(self.gh, &args, body, self.bound, self.grace, &extra);
        let there = done_if.is_some_and(|w| r.out.contains(w) || r.err.contains(w));
        let result = match r.status {
            Some(0) => "ok",
            None => "timed-out",
            Some(_) if there => "empty",
            Some(_) => "read-failed",
        };
        // A settle that fails leaves the action attempted: the next poll
        // settles it from a fresh read.
        let _ = store(&["journal-settle", "--action", &id, "--result", result]);
        if r.ok() || (there && r.status.is_some()) {
            Wrote::Done(r.out)
        } else {
            Wrote::Failed(gh_why(&r.err))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::call::GRACE;

    fn journal<'a>(
        poll_at: &'a mut i64,
        begun: &'a mut bool,
        gh: &'a str,
        cycle: Option<(u64, u64)>,
    ) -> Journal<'a> {
        Journal {
            repo: "a/b",
            gh,
            before: 5,
            bound: Duration::from_secs(5),
            grace: GRACE,
            token: None,
            poll_at,
            row_not_begun: begun,
            cycle,
        }
    }

    /// A store that takes everything and keeps what it was asked.
    fn taken(asked: &mut Vec<Vec<String>>, a: &[&str]) -> Result<Value, Value> {
        asked.push(a.iter().map(|s| s.to_string()).collect());
        Ok(match a[0] {
            "poll-begin" => json!({"at": 100}),
            "journal-record" => json!({"action": 7}),
            _ => json!({}),
        })
    }

    #[test]
    fn a_write_goes_begin_record_attempt_call_settle() {
        let (mut at, mut no_row, mut asked) = (0, false, vec![]);
        let mut j = journal(&mut at, &mut no_row, "true", None);
        let w = j.write(
            &mut |a| taken(&mut asked, a),
            7,
            "label-add",
            vec!["issue".into()],
            None,
            None,
        );
        assert_eq!(w, Wrote::Done(String::new()));
        let verbs: Vec<&str> = asked.iter().map(|a| a[0].as_str()).collect();
        assert_eq!(
            verbs,
            [
                "poll-begin",
                "journal-record",
                "journal-attempt",
                "journal-settle"
            ]
        );
        assert_eq!(at, 100);
    }

    #[test]
    fn a_cycle_names_itself_in_the_action() {
        let (mut at, mut no_row, mut asked) = (9, false, vec![]);
        let mut j = journal(&mut at, &mut no_row, "true", Some((7, 2)));
        j.write(
            &mut |a| taken(&mut asked, a),
            7,
            "comment-post",
            vec!["--body-file".into(), "-".into()],
            Some("text"),
            None,
        );
        let args: Value = serde_json::from_str(&asked[0][8]).unwrap();
        assert_eq!(args["cycle"], json!({"issue": 7, "attempt": 2}));
        assert_eq!(args["input"]["bytes"], json!(4));
        assert_eq!(args["gh"], json!(["--body-file", "-"]));
    }

    #[test]
    fn a_write_the_journal_cannot_record_is_not_made() {
        let (mut at, mut no_row) = (9, false);
        let mut j = journal(&mut at, &mut no_row, "/nonexistent/gh", None);
        let w = j.write(
            &mut |_| Err(json!({"failed": true})),
            7,
            "label-add",
            vec![],
            None,
            None,
        );
        assert_eq!(w, Wrote::Unrecorded);
    }

    #[test]
    fn a_body_given_as_json_is_summed_by_its_field() {
        let (mut at, mut no_row, mut asked) = (9, false, vec![]);
        let mut j = journal(&mut at, &mut no_row, "true", None);
        j.write(
            &mut |a| taken(&mut asked, a),
            7,
            "issue-edit",
            vec!["--input".into(), "-".into()],
            Some(r#"{"body":"text"}"#),
            None,
        );
        let args: Value = serde_json::from_str(&asked[0][8]).unwrap();
        assert_eq!(args["input"]["bytes"], json!(4));
    }
}

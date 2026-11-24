//! One write to GitHub through the action journal of the state store (section
//! 5 of its spec): intended, attempted, then settled with its typed result. A
//! write the journal could not record is not made. A poll and a cycle write
//! through this one path. Each brings its own store call: a poll's next read
//! decides a lost write again, and nothing decides a cycle's report again, so
//! a cycle asks a busy store again, and then makes the write with no row.

pub mod settle;

use crate::call::{gh_why, run_full, Ran};
use crate::read_only;
use regex::Regex;
use serde_json::{json, Value};
use std::sync::OnceLock;
use std::time::Duration;
use tehas_ports::{
    Blobs, Blueprints, CodeHost, Got, Outcome, Passing, Receipt, Refusal, Sent, Sha256, Tickets,
    Write, Writes,
};

/// How a journaled call ended.
#[derive(Debug, PartialEq)]
pub enum Wrote {
    /// What gh printed: a comment posted names its id there.
    Done(String),
    /// What gh gave as its reason, as `gh_why` has it.
    Failed(String),
    /// GitHub may have taken it: the call timed out or GitHub answered 5xx.
    /// Never sent again on this answer: a read settles it first (#1754). The
    /// reason as `Failed` has it.
    Unknown(String),
    /// Not made: the journal could not record it.
    Unrecorded,
}

/// How a write of the ports ended (`Journal::send`).
#[derive(Debug, PartialEq)]
pub enum Journaled {
    /// The site's answer, and the row settled by it.
    Answered(Got<Receipt>),
    /// The site may have taken it: `Unavailable { sent: Maybe }` or an
    /// answer that could not be read. The row is left unknown, never sent
    /// again on it: the caller's read settles it with [`Journal::settle`].
    Unknown { answer: Outcome, row: RowId },
    /// Not sent: the journal could not record it.
    Unrecorded,
}

/// A row `Journal::send` left unknown. Only `send` makes one, and
/// [`Journal::settle`] takes it, so a row the site's answer settled is never
/// settled again, and an unknown row is settled once.
#[derive(Debug, PartialEq, Eq)]
pub struct RowId(String);

/// What the caller's read found of a write left unknown, by the settle rule
/// of its kind (the ports spec, "How a row left unknown is settled").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Found {
    /// The write is there: the row is done.
    Landed,
    /// It is not: the row is closed as not sent, and the caller decides
    /// afresh from the same read. Nothing is replayed from the row.
    NotSent,
}

/// The ports a journal's writes go through, and the reads that settle a
/// write the site may have taken ([`settle::read`]). The program's wiring
/// makes them.
pub struct Handles {
    pub writes: Box<dyn Writes>,
    pub tickets: Box<dyn Tickets>,
    /// Where a ticket is its own blueprint, its notes are the blueprint's
    /// comments.
    pub blueprints: Box<dyn Blueprints>,
    pub code_host: Box<dyn CodeHost>,
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
        let body = input;
        let mut call = json!({"repo": self.repo, "gh": args, "input": summed(&args, input)});
        if let Some((n, attempt)) = self.cycle {
            call["cycle"] = json!({"issue": n, "attempt": attempt});
        }
        let issue = issue.to_string();
        let what = ["--kind", kind, "--issue", &issue];
        let Some(id) = self.intend(store, &call.to_string(), &what) else {
            return Wrote::Unrecorded;
        };
        let r = self.gh(&args, body);
        let there = there(&r, done_if);
        let result = match r.status {
            Some(0) => "ok",
            Some(_) if there => "empty",
            _ if self.maybe_taken(&r) => "unknown",
            None => "timed-out",
            Some(_) => "read-failed",
        };
        // A settle that fails leaves the action attempted, and an unknown one
        // open: the next poll settles it from a fresh read.
        let _ = store(&["journal-settle", "--action", &id, "--result", result]);
        self.wrote(r, there)
    }

    /// A write of the ports, journaled as `write` journals a call of gh: its
    /// row keeps the write with its bodies summed, and `writes` is sent the
    /// write as given. A read-only writer is sent nothing and no row is
    /// recorded.
    ///
    /// The row settles by the answer: `ok`; `unknown` for one the site may
    /// have taken, `Unavailable { sent: Maybe }` (a time-out too) or an
    /// answer that could not be read (`Unread`), which a read settles next;
    /// `timed-out` for a time-out that sent nothing; `read-failed`, the 1.x
    /// word for a failed call, for every other error (`Refused`,
    /// `Conflict`, `NotFound`, `Spent`, ...).
    pub fn send(
        &mut self,
        store: Store,
        write: &Write,
        writes: &mut dyn Writes,
        files: &dyn Blobs,
    ) -> Journaled {
        if writes.read_only() {
            read_only::tell(&format!("send {:?}", write.kind()));
            return Journaled::Answered(Err(Outcome::Refused(Refusal::ReadOnly)));
        }
        let row = match write.summed(|text| Sha256::new(sha256(text)).ok()) {
            Ok(row) => row,
            Err(o) => return Journaled::Answered(Err(o)),
        };
        let Ok(row) = serde_json::to_string(&row) else {
            return Journaled::Unrecorded;
        };
        let mut call = json!({"repo": self.repo});
        if let Some((n, attempt)) = self.cycle {
            call["cycle"] = json!({"issue": n, "attempt": attempt});
        }
        let Some(id) = self.intend(store, &call.to_string(), &["--write", &row]) else {
            return Journaled::Unrecorded;
        };
        let answer = writes.send(write, files);
        let result = match &answer {
            Ok(_) => "ok",
            // Before the time-out: one that may have been sent is unknown.
            Err(Outcome::Unavailable {
                sent: Sent::Maybe, ..
            }) => "unknown",
            Err(Outcome::Unavailable {
                why: Passing::Timeout,
                ..
            }) => "timed-out",
            Err(Outcome::Unread(_)) => "unknown",
            Err(
                Outcome::Unavailable { .. }
                | Outcome::NotFound
                | Outcome::Conflict(_)
                | Outcome::Refused(_)
                | Outcome::Forbidden
                | Outcome::SignedOut
                | Outcome::Spent { .. },
            ) => "read-failed",
        };
        let _ = store(&["journal-settle", "--action", &id, "--result", result]);
        match answer {
            Err(answer) if result == "unknown" => Journaled::Unknown {
                answer,
                row: RowId(id),
            },
            answer => Journaled::Answered(answer),
        }
    }

    /// Settles a row `send` left unknown from the caller's read. A read that
    /// failed settles nothing: the row stays unknown for the next read, and
    /// the poll's settle leaves a row of the ports open until it reads them.
    /// False when the store did not take it: the row stays as it was.
    pub fn settle(store: Store, row: RowId, found: Found) -> bool {
        let RowId(id) = row;
        let end: &[&str] = match found {
            Found::Landed => &["--result", "ok"],
            Found::NotSent => &["--abandoned"],
        };
        let mut asked = vec!["journal-settle", "--action", &id];
        asked.extend_from_slice(end);
        store(&asked).is_ok()
    }

    /// Records one action as intended, then attempted, and answers its id.
    /// `what` names the write to the store, `call` is its --args. None: not
    /// recorded, so not made.
    fn intend(&mut self, store: Store, call: &str, what: &[&str]) -> Option<String> {
        // The first write asks for the row. A store that cannot begin it
        // records, and so makes, no write: an answer with no time of its own
        // is refused the same way, since 0 is no row. Every write asks again
        // while no row stands.
        if *self.poll_at == 0 {
            let v = store(&["poll-begin"]).ok()?;
            match v.get("at").and_then(Value::as_i64) {
                Some(at) if at > 0 => *self.poll_at = at,
                _ => {
                    *self.row_not_begun = true;
                    return None;
                }
            }
        }
        let (poll, before) = (self.poll_at.to_string(), self.before.to_string());
        let mut asked = vec!["journal-record", "--poll", &poll];
        asked.extend_from_slice(what);
        asked.extend(["--args", call, "--before", &before]);
        let id = store(&asked)
            .ok()?
            .get("action")
            .and_then(Value::as_i64)?
            .to_string();
        store(&["journal-attempt", "--action", &id]).ok()?;
        Some(id)
    }

    /// Whether GitHub may have taken a call that did not answer as done: it
    /// timed out after gh started, GitHub answered 5xx, which it does for a
    /// write it went on to commit, or the connection broke after it was made.
    /// A gh that never ran, or never connected (`connection refused`, `no such
    /// host`), sent nothing.
    fn maybe_taken(&self, r: &Ran) -> bool {
        static TAKEN: OnceLock<Regex> = OnceLock::new();
        let taken = TAKEN.get_or_init(|| {
            Regex::new(
                r"(?i)HTTP 5[0-9]{2}\b|connection reset|\bEOF\b|GOAWAY|connection closed|closed network connection|server closed",
            )
            .expect("a regex")
        });
        match r.status {
            None => r.err != format!("{} could not be run", self.gh),
            Some(0) => false,
            Some(_) => taken.is_match(&r.err),
        }
    }

    fn wrote(&self, r: Ran, there: bool) -> Wrote {
        if r.ok() || (there && r.status.is_some()) {
            Wrote::Done(r.out)
        } else if self.maybe_taken(&r) {
            Wrote::Unknown(gh_why(&r.err))
        } else {
            Wrote::Failed(gh_why(&r.err))
        }
    }

    /// A write the journal could not record, made with no row. For a writer
    /// that nothing decides again: a row it left open is safe, as a settling
    /// poll replays nothing.
    pub fn unrecorded(
        &self,
        args: Vec<String>,
        input: Option<&str>,
        done_if: Option<&str>,
    ) -> Wrote {
        let r = self.gh(&args, input);
        let there = there(&r, done_if);
        self.wrote(r, there)
    }

    fn gh(&self, args: &[String], input: Option<&str>) -> Ran {
        let extra: Vec<(&str, &str)> = match self.token {
            Some(t) => vec![("TEHAS_GH_TOKEN", t)],
            None => Vec::new(),
        };
        run_full(self.gh, args, input, self.bound, self.grace, &extra)
    }
}

fn there(r: &Ran, done_if: Option<&str>) -> bool {
    done_if.is_some_and(|w| r.out.contains(w) || r.err.contains(w))
}

/// The body of a write by its sum and length, as an action keeps it: the
/// settling poll finds a comment by them, and the store's arguments stay short
/// and hold no text. Given as JSON (--input -), the body is its field: the text
/// GitHub keeps. A caller that looks for an earlier action of the same write
/// compares this.
pub fn summed(args: &[String], input: Option<&str>) -> Option<Value> {
    let json = args.iter().any(|a| a == "--input");
    let field = input
        .filter(|_| json)
        .and_then(|b| serde_json::from_str::<Value>(b).ok())
        .and_then(|v| v.get("body").and_then(Value::as_str).map(str::to_string));
    field
        .as_deref()
        .or(input)
        .map(|b| json!({"sha256": sha256(b), "bytes": b.len()}))
}

/// The sha-256 of a text, in lower-case hex.
fn sha256(text: &str) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(text.as_bytes())
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::call::GRACE;
    use tehas_ports::{Body, NoBlobs, Passing, Sent, Sha256, Sum, TicketKey, Unread};

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

    fn sh(script: &str) -> Vec<String> {
        vec!["-c".into(), script.into()]
    }

    #[test]
    fn a_write_with_no_row_says_as_a_journaled_one_does() {
        let (mut at, mut no_row) = (0, false);
        let j = journal(&mut at, &mut no_row, "sh", None);
        assert_eq!(
            j.unrecorded(sh("cat"), Some("text"), None),
            Wrote::Done("text".into())
        );
        assert_eq!(
            j.unrecorded(sh("echo Not Found >&2; exit 1"), None, Some("Not Found")),
            Wrote::Done(String::new())
        );
        assert!(matches!(
            j.unrecorded(sh("exit 1"), None, Some("Not Found")),
            Wrote::Failed(_)
        ));
    }

    /// The settle each answer of gh is given, and what the caller hears.
    fn settled(gh: &str, args: Vec<String>, bound: Duration) -> (Wrote, String) {
        let (mut at, mut no_row, mut asked) = (9, false, vec![]);
        let mut j = journal(&mut at, &mut no_row, gh, None);
        j.bound = bound;
        j.grace = Duration::from_millis(100);
        let w = j.write(
            &mut |a| taken(&mut asked, a),
            7,
            "comment-post",
            args,
            Some("text"),
            None,
        );
        let settle = asked.iter().find(|a| a[0] == "journal-settle").unwrap();
        (w, settle[4].clone())
    }

    /// #1754: a write GitHub may have taken is unknown, never failed: the
    /// caller does not send it again, and the row stays open for a read.
    #[test]
    fn a_write_that_timed_out_or_got_a_5xx_is_unknown() {
        let long = Duration::from_secs(5);
        let (w, result) = settled("sh", sh("sleep 5"), Duration::from_millis(300));
        assert!(matches!(w, Wrote::Unknown(_)), "{w:?}");
        assert_eq!(result, "unknown");
        for code in ["500", "502", "503", "504"] {
            let script = format!("cat >/dev/null; echo 'HTTP {code}: Bad Gateway' >&2; exit 1");
            let (w, result) = settled("sh", sh(&script), long);
            assert_eq!(w, Wrote::Unknown(format!(": HTTP {code}")));
            assert_eq!(result, "unknown");
        }
        let (w, result) = settled("sh", sh("echo 'HTTP 422: Unprocessable' >&2; exit 1"), long);
        assert_eq!(w, Wrote::Failed(": HTTP 422".into()));
        assert_eq!(result, "read-failed");
        // A gh that never ran sent nothing.
        let (w, result) = settled("/nonexistent/gh", vec![], long);
        assert!(matches!(w, Wrote::Failed(_)), "{w:?}");
        assert_eq!(result, "timed-out");
    }

    /// A connection that broke after the request went out may have carried
    /// it; one that was never made carried nothing.
    #[test]
    fn a_write_whose_connection_broke_is_unknown() {
        let long = Duration::from_secs(5);
        for said in [
            "read tcp: read: connection reset by peer",
            "Post \"https://api.github.com/graphql\": unexpected EOF",
            "Post \"https://api.github.com/graphql\": EOF",
            "http2: server sent GOAWAY and closed the connection",
            "http2: client connection closed",
            "write tcp: use of closed network connection",
            "http: server closed idle connection",
        ] {
            let script = format!("cat >/dev/null; echo '{said}' >&2; exit 1");
            let (w, result) = settled("sh", sh(&script), long);
            assert!(matches!(w, Wrote::Unknown(_)), "{said}: {w:?}");
            assert_eq!(result, "unknown", "{said}");
        }
        for said in [
            "dial tcp: connect: connection refused",
            "dial tcp: lookup api.github.com: no such host",
            "GraphQL: Could not add label: issue is closed",
        ] {
            let script = format!("cat >/dev/null; echo '{said}' >&2; exit 1");
            let (w, result) = settled("sh", sh(&script), long);
            assert!(matches!(w, Wrote::Failed(_)), "{said}: {w:?}");
            assert_eq!(result, "read-failed", "{said}");
        }
    }

    #[test]
    fn a_write_with_no_row_that_got_a_5xx_is_unknown_too() {
        let (mut at, mut no_row) = (0, false);
        let j = journal(&mut at, &mut no_row, "sh", None);
        assert_eq!(
            j.unrecorded(sh("echo 'HTTP 502: Bad Gateway' >&2; exit 1"), None, None),
            Wrote::Unknown(": HTTP 502".into())
        );
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

    /// A `Writes` that answers from a script and keeps what it was sent.
    struct Script {
        answers: Vec<Got<Receipt>>,
        sent: Vec<Write>,
        read_only: bool,
    }

    impl Writes for Script {
        fn send(&mut self, w: &Write, _: &dyn Blobs) -> Got<Receipt> {
            self.sent.push(w.clone());
            self.answers.remove(0)
        }
        fn read_only(&self) -> bool {
            self.read_only
        }
    }

    fn script(answers: Vec<Got<Receipt>>) -> Script {
        Script {
            answers,
            sent: vec![],
            read_only: false,
        }
    }

    fn note() -> Write {
        Write::TicketNote {
            ticket: TicketKey::Number(std::num::NonZeroU64::new(7).unwrap()),
            body: Body::Text("text".into()),
            markers: vec![],
        }
    }

    /// The flag's value in a store call.
    fn flag<'a>(call: &'a [String], name: &str) -> &'a str {
        let i = call.iter().position(|a| a == name).unwrap();
        &call[i + 1]
    }

    /// T4.4: a write of the ports goes the journal's way: begin, record,
    /// attempt, send, settle. The site is sent the text, the row keeps its
    /// sum.
    #[test]
    fn a_sent_write_goes_begin_record_attempt_send_settle() {
        let (mut at, mut no_row, mut asked) = (0, false, vec![]);
        let mut j = journal(&mut at, &mut no_row, "true", Some((7, 2)));
        let mut w = script(vec![Ok(Receipt::Done)]);
        let got = j.send(&mut |a| taken(&mut asked, a), &note(), &mut w, &NoBlobs);
        assert_eq!(got, Journaled::Answered(Ok(Receipt::Done)));
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
        assert_eq!(w.sent, [note()]);
        let record = &asked[1];
        let args: Value = serde_json::from_str(flag(record, "--args")).unwrap();
        assert_eq!(
            args,
            json!({"repo": "a/b", "cycle": {"issue": 7, "attempt": 2}})
        );
        let row: Write = serde_json::from_str(flag(record, "--write")).unwrap();
        let sum = Sum {
            sha256: Sha256::new("982d9e3eb996f559e633f4d194def3761d909f5a3b647d1a851fead67c32c9d1")
                .unwrap(),
            len: 4,
        };
        assert!(matches!(row, Write::TicketNote { body: Body::Summed(s), .. } if s == sum));
        assert!(!record.iter().any(|a| a == "--kind" || a == "--issue"));
        assert_eq!(flag(record, "--poll"), "100");
        assert_eq!(flag(record, "--before"), "5");
        assert_eq!(flag(&asked[3], "--result"), "ok");
    }

    /// The settle each answer of the site is given, and the caller hears
    /// the answer itself. An answer that may have landed stays unknown,
    /// whether it timed out or could not be read.
    #[test]
    fn each_answer_settles_by_its_outcome() {
        let maybe = Outcome::Unavailable {
            why: Passing::Server,
            sent: Sent::Maybe,
        };
        let late = Outcome::Unavailable {
            why: Passing::Timeout,
            sent: Sent::No,
        };
        let late_maybe = Outcome::Unavailable {
            why: Passing::Timeout,
            sent: Sent::Maybe,
        };
        let down = Outcome::Unavailable {
            why: Passing::Unreachable,
            sent: Sent::No,
        };
        for (answer, result) in [
            (Ok(Receipt::Done), "ok"),
            (Err(maybe), "unknown"),
            (Err(late_maybe), "unknown"),
            (Err(Outcome::Unread(Unread::Malformed)), "unknown"),
            (Err(Outcome::Unread(Unread::Inconsistent)), "unknown"),
            (Err(late), "timed-out"),
            (Err(down), "read-failed"),
            (Err(Outcome::NotFound), "read-failed"),
        ] {
            let (mut at, mut no_row, mut asked) = (9, false, vec![]);
            let mut j = journal(&mut at, &mut no_row, "true", None);
            let mut w = script(vec![answer.clone()]);
            let got = j.send(&mut |a| taken(&mut asked, a), &note(), &mut w, &NoBlobs);
            let settle = asked.iter().find(|a| a[0] == "journal-settle").unwrap();
            assert_eq!(flag(settle, "--result"), result);
            match (answer, got) {
                (Err(answer), Journaled::Unknown { answer: got, row }) => {
                    assert_eq!(result, "unknown");
                    assert_eq!((got, row), (answer, RowId("7".into())));
                }
                (answer, got) => {
                    assert_ne!(result, "unknown");
                    assert_eq!(got, Journaled::Answered(answer));
                }
            }
        }
    }

    /// The caller's read settles the row the site's answer left unknown:
    /// done when the write is there, abandoned when it is not, and the
    /// row's own id is named.
    #[test]
    fn a_row_left_unknown_is_settled_by_the_callers_read() {
        let maybe = Outcome::Unavailable {
            why: Passing::Server,
            sent: Sent::Maybe,
        };
        for (found, end) in [
            (Found::Landed, vec!["--result", "ok"]),
            (Found::NotSent, vec!["--abandoned"]),
        ] {
            let (mut at, mut no_row, mut asked) = (9, false, vec![]);
            let mut j = journal(&mut at, &mut no_row, "true", Some((7, 1)));
            let mut w = script(vec![Err(maybe.clone())]);
            let got = j.send(&mut |a| taken(&mut asked, a), &note(), &mut w, &NoBlobs);
            let Journaled::Unknown { row, .. } = got else {
                panic!("{got:?}");
            };
            assert!(Journal::settle(&mut |a| taken(&mut asked, a), row, found));
            let last = asked.last().unwrap();
            let mut want = vec!["journal-settle", "--action", "7"];
            want.extend(end);
            assert_eq!(last, &want);
            // The send's own settle left it unknown first.
            let settles: Vec<_> = asked.iter().filter(|a| a[0] == "journal-settle").collect();
            assert_eq!(settles.len(), 2);
            assert_eq!(flag(settles[0], "--result"), "unknown");
        }
    }

    /// A row the site's answer settled gives the caller no id, so nothing
    /// settles it again; a store that refuses a settle is the caller's to
    /// hear.
    #[test]
    fn only_a_row_left_unknown_can_be_settled_again() {
        for answer in [Ok(Receipt::Done), Err(Outcome::Refused(Refusal::Invalid))] {
            let (mut at, mut no_row, mut asked) = (9, false, vec![]);
            let mut j = journal(&mut at, &mut no_row, "true", None);
            let mut w = script(vec![answer.clone()]);
            let got = j.send(&mut |a| taken(&mut asked, a), &note(), &mut w, &NoBlobs);
            assert_eq!(got, Journaled::Answered(answer));
        }
        let refused = Journal::settle(
            &mut |_| Err(json!({"refused": "is settled already (done)"})),
            RowId("4".into()),
            Found::Landed,
        );
        assert!(!refused);
    }

    #[test]
    fn a_write_the_journal_cannot_record_is_not_sent() {
        let (mut at, mut no_row) = (9, false);
        let mut j = journal(&mut at, &mut no_row, "true", None);
        let mut w = script(vec![]);
        let got = j.send(
            &mut |_| Err(json!({"failed": true})),
            &note(),
            &mut w,
            &NoBlobs,
        );
        assert_eq!(got, Journaled::Unrecorded);
        assert!(w.sent.is_empty());
    }

    /// A read-only host records no row and sends nothing, and keeps the
    /// write it did not make in its log, as `read_only::refuse` does.
    #[test]
    fn a_read_only_writer_is_refused_with_no_row() {
        let home = std::env::temp_dir().join(format!("tehas-send-ro-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        read_only::TEST_HOME.with(|t| *t.borrow_mut() = Some(home.clone()));
        let (mut at, mut no_row, mut asked) = (0, false, vec![]);
        let mut j = journal(&mut at, &mut no_row, "true", None);
        let mut w = script(vec![]);
        w.read_only = true;
        let got = j.send(&mut |a| taken(&mut asked, a), &note(), &mut w, &NoBlobs);
        assert_eq!(
            got,
            Journaled::Answered(Err(Outcome::Refused(Refusal::ReadOnly)))
        );
        assert!(asked.is_empty());
        assert!(w.sent.is_empty());
        read_only::TEST_HOME.with(|t| *t.borrow_mut() = None);
        let log = std::fs::read_to_string(read_only::log_path(&home)).unwrap();
        assert!(
            log.ends_with("\tread-only: would send TicketNote\n"),
            "{log}"
        );
        let _ = std::fs::remove_dir_all(&home);
    }
}

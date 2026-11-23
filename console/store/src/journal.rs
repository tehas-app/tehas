//! The action journal of section 5 of the state store's spec: what a poll
//! meant to write on GitHub or the host, written before the call and settled
//! after it, so a poll that died between two writes can be read afterwards.
//! The dispatcher in Rust is the one caller (#254).
//!
//!   store poll-begin
//!     The poll's row in polls, dated now: what an action names as the poll
//!     that intended it. A second poll in the same second shares the row.
//!   store journal-record --poll <at> --kind <kind> --issue <n> --args <json>
//!                        --before <at>
//!     One action, intended: the whole call in --args (a JSON object), the
//!     read it rests on in --before. {"action":N}.
//!   store journal-record --poll <at> --write <json> --args <json> --before <at>
//!     The same for a `Write` of the ports, with its bodies summed: kept as
//!     the `write` field of --args, its kind the write's word and its issue
//!     the ticket, blueprint or change it names, 0 for none.
//!   store journal-attempt --action N
//!     The step just before the call: intended becomes attempted.
//!   store journal-settle --action N (--result <result> | --abandoned)
//!                        [--evidence <json>]
//!     After the call: ok or empty is done, read-failed or timed-out is
//!     failed. unknown is a call GitHub may have taken (a timeout, a 5xx):
//!     the row stays open as unknown, with no settled time, until a read
//!     settles it (#1754). --abandoned is the settling poll's word for an
//!     action whose fresh read says it did not happen and need not.
//!   store journal-open
//!     The actions intended, attempted or unknown, oldest first. A row
//!     whose stored args do not read as a JSON object carries
//!     `"args_unreadable": true` and no `args`, so a reader cannot take the
//!     absent arguments for empty ones.

use crate::verbs::{self, j, line, list, obj, refuse, Args, Fail, Kind, Q};
use rusqlite::{params, OptionalExtension};
use serde_json::Value;
use std::path::Path;
use tehas_ports::{TicketKey, Write, WriteKind};

/// The kinds of section 5's table, and the three a planning session's end
/// writes: an issue's body, a sub-issue link and a close. Then the words of
/// the writes of the ports that 1.x had none for (T4.4).
pub const KINDS: [&str; 37] = [
    "label-add",
    "label-remove",
    "label-remove-only",
    "comment-post",
    "comment-edit",
    "container-start",
    "unit-stop",
    "refresh-begin",
    "refresh-end",
    "issue-edit",
    "sub-issue-add",
    "issue-close",
    "ticket-reopen",
    "ticket-file",
    "ticket-kind",
    "ticket-priority",
    "ticket-wait",
    "ticket-unwait",
    "ticket-assign",
    "part-remove",
    "target-new",
    "target-edit",
    "ticket-target",
    "marks-prepare",
    "mark-ensure",
    "blueprint-open",
    "blueprint-close",
    "change-open",
    "change-comment",
    "change-close",
    "change-merge",
    "change-propose",
    "tag-make",
    "status-set",
    "release-new",
    "release-upload",
    "release-publish",
];

/// The word a write's row holds, the 1.x word where it had one. None for a
/// kind the journal does not record yet: a change's mark and edit take
/// theirs in B3, and an unknown write is never sent. Every kind is named,
/// so a new one is decided here.
pub fn word(kind: WriteKind) -> Option<&'static str> {
    use WriteKind as K;
    Some(match kind {
        K::Mark => "label-add",
        K::Unmark => "label-remove",
        K::Close => "issue-close",
        K::TicketNote | K::Comment => "comment-post",
        K::EditComment => "comment-edit",
        K::EditTicket | K::EditBlueprint => "issue-edit",
        K::AddPart => "sub-issue-add",
        K::Reopen => "ticket-reopen",
        K::FileTicket => "ticket-file",
        K::SetKind => "ticket-kind",
        K::SetPriority => "ticket-priority",
        K::WaitOn => "ticket-wait",
        K::StopWaiting => "ticket-unwait",
        K::Assign => "ticket-assign",
        K::RemovePart => "part-remove",
        K::NewTarget => "target-new",
        K::EditTarget => "target-edit",
        K::SetTarget => "ticket-target",
        K::Prepare => "marks-prepare",
        K::EnsureMark => "mark-ensure",
        K::OpenBlueprint => "blueprint-open",
        K::CloseBlueprint => "blueprint-close",
        K::OpenChange => "change-open",
        K::CommentChange => "change-comment",
        K::CloseChange => "change-close",
        K::Merge => "change-merge",
        K::Propose => "change-propose",
        K::Tag => "tag-make",
        K::SetStatus => "status-set",
        K::NewRelease => "release-new",
        K::Upload => "release-upload",
        K::PublishRelease => "release-publish",
        K::MarkChange | K::UnmarkChange | K::EditChange | K::Unknown => return None,
        // The ports may add a kind (`non_exhaustive`): not recorded, so
        // not sent, until it is named above.
        _ => return None,
    })
}

/// The most the issue column holds, as `--issue` takes it: ten digits.
const ISSUE_MAX: i64 = 9_999_999_999;

/// The number of the ticket, blueprint or change a write is on, as the
/// issue column holds it and a settle reads it: above 0, at most ten
/// digits. None for a write on no number (a ticket keyed by text, a new
/// item, a mark, target, commit or release): the store refuses its row, so
/// it is not sent, until a settle reads a row by its write (T4.7). Every
/// variant is named, so a new one is decided here.
fn issue_of(w: &Write) -> Option<i64> {
    let n = match w {
        Write::Mark { ticket, .. }
        | Write::Unmark { ticket, .. }
        | Write::Close { ticket, .. }
        | Write::Reopen { ticket }
        | Write::TicketNote { ticket, .. }
        | Write::EditTicket { ticket, .. }
        | Write::SetKind { ticket, .. }
        | Write::SetPriority { ticket, .. }
        | Write::WaitOn { ticket, .. }
        | Write::StopWaiting { ticket, .. }
        | Write::Assign { ticket, .. }
        | Write::SetTarget { ticket, .. }
        | Write::AddPart { parent: ticket, .. }
        | Write::RemovePart { parent: ticket, .. } => match ticket {
            TicketKey::Number(n) => n.get(),
            TicketKey::Key(_) => return None,
        },
        Write::EditBlueprint { no, .. }
        | Write::Comment { on: no, .. }
        | Write::CloseBlueprint { no, .. } => no.get(),
        Write::EditChange { no, .. }
        | Write::MarkChange { no, .. }
        | Write::UnmarkChange { no, .. }
        | Write::CommentChange { no, .. }
        | Write::CloseChange { no }
        | Write::Merge { no, .. } => no.get(),
        Write::FileTicket(_)
        | Write::NewTarget { .. }
        | Write::EditTarget { .. }
        | Write::Prepare
        | Write::EnsureMark(_)
        | Write::OpenBlueprint(_)
        | Write::EditComment { .. }
        | Write::OpenChange(_)
        | Write::Propose(_)
        | Write::Tag { .. }
        | Write::SetStatus { .. }
        | Write::NewRelease(_)
        | Write::Upload { .. }
        | Write::PublishRelease { .. }
        | Write::Unknown => return None,
        // The ports may add a write (`non_exhaustive`): refused until it
        // is named above.
        _ => return None,
    };
    i64::try_from(n)
        .ok()
        .filter(|n| (1..=ISSUE_MAX).contains(n))
}

/// A write the journal records: one it has a word for, with every body
/// summed, so the row holds no text.
fn a_write(s: &str) -> Option<(Write, &'static str)> {
    let w: Write = serde_json::from_str(&object(s)?).ok()?;
    let kind = word(w.kind())?;
    w.summed(|_| None).ok()?;
    Some((w, kind))
}

/// The typed results of every external call (section 3), and `unknown`: a
/// call GitHub may have taken, which a read settles later.
const RESULTS: [&str; 5] = ["ok", "empty", "read-failed", "timed-out", "unknown"];

/// The most of --args or --evidence taken: a comment's body is 65,536
/// characters at most on GitHub, and four bytes a character at worst.
const JSON_BOUND: usize = 300_000;

#[derive(Debug, PartialEq, Eq)]
pub enum Call {
    Begin,
    Record(Record),
    Attempt(i64),
    Settle(Settle),
    Open,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Record {
    pub poll: i64,
    pub kind: &'static str,
    pub issue: i64,
    pub args: String,
    pub before: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Settle {
    pub action: i64,
    /// None for --abandoned.
    pub result: Option<&'static str>,
    pub evidence: Option<String>,
}

/// A JSON object, written back compact.
fn object(s: &str) -> Option<String> {
    if s.len() > JSON_BOUND {
        return None;
    }
    match serde_json::from_str::<Value>(s) {
        Ok(v @ Value::Object(_)) => Some(v.to_string()),
        _ => None,
    }
}

fn one_of(list: &'static [&'static str]) -> impl Fn(&str) -> Option<&'static str> {
    move |s| list.iter().find(|k| **k == s).copied()
}

pub fn parse(name: &str, rest: &[String]) -> Option<Result<Call, String>> {
    Some(match name {
        "poll-begin" => Args::parse("poll-begin", rest, &[], false).map(|_| Call::Begin),
        "journal-record" => Args::parse(
            "journal-record",
            rest,
            &[
                ("--poll", Kind::Value),
                ("--kind", Kind::Value),
                ("--issue", Kind::Value),
                ("--args", Kind::Value),
                ("--before", Kind::Value),
                ("--write", Kind::Value),
            ],
            false,
        )
        .and_then(|a| {
            if a.has("--write") {
                return record_write(&a);
            }
            Ok(Call::Record(Record {
                poll: a.strict_need("--poll", "a Unix time", verbs::unix)?,
                kind: a.strict_need(
                    "--kind",
                    &format!("one of {}", KINDS.join(", ")),
                    one_of(&KINDS),
                )?,
                issue: a.strict_need("--issue", "an issue number", verbs::number)?,
                args: a.strict_need("--args", "a JSON object", object)?,
                before: a.strict_need("--before", "a Unix time", verbs::unix)?,
            }))
        }),
        "journal-attempt" => {
            Args::parse("journal-attempt", rest, &[("--action", Kind::Value)], false).and_then(
                |a| {
                    Ok(Call::Attempt(a.strict_need(
                        "--action",
                        "an action number",
                        verbs::number,
                    )?))
                },
            )
        }
        "journal-settle" => Args::parse(
            "journal-settle",
            rest,
            &[
                ("--action", Kind::Value),
                ("--result", Kind::Value),
                ("--abandoned", Kind::Switch),
                ("--evidence", Kind::Value),
            ],
            false,
        )
        .and_then(|a| {
            let result = a.strict(
                "--result",
                &format!("one of {}", RESULTS.join(", ")),
                one_of(&RESULTS),
            )?;
            match (result.is_some(), a.has("--abandoned")) {
                (true, true) => return Err(a.usage("--abandoned", "is not taken with --result")),
                (false, false) => return Err(a.usage("--result", "or --abandoned is required")),
                _ => {}
            }
            Ok(Call::Settle(Settle {
                action: a.strict_need("--action", "an action number", verbs::number)?,
                result,
                evidence: a.strict("--evidence", "a JSON object", object)?,
            }))
        }),
        "journal-open" => Args::parse("journal-open", rest, &[], false).map(|_| Call::Open),
        _ => return None,
    })
}

/// `journal-record --write`: the write kept in its arguments.
fn record_write(a: &Args) -> Result<Call, String> {
    a.only(&["--poll", "--write", "--args", "--before"], "--write")?;
    let (write, kind) = a.strict_need("--write", "a write the journal records", a_write)?;
    let args = a.strict_need("--args", "a JSON object", object)?;
    let mut args: serde_json::Map<String, Value> =
        serde_json::from_str(&args).map_err(|_| a.usage("--args", "is not a JSON object"))?;
    if args.contains_key("write") {
        return Err(a.usage("--args", "holds a write already"));
    }
    let kept = serde_json::to_value(&write).map_err(|_| a.usage("--write", "cannot be kept"))?;
    args.insert("write".into(), kept);
    Ok(Call::Record(Record {
        poll: a.strict_need("--poll", "a Unix time", verbs::unix)?,
        kind,
        issue: issue_of(&write)
            .ok_or_else(|| a.usage("--write", "names no issue the journal keys a row by"))?,
        args: Value::Object(args).to_string(),
        before: a.strict_need("--before", "a Unix time", verbs::unix)?,
    }))
}

pub fn run(home: &Path, call: &Call, now: i64) -> Result<String, Fail> {
    match call {
        Call::Begin => begin(home, now),
        Call::Record(r) => record(home, r, now),
        Call::Attempt(id) => attempt(home, *id, now),
        Call::Settle(s) => settle(home, s, now),
        Call::Open => open(home),
    }
}

fn begin(home: &Path, now: i64) -> Result<String, Fail> {
    // The poll's one check of the whole file, before its other writes.
    verbs::write_checked(home, |c| {
        c.execute(
            "INSERT OR IGNORE INTO polls (at, lock_taken_at) VALUES (?1, ?1)",
            [now],
        )
        .q()
    })?;
    Ok(line("poll-begin", &[("at", j(now))]))
}

fn record(home: &Path, r: &Record, now: i64) -> Result<String, Fail> {
    let id = verbs::write(home, |c| {
        let known: bool = c
            .query_row("SELECT count(*) FROM polls WHERE at = ?1", [r.poll], |x| {
                x.get::<_, i64>(0)
            })
            .q()?
            > 0;
        if !known {
            return Err(refuse(
                "poll",
                r.poll,
                "is not on record; run poll-begin first",
            ));
        }
        c.execute(
            "INSERT INTO actions (poll_at, kind, issue, args_json, before_snapshot, state, intended_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'intended', ?6)",
            params![r.poll, r.kind, r.issue, r.args, r.before, now],
        )
        .q()?;
        let id = c.last_insert_rowid();
        c.execute(
            "INSERT INTO journal (action, at, step) VALUES (?1, ?2, 'intended')",
            params![id, now],
        )
        .q()?;
        Ok(id)
    })?;
    Ok(line("journal-record", &[("action", j(id))]))
}

/// The state of an action, or the refusal for one that is not on record.
fn state(c: &rusqlite::Connection, id: i64) -> Result<String, Fail> {
    c.query_row("SELECT state FROM actions WHERE id = ?1", [id], |r| {
        r.get(0)
    })
    .optional()
    .q()?
    .ok_or_else(|| refuse("action", id, "is not on record"))
}

fn attempt(home: &Path, id: i64, now: i64) -> Result<String, Fail> {
    verbs::write(home, |c| {
        let was = state(c, id)?;
        if was != "intended" {
            return Err(refuse("action", id, &format!("is {was}, not intended")));
        }
        c.execute(
            "UPDATE actions SET state = 'attempted', attempted_at = ?2 WHERE id = ?1",
            params![id, now],
        )
        .q()?;
        c.execute(
            "INSERT INTO journal (action, at, step) VALUES (?1, ?2, 'attempt')",
            params![id, now],
        )
        .q()
    })?;
    Ok(line(
        "journal-attempt",
        &[("action", j(id)), ("state", j("attempted"))],
    ))
}

fn settle(home: &Path, s: &Settle, now: i64) -> Result<String, Fail> {
    let to = match s.result {
        Some("ok" | "empty") => "done",
        Some("unknown") => "unknown",
        Some(_) => "failed",
        None => "abandoned",
    };
    // An unknown end leaves the row open: no time, no result, until a read.
    let (at, result) = if to == "unknown" {
        (None, None)
    } else {
        (Some(now), s.result)
    };
    verbs::write(home, |c| {
        let was = state(c, s.action)?;
        if !matches!(was.as_str(), "intended" | "attempted" | "unknown") {
            return Err(refuse(
                "action",
                s.action,
                &format!("is settled already ({was})"),
            ));
        }
        if was == "unknown" && to == "unknown" {
            return Err(refuse(
                "action",
                s.action,
                "is unknown already; a read settles it",
            ));
        }
        // A result is a call's, and a call is made only once attempted.
        if was == "intended" && s.result.is_some() {
            return Err(refuse(
                "action",
                s.action,
                "was never attempted, so it has no result; --abandoned settles it",
            ));
        }
        c.execute(
            "UPDATE actions SET state = ?2, settled_at = ?3, result = ?4,
               evidence_json = coalesce(?5, evidence_json)
             WHERE id = ?1",
            params![s.action, to, at, result, s.evidence],
        )
        .q()?;
        c.execute(
            "INSERT INTO journal (action, at, step, detail) VALUES (?1, ?2, 'settle', ?3)",
            params![s.action, now, s.result.unwrap_or("abandoned")],
        )
        .q()
    })?;
    Ok(line(
        "journal-settle",
        &[
            ("action", j(s.action)),
            ("state", j(to)),
            ("result", j(result)),
        ],
    ))
}

fn open(home: &Path) -> Result<String, Fail> {
    verbs::look(home, |c| {
        let mut q = c
            .prepare(
                "SELECT id, poll_at, kind, issue, args_json, before_snapshot, state,
                        intended_at, attempted_at
                 FROM actions WHERE state IN ('intended','attempted','unknown')
                 ORDER BY intended_at, id",
            )
            .q()?;
        let rows = q
            .query_map([], |r| {
                let args: String = r.get(4)?;
                let mut row = vec![
                    ("id", j(r.get::<_, i64>(0)?)),
                    ("poll_at", j(r.get::<_, i64>(1)?)),
                    ("kind", j(r.get::<_, String>(2)?)),
                    ("issue", j(r.get::<_, i64>(3)?)),
                ];
                // An argument blob that is not a JSON object is said, not read
                // as null: the caller that settles cannot tell an unreadable
                // row from one whose action took no arguments.
                match serde_json::from_str::<Value>(&args) {
                    Ok(v @ Value::Object(_)) => row.push(("args", v.to_string())),
                    _ => row.push(("args_unreadable", "true".to_string())),
                }
                row.extend([
                    ("before", j(r.get::<_, i64>(5)?)),
                    ("state", j(r.get::<_, String>(6)?)),
                    ("intended_at", j(r.get::<_, i64>(7)?)),
                    ("attempted_at", j(r.get::<_, Option<i64>>(8)?)),
                ]);
                Ok(obj(&row))
            })
            .q()?
            .collect::<Result<Vec<_>, _>>()
            .q()?;
        Ok(line("journal-open", &[("actions", list(&rows))]))
    })
}

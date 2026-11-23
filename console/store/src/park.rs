//! The park and question verbs: park, park-show, park-retry, park-told,
//! question-asked, question-known and question-answered.
//!
//! The current park of an issue is its `parks` row of the cycle's attempt. The
//! question on record is the one comment a tick may count in: for a park the
//! newest unanswered park question of the current attempt, for a plan the open
//! session's `comment_id` while its question is unanswered. A withdrawn
//! question is never on record.
//!
//! A question has two numbers (schema 30): `id`, the store's own, given in
//! order and never reused, which `tehas question` and `tehas answer` print;
//! and `comment_id`, the comment that asked it. The verbs here are the code
//! host's door: `--id` and `--comment-id` take the comment's id, and `park`'s
//! `question` is the comment's id, which the dispatcher compares with the
//! issue's comments.

use crate::verbs::{
    self, cap_char, j, line, list, obj, refuse, row_json, stub, word_char, Args, Cleaned, Fail,
    Kind, Q,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub enum Call {
    Park {
        issue: i64,
        attempt: i64,
        reason: String,
        step: Option<String>,
        pr: Option<i64>,
        question: Option<(i64, String)>,
        cleaned: Cleaned,
    },
    /// With no issue, the parks owed a start (`--automatic`) or a refine
    /// (`--refine`).
    Show {
        issue: Option<i64>,
        refine: bool,
    },
    Retry {
        issue: i64,
    },
    Told {
        issue: i64,
    },
    Asked {
        issue: i64,
        id: i64,
        kind: &'static str,
        attempt: Option<i64>,
        body_sha: Option<String>,
        /// The choices a park's question offers, as their words joined.
        offers: Option<String>,
    },
    Known {
        issue: i64,
        id: i64,
    },
    /// `by` None is a withdrawal.
    Answered {
        issue: i64,
        id: i64,
        by: Option<String>,
    },
}

/// The arguments of `park-show` as the read of `store batch` takes them: the
/// issue of the row, refused by the verb's own grammar when the call asked for
/// the owed list instead or named nothing. The line of a refusal is the one the
/// single call prints, so a batch and a call made alone answer alike.
pub fn read_park_show(rest: &[String]) -> Option<Result<i64, String>> {
    Some(
        Args::parse(
            "park-show",
            rest,
            &[("--automatic", Kind::Switch), ("--refine", Kind::Switch)],
            true,
        )
        .and_then(|a| {
            let refine = a.has("--refine");
            if refine && a.has("--automatic") {
                return Err(a.usage("--refine", "is not taken with --automatic"));
            }
            a.target(if refine { "--refine" } else { "--automatic" })?
                .ok_or_else(|| a.usage("<issue>", "or --automatic is required"))
        }),
    )
}

/// The arguments of `question-known` as the read of `store batch` takes them:
/// the issue and the comment id, each refused by the verb's own grammar. The
/// line of a refusal is the one the single call prints.
pub fn read_question_known(rest: &[String]) -> Option<Result<(i64, i64), String>> {
    Some(
        Args::parse("question-known", rest, &[("--id", Kind::Value)], true).and_then(|a| {
            Ok((
                a.issue()?,
                a.strict_need("--id", "a comment id", verbs::comment_id)?,
            ))
        }),
    )
}

pub fn parse(name: &str, rest: &[String]) -> Option<Result<Call, String>> {
    Some(match name {
        "park" => park_args(rest),
        "park-show" => Args::parse(
            "park-show",
            rest,
            &[("--automatic", Kind::Switch), ("--refine", Kind::Switch)],
            true,
        )
        .and_then(|a| {
            let refine = a.has("--refine");
            if refine && a.has("--automatic") {
                return Err(a.usage("--refine", "is not taken with --automatic"));
            }
            Ok(Call::Show {
                issue: a.target(if refine { "--refine" } else { "--automatic" })?,
                refine,
            })
        }),
        "park-retry" => Args::parse("park-retry", rest, &[], true)
            .and_then(|a| Ok(Call::Retry { issue: a.issue()? })),
        "park-told" => Args::parse("park-told", rest, &[], true)
            .and_then(|a| Ok(Call::Told { issue: a.issue()? })),
        "question-asked" => asked_args(rest),
        "question-known" => Args::parse("question-known", rest, &[("--id", Kind::Value)], true)
            .and_then(|a| {
                Ok(Call::Known {
                    issue: a.issue()?,
                    id: a.strict_need("--id", "a comment id", verbs::comment_id)?,
                })
            }),
        "question-answered" => answered_args(rest),
        _ => return None,
    })
}

fn park_args(rest: &[String]) -> Result<Call, String> {
    let a = Args::parse(
        "park",
        rest,
        &[
            ("--attempt", Kind::Value),
            ("--reason", Kind::Value),
            ("--step", Kind::Value),
            ("--pr", Kind::Value),
            ("--comment-id", Kind::Value),
            ("--body-sha", Kind::Value),
        ],
        true,
    )?;
    let issue = a.issue()?;
    let attempt = a.strict_need("--attempt", "a number", verbs::number)?;
    let pr = a.strict("--pr", "a number", verbs::number)?;
    let id = a.strict("--comment-id", "a comment id", verbs::comment_id)?;
    let sha = a.strict("--body-sha", "a sha256", verbs::sha256)?;
    let question = match (id, sha) {
        (Some(id), Some(sha)) => Some((id, sha)),
        (None, None) => None,
        (Some(_), None) => return Err(a.usage("--body-sha", "is required with --comment-id")),
        (None, Some(_)) => return Err(a.usage("--comment-id", "is required with --body-sha")),
    };
    let mut c = Cleaned::default();
    let reason = c
        .word("--reason", Some(a.need("--reason")?), cap_char, 80)
        .unwrap_or_else(|| "unknown".to_string());
    let step = c.word("--step", a.value("--step"), word_char, 40);
    Ok(Call::Park {
        issue,
        attempt,
        reason,
        step,
        pr,
        question,
        cleaned: c,
    })
}

fn asked_args(rest: &[String]) -> Result<Call, String> {
    let a = Args::parse(
        "question-asked",
        rest,
        &[
            ("--id", Kind::Value),
            ("--kind", Kind::Value),
            ("--attempt", Kind::Value),
            ("--body-sha", Kind::Value),
            ("--offers", Kind::Value),
        ],
        true,
    )?;
    let issue = a.issue()?;
    let id = a.strict_need("--id", "a comment id", verbs::comment_id)?;
    a.need("--kind")?;
    let kind = a
        .word("--kind", &[("park", "park"), ("plan", "plan")])?
        .expect("--kind was given");
    // A park's choices, each a word of a park's question, once each.
    let offers = a.strict("--offers", "the words of a park's choices", |v| {
        use tehas_core::question::{Choice, Kind as Asked};
        let words: Vec<&str> = v.split(',').collect();
        let each = words.iter().enumerate().all(|(i, w)| {
            Choice::of(w).is_some_and(|c| c.kind() == Asked::Park) && !words[..i].contains(w)
        });
        each.then(|| v.to_string())
    })?;
    if offers.is_some() && kind != "park" {
        return Err(a.usage("--offers", "is given only with --kind park"));
    }
    Ok(Call::Asked {
        issue,
        id,
        kind,
        attempt: a.strict("--attempt", "a number", verbs::number)?,
        body_sha: a.strict("--body-sha", "a sha256", verbs::sha256)?,
        offers,
    })
}

fn answered_args(rest: &[String]) -> Result<Call, String> {
    let a = Args::parse(
        "question-answered",
        rest,
        &[
            ("--id", Kind::Value),
            ("--by", Kind::Value),
            ("--withdraw", Kind::Switch),
        ],
        true,
    )?;
    let issue = a.issue()?;
    let id = a.strict_need("--id", "a comment id", verbs::comment_id)?;
    let by = a.strict("--by", "an account", verbs::account)?;
    match (by.is_some(), a.has("--withdraw")) {
        (true, true) => Err(a.usage("--withdraw", "is not taken with --by")),
        (false, false) => Err(a.usage("--by", "or --withdraw is required")),
        _ => Ok(Call::Answered { issue, id, by }),
    }
}

pub fn run(home: &Path, call: &Call, now: i64) -> Result<String, Fail> {
    match call {
        Call::Park { .. } => park(home, call, now),
        Call::Show { issue, refine } => verbs::look(home, |c| show(c, *issue, *refine)),
        Call::Retry { issue } => retry(home, *issue),
        Call::Told { issue } => told(home, *issue, now),
        Call::Asked {
            issue,
            id,
            kind,
            attempt,
            body_sha,
            offers,
        } => asked(
            home,
            *issue,
            *id,
            kind,
            *attempt,
            body_sha.as_deref(),
            offers.as_deref(),
            now,
        ),
        Call::Known { issue, id } => verbs::look(home, |c| known(c, *issue, *id)),
        Call::Answered { issue, id, by } => answered(home, *issue, *id, by.as_deref(), now),
    }
}

/// The SELECT of a current park: the cycle's attempt joined in, `automatic`
/// derived, the park question on record, when the hold was told, the body
/// the parked attempt's approval bound (null for a run with none), the park
/// reason and the cap its end gave, and `refine` derived: an intent park as
/// underspecified or too-large, of a cycle a label or a retry started, on an
/// issue that never had a planning session, whose one start no start has spent.
///
/// `PARK_NAMES` names these columns and `current` reads three of them by
/// position, so a new column goes next to the two from the history row and
/// `refine` stays last: `row_json` pairs a name with the column of its own
/// index, so a name given in the wrong place is read off the wrong column.
const PARK_SELECT: &str = "SELECT p.issue, p.attempt, p.at, p.reason, p.step, p.pr, p.retries,
    c.started_at, c.started_by,
    (p.reason = 'provider-outage' AND p.retries = 0 AND c.requeued = 0 AND c.how <> 'hand'
      AND c.started_by IS NOT NULL),
    (SELECT q.comment_id FROM questions q WHERE q.issue = p.issue AND q.kind = 'park'
      AND q.answered_at IS NULL AND q.withdrawn_at IS NULL AND q.attempt = c.attempt
      ORDER BY q.posted_at DESC, q.id DESC LIMIT 1),
    p.told_at,
    (SELECT a.body_sha256 FROM approvals a WHERE a.issue = p.issue AND a.used_by = p.attempt
      ORDER BY a.used_at DESC, a.at DESC LIMIT 1),
    h.park_reason,
    h.cap,
    (p.reason = 'intent-park' AND h.park_reason IN ('underspecified', 'too-large')
      AND p.retries = 0 AND c.how IN ('approved', 'outage-retry', 'host', 'answer', 'reply')
      AND c.started_by IS NOT NULL
      AND NOT EXISTS (SELECT 1 FROM planning g WHERE g.issue = p.issue)
      AND NOT EXISTS (SELECT 1 FROM plan_history g WHERE g.issue = p.issue))
  FROM parks p JOIN cycles c ON c.issue = p.issue AND c.attempt = p.attempt
  LEFT JOIN cycle_history h ON h.id = (SELECT max(e.id) FROM cycle_history e
    WHERE e.issue = p.issue AND e.attempt = p.attempt)";

const PARK_NAMES: [&str; 16] = [
    "issue",
    "attempt",
    "at",
    "reason",
    "step",
    "pr",
    "retries",
    "started_at",
    "started_by",
    "automatic?",
    "question",
    "told_at",
    "body_sha256",
    "park_reason",
    "cap",
    "refine?",
];

/// The current park, with the answer `tehas answer` gave to its question on
/// record (null for none): a poll that reads the park and not the question,
/// as the lift of a hold does, reads the answer here.
fn park_json(c: &Connection, issue: i64) -> Result<String, Fail> {
    let row = c
        .query_row(&format!("{PARK_SELECT} WHERE p.issue = ?1"), [issue], |r| {
            row_json(r, &PARK_NAMES)
        })
        .optional()
        .q()?;
    let Some(mut p) = row else {
        return Ok("null".to_string());
    };
    let answer = match question_on_record(c, issue)? {
        Some(id) => crate::answer::waiting_json(c, issue, id)?,
        None => "null".to_string(),
    };
    p.push(("answer", answer));
    Ok(obj(&p))
}

/// `(retries, automatic, the comment of the question on record)` of the
/// current park.
fn current(c: &Connection, issue: i64) -> Result<Option<(i64, bool, Option<i64>)>, Fail> {
    c.query_row(&format!("{PARK_SELECT} WHERE p.issue = ?1"), [issue], |r| {
        Ok((r.get(6)?, r.get(9)?, r.get(10)?))
    })
    .optional()
    .q()
}

/// The park question on record of the issue, by the store's number: the
/// newest unanswered one of the current park's attempt that was not withdrawn,
/// as `PARK_SELECT` reads its comment.
pub(crate) fn question_on_record(c: &Connection, issue: i64) -> Result<Option<i64>, Fail> {
    c.query_row(
        "SELECT q.id FROM parks p JOIN cycles c ON c.issue = p.issue AND c.attempt = p.attempt
         JOIN questions q ON q.issue = p.issue AND q.kind = 'park' AND q.answered_at IS NULL
           AND q.withdrawn_at IS NULL AND q.attempt = c.attempt
         WHERE p.issue = ?1 ORDER BY q.posted_at DESC, q.id DESC LIMIT 1",
        [issue],
        |r| r.get(0),
    )
    .optional()
    .q()
}

/// The comment is taken when it is a question of another issue or of `kind`'s
/// other kind. Some(true) when the same question is already on record. A
/// withdrawn question takes nothing: its comment asked again is a new
/// question, with the next number.
fn taken(c: &Connection, issue: i64, id: i64, kind: &str) -> Result<Option<bool>, Fail> {
    let had: Option<(i64, String)> = c
        .query_row(
            "SELECT issue, kind FROM questions WHERE comment_id = ?1 AND withdrawn_at IS NULL",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .q()?;
    match had {
        None => Ok(Some(false)),
        Some((i, k)) if i == issue && k == kind => Ok(Some(true)),
        Some(_) => Ok(None),
    }
}

/// Records comment `id` as a park question of `attempt`, given the store's
/// next number. A replay of the same question changes nothing; a comment that
/// is another question is refused.
fn park_question(
    c: &Connection,
    issue: i64,
    attempt: i64,
    id: i64,
    sha: &str,
    now: i64,
) -> Result<(), Fail> {
    let had: Option<(i64, String, i64)> = c
        .query_row(
            "SELECT issue, kind, attempt FROM questions
             WHERE comment_id = ?1 AND withdrawn_at IS NULL",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .q()?;
    match had {
        None => {
            c.execute(
                "INSERT INTO questions (comment_id, issue, attempt, kind, posted_at, body_sha256)
                 VALUES (?1, ?2, ?3, 'park', ?4, ?5)",
                params![id, issue, attempt, now, sha],
            )
            .q()?;
            Ok(())
        }
        Some((i, k, a)) if i == issue && k == "park" && a == attempt => Ok(()),
        Some((i, k, a)) if i == issue && k == "park" => Err(refuse(
            "attempt",
            issue,
            &format!("comment {id} is the question of attempt {a}"),
        )),
        Some(_) => Err(refuse(
            "taken",
            issue,
            &format!("comment {id} is another question"),
        )),
    }
}

fn park(home: &Path, call: &Call, now: i64) -> Result<String, Fail> {
    let Call::Park {
        issue,
        attempt,
        reason,
        step,
        pr,
        question,
        cleaned,
    } = call
    else {
        unreachable!("park is called with a park");
    };
    let (issue, attempt, pr, step) = (*issue, *attempt, *pr, step.as_deref());
    verbs::write(home, |c| {
        stub(c, issue)?;
        let row = crate::cycle::row(c, issue)?
            .ok_or_else(|| refuse("nocycle", issue, "no cycle on record"))?;
        if row.attempt != attempt {
            return Err(refuse(
                "attempt",
                issue,
                &format!("the cycle is attempt {}", row.attempt),
            ));
        }
        let had: Option<(String, Option<String>, Option<i64>)> = c
            .query_row(
                "SELECT reason, step, pr FROM parks WHERE issue = ?1 AND attempt = ?2",
                params![issue, attempt],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .q()?;
        match had {
            Some((r, s, p)) if r == *reason && s.as_deref() == step && p == pr => {}
            Some(_) => {
                return Err(refuse(
                    "parked",
                    issue,
                    "this attempt is parked with another reason, step or pull request",
                ))
            }
            None => {
                c.execute(
                    "INSERT INTO parks (issue, attempt, at, reason, step, pr, retries)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
                    params![issue, attempt, now, reason, step, pr],
                )
                .q()?;
            }
        }
        if let Some((id, sha)) = question.as_ref() {
            park_question(c, issue, attempt, *id, sha, now)?;
        }
        let asked = current(c, issue)?.and_then(|(_, _, q)| q);
        Ok(line(
            "park",
            &[
                ("issue", j(issue)),
                ("question", j(asked)),
                ("park", park_json(c, issue)?),
                ("cleaned", cleaned.json()),
            ],
        ))
    })
}

/// `park-show <issue>`, or the parks owed a start (`--automatic`) or a refine
/// (`--refine`). The single call reads it through `verbs::look`; `verbs::look_many`
/// hands it the connection of the batch.
pub(crate) fn show(c: &Connection, issue: Option<i64>, refine: bool) -> Result<String, Fail> {
    let owed = if refine { "refine" } else { "automatic" };
    match issue {
        Some(issue) => Ok(line("park-show", &[("park", park_json(c, issue)?)])),
        None => {
            let mut q = c.prepare(&format!("{PARK_SELECT} ORDER BY p.issue")).q()?;
            let rows: Vec<Vec<(&str, String)>> = q
                .query_map([], |r| row_json(r, &PARK_NAMES))
                .q()?
                .collect::<Result<_, _>>()
                .q()?;
            let parks: Vec<String> = rows
                .iter()
                .filter(|p| p.iter().any(|(k, v)| *k == owed && v == "true"))
                .map(|p| obj(p))
                .collect();
            Ok(line("park-show", &[("parks", list(&parks))]))
        }
    }
}

fn retry(home: &Path, issue: i64) -> Result<String, Fail> {
    verbs::write(home, |c| {
        let (retries, automatic, _) =
            current(c, issue)?.ok_or_else(|| refuse("unparked", issue, "no current park"))?;
        c.execute(
            "UPDATE parks SET retries = retries + 1 WHERE issue = ?1
               AND attempt = (SELECT attempt FROM cycles WHERE issue = ?1)",
            [issue],
        )
        .q()?;
        Ok(line(
            "park-retry",
            &[("retries", j(retries + 1)), ("automatic", j(automatic))],
        ))
    })
}

/// `park-told`: the hold of the current park said once.
fn told(home: &Path, issue: i64, now: i64) -> Result<String, Fail> {
    verbs::write(home, |c| {
        current(c, issue)?.ok_or_else(|| refuse("nopark", issue, "no current park"))?;
        let n = c
            .execute(
                "UPDATE parks SET told_at = ?2 WHERE issue = ?1 AND told_at IS NULL
                   AND attempt = (SELECT attempt FROM cycles WHERE issue = ?1)",
                params![issue, now],
            )
            .q()?;
        Ok(line(
            "park-told",
            &[("said", j(n > 0)), ("park", park_json(c, issue)?)],
        ))
    })
}

/// The open planning session's `(turns, comment_id)`.
fn open_plan(c: &Connection, issue: i64) -> Result<Option<(i64, Option<i64>)>, Fail> {
    c.query_row(
        "SELECT turns, comment_id FROM planning WHERE issue = ?1 AND ended_at IS NULL",
        [issue],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()
    .q()
}

const QUESTION_NAMES: [&str; 10] = [
    "id",
    "comment_id",
    "issue",
    "attempt",
    "kind",
    "posted_at",
    "body_sha256",
    "answered_at",
    "answered_by",
    "withdrawn_at",
];

/// The newest question that comment `id` asked, with the store's number as
/// `id`.
fn question_json(c: &Connection, id: i64) -> Result<String, Fail> {
    Ok(c.query_row(
        "SELECT id, comment_id, issue, attempt, kind, posted_at, body_sha256, answered_at,
               answered_by, withdrawn_at
             FROM questions WHERE comment_id = ?1 ORDER BY id DESC LIMIT 1",
        [id],
        |r| row_json(r, &QUESTION_NAMES),
    )
    .optional()
    .q()?
    .map_or("null".to_string(), |p| obj(&p)))
}

#[allow(clippy::too_many_arguments)]
fn asked(
    home: &Path,
    issue: i64,
    id: i64,
    kind: &str,
    attempt: Option<i64>,
    body_sha: Option<&str>,
    offers: Option<&str>,
    now: i64,
) -> Result<String, Fail> {
    verbs::write(home, |c| {
        stub(c, issue)?;
        let derived = if kind == "park" {
            current(c, issue)?.ok_or_else(|| refuse("unparked", issue, "no current park"))?;
            crate::cycle::row(c, issue)?
                .map(|r| r.attempt)
                .expect("a current park has a cycle")
        } else {
            open_plan(c, issue)?
                .ok_or_else(|| refuse("noplan", issue, "no open planning session"))?
                .0
        };
        if attempt.is_some_and(|a| a != derived) {
            return Err(refuse(
                "attempt",
                issue,
                &format!("the question is for {derived}"),
            ));
        }
        let had = taken(c, issue, id, kind)?;
        match had {
            None => {
                return Err(refuse(
                    "taken",
                    issue,
                    &format!("comment {id} is another question"),
                ))
            }
            Some(true) => {}
            Some(false) => {
                c.execute(
                    "INSERT INTO questions (comment_id, issue, attempt, kind, posted_at,
                       body_sha256, offers)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![id, issue, derived, kind, now, body_sha, offers],
                )
                .q()?;
                if kind == "plan" {
                    c.execute(
                        "UPDATE planning SET comment_id = ?2 WHERE issue = ?1",
                        params![issue, id],
                    )
                    .q()?;
                }
            }
        }
        // `already` for a replay: the call that recorded the question
        // raises it (T11.9).
        Ok(line(
            "question-asked",
            &[
                ("question", question_json(c, id)?),
                ("already", j(had == Some(true))),
            ],
        ))
    })
}

/// Whether comment `id` is the question on record of the issue.
pub fn on_record(c: &Connection, issue: i64, id: i64) -> Result<bool, Fail> {
    if current(c, issue)?.is_some_and(|(_, _, q)| q == Some(id)) {
        return Ok(true);
    }
    let plan: bool = c
        .query_row(
            "SELECT count(*) > 0 FROM planning p JOIN questions q ON q.comment_id = p.comment_id
             WHERE p.issue = ?1 AND p.ended_at IS NULL AND p.comment_id = ?2
               AND q.issue = ?1 AND q.kind = 'plan' AND q.answered_at IS NULL
               AND q.withdrawn_at IS NULL",
            params![issue, id],
            |r| r.get(0),
        )
        .q()?;
    Ok(plan)
}

/// `question-known <issue> --id <id>`: whether the comment is the question the
/// host has on record for the issue. A withdrawn question is not recorded:
/// its row is kept, and it asks nothing. The single call reads it through
/// `verbs::look`; `verbs::look_many` hands it the connection of the batch.
pub(crate) fn known(c: &Connection, issue: i64, id: i64) -> Result<String, Fail> {
    let row: Option<(String, Option<i64>, bool, i64)> = c
        .query_row(
            "SELECT kind, answered_at, reask, id FROM questions
             WHERE comment_id = ?1 AND issue = ?2 AND withdrawn_at IS NULL",
            params![id, issue],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .q()?;
    let known = on_record(c, issue, id)?;
    Ok(line(
        "question-known",
        &[
            ("known", j(known)),
            ("kind", j(row.as_ref().map(|r| r.0.clone()))),
            ("recorded", j(row.is_some())),
            ("answered", j(row.as_ref().is_some_and(|r| r.1.is_some()))),
            // Asked before 2.0, open at the upgrade: the poll asks it again
            // in 2.0's words, once.
            ("reask", j(row.as_ref().is_some_and(|r| r.2))),
            (
                "answer",
                match row.as_ref().filter(|_| known) {
                    Some(r) => crate::answer::waiting_json(c, issue, r.3)?,
                    None => "null".to_string(),
                },
            ),
        ],
    ))
}

/// `question-answered`: the question comment `id` asked, answered by `by`, or
/// withdrawn when `by` is None. A withdrawal keeps the row and its answer,
/// with when it was withdrawn, and takes it off the record; a withdrawn
/// question is answered and withdrawn no more.
fn answered(home: &Path, issue: i64, id: i64, by: Option<&str>, now: i64) -> Result<String, Fail> {
    verbs::write(home, |c| {
        let there: bool = c
            .query_row(
                "SELECT count(*) > 0 FROM questions
                 WHERE comment_id = ?1 AND issue = ?2 AND withdrawn_at IS NULL",
                params![id, issue],
                |r| r.get(0),
            )
            .q()?;
        if !there {
            return Err(refuse(
                "unknown",
                issue,
                &format!("no question {id} on this issue"),
            ));
        }
        match by {
            Some(by) => {
                c.execute(
                    "UPDATE questions SET answered_at = ?2, answered_by = ?3
                     WHERE comment_id = ?1 AND answered_at IS NULL AND withdrawn_at IS NULL",
                    params![id, now, by],
                )
                .q()?;
                Ok(line(
                    "question-answered",
                    &[("question", question_json(c, id)?)],
                ))
            }
            None => {
                c.execute(
                    "UPDATE planning SET comment_id = NULL WHERE issue = ?1 AND comment_id = ?2",
                    params![issue, id],
                )
                .q()?;
                c.execute(
                    "UPDATE questions SET withdrawn_at = ?2
                     WHERE comment_id = ?1 AND withdrawn_at IS NULL",
                    params![id, now],
                )
                .q()?;
                Ok(line(
                    "question-answered",
                    &[("withdrawn", j(true)), ("question", question_json(c, id)?)],
                ))
            }
        }
    })
}

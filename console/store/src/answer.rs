//! The answer verbs: answer-record and answer-drop (#2782).
//!
//! An answer is what `tehas answer` gave for a person to the question on
//! record of an issue: a park's or a planning turn's. It is one row per
//! question, so a second answer to the same question takes the place of the
//! first, and it lives no longer than its question: a question answered,
//! withdrawn or replaced by a newer one leaves its answer with nothing to
//! answer, and `question-known` gives an answer only for the question on
//! record. The poll reads it there and acts on it as on a box ticked by the
//! same account; `answer-drop` takes away an answer the poll could not act
//! on, as it unticks a box.
//!
//! The choices are words of the question's kind. A park's: `retry` (with the
//! issue and its comments), `retry-text` (the issue text only), `park` (leave
//! it parked). A planning question's: `option` with its number, `update` (the
//! option "Update this issue", kept as its number), `end`, and `text`, a
//! reply in words.

use crate::verbs::{self, j, line, obj, refuse, row_json, Args, Fail, Kind, Q};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};

/// The choices of a park's question.
const PARK: [&str; 3] = ["retry", "retry-text", "park"];
/// The choices of a planning question.
const PLAN: [&str; 4] = ["option", "update", "end", "text"];

/// The words `--choice` takes, each its own spelling.
const CHOICES: [(&str, &str); 7] = [
    ("retry", "retry"),
    ("retry-text", "retry-text"),
    ("park", "park"),
    ("option", "option"),
    ("update", "update"),
    ("end", "end"),
    ("text", "text"),
];

/// The label of the planner's option that makes the plan the issue's text.
pub const UPDATE: &str = "Update this issue";

/// The options a planning question lists, as tehas_core::question::MAX_OPTIONS
/// cuts them: an option past it is in no comment.
const MAX_OPTIONS: i64 = 20;

/// The longest reply in words: the bound of a planning answer's text.
const TEXT_BOUND: u64 = tehas_core::file::READ_BOUND;

#[derive(Debug, PartialEq, Eq)]
pub enum Call {
    Record {
        issue: i64,
        by: String,
        choice: &'static str,
        option: Option<i64>,
        text: Option<PathBuf>,
    },
    Drop {
        issue: i64,
        id: i64,
    },
}

pub fn parse(name: &str, rest: &[String]) -> Option<Result<Call, String>> {
    Some(match name {
        "answer-record" => record_args(rest),
        "answer-drop" => {
            Args::parse("answer-drop", rest, &[("--id", Kind::Value)], true).and_then(|a| {
                Ok(Call::Drop {
                    issue: a.issue()?,
                    id: a.strict_need("--id", "a comment id", verbs::comment_id)?,
                })
            })
        }
        _ => return None,
    })
}

fn record_args(rest: &[String]) -> Result<Call, String> {
    let a = Args::parse(
        "answer-record",
        rest,
        &[
            ("--by", Kind::Value),
            ("--choice", Kind::Value),
            ("--option", Kind::Value),
            ("--text-file", Kind::Value),
        ],
        true,
    )?;
    let issue = a.issue()?;
    let by = a.strict_need("--by", "an account", verbs::account)?;
    a.need("--choice")?;
    let choice = a.word("--choice", &CHOICES)?.expect("--choice was given");
    let option = a.strict("--option", "a number of 1 to 20", |v| {
        verbs::number(v).filter(|n| (1..=MAX_OPTIONS).contains(n))
    })?;
    let text = a.path("--text-file")?;
    if (choice == "option") != option.is_some() {
        return Err(a.usage("--option", "is given exactly with --choice option"));
    }
    if (choice == "text") != text.is_some() {
        return Err(a.usage("--text-file", "is given exactly with --choice text"));
    }
    Ok(Call::Record {
        issue,
        by,
        choice,
        option,
        text,
    })
}

pub fn run(home: &Path, call: &Call, now: i64) -> Result<String, Fail> {
    match call {
        Call::Record {
            issue,
            by,
            choice,
            option,
            text,
        } => record(home, *issue, by, choice, *option, text.as_deref(), now),
        Call::Drop { issue, id } => drop(home, *issue, *id),
    }
}

/// The names of an answer's columns, in the order `waiting_json` reads them;
/// the text is read past them.
const ANSWER_NAMES: [&str; 7] = ["question", "issue", "kind", "choice", "option", "by", "at"];

/// The answer waiting on question `id` of the issue: null for none. `text` is
/// the reply in words, null for a choice, and `how` is always `answer`.
pub(crate) fn waiting_json(c: &Connection, issue: i64, id: i64) -> Result<String, Fail> {
    let row = c
        .query_row(
            "SELECT a.question, a.issue, q.kind, a.choice, a.option_key, a.by_account, a.at,
               a.text
             FROM answers a JOIN questions q ON q.id = a.question
             WHERE a.question = ?1 AND a.issue = ?2",
            params![id, issue],
            |r| {
                let mut p = row_json(r, &ANSWER_NAMES)?;
                p.push(("text", j(r.get::<_, Option<String>>(7)?)));
                p.push(("how", j("answer")));
                Ok(p)
            },
        )
        .optional()
        .q()?;
    Ok(row.map_or("null".to_string(), |p| obj(&p)))
}

/// The open planning session's question, while it waits for an answer, and
/// the options the turn offered.
fn plan_question(c: &Connection, issue: i64) -> Result<Option<(i64, Option<String>)>, Fail> {
    c.query_row(
        "SELECT q.id, p.options_json FROM planning p JOIN questions q ON q.id = p.comment_id
         WHERE p.issue = ?1 AND p.ended_at IS NULL AND p.running = 0
           AND q.issue = ?1 AND q.kind = 'plan' AND q.answered_at IS NULL",
        [issue],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()
    .q()
}

/// The labels of the options on record, a JSON list of text as `plan-end`
/// writes it, or the refusal that says they cannot be read.
fn options(issue: i64, json: Option<&str>) -> Result<Vec<String>, Fail> {
    let unread = || {
        refuse(
            "options",
            issue,
            "the options of the planning question cannot be read",
        )
    };
    serde_json::from_str::<Vec<String>>(json.unwrap_or("[]")).map_err(|_| unread())
}

fn record(
    home: &Path,
    issue: i64,
    by: &str,
    choice: &str,
    option: Option<i64>,
    text: Option<&Path>,
    now: i64,
) -> Result<String, Fail> {
    let text = match text {
        Some(p) => {
            let t = verbs::read_text(p, TEXT_BOUND, "1 MiB", issue, "answer", "the answer")?;
            if t.trim().is_empty() {
                return Err(refuse("answer", issue, "the answer holds no words"));
            }
            Some(t)
        }
        None => None,
    };
    verbs::write(home, |c| {
        let (id, key) = if PARK.contains(&choice) {
            let Some(id) = crate::park::question_on_record(c, issue)? else {
                return Err(refuse(
                    "unasked",
                    issue,
                    "no stop waits for an answer: the issue has no park question on record",
                ));
            };
            (id, None)
        } else {
            debug_assert!(PLAN.contains(&choice));
            let Some((id, json)) = plan_question(c, issue)? else {
                return Err(refuse(
                    "unasked",
                    issue,
                    "no planning question waits for an answer",
                ));
            };
            let labels = options(issue, json.as_deref())?;
            let offered = (labels.len() as i64).min(MAX_OPTIONS);
            let key = match (choice, option) {
                ("option", Some(k)) if k <= offered => Some(k),
                ("option", Some(k)) => {
                    return Err(refuse(
                        "unoffered",
                        issue,
                        &format!("the question offers {offered} options, not {k}"),
                    ))
                }
                ("update", _) => match labels
                    .iter()
                    .take(offered as usize)
                    .position(|l| l == UPDATE)
                {
                    Some(i) => Some(i as i64 + 1),
                    None => {
                        return Err(refuse(
                            "unoffered",
                            issue,
                            &format!("the question does not offer \"{UPDATE}\""),
                        ))
                    }
                },
                _ => None,
            };
            (id, key)
        };
        let replaced: bool = c
            .query_row(
                "SELECT count(*) > 0 FROM answers WHERE question = ?1",
                [id],
                |r| r.get(0),
            )
            .q()?;
        c.execute(
            "INSERT INTO answers (question, issue, at, by_account, choice, option_key, text)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (question) DO UPDATE SET at = excluded.at,
               by_account = excluded.by_account, choice = excluded.choice,
               option_key = excluded.option_key, text = excluded.text",
            params![id, issue, now, by, choice, key, text],
        )
        .q()?;
        Ok(line(
            "answer-record",
            &[
                ("answer", waiting_json(c, issue, id)?),
                ("replaced", j(replaced)),
            ],
        ))
    })
}

/// `answer-drop <issue> --id <id>`: the answer to question `id` taken away.
/// One already gone is done.
fn drop(home: &Path, issue: i64, id: i64) -> Result<String, Fail> {
    verbs::write(home, |c| {
        let gone = c
            .execute(
                "DELETE FROM answers WHERE question = ?1 AND issue = ?2",
                params![id, issue],
            )
            .q()?;
        Ok(line("answer-drop", &[("dropped", j(gone > 0))]))
    })
}

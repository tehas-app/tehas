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
use tehas_core::plan::UPDATE;
use tehas_core::question::{Choice, Kind as Asked, CHOICES};

/// The options a planning question lists: an option past it is in no comment.
const MAX_OPTIONS: i64 = tehas_core::question::MAX_OPTIONS as i64;

/// The longest reply in words: the bound of a planning answer's text.
const TEXT_BOUND: u64 = tehas_core::file::READ_BOUND;

#[derive(Debug, PartialEq, Eq)]
pub enum Call {
    Record {
        issue: i64,
        by: String,
        choice: Choice,
        option: Option<i64>,
        text: Option<PathBuf>,
    },
    Drop {
        issue: i64,
        id: i64,
    },
    /// `question-show <issue>`: the question that waits for an answer.
    Show {
        issue: i64,
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
        "question-show" => Args::parse("question-show", rest, &[], true)
            .and_then(|a| Ok(Call::Show { issue: a.issue()? })),
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
    let given = a.need("--choice")?;
    let Some(chosen) = Choice::of(given) else {
        let words: Vec<&str> = CHOICES.iter().map(|c| c.word()).collect();
        return Err(a.usage(
            "--choice",
            &format!(
                "is not one of {}: {}",
                words.join(", "),
                verbs::shown(given)
            ),
        ));
    };
    let option = a.strict("--option", "a number of 1 to 20", |v| {
        verbs::number(v).filter(|n| (1..=MAX_OPTIONS).contains(n))
    })?;
    let text = a.path("--text-file")?;
    if (chosen == Choice::Option) != option.is_some() {
        return Err(a.usage("--option", "is given exactly with --choice option"));
    }
    if (chosen == Choice::Text) != text.is_some() {
        return Err(a.usage("--text-file", "is given exactly with --choice text"));
    }
    Ok(Call::Record {
        issue,
        by,
        choice: chosen,
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
        } => record(home, *issue, by, *choice, *option, text.as_deref(), now),
        Call::Drop { issue, id } => drop(home, *issue, *id),
        Call::Show { issue } => verbs::look(home, |c| show(c, *issue)),
    }
}

/// The names of an answer's columns, in the order `waiting_json` reads them;
/// the text is read past them.
const ANSWER_NAMES: [&str; 8] = [
    "question", "comment", "issue", "kind", "choice", "option", "by", "at",
];

/// The answer waiting on question `id` of the issue, by the store's number:
/// null for none. `question` is the store's number and `comment` the id of
/// the comment that asked it, which the dispatcher matches with the issue's
/// comments. `text` is the reply in words, null for a choice, and `how` is
/// always `answer`.
pub(crate) fn waiting_json(c: &Connection, issue: i64, id: i64) -> Result<String, Fail> {
    let row = c
        .query_row(
            "SELECT a.question, q.comment_id, a.issue, q.kind, a.choice, a.option_key,
               a.by_account, a.at, a.text
             FROM answers a JOIN questions q ON q.id = a.question
             WHERE a.question = ?1 AND a.issue = ?2",
            params![id, issue],
            |r| {
                let mut p = row_json(r, &ANSWER_NAMES)?;
                p.push(("text", j(r.get::<_, Option<String>>(8)?)));
                p.push(("how", j("answer")));
                Ok(p)
            },
        )
        .optional()
        .q()?;
    Ok(row.map_or("null".to_string(), |p| obj(&p)))
}

/// The open planning session's question, by the store's number, while it
/// waits for an answer, and the options the turn offered.
fn plan_question(c: &Connection, issue: i64) -> Result<Option<(i64, Option<String>)>, Fail> {
    c.query_row(
        "SELECT q.id, p.options_json FROM planning p JOIN questions q ON q.comment_id = p.comment_id
         WHERE p.issue = ?1 AND p.ended_at IS NULL AND p.running = 0
           AND q.issue = ?1 AND q.kind = 'plan' AND q.answered_at IS NULL
           AND q.withdrawn_at IS NULL",
        [issue],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()
    .q()
}

/// `question-show <issue>`: the question that waits for an answer, a
/// planning session's open question or else the park question on record,
/// with when it was asked, the choices a park's question offers (null for a
/// question asked before they were kept) and a planning question's options.
/// `id` is the store's number of the question. `question` is null when none
/// waits.
fn show(c: &Connection, issue: i64) -> Result<String, Fail> {
    let (id, kind, options) = match plan_question(c, issue)? {
        Some((id, json)) => (id, "plan", Some(options(issue, json.as_deref())?)),
        None => match crate::park::question_on_record(c, issue)? {
            Some(id) => (id, "park", None),
            None => return Ok(line("question-show", &[("question", "null".into())])),
        },
    };
    let (posted, offers): (i64, Option<String>) = c
        .query_row(
            "SELECT posted_at, offers FROM questions WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .q()?;
    let offers = offers.map(|o| o.split(',').map(str::to_string).collect::<Vec<_>>());
    Ok(line(
        "question-show",
        &[(
            "question",
            obj(&[
                ("id", j(id)),
                ("kind", j(kind)),
                ("posted_at", j(posted)),
                ("offers", j(offers)),
                ("options", j(options)),
            ]),
        )],
    ))
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
    choice: Choice,
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
        let (id, key) = if choice.kind() == Asked::Park {
            let Some(id) = crate::park::question_on_record(c, issue)? else {
                return Err(refuse(
                    "unasked",
                    issue,
                    "no stop waits for an answer: the issue has no park question on record",
                ));
            };
            (id, None)
        } else {
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
                (Choice::Option, Some(k)) if k <= offered => Some(k),
                (Choice::Option, Some(k)) => {
                    return Err(refuse(
                        "unoffered",
                        issue,
                        &format!("the question offers {offered} options, not {k}"),
                    ))
                }
                (Choice::Update, _) => match labels
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
            params![id, issue, now, by, choice.word(), key, text],
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

/// `answer-drop <issue> --id <id>`: the answer to the question comment `id`
/// asked taken away. One already gone is done.
fn drop(home: &Path, issue: i64, id: i64) -> Result<String, Fail> {
    verbs::write(home, |c| {
        let gone = c
            .execute(
                "DELETE FROM answers WHERE issue = ?2
                   AND question = (SELECT q.id FROM questions q
                     WHERE q.comment_id = ?1 AND q.withdrawn_at IS NULL)",
                params![id, issue],
            )
            .q()?;
        Ok(line("answer-drop", &[("dropped", j(gone > 0))]))
    })
}

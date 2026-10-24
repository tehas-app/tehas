//! The planning verbs: plan-begin, plan-answer, plan-end, plan-note and
//! plan-show. The open session of an issue is its `planning` row with no
//! `ended_at`; an ended row stays until the next plan-begin replaces it. The
//! notes of a session go when it ends or is replaced.

use crate::verbs::{
    self, j, line, list, obj, refuse, row_json, stub, Args, Cleaned, Fail, Kind, Q,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub enum Call {
    Begin {
        issue: i64,
        by: String,
        label: &'static str,
        brief: PathBuf,
    },
    BeginUndo {
        issue: i64,
    },
    Answer {
        issue: i64,
        turn: i64,
        by: String,
        fixer: bool,
        text: Option<PathBuf>,
    },
    AnswerUndo {
        issue: i64,
        turn: i64,
    },
    End(Box<TurnEnd>),
    Posted {
        issue: i64,
    },
    Close {
        issue: i64,
    },
    Show {
        issue: Option<i64>,
    },
    Note {
        issue: i64,
        kind: &'static str,
        id: i64,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub struct TurnEnd {
    pub issue: i64,
    pub turn: i64,
    pub outcome: &'static str,
    pub rc: Option<i64>,
    pub session: Option<String>,
    pub options: Vec<String>,
    /// `(url, issue number)`.
    pub filed: Vec<(String, i64)>,
    pub out: Option<PathBuf>,
    pub cleaned: Cleaned,
}

pub const OUTCOMES: [&str; 6] = [
    "finished",
    "failed",
    "interrupted",
    "not-started",
    "not-readable",
    "lost",
];

const BRIEF_BOUND: u64 = 1 << 20;
const TEXT_BOUND: u64 = 64 << 10;

/// The kinds of `plan-note`, and the keys plan-show lists them under.
const NOTES: [(&str, &str); 2] = [("fixer-refused", "fixer_refused"), ("held", "held")];
const NOTE_KINDS: [&str; 2] = [NOTES[0].0, NOTES[1].0];
const MAX_OPTIONS: usize = 20;
const OPTION_BYTES: usize = 300;

pub fn parse(name: &str, rest: &[String]) -> Option<Result<Call, String>> {
    Some(match name {
        "plan-begin" => begin_args(rest),
        "plan-answer" => answer_args(rest),
        "plan-end" => end_args(rest),
        "plan-show" => {
            Args::parse("plan-show", rest, &[("--open", Kind::Switch)], true).and_then(|a| {
                Ok(Call::Show {
                    issue: a.target("--open")?,
                })
            })
        }
        "plan-note" => note_args(rest),
        _ => return None,
    })
}

fn note_args(rest: &[String]) -> Result<Call, String> {
    let a = Args::parse(
        "plan-note",
        rest,
        &[("--kind", Kind::Value), ("--id", Kind::Value)],
        true,
    )?;
    let issue = a.issue()?;
    a.need("--kind")?;
    Ok(Call::Note {
        issue,
        kind: a.word("--kind", &NOTE_KINDS)?.expect("--kind was given"),
        id: a.strict_need("--id", "a comment id", verbs::comment_id)?,
    })
}

fn begin_args(rest: &[String]) -> Result<Call, String> {
    let a = Args::parse(
        "plan-begin",
        rest,
        &[
            ("--by", Kind::Value),
            ("--label", Kind::Value),
            ("--brief-file", Kind::Value),
            ("--undo", Kind::Switch),
        ],
        true,
    )?;
    let issue = a.issue()?;
    if a.has("--undo") {
        a.only(&["--undo"], "--undo")?;
        return Ok(Call::BeginUndo { issue });
    }
    let by = a.strict_need("--by", "an account", verbs::account)?;
    a.need("--label")?;
    let label = a
        .word("--label", &["tehas:plan", "tehas:refine"])?
        .expect("--label was given");
    a.need("--brief-file")?;
    let brief = a.path("--brief-file")?.expect("--brief-file was given");
    Ok(Call::Begin {
        issue,
        by,
        label,
        brief,
    })
}

fn answer_args(rest: &[String]) -> Result<Call, String> {
    let a = Args::parse(
        "plan-answer",
        rest,
        &[
            ("--turn", Kind::Value),
            ("--by", Kind::Value),
            ("--fixer", Kind::Switch),
            ("--text-file", Kind::Value),
            ("--undo", Kind::Switch),
        ],
        true,
    )?;
    let issue = a.issue()?;
    let turn = a.strict_need("--turn", "a number", verbs::number)?;
    if a.has("--undo") {
        a.only(&["--undo", "--turn"], "--undo")?;
        return Ok(Call::AnswerUndo { issue, turn });
    }
    Ok(Call::Answer {
        issue,
        turn,
        by: a.strict_need("--by", "an account", verbs::account)?,
        fixer: a.has("--fixer"),
        text: a.path("--text-file")?,
    })
}

fn session_ok(s: &str) -> bool {
    (1..=128).contains(&s.len()) && s.bytes().all(verbs::cap_char)
}

/// The issue number of a GitHub issue URL of any repository.
fn filed_issue(url: &str) -> Option<i64> {
    let rest = url.strip_prefix("https://github.com/")?;
    let mut parts = rest.split('/');
    let (owner, repo, issues, n) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || owner.is_empty() || repo.is_empty() || issues != "issues" {
        return None;
    }
    verbs::number(n)
}

fn end_args(rest: &[String]) -> Result<Call, String> {
    let a = Args::parse(
        "plan-end",
        rest,
        &[
            ("--turn", Kind::Value),
            ("--outcome", Kind::Value),
            ("--rc", Kind::Value),
            ("--session", Kind::Value),
            ("--option", Kind::Repeat),
            ("--filed", Kind::Repeat),
            ("--out", Kind::Value),
            ("--posted", Kind::Switch),
            ("--close", Kind::Switch),
        ],
        true,
    )?;
    let issue = a.issue()?;
    if a.has("--posted") && a.has("--close") {
        return Err(a.usage("--close", "is not taken with --posted"));
    }
    if a.has("--posted") {
        a.only(&["--posted"], "--posted")?;
        return Ok(Call::Posted { issue });
    }
    if a.has("--close") {
        a.only(&["--close"], "--close")?;
        return Ok(Call::Close { issue });
    }
    let turn = a.strict_need("--turn", "a number", verbs::number)?;
    a.need("--outcome")?;
    let outcome = a
        .word("--outcome", &OUTCOMES)?
        .expect("--outcome was given");
    let rc = a.strict("--rc", "an exit status", verbs::count)?;
    match (outcome, rc) {
        ("finished", Some(n)) if n != 0 => return Err(a.usage("--rc", "is 0 with finished")),
        ("finished" | "failed", None) => {
            return Err(a.usage("--rc", &format!("is required with {outcome}")))
        }
        ("failed", Some(0)) => return Err(a.usage("--rc", "is not 0 with failed")),
        _ => {}
    }
    let out = a.path("--out")?;
    if out
        .as_ref()
        .is_some_and(|p| p.components().any(|c| c == Component::ParentDir))
    {
        return Err(a.usage("--out", "holds a .. component"));
    }
    let mut c = Cleaned::default();
    let session = c.whole("--session", a.value("--session"), session_ok);
    let given = a.values("--option");
    if given.len() > MAX_OPTIONS {
        c.mark("--option");
    }
    let options = given
        .iter()
        .take(MAX_OPTIONS)
        .map(|label| {
            let mut v: String = label.chars().filter(|ch| !ch.is_control()).collect();
            let mut end = v.len().min(OPTION_BYTES);
            while !v.is_char_boundary(end) {
                end -= 1;
            }
            v.truncate(end);
            if v.is_empty() {
                v = "-".to_string();
            }
            if v != *label {
                c.mark("--option");
            }
            v
        })
        .collect();
    let mut filed = Vec::new();
    for url in a.values("--filed") {
        match filed_issue(url) {
            Some(n) => filed.push((url.to_string(), n)),
            None => c.mark("--filed"),
        }
    }
    Ok(Call::End(Box::new(TurnEnd {
        issue,
        turn,
        outcome,
        rc,
        session,
        options,
        filed,
        out,
        cleaned: c,
    })))
}

pub fn run(home: &Path, call: &Call, now: i64) -> Result<String, Fail> {
    match call {
        Call::Begin {
            issue,
            by,
            label,
            brief,
        } => begin(home, *issue, by, label, brief, now),
        Call::BeginUndo { issue } => begin_undo(home, *issue),
        Call::Answer {
            issue,
            turn,
            by,
            fixer,
            text,
        } => answer(home, *issue, *turn, by, *fixer, text.as_deref(), now),
        Call::AnswerUndo { issue, turn } => answer_undo(home, *issue, *turn),
        Call::End(e) => end(home, e, now),
        Call::Posted { issue } => posted(home, *issue),
        Call::Close { issue } => close(home, *issue, now),
        Call::Show { issue } => show(home, *issue),
        Call::Note { issue, kind, id } => note(home, *issue, kind, *id, now),
    }
}

const PLAN_NAMES: [&str; 15] = [
    "issue",
    "session",
    "turns",
    "pending?",
    "pending_outcome",
    "failed?",
    "running?",
    "started_at",
    "comment_id",
    "lost?",
    "by",
    "label",
    "brief",
    "turn_rc",
    "opened_at",
];

/// The planning object of the answer, or `null`.
pub fn planning_json(c: &Connection, issue: i64) -> Result<String, Fail> {
    let row = c
        .query_row(
            "SELECT issue, session, turns, pending, pending_outcome, failed, running, started_at,
               comment_id, lost, by_account, label, brief, turn_rc, opened_at, ended_at,
               options_json
             FROM planning WHERE issue = ?1",
            [issue],
            |r| {
                let mut p = row_json(r, &PLAN_NAMES)?;
                p.push(("ended_at", j(r.get::<_, Option<i64>>(15)?)));
                Ok((p, r.get::<_, Option<String>>(16)?))
            },
        )
        .optional()
        .q()?;
    let Some((mut p, options)) = row else {
        return Ok("null".to_string());
    };
    let options = options
        .and_then(|o| serde_json::from_str::<Value>(&o).ok())
        .unwrap_or(Value::Array(Vec::new()));
    p.insert(3, ("options", options.to_string()));
    let mut q = c
        .prepare(
            "SELECT turn, by_account, fixer FROM planning_answers WHERE issue = ?1 ORDER BY turn",
        )
        .q()?;
    let answers: Vec<String> = q
        .query_map([issue], |r| row_json(r, &["turn", "by", "fixer?"]))
        .q()?
        .map(|r| r.map(|p| obj(&p)))
        .collect::<Result<_, _>>()
        .q()?;
    p.push(("answers", list(&answers)));
    let mut q = c
        .prepare("SELECT url FROM planning_filed WHERE issue = ?1 ORDER BY at, url")
        .q()?;
    let filed: Vec<String> = q
        .query_map([issue], |r| r.get::<_, String>(0))
        .q()?
        .map(|r| r.map(j))
        .collect::<Result<_, _>>()
        .q()?;
    p.push(("filed", list(&filed)));
    // The answer of the turn running (turns + 1), or of the one whose outcome
    // waits to be posted (turns).
    let answer: Option<String> = c
        .query_row(
            "SELECT a.text FROM planning p JOIN planning_answers a ON a.issue = p.issue
             WHERE p.issue = ?1 AND p.ended_at IS NULL
               AND ((p.running = 1 AND a.turn = p.turns + 1)
                 OR (p.running = 0 AND p.pending = 1 AND a.turn = p.turns))",
            [issue],
            |r| r.get(0),
        )
        .optional()
        .q()?
        .flatten();
    p.push(("answer", j(answer)));
    let mut notes = Vec::new();
    for (kind, key) in NOTES {
        let mut q = c
            .prepare(
                "SELECT comment_id FROM planning_notes WHERE issue = ?1 AND kind = ?2
                 ORDER BY at, rowid",
            )
            .q()?;
        let ids: Vec<i64> = q
            .query_map(params![issue, kind], |r| r.get(0))
            .q()?
            .collect::<Result<_, _>>()
            .q()?;
        notes.push((key, j(ids)));
    }
    p.push(("notes", obj(&notes)));
    Ok(obj(&p))
}

/// The open session's `(turns, running, session, lost, comment_id, started_at)`.
struct Open {
    turns: i64,
    running: bool,
    session: Option<String>,
    lost: bool,
    comment_id: Option<i64>,
    started_at: Option<i64>,
}

fn open(c: &Connection, issue: i64) -> Result<Option<Open>, Fail> {
    c.query_row(
        "SELECT turns, running, session, lost, comment_id, started_at FROM planning
         WHERE issue = ?1 AND ended_at IS NULL",
        [issue],
        |r| {
            Ok(Open {
                turns: r.get(0)?,
                running: r.get(1)?,
                session: r.get(2)?,
                lost: r.get(3)?,
                comment_id: r.get(4)?,
                started_at: r.get(5)?,
            })
        },
    )
    .optional()
    .q()
}

fn need_open(c: &Connection, issue: i64) -> Result<Open, Fail> {
    open(c, issue)?.ok_or_else(|| refuse("noplan", issue, "no open planning session"))
}

/// The issue's unanswered questions of `kind`, withdrawn.
fn withdraw(c: &Connection, issue: i64, kind: &str) -> Result<(), Fail> {
    c.execute(
        "DELETE FROM questions WHERE issue = ?1 AND kind = ?2 AND answered_at IS NULL",
        params![issue, kind],
    )
    .q()
    .map(|_| ())
}

/// The notes go with the session that ends; a replaced one takes them with
/// its row (ON DELETE CASCADE).
fn drop_notes(c: &Connection, issue: i64) -> Result<(), Fail> {
    c.execute("DELETE FROM planning_notes WHERE issue = ?1", [issue])
        .q()
        .map(|_| ())
}

/// `plan-note`: a comment the poll spoke about once in the open session.
fn note(home: &Path, issue: i64, kind: &str, id: i64, now: i64) -> Result<String, Fail> {
    verbs::write(home, |c| {
        need_open(c, issue)?;
        let n = c
            .execute(
                "INSERT OR IGNORE INTO planning_notes (issue, kind, comment_id, at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![issue, kind, id, now],
            )
            .q()?;
        Ok(line(
            "plan-note",
            &[("said", j(n > 0)), ("planning", planning_json(c, issue)?)],
        ))
    })
}

fn begin(
    home: &Path,
    issue: i64,
    by: &str,
    label: &str,
    brief: &Path,
    now: i64,
) -> Result<String, Fail> {
    let text = verbs::read_text(brief, BRIEF_BOUND, "1 MiB", issue, "brief", "the brief")?;
    verbs::write(home, |c| {
        stub(c, issue)?;
        let had: Option<(Option<i64>, bool)> = c
            .query_row(
                "SELECT ended_at, running FROM planning WHERE issue = ?1",
                [issue],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .q()?;
        let replaced = match had {
            None => "none",
            Some((None, true)) => {
                return Err(refuse("running", issue, "a planning turn is running"))
            }
            Some((None, false)) => "open",
            Some((Some(_), _)) => "ended",
        };
        withdraw(c, issue, "plan")?;
        withdraw(c, issue, "park")?;
        c.execute("DELETE FROM planning WHERE issue = ?1", [issue])
            .q()?;
        c.execute(
            "INSERT INTO planning (issue, session, turns, options_json, pending, pending_outcome,
               failed, running, started_at, comment_id, lost, by_account, label, brief, turn_rc,
               opened_at, ended_at)
             VALUES (?1, NULL, 0, '[]', 0, NULL, 0, 1, ?2, NULL, 0, ?3, ?4, ?5, NULL, ?2, NULL)",
            params![issue, now, by, label, text],
        )
        .q()?;
        Ok(line(
            "plan-begin",
            &[
                ("planning", planning_json(c, issue)?),
                ("replaced", j(replaced)),
            ],
        ))
    })
}

fn begin_undo(home: &Path, issue: i64) -> Result<String, Fail> {
    verbs::write(home, |c| {
        let o = need_open(c, issue)?;
        if o.turns != 0 || !o.running || o.session.is_some() {
            return Err(refuse("turn", issue, "the session is past its first turn"));
        }
        c.execute("DELETE FROM planning WHERE issue = ?1", [issue])
            .q()?;
        Ok(line("plan-begin", &[("undone", j(true))]))
    })
}

fn answer(
    home: &Path,
    issue: i64,
    turn: i64,
    by: &str,
    fixer: bool,
    text: Option<&Path>,
    now: i64,
) -> Result<String, Fail> {
    let text = match text {
        Some(p) => Some(verbs::read_text(
            p,
            TEXT_BOUND,
            "64 KiB",
            issue,
            "answer",
            "the answer",
        )?),
        None => None,
    };
    verbs::write(home, |c| {
        let o = need_open(c, issue)?;
        if turn != o.turns + 1 || o.running || o.session.is_none() || o.lost {
            return Err(refuse(
                "turn",
                issue,
                &format!(
                    "turn {turn} cannot begin: the session is at turn {}",
                    o.turns
                ),
            ));
        }
        c.execute(
            "INSERT INTO planning_answers (issue, turn, by_account, fixer, text)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![issue, turn, by, fixer, text],
        )
        .q()?;
        c.execute(
            "UPDATE planning SET running = 1, started_at = ?2, pending = 0, comment_id = NULL
             WHERE issue = ?1",
            params![issue, now],
        )
        .q()?;
        if let Some(id) = o.comment_id {
            c.execute(
                "UPDATE questions SET answered_at = ?3, answered_by = ?4
                 WHERE id = ?1 AND issue = ?2 AND kind = 'plan' AND answered_at IS NULL",
                params![id, issue, now, by],
            )
            .q()?;
        }
        Ok(line(
            "plan-answer",
            &[("turn", j(turn)), ("planning", planning_json(c, issue)?)],
        ))
    })
}

fn answer_undo(home: &Path, issue: i64, turn: i64) -> Result<String, Fail> {
    verbs::write(home, |c| {
        let o = need_open(c, issue)?;
        let by: Option<String> = c
            .query_row(
                "SELECT by_account FROM planning_answers WHERE issue = ?1 AND turn = ?2",
                params![issue, turn],
                |r| r.get(0),
            )
            .optional()
            .q()?;
        let Some(by) = by.filter(|_| o.running && o.turns + 1 == turn) else {
            return Err(refuse(
                "turn",
                issue,
                &format!("turn {turn} is not the one begun"),
            ));
        };
        c.execute(
            "DELETE FROM planning_answers WHERE issue = ?1 AND turn = ?2",
            params![issue, turn],
        )
        .q()?;
        let reopened: Option<i64> = c
            .query_row(
                "SELECT id FROM questions WHERE issue = ?1 AND kind = 'plan' AND attempt = ?2
                   AND answered_at = ?3 AND answered_by = ?4
                 ORDER BY posted_at DESC, id DESC LIMIT 1",
                params![issue, o.turns, o.started_at, by],
                |r| r.get(0),
            )
            .optional()
            .q()?;
        if let Some(id) = reopened {
            c.execute(
                "UPDATE questions SET answered_at = NULL, answered_by = NULL WHERE id = ?1",
                [id],
            )
            .q()?;
        }
        c.execute(
            "UPDATE planning SET running = 0, comment_id = ?2 WHERE issue = ?1",
            params![issue, reopened],
        )
        .q()?;
        Ok(line(
            "plan-answer",
            &[("turn", j(turn)), ("planning", planning_json(c, issue)?)],
        ))
    })
}

fn end(home: &Path, e: &TurnEnd, now: i64) -> Result<String, Fail> {
    let issue = e.issue;
    let out = match &e.out {
        None => None,
        Some(p) => Some(
            p.strip_prefix(home)
                .ok()
                .and_then(Path::to_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .ok_or((
                    64,
                    format!("usage plan-end: --out is not under {}", home.display()),
                ))?,
        ),
    };
    verbs::write(home, |c| {
        let o = need_open(c, issue)?;
        if !o.running || e.turn != o.turns + 1 {
            return Err(refuse(
                "turn",
                issue,
                &format!("turn {} is not running", e.turn),
            ));
        }
        let urls: Vec<&str> = e.filed.iter().map(|(u, _)| u.as_str()).collect();
        c.execute(
            "INSERT INTO plan_history (issue, turn, at, outcome, session, started_at, rc,
               by_account, label, filed_json)
             SELECT issue, ?2, ?3, ?4, coalesce(?5, session), started_at, ?6, by_account, label, ?7
             FROM planning WHERE issue = ?1",
            params![issue, e.turn, now, e.outcome, e.session, e.rc, j(urls)],
        )
        .q()?;
        let history_id = c.last_insert_rowid();
        c.execute(
            "DELETE FROM plan_history WHERE issue = ?1 AND id NOT IN
               (SELECT id FROM plan_history WHERE issue = ?1 ORDER BY at DESC, id DESC LIMIT 50)",
            [issue],
        )
        .q()?;
        if e.outcome == "lost" {
            c.execute(
                "UPDATE planning SET lost = 1, turn_rc = ?2 WHERE issue = ?1",
                params![issue, e.rc],
            )
            .q()?;
        } else if e.outcome == "finished" && e.session.is_some() {
            c.execute(
                "UPDATE planning SET session = ?2, turns = ?3, options_json = ?4, pending = 1,
                   pending_outcome = ?5, failed = 0, running = 0, turn_rc = 0
                 WHERE issue = ?1",
                params![issue, e.session, e.turn, j(e.options.clone()), out],
            )
            .q()?;
            for (url, n) in &e.filed {
                c.execute(
                    "INSERT OR IGNORE INTO planning_filed (issue, url, filed_issue, at)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![issue, url, n, now],
                )
                .q()?;
            }
        } else if e.session.is_some() || o.session.is_some() {
            c.execute(
                "UPDATE planning SET session = coalesce(?2, session), turns = ?3,
                   options_json = '[]', pending = 1, pending_outcome = ?4, failed = 1,
                   running = 0, turn_rc = ?5
                 WHERE issue = ?1",
                params![issue, e.session, e.turn, out, e.rc],
            )
            .q()?;
        } else {
            c.execute(
                "UPDATE planning SET turns = ?2, running = 0, turn_rc = ?3, ended_at = ?4
                 WHERE issue = ?1",
                params![issue, e.turn, e.rc, now],
            )
            .q()?;
            drop_notes(c, issue)?;
        }
        Ok(line(
            "plan-end",
            &[
                ("planning", planning_json(c, issue)?),
                ("outcome", j(e.outcome)),
                ("history_id", j(history_id)),
                ("cleaned", e.cleaned.json()),
            ],
        ))
    })
}

fn posted(home: &Path, issue: i64) -> Result<String, Fail> {
    verbs::write(home, |c| {
        let has = open(c, issue)?.is_some();
        if has {
            c.execute(
                "UPDATE planning SET pending = 0, pending_outcome = NULL
                 WHERE issue = ?1 AND ended_at IS NULL",
                [issue],
            )
            .q()?;
        }
        let p = if has {
            planning_json(c, issue)?
        } else {
            "null".to_string()
        };
        Ok(line("plan-end", &[("planning", p)]))
    })
}

fn close(home: &Path, issue: i64, now: i64) -> Result<String, Fail> {
    verbs::write(home, |c| {
        if open(c, issue)?.is_none() {
            return Ok(line("plan-end", &[("closed", j(false))]));
        }
        c.execute(
            "UPDATE planning SET ended_at = ?2, running = 0, pending = 0, pending_outcome = NULL,
               comment_id = NULL
             WHERE issue = ?1",
            params![issue, now],
        )
        .q()?;
        withdraw(c, issue, "plan")?;
        drop_notes(c, issue)?;
        Ok(line(
            "plan-end",
            &[("closed", j(true)), ("planning", planning_json(c, issue)?)],
        ))
    })
}

fn show(home: &Path, issue: Option<i64>) -> Result<String, Fail> {
    verbs::look(home, |c| match issue {
        Some(issue) => Ok(line("plan-show", &[("planning", planning_json(c, issue)?)])),
        None => {
            let mut q = c
                .prepare("SELECT issue FROM planning WHERE ended_at IS NULL ORDER BY issue")
                .q()?;
            let issues: Vec<i64> = q
                .query_map([], |r| r.get(0))
                .q()?
                .collect::<Result<_, _>>()
                .q()?;
            let sessions = issues
                .iter()
                .map(|i| planning_json(c, *i))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(line("plan-show", &[("sessions", list(&sessions))]))
        }
    })
}

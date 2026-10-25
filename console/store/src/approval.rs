//! The approval verbs: approval-record and approval-current. An issue has at
//! most one unused approval; the current one is the used row with the highest
//! `used_by`. Order never depends on `at`, which mixes GitHub's clock with the
//! host's.

use crate::verbs::{self, j, line, obj, refuse, row_json, stub, Args, Fail, Kind, Q};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub enum Call {
    Record(Box<Record>),
    Undo { issue: i64, at: i64 },
    Current { issue: i64 },
}

#[derive(Debug, PartialEq, Eq)]
pub struct Record {
    pub issue: i64,
    pub at: i64,
    pub by: String,
    pub how: &'static str,
    pub label: String,
    pub body_sha: String,
    pub title_sha: Option<String>,
    pub comment_id: Option<i64>,
    pub refresh: Option<(i64, String)>,
    pub input: Option<PathBuf>,
}

/// The hows `approval-record` takes. `fixer` is the old name of `millwright`:
/// the shell still sends it until part 5 of #686, and a record given it is
/// written as `millwright`.
pub const HOWS: [&str; 6] = [
    "label",
    "refresh",
    "retry-tick",
    "outage-retry",
    "millwright",
    "fixer",
];

/// The word a millwright's start is written as, whatever spelling it was given.
pub const MILLWRIGHT: &str = "millwright";

/// Whether `how` names the millwright's start, in either spelling.
pub fn is_millwright(how: &str) -> bool {
    how == MILLWRIGHT || how == "fixer"
}

/// The keys `--input-file` may hold: what the cycle is given with the
/// approval. Each list is a JSON array and `refresh_of` an object or null; the
/// readers check the entries.
const INPUT_LISTS: [&str; 3] = ["links", "comment_links", "retry_context"];
const INPUT_BOUND: u64 = 1 << 20;

/// The kinds of approval a person gave, which a fixer or refresh start rests on.
const BASES: &str = "('label', 'retry-tick', 'outage-retry')";

pub fn parse(name: &str, rest: &[String]) -> Option<Result<Call, String>> {
    Some(match name {
        "approval-record" => record_args(rest),
        "approval-current" => Args::parse("approval-current", rest, &[], true)
            .and_then(|a| Ok(Call::Current { issue: a.issue()? })),
        _ => return None,
    })
}

fn approval_label(s: &str) -> Option<String> {
    let t = s.strip_prefix("tehas:")?;
    let b = t.as_bytes();
    let ok = !b.is_empty()
        && b.len() <= 40
        && b[0].is_ascii_lowercase()
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-');
    ok.then(|| s.to_string())
}

fn record_args(rest: &[String]) -> Result<Call, String> {
    let a = Args::parse(
        "approval-record",
        rest,
        &[
            ("--at", Kind::Value),
            ("--by", Kind::Value),
            ("--how", Kind::Value),
            ("--label", Kind::Value),
            ("--body-sha", Kind::Value),
            ("--title-sha", Kind::Value),
            ("--comment-id", Kind::Value),
            ("--refresh-pr", Kind::Value),
            ("--refresh-head", Kind::Value),
            ("--input-file", Kind::Value),
            ("--undo", Kind::Switch),
        ],
        true,
    )?;
    let issue = a.issue()?;
    let at = a.strict_need("--at", "a Unix time", verbs::unix)?;
    if a.has("--undo") {
        a.only(&["--undo", "--at"], "--undo")?;
        return Ok(Call::Undo { issue, at });
    }
    a.need("--how")?;
    let given = a.word("--how", &HOWS)?.expect("--how was given");
    let how = if is_millwright(given) {
        MILLWRIGHT
    } else {
        given
    };
    let by = a.strict_need("--by", "an account", verbs::account)?;
    let body_sha = a.strict_need("--body-sha", "a sha256", verbs::sha256)?;
    let title_sha = a.strict("--title-sha", "a sha256", verbs::sha256)?;
    let comment_id = a.strict("--comment-id", "a comment id", verbs::comment_id)?;
    let pr = a.strict("--refresh-pr", "a number", verbs::number)?;
    let head = a.strict("--refresh-head", "a head sha", verbs::head_sha)?;
    let input = a.path("--input-file")?;
    let refresh = match (pr, head) {
        (Some(pr), Some(head)) => Some((pr, head)),
        (None, None) => None,
        (Some(_), None) => return Err(a.usage("--refresh-head", "is required with --refresh-pr")),
        (None, Some(_)) => return Err(a.usage("--refresh-pr", "is required with --refresh-head")),
    };
    if (how == "refresh") != refresh.is_some() {
        return Err(a.usage(
            "--refresh-pr",
            "and --refresh-head are given exactly with --how refresh",
        ));
    }
    let labelled = match how {
        "label" => true,
        // A millwright's start is recorded on the person's label, in either
        // spelling of the word.
        "millwright" => true,
        _ => false,
    };
    let label = match a.value("--label") {
        Some(v) => Some(approval_label(v).ok_or_else(|| {
            a.usage(
                "--label",
                &format!("is not a tehas:* label: {}", verbs::shown(v)),
            )
        })?),
        None => None,
    };
    let label = match label {
        Some(l) if labelled => l,
        None if !labelled => how.to_string(),
        Some(_) => return Err(a.usage("--label", &format!("is not taken with --how {how}"))),
        None => return Err(a.usage("--label", &format!("is required with --how {how}"))),
    };
    Ok(Call::Record(Box::new(Record {
        issue,
        at,
        by,
        how,
        label,
        body_sha,
        title_sha,
        comment_id,
        refresh,
        input,
    })))
}

/// The inputs as stored: the file's JSON object, with only the four keys and
/// each of its type, written again compactly.
fn read_input(path: &Path, issue: i64) -> Result<String, Fail> {
    let text = verbs::read_text(path, INPUT_BOUND, "1 MiB", issue, "input", "the input")?;
    let bad = |why: &str| refuse("input", issue, &format!("the input {why}"));
    let v: Value = serde_json::from_str(&text).map_err(|_| bad("is not JSON"))?;
    let Value::Object(map) = &v else {
        return Err(bad("is not a JSON object"));
    };
    for (key, value) in map {
        let fits = if INPUT_LISTS.contains(&key.as_str()) {
            value.is_array()
        } else if key == "refresh_of" {
            value.is_object() || value.is_null()
        } else {
            return Err(bad(&format!(
                "holds a key it may not: {}",
                verbs::shown(key)
            )));
        };
        if !fits {
            return Err(bad(&format!("holds {key} of another type")));
        }
    }
    Ok(v.to_string())
}

pub fn run(home: &Path, call: &Call) -> Result<String, Fail> {
    match call {
        Call::Record(r) => record(home, r),
        Call::Undo { issue, at } => undo(home, *issue, *at),
        Call::Current { issue } => current(home, *issue),
    }
}

const COLUMNS: &str = "issue, at, by_account, how, label, body_sha256, title_sha256, comment_id,
  refresh_pr, refresh_head, used_at, used_by, input_json";
const NAMES: [&str; 12] = [
    "issue",
    "at",
    "by",
    "how",
    "label",
    "body_sha256",
    "title_sha256",
    "comment_id",
    "refresh_pr",
    "refresh_head",
    "used_at",
    "used_by",
];

fn approval_json(
    c: &Connection,
    filter: &str,
    args: impl rusqlite::Params,
) -> Result<String, Fail> {
    let sql = format!("SELECT {COLUMNS} FROM approvals WHERE {filter} LIMIT 1");
    let row = c
        .query_row(&sql, args, |r| {
            Ok((
                row_json(r, &NAMES)?,
                r.get::<_, Option<String>>(NAMES.len())?,
            ))
        })
        .optional()
        .q()?;
    let Some((mut p, input)) = row else {
        return Ok("null".to_string());
    };
    // Written by this store as JSON; one that does not parse is damage.
    let input = match input {
        None => "null".to_string(),
        Some(t) => serde_json::from_str::<Value>(&t)
            .map_err(|_| {
                crate::line(crate::db::DbError::Corrupt(
                    "an approval's input_json is not JSON".to_string(),
                ))
            })?
            .to_string(),
    };
    p.push(("input", input));
    Ok(obj(&p))
}

fn record(home: &Path, r: &Record) -> Result<String, Fail> {
    let input = match &r.input {
        Some(p) => Some(read_input(p, r.issue)?),
        None => None,
    };
    verbs::write(home, |c| {
        stub(c, r.issue)?;
        let used: bool = c
            .query_row(
                "SELECT count(*) > 0 FROM approvals WHERE issue = ?1 AND at = ?2 AND used_at IS NOT NULL",
                params![r.issue, r.at],
                |x| x.get(0),
            )
            .q()?;
        if used {
            return Err(refuse(
                "used",
                r.issue,
                &format!("the approval at {} has started a cycle", r.at),
            ));
        }
        c.execute(
            "DELETE FROM approvals WHERE issue = ?1 AND used_at IS NULL AND at <> ?2",
            params![r.issue, r.at],
        )
        .q()?;
        let (pr, head) = match &r.refresh {
            Some((pr, head)) => (Some(*pr), Some(head.as_str())),
            None => (None, None),
        };
        c.execute(
            "INSERT INTO approvals (issue, at, by_account, how, label, body_sha256, title_sha256,
               comment_id, refresh_pr, refresh_head, used_at, used_by, input_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL, NULL, ?11)
             ON CONFLICT (issue, at) DO UPDATE SET by_account = excluded.by_account,
               how = excluded.how, label = excluded.label, body_sha256 = excluded.body_sha256,
               title_sha256 = excluded.title_sha256, comment_id = excluded.comment_id,
               refresh_pr = excluded.refresh_pr, refresh_head = excluded.refresh_head,
               used_at = NULL, used_by = NULL, input_json = excluded.input_json",
            params![
                r.issue,
                r.at,
                r.by,
                r.how,
                r.label,
                r.body_sha,
                r.title_sha,
                r.comment_id,
                pr,
                head,
                input
            ],
        )
        .q()?;
        Ok(line(
            "approval-record",
            &[(
                "approval",
                approval_json(c, "issue = ?1 AND at = ?2", params![r.issue, r.at])?,
            )],
        ))
    })
}

fn undo(home: &Path, issue: i64, at: i64) -> Result<String, Fail> {
    verbs::write(home, |c| {
        let n = c
            .execute(
                "DELETE FROM approvals WHERE issue = ?1 AND at = ?2 AND used_at IS NULL",
                params![issue, at],
            )
            .q()?;
        Ok(line("approval-record", &[("undone", j(n > 0))]))
    })
}

/// The person's approval the current one rests on: `(at, by, how, body_sha256,
/// used_by)`, from used rows only.
type Base = (i64, String, String, String, i64);

fn current_used_by(c: &Connection, issue: i64) -> Result<Option<(i64, String)>, Fail> {
    c.query_row(
        "SELECT used_by, how FROM approvals WHERE issue = ?1 AND used_by IS NOT NULL
         ORDER BY used_by DESC LIMIT 1",
        [issue],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()
    .q()
}

fn rests_on(c: &Connection, issue: i64) -> Result<Option<Base>, Fail> {
    let Some((top, how)) = current_used_by(c, issue)? else {
        return Ok(None);
    };
    let below = if is_millwright(&how) || how == "refresh" {
        top - 1
    } else {
        top
    };
    c.query_row(
        &format!(
            "SELECT at, by_account, how, body_sha256, used_by FROM approvals
             WHERE issue = ?1 AND used_by IS NOT NULL AND used_by <= ?2 AND how IN {BASES}
             ORDER BY used_by DESC LIMIT 1"
        ),
        params![issue, below],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )
    .optional()
    .q()
}

/// Run by cycle-start after it used an approval: what stays is what
/// approval-current reads, the base and the starts on it.
pub fn retain(c: &Connection, issue: i64) -> Result<(), Fail> {
    let keep_from = match rests_on(c, issue)? {
        Some(base) => base.4,
        None => match current_used_by(c, issue)? {
            Some((top, _)) => top,
            None => return Ok(()),
        },
    };
    c.execute(
        "DELETE FROM approvals WHERE issue = ?1 AND used_by IS NOT NULL AND used_by < ?2",
        params![issue, keep_from],
    )
    .q()?;
    Ok(())
}

fn current(home: &Path, issue: i64) -> Result<String, Fail> {
    verbs::look(home, |c| {
        let unused = approval_json(
            c,
            "issue = ?1 AND used_at IS NULL ORDER BY at DESC",
            [issue],
        )?;
        let cur = approval_json(
            c,
            "issue = ?1 AND used_by IS NOT NULL ORDER BY used_by DESC",
            [issue],
        )?;
        let base = rests_on(c, issue)?;
        let floor = base.as_ref().map_or(0, |b| b.4);
        let starts = |how: &str| -> Result<i64, Fail> {
            c.query_row(
                "SELECT count(*) FROM approvals WHERE issue = ?1 AND how = ?2 AND used_by > ?3",
                params![issue, how, floor],
                |r| r.get(0),
            )
            .q()
        };
        // An old row still says `fixer`, so a millwright start is either word.
        let millwright_starts = starts(MILLWRIGHT)? + starts("fixer")?;
        let refreshes = starts("refresh")?;
        let base = base.map_or("null".to_string(), |(at, by, how, sha, used_by)| {
            obj(&[
                ("at", j(at)),
                ("by", j(by)),
                ("how", j(how)),
                ("body_sha256", j(sha)),
                ("used_by", j(used_by)),
            ])
        });
        Ok(line(
            "approval-current",
            &[
                ("approval", unused),
                ("current", cur),
                ("rests_on", base),
                ("millwright_starts", j(millwright_starts)),
                // The old key, the same number, until part 5 moves the shell's
                // readers to the new one.
                ("fixer_starts", j(millwright_starts)),
                ("refreshes", j(refreshes)),
            ],
        ))
    })
}

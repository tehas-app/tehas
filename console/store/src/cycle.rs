//! The cycle verbs: cycle-start, cycle-image, cycle-end, cycle-show,
//! cycle-note and cycle-stop-asked.

use crate::approval;
use crate::lock::{self, Lock};
use crate::verbs::{
    self, cap_char, j, kebab_char, line, list, obj, refuse, row_json, stub, word_char, Args,
    Cleaned, Fail, FileErr, Kind, Q,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub enum Call {
    Start {
        issue: i64,
        rukas: Option<String>,
        by_hand: bool,
    },
    Image {
        issue: i64,
        attempt: i64,
        image: String,
        rukas: Option<String>,
        settings: Option<PathBuf>,
    },
    End {
        issue: i64,
        attempt: i64,
        end: End,
    },
    Show {
        issue: Option<i64>,
    },
    Note {
        issue: i64,
        said_for: String,
    },
    StopAsked {
        issue: i64,
        said_only: bool,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum End {
    Report(Box<Report>),
    Refused,
    Stopped,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub status: String,
    pub step: Option<String>,
    pub rc: Option<i64>,
    pub runner_exit: Option<i64>,
    pub watched: bool,
    pub host_saw: Option<&'static str>,
    pub oom: Option<i64>,
    pub forks: Option<i64>,
    pub stalled: Option<i64>,
    pub timed: bool,
    pub branch: Option<String>,
    pub pr: Option<i64>,
    pub result: Option<PathBuf>,
    pub progress: Option<PathBuf>,
    pub cap: Option<String>,
    pub park_reason: Option<String>,
    pub failure_kind: Option<String>,
    pub failure_cause: Option<String>,
    pub provider: bool,
    pub parked: bool,
    pub never_ran: bool,
    pub cleaned: Cleaned,
}

/// What `cycles.end_reason` can hold. A status word outside it ends as
/// `unknown`.
pub const END_REASONS: [&str; 12] = [
    "closed",
    "merged",
    "handoff",
    "parked",
    "failed",
    "stopped",
    "timeout",
    "killed",
    "lost",
    "interrupted",
    "stalled",
    "unknown",
];

const RESULT_BOUND: u64 = 16 << 20;
const PROGRESS_BOUND: u64 = 1 << 20;
const SETTINGS_BOUND: u64 = 64 << 10;
const SETTING_VALUE_MAX: usize = 4096;

const END_REPORT: [(&str, Kind); 23] = [
    ("--attempt", Kind::Value),
    ("--status", Kind::Value),
    ("--step", Kind::Value),
    ("--rc", Kind::Value),
    ("--runner-exit", Kind::Value),
    ("--watched", Kind::Switch),
    ("--host-saw", Kind::Value),
    ("--oom", Kind::Value),
    ("--forks", Kind::Value),
    ("--stalled", Kind::Value),
    ("--timed", Kind::Switch),
    ("--branch", Kind::Value),
    ("--pr", Kind::Value),
    ("--result", Kind::Value),
    ("--progress", Kind::Value),
    ("--cap", Kind::Value),
    ("--park-reason", Kind::Value),
    ("--failure-kind", Kind::Value),
    ("--failure-cause", Kind::Value),
    ("--provider", Kind::Switch),
    ("--parked", Kind::Switch),
    ("--never-ran", Kind::Switch),
    ("--refused", Kind::Switch),
];

pub fn parse(name: &str, rest: &[String]) -> Option<Result<Call, String>> {
    Some(match name {
        "cycle-start" => start_args(rest),
        "cycle-image" => image_args(rest),
        "cycle-end" => end_args(rest),
        "cycle-show" => Args::parse("cycle-show", rest, &[("--running", Kind::Switch)], true)
            .and_then(|a| {
                Ok(Call::Show {
                    issue: a.target("--running")?,
                })
            }),
        "cycle-note" => note_args(rest),
        "cycle-stop-asked" => Args::parse(
            "cycle-stop-asked",
            rest,
            &[("--said-only", Kind::Switch)],
            true,
        )
        .and_then(|a| {
            Ok(Call::StopAsked {
                issue: a.issue()?,
                said_only: a.has("--said-only"),
            })
        }),
        _ => return None,
    })
}

fn start_args(rest: &[String]) -> Result<Call, String> {
    let a = Args::parse(
        "cycle-start",
        rest,
        &[("--rukas", Kind::Value), ("--by-hand", Kind::Switch)],
        true,
    )?;
    Ok(Call::Start {
        issue: a.issue()?,
        rukas: a.strict("--rukas", "a commit", verbs::commit)?,
        by_hand: a.has("--by-hand"),
    })
}

fn image_args(rest: &[String]) -> Result<Call, String> {
    let a = Args::parse(
        "cycle-image",
        rest,
        &[
            ("--attempt", Kind::Value),
            ("--image", Kind::Value),
            ("--rukas", Kind::Value),
            ("--settings", Kind::Value),
        ],
        true,
    )?;
    Ok(Call::Image {
        issue: a.issue()?,
        attempt: a.strict_need("--attempt", "a number", verbs::number)?,
        image: a.strict_need("--image", "an image id", verbs::image)?,
        rukas: a.strict("--rukas", "a commit", verbs::commit)?,
        settings: a.path("--settings")?,
    })
}

fn end_args(rest: &[String]) -> Result<Call, String> {
    let mut flags = END_REPORT.to_vec();
    flags.push(("--stopped", Kind::Switch));
    let a = Args::parse("cycle-end", rest, &flags, true)?;
    let issue = a.issue()?;
    let attempt = a.strict_need("--attempt", "a number", verbs::number)?;
    if a.has("--refused") && a.has("--stopped") {
        return Err(a.usage("--stopped", "is not taken with --refused"));
    }
    if a.has("--refused") {
        a.only(&["--attempt", "--refused"], "--refused")?;
        return Ok(Call::End {
            issue,
            attempt,
            end: End::Refused,
        });
    }
    if a.has("--stopped") {
        a.only(&["--attempt", "--stopped"], "--stopped")?;
        return Ok(Call::End {
            issue,
            attempt,
            end: End::Stopped,
        });
    }
    let mut c = Cleaned::default();
    let status = c
        .word("--status", Some(a.need("--status")?), word_char, 40)
        .unwrap_or_else(|| "unknown".to_string());
    let branch_ok = |b: &str| branch(issue, b);
    let r = Report {
        status,
        step: c.word("--step", a.value("--step"), word_char, 40),
        rc: a.strict("--rc", "an exit status", verbs::count)?,
        runner_exit: a.strict("--runner-exit", "an exit status", verbs::count)?,
        watched: a.has("--watched"),
        host_saw: a.word("--host-saw", &["killed"])?,
        oom: a.strict("--oom", "a count", verbs::count)?,
        forks: a.strict("--forks", "a count", verbs::count)?,
        stalled: a.strict("--stalled", "a count", verbs::count)?,
        timed: a.has("--timed"),
        branch: c.whole("--branch", a.value("--branch"), branch_ok),
        pr: a.strict("--pr", "a number", verbs::number)?,
        result: a.path("--result")?,
        progress: a.path("--progress")?,
        cap: c.word("--cap", a.value("--cap"), cap_char, 80),
        park_reason: c.word("--park-reason", a.value("--park-reason"), kebab_char, 40),
        failure_kind: c.word("--failure-kind", a.value("--failure-kind"), kebab_char, 40),
        failure_cause: c.word("--failure-cause", a.value("--failure-cause"), cap_char, 60),
        provider: a.has("--provider"),
        parked: a.has("--parked"),
        never_ran: a.has("--never-ran"),
        cleaned: c,
    };
    Ok(Call::End {
        issue,
        attempt,
        end: End::Report(Box::new(r)),
    })
}

fn note_args(rest: &[String]) -> Result<Call, String> {
    let a = Args::parse(
        "cycle-note",
        rest,
        &[("--kind", Kind::Value), ("--said-for", Kind::Value)],
        true,
    )?;
    let issue = a.issue()?;
    a.need("--kind")?;
    a.word("--kind", &["quiet"])?;
    let said_for = a.strict_need("--said-for", "a note key", |s| {
        let ok = (1..=64).contains(&s.len()) && s.bytes().all(cap_char);
        ok.then(|| s.to_string())
    })?;
    Ok(Call::Note { issue, said_for })
}

/// The branch of this issue as pi-rukas names it, or a name of the shape a
/// container's own branch may have.
pub fn branch(issue: i64, b: &str) -> bool {
    let tail_ok = |t: &str| (1..=80).contains(&t.len()) && t.bytes().all(word_char);
    if let Some(rest) = b.strip_prefix(&format!("feature/issue-{issue}")) {
        if rest.is_empty() || rest.strip_prefix('-').is_some_and(tail_ok) {
            return true;
        }
    }
    let bytes = b.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 80
        && bytes[0].is_ascii_alphanumeric()
        && bytes.iter().all(|&c| word_char(c) || c == b'/')
}

pub fn run(home: &Path, call: &Call, now: i64) -> Result<String, Fail> {
    match call {
        Call::Start {
            issue,
            rukas,
            by_hand,
        } => start(home, *issue, rukas.as_deref(), *by_hand, now),
        Call::Image {
            issue,
            attempt,
            image,
            rukas,
            settings,
        } => cycle_image(
            home,
            *issue,
            *attempt,
            image,
            rukas.as_deref(),
            settings.as_deref(),
        ),
        Call::End {
            issue,
            attempt,
            end,
        } => match end {
            End::Report(r) => end_report(home, *issue, *attempt, r, now),
            End::Refused | End::Stopped => end_short(home, *issue, *attempt, end, now),
        },
        Call::Show { issue } => show(home, *issue, now),
        Call::Note { issue, said_for } => note(home, *issue, said_for, now),
        Call::StopAsked { issue, said_only } => stop_asked(home, *issue, *said_only, now),
    }
}

/// What the verbs decide on from a cycle row.
pub struct Row {
    pub attempt: i64,
    pub started_at: i64,
    pub started_by: Option<String>,
    pub how: String,
    pub ended_at: Option<i64>,
    pub stop_asked_at: Option<i64>,
    pub stop_said_times: i64,
}

pub fn row(conn: &Connection, issue: i64) -> Result<Option<Row>, Fail> {
    conn.query_row(
        "SELECT attempt, started_at, started_by, how, ended_at, stop_asked_at, stop_said_times
         FROM cycles WHERE issue = ?1",
        [issue],
        |r| {
            Ok(Row {
                attempt: r.get(0)?,
                started_at: r.get(1)?,
                started_by: r.get(2)?,
                how: r.get(3)?,
                ended_at: r.get(4)?,
                stop_asked_at: r.get(5)?,
                stop_said_times: r.get(6)?,
            })
        },
    )
    .optional()
    .q()
}

const CYCLE_COLUMNS: [&str; 20] = [
    "issue",
    "attempt",
    "started_at",
    "started_by",
    "how",
    "requeued?",
    "rukas_commit",
    "image",
    "branch",
    "end_reason",
    "ended_at",
    "oom_kills",
    "forks_refused",
    "stalled_seconds",
    "timed_out?",
    "runner_exit",
    "watched?",
    "host_saw",
    "stop_asked_at",
    "stop_said_times",
];

/// The cycle object of the answer, or `null`.
pub fn cycle_json(conn: &Connection, issue: i64) -> Result<String, Fail> {
    let sql = format!(
        "SELECT {} FROM cycles WHERE issue = ?1",
        CYCLE_COLUMNS
            .iter()
            .map(|c| c.trim_end_matches('?'))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let Some(mut pairs) = conn
        .query_row(&sql, [issue], |r| row_json(r, &CYCLE_COLUMNS))
        .optional()
        .q()?
    else {
        return Ok("null".to_string());
    };
    let mut q = conn
        .prepare("SELECT key, value FROM cycle_settings WHERE issue = ?1 ORDER BY key")
        .q()?;
    let settings: Vec<(String, String)> = q
        .query_map([issue], |r| Ok((r.get(0)?, r.get::<_, String>(1)?)))
        .q()?
        .collect::<Result<_, _>>()
        .q()?;
    let settings: Vec<(&str, String)> = settings
        .iter()
        .map(|(k, v)| (k.as_str(), j(v.as_str())))
        .collect();
    pairs.push(("settings", obj(&settings)));
    let quiet = conn
        .query_row(
            "SELECT said_for, at FROM cycle_notes WHERE issue = ?1 AND kind = 'quiet'
             ORDER BY at DESC LIMIT 1",
            [issue],
            |r| row_json(r, &["said_for", "at"]),
        )
        .optional()
        .q()?;
    pairs.push(("quiet", quiet.map_or("null".to_string(), |p| obj(&p))));
    Ok(obj(&pairs))
}

/// The probe as a precondition: `want` is the rc the verb needs.
fn locked(home: &Path, issue: i64, open_row: bool, want: u8) -> Result<(), Fail> {
    let l = lock::probe(home, issue, open_row);
    if l.rc == want {
        return Ok(());
    }
    Err(match l.rc {
        2 => refuse("probe", issue, l.why.as_deref().unwrap_or("could not tell")),
        0 => refuse("unlocked", issue, "the cycle lock is not held"),
        _ => refuse("running", issue, "the cycle lock is held"),
    })
}

fn start(
    home: &Path,
    issue: i64,
    rukas: Option<&str>,
    by_hand: bool,
    now: i64,
) -> Result<String, Fail> {
    verbs::write(home, |c| {
        stub(c, issue)?;
        let prev = row(c, issue)?;
        let unended = prev.as_ref().is_some_and(|r| r.ended_at.is_none());
        locked(home, issue, unended, 1)?;
        let approval = if by_hand {
            None
        } else {
            let a = c
                .query_row(
                    "SELECT at, by_account, how FROM approvals
                     WHERE issue = ?1 AND used_at IS NULL ORDER BY at DESC LIMIT 1",
                    [issue],
                    |r| {
                        Ok((
                            r.get::<_, i64>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()
                .q()?;
            Some(a.ok_or_else(|| refuse("unapproved", issue, "no unused approval"))?)
        };
        let attempt: i64 = c
            .query_row(
                "SELECT 1 + max(
                   coalesce((SELECT max(attempt) FROM cycles WHERE issue = ?1), 0),
                   coalesce((SELECT max(attempt) FROM cycle_history WHERE issue = ?1), 0))",
                [issue],
                |r| r.get(0),
            )
            .q()?;
        let (by, how, requeued) = match &approval {
            Some((_, by, how)) => (Some(by.clone()), how.clone(), how == "outage-retry"),
            None => (None, "hand".to_string(), false),
        };
        c.execute(
            "INSERT INTO cycles (issue, attempt, started_at, started_by, how, requeued,
               rukas_commit, image, branch, end_reason, ended_at, oom_kills, forks_refused,
               stalled_seconds, timed_out, runner_exit, watched, host_saw, stop_asked_at,
               stop_said_times)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL, NULL, NULL, NULL, NULL, NULL, 0,
               NULL, 0, NULL, NULL, 0)
             ON CONFLICT (issue) DO UPDATE SET attempt = excluded.attempt,
               started_at = excluded.started_at, started_by = excluded.started_by,
               how = excluded.how, requeued = excluded.requeued,
               rukas_commit = excluded.rukas_commit, image = NULL, branch = NULL,
               end_reason = NULL, ended_at = NULL, oom_kills = NULL, forks_refused = NULL,
               stalled_seconds = NULL, timed_out = 0, runner_exit = NULL, watched = 0,
               host_saw = NULL, stop_asked_at = NULL, stop_said_times = 0",
            params![issue, attempt, now, by, how, requeued, rukas],
        )
        .q()?;
        for table in [
            "cycle_settings",
            "cycle_notes",
            "cycle_locks",
            "cycle_results",
            "cycle_progress",
        ] {
            c.execute(&format!("DELETE FROM {table} WHERE issue = ?1"), [issue])
                .q()?;
        }
        if let Some((at, _, _)) = &approval {
            c.execute(
                "UPDATE approvals SET used_at = ?3, used_by = ?4 WHERE issue = ?1 AND at = ?2",
                params![issue, at, now, attempt],
            )
            .q()?;
            approval::retain(c, issue)?;
        }
        Ok(line(
            "cycle-start",
            &[
                ("issue", j(issue)),
                ("attempt", j(attempt)),
                ("started_at", j(now)),
                ("started_by", j(by)),
                ("how", j(how)),
                ("requeued", j(requeued)),
                ("approval_at", j(approval.map(|a| a.0))),
                ("replaced_unended", j(unended)),
            ],
        ))
    })
}

/// What `--settings` came to before the store was opened.
pub enum SettingsIn {
    Good(Vec<(String, String)>),
    Refused,
    Secret,
}

fn read_settings(path: &Path) -> SettingsIn {
    let Ok((bytes, _)) = verbs::read_file(path, SETTINGS_BOUND) else {
        return SettingsIn::Refused;
    };
    settings_of(&bytes)
}

/// The settings a file's bytes hold, judged as `cycle-image` judges them. The
/// import reads `state/cycles/<n>.settings` with it.
pub fn settings_of(bytes: &[u8]) -> SettingsIn {
    if bytes.len() as u64 > SETTINGS_BOUND {
        return SettingsIn::Refused;
    }
    if secret(bytes) {
        return SettingsIn::Secret;
    }
    let Ok(Value::Object(top)) = serde_json::from_slice::<Value>(bytes) else {
        return SettingsIn::Refused;
    };
    let mut rows = Vec::new();
    let mut secret_seen = false;
    let half = |v: Option<&Value>, prefix: &str, rows: &mut Vec<(String, String)>, s: &mut bool| {
        let Some(Value::Object(m)) = v else {
            return false;
        };
        for (k, v) in m {
            let key_ok = k.strip_prefix(prefix).is_some_and(|t| {
                !t.is_empty()
                    && t.bytes()
                        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            });
            let Value::String(v) = v else { return false };
            if !key_ok || v.len() > SETTING_VALUE_MAX {
                return false;
            }
            *s |= secret(k.as_bytes()) || secret(v.as_bytes());
            rows.push((k.clone(), v.clone()));
        }
        true
    };
    if top
        .keys()
        .any(|k| !["container", "host", "concurrentCycles"].contains(&k.as_str()))
        || !half(
            top.get("container"),
            "PI_ENSEMBLE_",
            &mut rows,
            &mut secret_seen,
        )
        || !half(top.get("host"), "TEHAS_", &mut rows, &mut secret_seen)
    {
        return SettingsIn::Refused;
    }
    match top.get("concurrentCycles") {
        None | Some(Value::Null) => {}
        Some(Value::Number(n)) if n.is_i64() || n.is_u64() => {
            rows.push(("concurrentCycles".to_string(), n.to_string()));
        }
        Some(_) => return SettingsIn::Refused,
    }
    if secret_seen {
        return SettingsIn::Secret;
    }
    SettingsIn::Good(rows)
}

fn run_of(b: &[u8], from: usize, keep: impl Fn(u8) -> bool) -> usize {
    b.get(from..)
        .map_or(0, |t| t.iter().take_while(|&&c| keep(c)).count())
}

/// The six shapes of `tehas_improve_secret` (lib/improve.sh).
pub fn secret(b: &[u8]) -> bool {
    let alnum = |c: u8| c.is_ascii_alphanumeric();
    for i in 0..b.len() {
        let t = &b[i..];
        if t.len() >= 4
            && t.starts_with(b"gh")
            && b"pousr".contains(&t[2])
            && t[3] == b'_'
            && run_of(b, i + 4, alnum) >= 36
        {
            return true;
        }
        if t.starts_with(b"github_pat_") && run_of(b, i + 11, |c| alnum(c) || c == b'_') >= 22 {
            return true;
        }
        if t.starts_with(b"sk-")
            && (i == 0 || !alnum(b[i - 1]))
            && run_of(b, i + 3, |c| alnum(c) || c == b'_' || c == b'-') >= 20
        {
            return true;
        }
        if t.len() >= 5
            && t.starts_with(b"xox")
            && b"abprs".contains(&t[3])
            && t[4] == b'-'
            && run_of(b, i + 5, |c| alnum(c) || c == b'-') >= 10
        {
            return true;
        }
        if t.starts_with(b"AKIA")
            && run_of(b, i + 4, |c| c.is_ascii_digit() || c.is_ascii_uppercase()) >= 16
        {
            return true;
        }
        if t.starts_with(b"-----BEGIN ") {
            let from = i + 11;
            let end = from + run_of(b, from, |c| c.is_ascii_uppercase() || c == b' ');
            let key = b"PRIVATE KEY";
            if end - from >= key.len() && b[..end].ends_with(key) && b[end..].starts_with(b"-----")
            {
                return true;
            }
        }
    }
    false
}

fn cycle_image(
    home: &Path,
    issue: i64,
    attempt: i64,
    image: &str,
    rukas: Option<&str>,
    settings: Option<&Path>,
) -> Result<String, Fail> {
    let settings = settings.map(read_settings);
    verbs::write(home, |c| {
        let r = row(c, issue)?.ok_or_else(|| refuse("nocycle", issue, "no cycle on record"))?;
        if r.attempt != attempt {
            return Err(refuse(
                "attempt",
                issue,
                &format!("the cycle is attempt {}", r.attempt),
            ));
        }
        if r.ended_at.is_some() {
            return Err(refuse("ended", issue, "the cycle has ended"));
        }
        locked(home, issue, true, 1)?;
        c.execute(
            "UPDATE cycles SET image = ?2, rukas_commit = coalesce(?3, rukas_commit) WHERE issue = ?1",
            params![issue, image, rukas],
        )
        .q()?;
        let said = match &settings {
            None => "null".to_string(),
            Some(SettingsIn::Refused) => j("refused"),
            Some(SettingsIn::Secret) => j("secret"),
            Some(SettingsIn::Good(rows)) => {
                c.execute("DELETE FROM cycle_settings WHERE issue = ?1", [issue])
                    .q()?;
                for (k, v) in rows {
                    c.execute(
                        "INSERT INTO cycle_settings (issue, key, value) VALUES (?1, ?2, ?3)",
                        params![issue, k, v],
                    )
                    .q()?;
                }
                j(rows.len())
            }
        };
        let commit: Option<String> = c
            .query_row(
                "SELECT rukas_commit FROM cycles WHERE issue = ?1",
                [issue],
                |r| r.get(0),
            )
            .q()?;
        Ok(line(
            "cycle-image",
            &[
                ("issue", j(issue)),
                ("attempt", j(attempt)),
                ("image", j(image)),
                ("rukas_commit", j(commit)),
                ("settings", said),
            ],
        ))
    })
}

/// A result or progress file as judged before the store was opened.
enum Judged {
    Empty,
    Refused,
    TooLarge,
    Unreadable,
    Ok(Value, Vec<u8>, i64),
}

impl Judged {
    fn word(&self) -> Option<&'static str> {
        match self {
            Judged::Empty => Some("none"),
            Judged::Refused => Some("refused"),
            Judged::TooLarge => Some("too-large"),
            Judged::Unreadable => Some("unreadable"),
            Judged::Ok(..) => None,
        }
    }
}

fn judge(path: &Path, bound: u64) -> Judged {
    match verbs::read_file(path, bound) {
        Err(FileErr::Refused) => Judged::Refused,
        Err(FileErr::TooLarge) => Judged::TooLarge,
        Ok((bytes, _)) if bytes.is_empty() => Judged::Empty,
        Ok((bytes, mtime)) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(v @ Value::Object(_)) => Judged::Ok(v, bytes, mtime),
            _ => Judged::Unreadable,
        },
    }
}

fn end_report(home: &Path, issue: i64, attempt: i64, r: &Report, now: i64) -> Result<String, Fail> {
    let result = r.result.as_deref().map(|p| judge(p, RESULT_BOUND));
    let progress = r.progress.as_deref().map(|p| judge(p, PROGRESS_BOUND));
    let end_reason = if END_REASONS.contains(&r.status.as_str()) {
        r.status.as_str()
    } else {
        "unknown"
    };
    verbs::write(home, |c| {
        let row = row(c, issue)?.ok_or_else(|| refuse("nocycle", issue, "no cycle on record"))?;
        if row.attempt != attempt {
            return Err(refuse(
                "attempt",
                issue,
                &format!("the cycle is attempt {}", row.attempt),
            ));
        }
        if row.ended_at.is_some() {
            return Ok(line(
                "cycle-end",
                &[
                    ("cycle", cycle_json(c, issue)?),
                    ("result", "null".to_string()),
                    ("progress", "null".to_string()),
                    ("history_id", "null".to_string()),
                    ("cleaned", r.cleaned.json()),
                    ("already", j(true)),
                ],
            ));
        }
        c.execute(
            "UPDATE cycles SET end_reason = ?2, ended_at = ?3, oom_kills = ?4, forks_refused = ?5,
               stalled_seconds = ?6, timed_out = ?7, runner_exit = ?8, watched = ?9,
               host_saw = ?10, branch = coalesce(?11, branch)
             WHERE issue = ?1",
            params![
                issue,
                end_reason,
                now,
                r.oom,
                r.forks,
                r.stalled,
                r.timed,
                r.runner_exit,
                r.watched || r.runner_exit.is_some(),
                r.host_saw,
                r.branch
            ],
        )
        .q()?;
        let result_said = match &result {
            None => "null".to_string(),
            Some(Judged::Ok(v, bytes, _)) => match v.get("schemaVersion").and_then(Value::as_i64) {
                Some(version) => {
                    c.execute(
                        "INSERT INTO cycle_results (issue, received_at, schema_version, payload)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![issue, now, version, String::from_utf8_lossy(bytes)],
                    )
                    .q()?;
                    obj(&[("schema_version", j(version))])
                }
                None => j("unreadable"),
            },
            Some(other) => j(other.word().unwrap_or("unreadable")),
        };
        let progress_said = match &progress {
            None => "null".to_string(),
            Some(Judged::Ok(_, _, mtime)) if *mtime < row.started_at => j("stale"),
            Some(Judged::Ok(v, _, mtime)) => {
                let text = |k: &str| match v.get(k) {
                    None | Some(Value::Null) => None,
                    Some(x) => Some(x.to_string()),
                };
                c.execute(
                    "INSERT INTO cycle_progress (issue, received_at, reason, review_held,
                       steps_json, roles_json, written_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        issue,
                        now,
                        v.get("reason").and_then(Value::as_str),
                        v.get("reviewHeld").and_then(Value::as_bool),
                        text("steps"),
                        text("roles"),
                        mtime
                    ],
                )
                .q()?;
                obj(&[("written_at", j(*mtime))])
            }
            Some(other) => j(other.word().unwrap_or("unreadable")),
        };
        c.execute(
            "INSERT OR REPLACE INTO endings (issue, started_at, ended_at, status, step)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![issue, row.started_at, now, r.status, r.step],
        )
        .q()?;
        c.execute(
            "DELETE FROM endings WHERE rowid NOT IN
               (SELECT rowid FROM endings ORDER BY ended_at DESC, rowid DESC LIMIT 10)",
            [],
        )
        .q()?;
        c.execute(
            "INSERT OR REPLACE INTO heartbeats (issue, at, running, step, status)
             VALUES (?1, ?2, 0, ?3, ?4)",
            params![issue, now, r.step, r.status],
        )
        .q()?;
        c.execute(
            "INSERT INTO cycle_history (issue, attempt, at, status, step, by_account, how, pr,
               started_at, rc, cap, park_reason, failure_kind, failure_cause, provider, parked)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                issue,
                attempt,
                now,
                r.status,
                r.step,
                row.started_by,
                row.how,
                r.pr,
                row.started_at,
                r.rc,
                r.cap,
                r.park_reason,
                r.failure_kind,
                r.failure_cause,
                r.provider,
                r.parked
            ],
        )
        .q()?;
        let history_id = c.last_insert_rowid();
        c.execute(
            "DELETE FROM cycle_history WHERE issue = ?1 AND id NOT IN
               (SELECT id FROM cycle_history WHERE issue = ?1 ORDER BY at DESC, id DESC LIMIT 50)",
            [issue],
        )
        .q()?;
        if r.never_ran {
            c.execute(
                "DELETE FROM approvals WHERE issue = ?1 AND used_by IS NOT NULL",
                [issue],
            )
            .q()?;
        }
        Ok(line(
            "cycle-end",
            &[
                ("cycle", cycle_json(c, issue)?),
                ("result", result_said),
                ("progress", progress_said),
                ("history_id", j(history_id)),
                ("cleaned", r.cleaned.json()),
                ("already", j(false)),
            ],
        ))
    })
}

fn end_short(home: &Path, issue: i64, attempt: i64, end: &End, now: i64) -> Result<String, Fail> {
    verbs::write(home, |c| {
        let row = row(c, issue)?.ok_or_else(|| refuse("nocycle", issue, "no cycle on record"))?;
        if row.attempt != attempt {
            return Err(refuse(
                "attempt",
                issue,
                &format!("the cycle is attempt {}", row.attempt),
            ));
        }
        let already = row.ended_at.is_some();
        if !already {
            if *end == End::Refused {
                locked(home, issue, true, 1)?;
                c.execute(
                    "UPDATE cycles SET ended_at = ?2 WHERE issue = ?1",
                    params![issue, now],
                )
                .q()?;
            } else {
                locked(home, issue, true, 0)?;
                c.execute(
                    "UPDATE cycles SET ended_at = ?2, end_reason = 'stopped' WHERE issue = ?1",
                    params![issue, now],
                )
                .q()?;
            }
        }
        Ok(line(
            "cycle-end",
            &[("cycle", cycle_json(c, issue)?), ("already", j(already))],
        ))
    })
}

fn entry(
    home: &Path,
    c: &Connection,
    issue: i64,
    open: bool,
    now: i64,
) -> Result<(Lock, String), Fail> {
    let l = lock::probe(home, issue, open);
    let s = obj(&[
        ("issue", j(issue)),
        ("lock", l.json(now)),
        ("cycle", cycle_json(c, issue)?),
    ]);
    Ok((l, s))
}

fn show(home: &Path, issue: Option<i64>, now: i64) -> Result<String, Fail> {
    verbs::look(home, |c| {
        if let Some(issue) = issue {
            let open = row(c, issue)?.is_some_and(|r| r.ended_at.is_none());
            let l = lock::probe(home, issue, open);
            return Ok(line(
                "cycle-show",
                &[
                    ("issue", j(issue)),
                    ("lock", l.json(now)),
                    ("cycle", cycle_json(c, issue)?),
                ],
            ));
        }
        let mut q = c
            .prepare("SELECT issue FROM cycles WHERE ended_at IS NULL")
            .q()?;
        let open: BTreeSet<i64> = q
            .query_map([], |r| r.get(0))
            .q()?
            .collect::<Result<_, _>>()
            .q()?;
        let mut why = None;
        let files = lock::listed(home).unwrap_or_else(|e| {
            why = Some(e);
            Vec::new()
        });
        let all: BTreeSet<i64> = open.iter().copied().chain(files).collect();
        let mut entries = Vec::new();
        for issue in all {
            let is_open = open.contains(&issue);
            let (l, s) = entry(home, c, issue, is_open, now)?;
            if l.rc == 2 && why.is_none() {
                why = Some(format!(
                    "#{issue}: {}",
                    l.why.as_deref().unwrap_or("could not tell")
                ));
            }
            if is_open || l.rc != 0 {
                entries.push(s);
            }
        }
        let mut pairs = vec![("probe_rc", j(if why.is_some() { 2 } else { 0 }))];
        if let Some(w) = why {
            pairs.push(("why", j(w)));
        }
        pairs.push(("cycles", list(&entries)));
        Ok(line("cycle-show", &pairs))
    })
}

fn quiet_note(c: &Connection, issue: i64) -> Result<Option<i64>, Fail> {
    c.query_row(
        "SELECT at FROM cycle_notes WHERE issue = ?1 AND kind = 'quiet' ORDER BY at DESC LIMIT 1",
        [issue],
        |r| r.get(0),
    )
    .optional()
    .q()
}

fn running_row(c: &Connection, issue: i64) -> Result<Row, Fail> {
    let r = row(c, issue)?.ok_or_else(|| refuse("nocycle", issue, "no cycle on record"))?;
    if r.ended_at.is_some() {
        return Err(refuse("ended", issue, "the cycle has ended"));
    }
    Ok(r)
}

fn note(home: &Path, issue: i64, said_for: &str, now: i64) -> Result<String, Fail> {
    verbs::write(home, |c| {
        let r = running_row(c, issue)?;
        c.execute(
            "DELETE FROM cycle_notes WHERE issue = ?1 AND kind = 'quiet' AND said_for <> ?2",
            params![issue, said_for],
        )
        .q()?;
        let had: Option<i64> = c
            .query_row(
                "SELECT at FROM cycle_notes WHERE issue = ?1 AND kind = 'quiet' AND said_for = ?2",
                params![issue, said_for],
                |r| r.get(0),
            )
            .optional()
            .q()?;
        let at = match had {
            Some(at) => at,
            None => {
                c.execute(
                    "INSERT INTO cycle_notes (issue, kind, said_for, at) VALUES (?1, 'quiet', ?2, ?3)",
                    params![issue, said_for, now],
                )
                .q()?;
                now
            }
        };
        let times = match r.stop_asked_at {
            Some(asked) if asked >= at => r.stop_said_times,
            _ => 0,
        };
        Ok(line(
            "cycle-note",
            &[
                ("said", j(had.is_none())),
                ("episode_at", j(at)),
                ("times", j(times)),
                ("stop_asked_at", j(r.stop_asked_at)),
            ],
        ))
    })
}

fn stop_asked(home: &Path, issue: i64, said_only: bool, now: i64) -> Result<String, Fail> {
    verbs::write(home, |c| {
        let r = running_row(c, issue)?;
        let note_at =
            quiet_note(c, issue)?.ok_or_else(|| refuse("nonote", issue, "no quiet note"))?;
        let times = if said_only {
            r.stop_said_times
        } else {
            match r.stop_asked_at {
                Some(asked) if asked >= note_at => r.stop_said_times + 1,
                _ => 1,
            }
        };
        c.execute(
            "UPDATE cycles SET stop_asked_at = ?2, stop_said_times = ?3 WHERE issue = ?1",
            params![issue, now, times],
        )
        .q()?;
        Ok(line(
            "cycle-stop-asked",
            &[("times", j(times)), ("asked_at", j(now))],
        ))
    })
}

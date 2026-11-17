//! The outsource state: the contractors, the one-time invite codes, and the
//! job state machine of the spec's "Job states".
//!
//! The five tables of section 3.15 are in every store of schema 15. Every verb
//! here asks `db::has_outsource` first and answers `refused` when the feature
//! is off, so an off factory's tables stay empty.
//!
//!   store contractor-add --name <name> --key <hex> [--by <who>]
//!     One contractor and the public key it joined with. The registered keys
//!     are the allowlist, so there is no allowlist setting. Re-adding a name
//!     that is there keeps what the store holds: a name has one key, and a
//!     later call that names another is refused as `used`, which is what makes
//!     a stolen name unable to swap its key. --by is who said so, kept for the
//!     line a person reads with `tehas outsource list`.
//!   store contractor-list
//!     The contractors as an array: name, whether it is revoked, draining or
//!     resting, when it was last heard of, its two counters, and the issue
//!     and state of the job open in its hand, if any.
//!   store contractor-seen --name <name>
//!     The check-in of the spec's "Durability" 1: the second this contractor
//!     was last heard of.
//!   store contractor-rest --name <name> --for <seconds>
//!     The rest period of "Durability" 4: the contractor gets no work until
//!     the seconds have passed.
//!   store contractor-drain --name <name> [--on 0|1]
//!     `tehas outsource drain`: it is asked to take no new job. --on 0 calls
//!     the drain off again.
//!   store contractor-revoke --name <name>
//!     Ends it at once: every job of its hand is lost with reason `revoked`,
//!     and a later check-in of its is refused.
//!   store invite-new --name <name> --code <sha256> [--ttl <seconds>]
//!     One one-time code, held only as the hash of what will be printed. The
//!     hash is the caller's: the store holds no hash function and no code, so
//!     a reader of the store cannot mint an invitation from a row. A name that
//!     still has a live code has that code replaced, never doubled.
//!   store invite-list
//!     The codes as an array, each with whether it is still live.
//!   store invite-use --code <sha256> --name <name> --key <hex>
//!     The join: the hash is looked up, and the one code that is live spends
//!     itself on the joining host's name and public key at once. answers
//!     `joined`, `expired`, `unknown` or `used`.
//!   store invite-expire
//!     The codes past their expiry, marked used: the sweep of codes nobody
//!     ever joined with, which `jobs-sweep --now` also runs.
//!   store job-offer --issue <n> --contractor <name> [--expires <unix>]
//!     A job is given out, and the issue's attempt number rises: the first
//!     offer of an issue is attempt 1, so the number a result arrives with
//!     says which offer it answers. answers `offered` with the job, or
//!     `refused` when the contractor cannot take work.
//!   store job-check-in --job <id>
//!     The contractor's own timer, and its progress stream: the second the
//!     timeout is counted from. A job of a superseded attempt is refused.
//!   store job-finish --job <id> [--sha256 <hex>]
//!     The result came in from the current attempt. A late contractor with an
//!     older attempt answers `stale`, which is the store's answer and not its
//!     caller's. A job lost to a silence, not to a revocation, still takes the
//!     result of its attempt until the issue's next attempt is given out.
//!   store job-release --job <id>
//!     The contractor handed the job back on purpose. Not a failure, and it
//!     costs the contractor nothing.
//!   store job-lose --job <id> --reason <timeout|revoked|expired> [--rest <s>]
//!     A job is lost: the timeout passed with no check-in, or the contractor
//!     was revoked. The counters of both sides go up here, once, and the
//!     contractor starts its rest period of --rest seconds.
//!   store job-stop --job <id>
//!     The contractor is ending the job: "Dying gracefully" 2.
//!   store job-show --job <id> | --issue <n>
//!     One job with its history, or the issue's current one.
//!   store jobs-sweep --timeout <seconds> [--now]
//!     The one place a job is lost for being silent or for never being taken,
//!     and the one place the expiry of an offer is honoured. The factory runs
//!     it once on start (--now) and again every poll: the jobs that expired
//!     while it was down are sorted out by the same call that sorts out the
//!     ones that expire now, so a verb cannot lose a job by forgetting to.
//!     answers how many of each kind it closed out.
//!   store contractor-auth --name <name> --key <hex>
//!     Whether a contractor's call may be answered: the name is registered,
//!     not revoked, and holds this key. answers `allowed` with whether it is
//!     draining, or `refused` with `revoked`, `unknown` or `another key`. The
//!     key is the hash the contractor joined with: the store compares, so the
//!     caller is never handed the key it is checking against.
//!   store job-offered --contractor <name>
//!     The oldest offer still open in this contractor's hand, which it takes
//!     when it asks for work: answers `job` with its number, issue and attempt,
//!     or `none`.
//!   store job-attempts --issue <n>
//!     The record of what the outsourced attempts of an issue have cost, in
//!     the words "this issue has spent its outsourced attempts" are read:
//!     `given`, `lost` and `open`. The retry cap is the instance's setting,
//!     which the caller loaded; this says what was spent and stays out of the
//!     comparison.
//!
//! Exit: 0 the call was answered; 64 an argument outside its grammar, and
//! nothing was written; 1 the job or the contractor is not there, said as
//! `nojob <n>` or `unknown <name>`; 78 the database. A store with the feature off answers the verbs of this module
//! with `refused` and exit 0, as the verbs of a park of another issue are
//! refused: the call is refused, not the argument.

use crate::db;
use crate::verbs::{self, j, line, obj, Args, Fail, Kind, Q};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::Path;

/// The six states of the spec's "Job states", in the order the machine draws
/// them. The schema's CHECK clauses hold the same six.
pub const STATES: [&str; 6] = [
    "offered", "running", "stopping", "finished", "released", "lost",
];

/// Why a job is lost, the three the spec names: the timeout passed, the
/// contractor was revoked, or an offer was never taken.
pub const LOST_REASONS: [&str; 3] = ["timeout", "revoked", "expired"];

/// The word of every row of job_history that the factory moved the job. A
/// contractor's rows name the contractor instead, so a person reading a job's
/// history can tell who said what.
pub const FACTORY: &str = "factory";

/// The states a job is open in: the ones a result is honoured for, and the
/// ones the sweep and a revocation look at.
pub const OPEN: [&str; 3] = ["offered", "running", "stopping"];

/// How long an offer stays open when its caller names no --expires. The
/// command that loaded the instance's settings passes TEHAS_OUTSOURCE_TIMEOUT
/// as --expires, and this is the fall-back of a caller that said nothing.
pub const OFFER_TTL: i64 = 3600;

/// The longest a contractor's name may be. A name is what a person reads in
/// `tehas outsource list` and what a check-in arrives with; it is a host's
/// name, not a word of the model's.
const NAME_MAX: usize = 64;
/// The longest the word of who added a contractor is.
const BY_MAX: usize = 39;
/// The longest one line of a job's history may say.
const DETAIL_MAX: usize = 300;
/// The most a timeout, a grace or a rest period is believed to be: 27 days.
/// The settings hold their own bounds; this is the store's guard against a
/// caller that loaded nothing.
const SPAN_MAX: i64 = 2_419_200;

#[derive(Debug, PartialEq, Eq)]
pub enum Call {
    ContractorAdd(Add),
    ContractorList,
    Seen(String),
    Rest {
        name: String,
        secs: i64,
    },
    Drain {
        name: String,
        on: bool,
    },
    Revoke(String),
    InviteNew {
        name: String,
        code: String,
        ttl: Option<i64>,
    },
    InviteList,
    InviteUse {
        code: String,
        name: String,
        key: String,
    },
    InviteExpire,
    Offer(Offer),
    CheckIn(i64),
    Finish {
        job: i64,
        sha: Option<String>,
    },
    Release(i64),
    Lose {
        job: i64,
        reason: String,
        rest: i64,
    },
    Stop(i64),
    Show(JobShow),
    Sweep {
        timeout: i64,
        at_start: bool,
    },
    Attempts(i64),
    Auth {
        name: String,
        key: String,
    },
    Offered(String),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Add {
    pub name: String,
    pub key: String,
    pub by: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Offer {
    pub issue: i64,
    pub contractor: String,
    /// The second the offer dies, when the caller names one. An offer nobody
    /// takes is closed out by `jobs-sweep` at this second.
    pub expires: Option<i64>,
}

/// `job-show`: one job by its number, or the issue's current one.
#[derive(Debug, PartialEq, Eq)]
pub enum JobShow {
    Job(i64),
    Issue(i64),
}

/// A contractor's name, as a join and a check-in arrive with it: the shape of
/// a host's name, and nothing wider.
fn host(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let ok = !bytes.is_empty()
        && bytes.len() <= NAME_MAX
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_');
    ok.then(|| s.to_string())
}

/// Who added a contractor: a login's shape, or the word of the command that
/// did it.
fn by(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let ok = !bytes.is_empty()
        && bytes.len() <= BY_MAX
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || b"._-".contains(&b));
    ok.then(|| s.to_string())
}

/// A seconds count the store is asked to wait by: never 0, and at most a
/// month.
fn span(s: &str) -> Option<i64> {
    verbs::number(s).filter(|n| *n > 0 && *n <= SPAN_MAX)
}

/// A rest period, which 0 says is none: the loss of a revoked job costs a
/// contractor no rest.
fn rest_span(s: &str) -> Option<i64> {
    tehas_core::grammar::digits(s, 9, true).filter(|n| *n <= SPAN_MAX)
}

fn one_of(list: &'static [&'static str]) -> impl Fn(&str) -> Option<&'static str> {
    move |s| list.iter().find(|k| **k == s).copied()
}

pub fn parse(name: &str, rest: &[String]) -> Option<Result<Call, String>> {
    Some(match name {
        "contractor-add" => Args::parse(
            "contractor-add",
            rest,
            &[
                ("--name", Kind::Value),
                ("--key", Kind::Value),
                ("--by", Kind::Value),
            ],
            false,
        )
        .and_then(|a| {
            Ok(Call::ContractorAdd(Add {
                name: a.strict_need("--name", "a host's name", host)?,
                key: a.strict_need("--key", "a public key as 64 hex digits", verbs::sha256)?,
                by: a.strict("--by", "a login", by)?,
            }))
        }),
        "contractor-list" => {
            Args::parse("contractor-list", rest, &[], false).map(|_| Call::ContractorList)
        }
        "contractor-seen" => {
            Args::parse("contractor-seen", rest, &[("--name", Kind::Value)], false).and_then(|a| {
                Ok(Call::Seen(a.strict_need(
                    "--name",
                    "a host's name",
                    host,
                )?))
            })
        }
        "contractor-rest" => Args::parse(
            "contractor-rest",
            rest,
            &[("--name", Kind::Value), ("--for", Kind::Value)],
            false,
        )
        .and_then(|a| {
            Ok(Call::Rest {
                name: a.strict_need("--name", "a host's name", host)?,
                secs: a.strict_need("--for", "a number of seconds", span)?,
            })
        }),
        "contractor-drain" => Args::parse(
            "contractor-drain",
            rest,
            &[("--name", Kind::Value), ("--on", Kind::Value)],
            false,
        )
        .and_then(|a| {
            let on = a.strict("--on", "0 or 1", |s| match s {
                "1" => Some(true),
                "0" => Some(false),
                _ => None,
            })?;
            Ok(Call::Drain {
                name: a.strict_need("--name", "a host's name", host)?,
                on: on.unwrap_or(true),
            })
        }),
        "contractor-revoke" => {
            Args::parse("contractor-revoke", rest, &[("--name", Kind::Value)], false).and_then(
                |a| {
                    Ok(Call::Revoke(a.strict_need(
                        "--name",
                        "a host's name",
                        host,
                    )?))
                },
            )
        }
        "invite-new" => Args::parse(
            "invite-new",
            rest,
            &[
                ("--name", Kind::Value),
                ("--code", Kind::Value),
                ("--ttl", Kind::Value),
            ],
            false,
        )
        .and_then(|a| {
            Ok(Call::InviteNew {
                name: a.strict_need("--name", "a host's name", host)?,
                code: a.strict_need(
                    "--code",
                    "the hash of a code, 64 hex digits",
                    verbs::sha256,
                )?,
                ttl: a.strict("--ttl", "a number of seconds", span)?,
            })
        }),
        "invite-list" => Args::parse("invite-list", rest, &[], false).map(|_| Call::InviteList),
        "invite-use" => Args::parse(
            "invite-use",
            rest,
            &[
                ("--code", Kind::Value),
                ("--name", Kind::Value),
                ("--key", Kind::Value),
            ],
            false,
        )
        .and_then(|a| {
            Ok(Call::InviteUse {
                code: a.strict_need(
                    "--code",
                    "the hash of a code, 64 hex digits",
                    verbs::sha256,
                )?,
                name: a.strict_need("--name", "a host's name", host)?,
                key: a.strict_need("--key", "a public key as 64 hex digits", verbs::sha256)?,
            })
        }),
        "invite-expire" => {
            Args::parse("invite-expire", rest, &[], false).map(|_| Call::InviteExpire)
        }
        "job-offer" => Args::parse(
            "job-offer",
            rest,
            &[
                ("--issue", Kind::Value),
                ("--contractor", Kind::Value),
                ("--expires", Kind::Value),
            ],
            false,
        )
        .and_then(|a| {
            Ok(Call::Offer(Offer {
                issue: a.strict_need("--issue", "an issue number", verbs::number)?,
                contractor: a.strict_need("--contractor", "a host's name", host)?,
                expires: a.strict("--expires", "a Unix time", verbs::unix)?,
            }))
        }),
        "job-check-in" => Args::parse("job-check-in", rest, &[("--job", Kind::Value)], false)
            .and_then(|a| {
                Ok(Call::CheckIn(a.strict_need(
                    "--job",
                    "a job number",
                    verbs::number,
                )?))
            }),
        "job-finish" => Args::parse(
            "job-finish",
            rest,
            &[("--job", Kind::Value), ("--sha256", Kind::Value)],
            false,
        )
        .and_then(|a| {
            Ok(Call::Finish {
                job: a.strict_need("--job", "a job number", verbs::number)?,
                sha: a.strict("--sha256", "64 hex digits", verbs::sha256)?,
            })
        }),
        "job-release" => Args::parse("job-release", rest, &[("--job", Kind::Value)], false)
            .and_then(|a| {
                Ok(Call::Release(a.strict_need(
                    "--job",
                    "a job number",
                    verbs::number,
                )?))
            }),
        "job-lose" => Args::parse(
            "job-lose",
            rest,
            &[
                ("--job", Kind::Value),
                ("--reason", Kind::Value),
                ("--rest", Kind::Value),
            ],
            false,
        )
        .and_then(|a| {
            Ok(Call::Lose {
                job: a.strict_need("--job", "a job number", verbs::number)?,
                reason: a
                    .strict_need(
                        "--reason",
                        &format!("one of {}", LOST_REASONS.join(", ")),
                        one_of(&LOST_REASONS),
                    )?
                    .to_string(),
                rest: a
                    .strict("--rest", "a number of seconds", rest_span)?
                    .unwrap_or(0),
            })
        }),
        "job-stop" => {
            Args::parse("job-stop", rest, &[("--job", Kind::Value)], false).and_then(|a| {
                Ok(Call::Stop(a.strict_need(
                    "--job",
                    "a job number",
                    verbs::number,
                )?))
            })
        }
        "job-show" => Args::parse(
            "job-show",
            rest,
            &[("--job", Kind::Value), ("--issue", Kind::Value)],
            false,
        )
        .and_then(|a| {
            match (
                a.strict("--job", "a job number", verbs::number)?,
                a.strict("--issue", "an issue number", verbs::number)?,
            ) {
                (Some(job), None) => Ok(Call::Show(JobShow::Job(job))),
                (None, Some(issue)) => Ok(Call::Show(JobShow::Issue(issue))),
                (None, None) => Err(a.usage("--job", "or --issue is required")),
                (Some(_), Some(_)) => Err(a.usage("--job", "and --issue are not taken together")),
            }
        }),
        "jobs-sweep" => Args::parse(
            "jobs-sweep",
            rest,
            &[("--timeout", Kind::Value), ("--now", Kind::Switch)],
            false,
        )
        .and_then(|a| {
            Ok(Call::Sweep {
                timeout: a.strict_need("--timeout", "a number of seconds", span)?,
                at_start: a.has("--now"),
            })
        }),
        "contractor-auth" => Args::parse(
            "contractor-auth",
            rest,
            &[("--name", Kind::Value), ("--key", Kind::Value)],
            false,
        )
        .and_then(|a| {
            Ok(Call::Auth {
                name: a.strict_need("--name", "a host's name", host)?,
                key: a.strict_need("--key", "a key as 64 hex digits", verbs::sha256)?,
            })
        }),
        "job-offered" => Args::parse("job-offered", rest, &[("--contractor", Kind::Value)], false)
            .and_then(|a| {
                Ok(Call::Offered(a.strict_need(
                    "--contractor",
                    "a host's name",
                    host,
                )?))
            }),
        "job-attempts" => Args::parse("job-attempts", rest, &[("--issue", Kind::Value)], false)
            .and_then(|a| {
                Ok(Call::Attempts(a.strict_need(
                    "--issue",
                    "an issue number",
                    verbs::number,
                )?))
            }),
        _ => return None,
    })
}

pub fn run(home: &Path, call: &Call, now: i64) -> Result<String, Fail> {
    // Every verb asks the store whether it holds the tables, and answers the
    // one refusal when it does not; the writers ask under the lock they are
    // about to write under, and the readers as any other read verb does.
    match call {
        Call::ContractorAdd(a) => add(home, a, now),
        Call::ContractorList => contractors(home, now),
        Call::Seen(n) => seen(home, n, now),
        Call::Rest { name, secs } => rest(home, name, *secs, now),
        Call::Drain { name, on } => drain(home, name, *on, now),
        Call::Revoke(n) => revoke(home, n, now),
        Call::InviteNew { name, code, ttl } => invite_new(home, name, code, *ttl, now),
        Call::InviteList => invites(home, now),
        Call::InviteUse { code, name, key } => invite_use(home, code, name, key, now),
        Call::InviteExpire => invite_expire(home, now),
        Call::Offer(o) => offer(home, o, now),
        Call::CheckIn(id) => check_in(home, *id, now),
        Call::Finish { job, sha } => finish(home, *job, sha.as_deref(), now),
        Call::Release(id) => release(home, *id, now),
        Call::Lose { job, reason, rest } => lose(home, *job, reason, *rest, now),
        Call::Stop(id) => stop(home, *id, now),
        Call::Show(s) => show(home, s),
        Call::Sweep { timeout, at_start } => sweep(home, *timeout, *at_start, now),
        Call::Attempts(issue) => attempts(home, *issue),
        Call::Auth { name, key } => auth(home, name, key),
        Call::Offered(name) => offered(home, name, now),
    }
}

/// Whether the feature is on, read without the write lock.
fn available(home: &Path) -> Result<bool, Fail> {
    let file = db::path(home);
    verbs::not_empty(&file)?;
    let conn = db::open_read_only(&file).map_err(crate::line)?;
    db::has_outsource(&conn).map_err(crate::line)
}

/// One transaction that writes the state of the feature. The feature is judged
/// after the write lock is held and the transaction has begun, which is the one
/// moment the answer and the write cannot disagree; see
/// [`db::has_outsource_held`] for what that rules out.
fn work(home: &Path, f: impl FnOnce(&Connection) -> Result<String, Fail>) -> Result<String, Fail> {
    match verbs::write(home, |c| {
        db::has_outsource_held(c)
            .map_err(crate::line)
            .and_then(|on| if on { f(c).map(Some) } else { Ok(None) })
    })? {
        Some(said) => Ok(said),
        None => Ok(line("refused", &[("said", j("outsource is off"))])),
    }
}

/// One transaction that only reads the feature's rows. The answer is the one
/// the store gives to a reader that holds no lock, which is what every other
/// read verb of the store does.
fn look(home: &Path, f: impl FnOnce(&Connection) -> Result<String, Fail>) -> Result<String, Fail> {
    if !available(home)? {
        return Ok(line("refused", &[("said", j("outsource is off"))]));
    }
    verbs::look(home, f)
}

// --- the contractors -------------------------------------------------------------

fn add(home: &Path, a: &Add, now: i64) -> Result<String, Fail> {
    let (name, key, by) = (a.name.clone(), a.key.clone(), a.by.clone());
    work(home, move |c| {
        let held: Option<(String, Option<i64>)> = c
            .query_row(
                "SELECT public_key, revoked_at FROM contractors WHERE name = ?1",
                [&name],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .q()?;
        if let Some((held, revoked)) = held {
            // A name has one key, and a revoked name is not re-added by the
            // same verb: a person revokes it and adds it again on purpose,
            // which is two calls.
            if held != key || revoked.is_some() {
                return Ok(line(
                    "used",
                    &[
                        ("name", j(name.as_str())),
                        ("said", j("the name holds another key")),
                    ],
                ));
            }
            return Ok(line(
                "added",
                &[("name", j(name.as_str())), ("again", j(1))],
            ));
        }
        c.execute(
            "INSERT INTO contractors (name, public_key, added_at, added_by) VALUES (?1, ?2, ?3, ?4)",
            params![name, key, now, by],
        )
        .q()?;
        Ok(line("added", &[("name", j(name.as_str()))]))
    })
}

fn contractors(home: &Path, now: i64) -> Result<String, Fail> {
    look(home, move |c| {
        let mut rows = c
            .prepare(
                "SELECT name, revoked_at, draining, resting_until, last_seen_at, jobs_done, jobs_lost
                   FROM contractors ORDER BY name",
            )
            .q()?;
        let taken = rows
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<i64>>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                    r.get::<_, Option<i64>>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, i64>(6)?,
                ))
            })
            .q()?;
        let mut out = Vec::new();
        for row in taken {
            let (name, revoked, draining, resting, seen, done, lost) = row.q()?;
            let resting = resting.filter(|until| *until > now);
            let open: Option<(i64, String)> = c
                .query_row(
                    "SELECT issue, state FROM jobs WHERE contractor = ?1
                       AND state IN ('offered','running','stopping') ORDER BY id DESC LIMIT 1",
                    [&name],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .q()?;
            let (issue, state) = open.unzip();
            out.push(obj(&[
                ("name", j(name.as_str())),
                ("job_issue", j(issue)),
                ("job_state", j(state.as_deref())),
                ("revoked", j(revoked.is_some())),
                ("draining", j(draining == 1)),
                ("resting", j(resting.is_some())),
                ("resting_until", j(resting)),
                ("last_seen_at", j(seen)),
                ("jobs_done", j(done)),
                ("jobs_lost", j(lost)),
            ]));
        }
        Ok(line("contractors", &[("rows", j(out))]))
    })
}

fn seen(home: &Path, name: &str, now: i64) -> Result<String, Fail> {
    let name = name.to_string();
    work(home, move |c| {
        let changed = c
            .execute(
                "UPDATE contractors SET last_seen_at = ?1 WHERE name = ?2",
                params![now, name],
            )
            .q()?;
        if changed == 0 {
            return Err(unknown(&name));
        }
        Ok(line("seen", &[("name", j(name.as_str())), ("at", j(now))]))
    })
}

fn rest(home: &Path, name: &str, secs: i64, now: i64) -> Result<String, Fail> {
    let name = name.to_string();
    let until = now + secs;
    work(home, move |c| {
        let changed = c
            .execute(
                "UPDATE contractors SET resting_until = ?1 WHERE name = ?2",
                params![until, name],
            )
            .q()?;
        if changed == 0 {
            return Err(unknown(&name));
        }
        Ok(line(
            "resting",
            &[("name", j(name.as_str())), ("until", j(until))],
        ))
    })
}

fn drain(home: &Path, name: &str, on: bool, now: i64) -> Result<String, Fail> {
    let name = name.to_string();
    work(home, move |c| {
        let changed = c
            .execute(
                "UPDATE contractors SET draining = ?1 WHERE name = ?2",
                params![i64::from(on), name],
            )
            .q()?;
        if changed == 0 {
            return Err(unknown(&name));
        }
        Ok(line(
            "draining",
            &[
                ("name", j(name.as_str())),
                ("draining", j(on)),
                ("at", j(now)),
            ],
        ))
    })
}

/// `tehas outsource revoke`: the contractor is ended, and every job of its
/// hand is lost with reason `revoked` in the same transaction, so no poll can
/// see a revoked contractor holding a job.
fn revoke(home: &Path, name: &str, now: i64) -> Result<String, Fail> {
    let name = name.to_string();
    work(home, move |c| {
        let changed = c
            .execute(
                "UPDATE contractors SET revoked_at = ?1 WHERE name = ?2 AND revoked_at IS NULL",
                params![now, name],
            )
            .q()?;
        if changed == 0 && !is_contractor(c, &name)? {
            return Err(unknown(&name));
        }
        let mut lost = 0;
        for id in open_of(c, &name)? {
            if lose_one(c, id, "revoked", FACTORY, 0, now)? {
                lost += 1;
            }
        }
        Ok(line(
            "revoked",
            &[
                ("name", j(name.as_str())),
                ("at", j(now)),
                ("lost", j(lost)),
            ],
        ))
    })
}

fn is_contractor(c: &Connection, name: &str) -> Result<bool, Fail> {
    let n: i64 = c
        .query_row(
            "SELECT count(*) FROM contractors WHERE name = ?1",
            [name],
            |r| r.get(0),
        )
        .q()?;
    Ok(n > 0)
}

/// Whether a contractor may take work now. None says it may; the word is why
/// it may not, and goes back with the answer of `job-offer`.
fn able(c: &Connection, name: &str, now: i64, timeout: i64) -> Result<Option<&'static str>, Fail> {
    let held: Option<(Option<i64>, i64, Option<i64>)> = c
        .query_row(
            "SELECT revoked_at, draining, resting_until FROM contractors WHERE name = ?1",
            [name],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .q()?;
    let Some((revoked, draining, resting)) = held else {
        return Ok(Some("no such contractor"));
    };
    if revoked.is_some() {
        return Ok(Some("revoked"));
    }
    if draining == 1 {
        return Ok(Some("draining"));
    }
    if resting.is_some_and(|until| until > now) {
        return Ok(Some("resting"));
    }
    // One contractor holds one job at a time: the machine that runs it has the
    // one checkout, and a second job given into its hand is a job nobody works.
    // A job that has gone quiet holds nothing: the offered job whose expiry has
    // passed, or the running or stopping job whose last check-in is older than
    // the caller's timeout, is one the poll's sweep is about to lose, and the
    // machine is free for the next job while the row waits for it. The row is
    // kept until the sweep closes it, so a late word of that contractor is
    // answered against a row that is still there rather than refused for a
    // missing one. The timeout is the instance's setting, which the command that
    // loaded it passes down; the store reads no setting of a host itself.
    let busy: i64 = c
        .query_row(
            "SELECT count(*) FROM jobs WHERE contractor = ?1
               AND ((state = 'offered' AND expires_at > ?2)
                    OR (state IN ('running','stopping')
                        AND coalesce(last_check_in_at, offered_at) > ?3))",
            params![name, now, now - timeout],
            |r| r.get(0),
        )
        .q()?;
    if busy > 0 {
        return Ok(Some("busy"));
    }
    Ok(None)
}

// --- the invite codes -------------------------------------------------------------

/// One code, as the hash of what its caller will print. A name that still has
/// a live code has that row replaced, never kept beside an older one, so a
/// code a person has printed twice cannot be spent from the first printing.
fn invite_new(
    home: &Path,
    name: &str,
    code: &str,
    ttl: Option<i64>,
    now: i64,
) -> Result<String, Fail> {
    let (name, code) = (name.to_string(), code.to_string());
    let expires = now + ttl.unwrap_or(SPAN_MAX);
    work(home, move |c| {
        c.execute(
            "INSERT INTO join_codes (name, code_sha256, created_at, expires_at, used_at)
               VALUES (?1, ?2, ?3, ?4, NULL)
               ON CONFLICT (name) DO UPDATE SET
                 code_sha256 = excluded.code_sha256,
                 created_at = excluded.created_at,
                 expires_at = excluded.expires_at,
                 used_at = NULL",
            params![name, code, now, expires],
        )
        .q()?;
        Ok(line(
            "invited",
            &[
                ("name", j(name.as_str())),
                ("created_at", j(now)),
                ("expires_at", j(expires)),
            ],
        ))
    })
}

fn invites(home: &Path, now: i64) -> Result<String, Fail> {
    look(home, move |c| {
        let mut rows = c
            .prepare("SELECT name, created_at, expires_at, used_at FROM join_codes ORDER BY name")
            .q()?;
        let taken = rows
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                ))
            })
            .q()?;
        let mut out = Vec::new();
        for row in taken {
            let (name, created, expires, used) = row.q()?;
            out.push(obj(&[
                ("name", j(name.as_str())),
                ("created_at", j(created)),
                ("expires_at", j(expires)),
                ("used_at", j(used)),
                ("live", j(used.is_none() && expires > now)),
            ]));
        }
        Ok(line("invites", &[("rows", j(out))]))
    })
}

/// The join: the hash of the code the host was given is looked up, and the one
/// code that holds it and is still live spends itself here, in the same
/// transaction as the contractor it admits. A wrong hash answers `unknown`; a
/// spent or expired one answers `expired`; and a code is never spent twice,
/// because the row that spent it says so and the UPDATE that spends it reports
/// nothing changed.
fn invite_use(home: &Path, code: &str, name: &str, key: &str, now: i64) -> Result<String, Fail> {
    let (code, name, key) = (code.to_string(), name.to_string(), key.to_string());
    work(home, move |c| {
        let held: Option<(String, i64, Option<i64>)> = c
            .query_row(
                "SELECT name, expires_at, used_at FROM join_codes WHERE code_sha256 = ?1
                   ORDER BY expires_at DESC LIMIT 1",
                [&code],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .q()?;
        let Some((_, expires, used)) = held else {
            return Ok(line("unknown", &[("name", j(name.as_str()))]));
        };
        if used.is_some() || expires <= now {
            return Ok(line("expired", &[("name", j(name.as_str()))]));
        }
        let spent = c
            .execute(
                "UPDATE join_codes SET used_at = ?1 WHERE name = ?2 AND code_sha256 = ?3
                   AND used_at IS NULL",
                params![now, name, code],
            )
            .q()?;
        if spent == 0 {
            // The row was spent by a call that came in between the read and the
            // lock: the code is gone whatever its expiry said.
            return Ok(line("used", &[("name", j(name.as_str()))]));
        }
        match key_of(c, &name)? {
            // A host that joins again with another key is not taken as the
            // host the code was printed for: the name keeps the key it joined
            // with, and the code it joins with now is not spent on it.
            Some(held) if held != key => Ok(line(
                "used",
                &[
                    ("name", j(name.as_str())),
                    ("said", j("the name holds another key")),
                ],
            )),
            Some(_) => Ok(line(
                "joined",
                &[("name", j(name.as_str())), ("at", j(now)), ("again", j(1))],
            )),
            None => {
                c.execute(
                    "INSERT INTO contractors (name, public_key, added_at, added_by)
                       VALUES (?1, ?2, ?3, 'invite')",
                    params![name, key, now],
                )
                .q()?;
                Ok(line(
                    "joined",
                    &[("name", j(name.as_str())), ("at", j(now))],
                ))
            }
        }
    })
}

fn lost_reason(c: &Connection, id: i64) -> Result<Option<String>, Fail> {
    c.query_row(
        "SELECT lost_reason FROM jobs WHERE id = ?1",
        params![id],
        |r| r.get::<_, Option<String>>(0),
    )
    .optional()
    .q()
    .map(Option::flatten)
}

fn auth(home: &Path, name: &str, key: &str) -> Result<String, Fail> {
    look(home, move |c| {
        let held: Option<(String, Option<i64>, i64)> = c
            .query_row(
                "SELECT public_key, revoked_at, draining FROM contractors WHERE name = ?1",
                [name],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .q()?;
        let said = match held {
            None => "unknown",
            Some((_, Some(_), _)) => "revoked",
            Some((held, None, _)) if held != key => "another key",
            Some((_, None, draining)) => {
                return Ok(line(
                    "allowed",
                    &[("name", j(name)), ("draining", j(draining == 1))],
                ))
            }
        };
        Ok(line("refused", &[("name", j(name)), ("said", j(said))]))
    })
}

fn offered(home: &Path, name: &str, now: i64) -> Result<String, Fail> {
    look(home, move |c| {
        let held: Option<(i64, i64, i64, i64)> = c
            .query_row(
                "SELECT id, issue, attempt, expires_at FROM jobs
                   WHERE contractor = ?1 AND state = 'offered' AND expires_at > ?2
                   ORDER BY id LIMIT 1",
                params![name, now],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()
            .q()?;
        Ok(match held {
            Some((id, issue, attempt, expires)) => line(
                "job",
                &[
                    ("job", j(id)),
                    ("issue", j(issue)),
                    ("attempt", j(attempt)),
                    ("contractor", j(name)),
                    ("state", j("offered")),
                    ("expires_at", j(expires)),
                ],
            ),
            None => line("none", &[("contractor", j(name))]),
        })
    })
}

fn key_of(c: &Connection, name: &str) -> Result<Option<String>, Fail> {
    c.query_row(
        "SELECT public_key FROM contractors WHERE name = ?1",
        [name],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .q()
}

/// The codes nobody ever joined with, closed out. Their rows are kept: a code
/// that expired is the record that a name was invited and never came.
fn invite_expire(home: &Path, now: i64) -> Result<String, Fail> {
    work(home, move |c| {
        let changed = expire_codes(c, now)?;
        Ok(line("expired", &[("codes", j(changed))]))
    })
}

fn expire_codes(c: &Connection, now: i64) -> Result<i64, Fail> {
    Ok(c.execute(
        "UPDATE join_codes SET used_at = ?1 WHERE used_at IS NULL AND expires_at <= ?1",
        params![now],
    )
    .q()? as i64)
}

// --- the jobs ---------------------------------------------------------------------

fn offer(home: &Path, o: &Offer, now: i64) -> Result<String, Fail> {
    let (issue, contractor) = (o.issue, o.contractor.clone());
    let expires = o.expires.unwrap_or(now + OFFER_TTL);
    work(home, move |c| {
        // The offer's own bound is the timeout of the machine too: a job is
        // given for the time its caller means it to live, and the same span is
        // what says a job of that hand has gone quiet.
        if let Some(why) = able(c, &contractor, now, expires - now)? {
            return Ok(line(
                "refused",
                &[
                    ("issue", j(issue)),
                    ("contractor", j(contractor.as_str())),
                    ("said", j(why)),
                ],
            ));
        }
        // The attempt number of the issue's next job, the first offer of an
        // issue being attempt 1. It rises for every job given out, whether it
        // was taken or not, so a result that arrives with an older number
        // answers an offer nobody has to honour.
        let given: i64 = c
            .query_row(
                "SELECT count(*) FROM jobs WHERE issue = ?1",
                params![issue],
                |r| r.get(0),
            )
            .q()?;
        let attempt = given + 1;
        c.execute(
            "INSERT INTO jobs (issue, contractor, attempt, state, offered_at, expires_at, updated_at)
               VALUES (?1, ?2, ?3, 'offered', ?4, ?5, ?4)",
            params![issue, contractor, attempt, now, expires],
        )
        .q()?;
        let id = c.last_insert_rowid();
        history(c, id, now, None, "offered", FACTORY, None)?;
        c.execute(
            "INSERT INTO issue_attempts (issue, given, lost, last_given_at)
               VALUES (?1, 1, 0, ?2)
               ON CONFLICT (issue) DO UPDATE SET
                 given = given + 1, last_given_at = excluded.last_given_at",
            params![issue, now],
        )
        .q()?;
        Ok(line(
            "offered",
            &[
                ("job", j(id)),
                ("issue", j(issue)),
                ("attempt", j(attempt)),
                ("contractor", j(contractor.as_str())),
                ("expires_at", j(expires)),
            ],
        ))
    })
}

fn check_in(home: &Path, id: i64, now: i64) -> Result<String, Fail> {
    work(home, move |c| {
        let job = held(c, id)?;
        // The attempt is asked before the state, as in `job-finish`.
        if stale(c, &job)? {
            return Ok(stale_line(&job));
        }
        if !matches!(job.state.as_str(), "offered" | "running") {
            return Ok(held_line(
                "refused",
                &job,
                &format!("{} does not check in", job.state),
            ));
        }
        c.execute(
            "UPDATE jobs SET state = 'running', last_check_in_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![now, id],
        )
        .q()?;
        history(c, id, now, Some(&job.state), "running", FACTORY, None)?;
        Ok(line(
            "checked-in",
            &[("job", j(id)), ("issue", j(job.issue)), ("at", j(now))],
        ))
    })
}

fn finish(home: &Path, id: i64, sha: Option<&str>, now: i64) -> Result<String, Fail> {
    let sha = sha.map(str::to_string);
    work(home, move |c| {
        let job = held(c, id)?;
        // The attempt is asked before the state: a result of a superseded
        // attempt belongs to no job whatever its row says now, and a state
        // would answer `refused` for a contractor that has come back too late.
        if stale(c, &job)? {
            return Ok(stale_line(&job));
        }
        let salvaged = job.state == "lost" && lost_reason(c, id)?.as_deref() != Some("revoked");
        if !matches!(job.state.as_str(), "running" | "stopping") && !salvaged {
            return Ok(held_line(
                "refused",
                &job,
                &format!("{} does not finish", job.state),
            ));
        }
        match &sha {
            Some(sha) => c
                .execute(
                    "UPDATE jobs SET state = 'finished', result_sha256 = ?1, updated_at = ?2 WHERE id = ?3",
                    params![sha, now, id],
                )
                .q()?,
            None => c
                .execute(
                    "UPDATE jobs SET state = 'finished', updated_at = ?1 WHERE id = ?2",
                    params![now, id],
                )
                .q()?,
        };
        history(
            c,
            id,
            now,
            Some(&job.state),
            "finished",
            FACTORY,
            sha.as_deref(),
        )?;
        c.execute(
            "UPDATE contractors SET jobs_done = jobs_done + 1 WHERE name = ?1",
            params![job.contractor],
        )
        .q()?;
        Ok(line(
            "finished",
            &[
                ("job", j(id)),
                ("issue", j(job.issue)),
                ("state", j("finished")),
            ],
        ))
    })
}

/// A release is not a failure and costs the contractor nothing: neither of its
/// counters moves here, which is the difference between it and a loss.
fn release(home: &Path, id: i64, now: i64) -> Result<String, Fail> {
    work(home, move |c| {
        let job = held(c, id)?;
        // The hand that holds a job may always give it back, whatever its
        // attempt number says: a release is not a result, and a job given back
        // is one the factory does not have to honour. Refusing it would hold a
        // machine on work it has stopped doing. A row the store has already
        // closed out has nothing left to give back, and answers as what it is.
        let late = superseded(c, &job)?;
        if !OPEN.contains(&job.state.as_str()) {
            return Ok(held_line(
                "refused",
                &job,
                &format!("{} does not release", job.state),
            ));
        }
        c.execute(
            "UPDATE jobs SET state = 'released', updated_at = ?1 WHERE id = ?2",
            params![now, id],
        )
        .q()?;
        history(c, id, now, Some(&job.state), "released", FACTORY, None)?;
        Ok(line(
            "released",
            &[
                ("job", j(id)),
                ("issue", j(job.issue)),
                ("state", j("released")),
                ("said", j(if late { "a superseded attempt" } else { "" })),
            ],
        ))
    })
}

fn stop(home: &Path, id: i64, now: i64) -> Result<String, Fail> {
    work(home, move |c| {
        let job = held(c, id)?;
        // The attempt is asked before the state, as in `job-finish`.
        if stale(c, &job)? {
            return Ok(stale_line(&job));
        }
        if job.state != "running" {
            return Ok(held_line(
                "refused",
                &job,
                &format!("{} does not stop", job.state),
            ));
        }
        c.execute(
            "UPDATE jobs SET state = 'stopping', updated_at = ?1 WHERE id = ?2",
            params![now, id],
        )
        .q()?;
        history(c, id, now, Some(&job.state), "stopping", FACTORY, None)?;
        Ok(line(
            "stopping",
            &[
                ("job", j(id)),
                ("issue", j(job.issue)),
                ("state", j("stopping")),
            ],
        ))
    })
}

fn lose(home: &Path, id: i64, reason: &str, rest: i64, now: i64) -> Result<String, Fail> {
    let reason = reason.to_string();
    work(home, move |c| {
        let job = held(c, id)?;
        let counted = lose_one(c, id, &reason, FACTORY, rest, now)?;
        Ok(line(
            "lost",
            &[
                ("job", j(id)),
                ("issue", j(job.issue)),
                ("reason", j(reason.as_str())),
                ("said", j(if counted { "counted" } else { "again" })),
            ],
        ))
    })
}

/// One job lost: its row, its history row, and the three counters a loss moves
/// (the contractor's lost jobs, its rest period, and the issue's lost
/// attempts). The sweep, `job-lose` and a revocation all come here, so a loss
/// is counted once whoever saw it. answers whether anything was counted: a job
/// lost already moves nothing again.
fn lose_one(
    c: &Connection,
    id: i64,
    reason: &str,
    said_by: &str,
    rest: i64,
    now: i64,
) -> Result<bool, Fail> {
    let job = held(c, id)?;
    if job.state == "lost" {
        return Ok(false);
    }
    c.execute(
        "UPDATE jobs SET state = 'lost', lost_reason = ?1, updated_at = ?2 WHERE id = ?3",
        params![reason, now, id],
    )
    .q()?;
    history(c, id, now, Some(&job.state), "lost", said_by, Some(reason))?;
    // The counters go up once, with the row. A job lost by the factory is a
    // loss of the machine, not of the contractor, and costs it no rest period:
    // the rest of `job-lose` belongs to the contractor that let a job it had
    // taken go quiet, and the poll's sweep passes none.
    let resting = now.checked_add(rest);
    let (counted, params): (&str, Vec<&dyn rusqlite::ToSql>) = if rest > 0 {
        (
            "UPDATE contractors SET jobs_lost = jobs_lost + 1, resting_until = ?1 WHERE name = ?2",
            vec![&resting, &job.contractor],
        )
    } else {
        (
            "UPDATE contractors SET jobs_lost = jobs_lost + 1 WHERE name = ?1",
            vec![&job.contractor],
        )
    };
    c.execute(counted, rusqlite::params_from_iter(params)).q()?;
    c.execute(
        "UPDATE issue_attempts SET lost = lost + 1 WHERE issue = ?1",
        params![job.issue],
    )
    .q()?;
    Ok(true)
}

/// The row of job_history of one move. A row's `at` is the second the move was
/// said, so a restart replays a job's life in the order it happened.
fn history(
    c: &Connection,
    job: i64,
    at: i64,
    from: Option<&str>,
    to: &str,
    said_by: &str,
    detail: Option<&str>,
) -> Result<(), Fail> {
    let detail = detail.map(|d| tehas_core::jq::first(d, DETAIL_MAX));
    c.execute(
        "INSERT INTO job_history (job, at, from_state, to_state, said_by, detail)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![job, at, from, to, said_by, detail],
    )
    .q()?;
    Ok(())
}

/// A job as this module reads it, with `unknown` in the state column of a row
/// the schema's CHECK should have kept one of `STATES`.
#[derive(Debug)]
struct Job {
    id: i64,
    issue: i64,
    contractor: String,
    attempt: i64,
    state: String,
}

fn held(c: &Connection, id: i64) -> Result<Job, Fail> {
    c.query_row(
        "SELECT id, issue, contractor, attempt, state FROM jobs WHERE id = ?1",
        params![id],
        read_job,
    )
    .optional()
    .q()?
    .ok_or_else(|| (1, format!("store: nojob {id}")))
}

fn read_job(r: &Row<'_>) -> rusqlite::Result<Job> {
    let state: String = r.get(4)?;
    let state = STATES
        .iter()
        .find(|s| **s == state)
        .map_or_else(|| "unknown".to_string(), |s| (*s).to_string());
    Ok(Job {
        id: r.get(0)?,
        issue: r.get(1)?,
        contractor: r.get(2)?,
        attempt: r.get(3)?,
        state,
    })
}

/// The issue's current job: the open one of its highest attempt, which is the
/// one a result is honoured for. None when the issue has no job open.
fn current(c: &Connection, issue: i64) -> Result<Option<Job>, Fail> {
    let marks = OPEN.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let mut args: Vec<&dyn rusqlite::ToSql> = vec![&issue];
    args.extend(OPEN.iter().map(|s| s as &dyn rusqlite::ToSql));
    c.query_row(
        &format!(
            "SELECT id, issue, contractor, attempt, state FROM jobs
               WHERE issue = ?1 AND state IN ({marks})
                 ORDER BY attempt DESC LIMIT 1"
        ),
        rusqlite::params_from_iter(args),
        read_job,
    )
    .optional()
    .q()
}

/// Whether a job is a contractor's late answer to an offer the issue has moved
/// past: "Durability" 2, the store's own refusal of it. Only a job still held
/// by the hand that was given it can be late: a job the store has already
/// closed out is refused by the state it ended in, which is the truer word, so
/// a `lost`, `finished` or `released` row is never called stale.
fn superseded(c: &Connection, job: &Job) -> Result<bool, Fail> {
    // The attempt a word of a contractor is honoured for is the issue's highest,
    // which its last offer raised. A row of any older attempt is of work the
    // factory has stopped believing in, whatever the row itself says now.
    let highest: i64 = c
        .query_row(
            "SELECT coalesce(max(attempt), 0) FROM jobs WHERE issue = ?1",
            params![job.issue],
            |r| r.get(0),
        )
        .q()?;
    Ok(job.attempt != highest)
}

/// Whether a call about `job` comes too late to do anything. The row of a
/// superseded attempt is always too late, in whichever state it ended: the
/// question a late contractor asks is which attempt its work belongs to, and
/// the answer is the same word whether the row is running or was lost a minute
/// ago. A row of the current attempt is worked on while it is open and refused
/// by its state once it is not: each verb asks first whether the state lets
/// the call be made at all, and a job the factory has already closed out is
/// refused by what it is.
fn stale(c: &Connection, job: &Job) -> Result<bool, Fail> {
    superseded(c, job)
}

fn stale_line(job: &Job) -> String {
    line(
        "stale",
        &[
            ("job", j(job.id)),
            ("issue", j(job.issue)),
            ("attempt", j(job.attempt)),
            ("said", j("an attempt of this issue has replaced it")),
        ],
    )
}

fn held_line(word: &str, job: &Job, why: &str) -> String {
    line(
        word,
        &[
            ("job", j(job.id)),
            ("issue", j(job.issue)),
            ("state", j(job.state.as_str())),
            ("said", j(why)),
        ],
    )
}

fn show(home: &Path, s: &JobShow) -> Result<String, Fail> {
    look(home, move |c| {
        let job = match s {
            JobShow::Job(id) => held(c, *id)?,
            JobShow::Issue(issue) => {
                current(c, *issue)?.ok_or_else(|| (1, format!("store: nojob of issue {issue}")))?
            }
        };
        let mut rows = c
            .prepare(
                "SELECT at, from_state, to_state, said_by, detail FROM job_history
                   WHERE job = ?1 ORDER BY at, id",
            )
            .q()?;
        let taken = rows
            .query_map(params![job.id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<String>>(4)?,
                ))
            })
            .q()?;
        let mut history = Vec::new();
        for row in taken {
            let (at, from, to, said_by, detail) = row.q()?;
            history.push(obj(&[
                ("at", j(at)),
                ("from", j(from)),
                ("to", j(to)),
                ("by", j(said_by)),
                ("detail", j(detail)),
            ]));
        }
        let (expires, check_in, reason, sha, updated): (
            i64,
            Option<i64>,
            Option<String>,
            Option<String>,
            i64,
        ) = c
            .query_row(
                "SELECT expires_at, last_check_in_at, lost_reason, result_sha256, updated_at
                   FROM jobs WHERE id = ?1",
                params![job.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()
            .q()?
            .ok_or_else(|| (1, format!("store: nojob {}", job.id)))?;
        Ok(line(
            "job",
            &[
                ("job", j(job.id)),
                ("issue", j(job.issue)),
                ("attempt", j(job.attempt)),
                ("contractor", j(job.contractor.as_str())),
                ("state", j(job.state.as_str())),
                ("expires_at", j(expires)),
                ("last_check_in_at", j(check_in)),
                ("lost_reason", j(reason)),
                ("result_sha256", j(sha)),
                ("updated_at", j(updated)),
                ("history", j(history)),
            ],
        ))
    })
}

/// The jobs of one contractor that are still open.
fn open_of(c: &Connection, contractor: &str) -> Result<Vec<i64>, Fail> {
    let marks = OPEN.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let mut args: Vec<&dyn rusqlite::ToSql> = vec![&contractor];
    args.extend(OPEN.iter().map(|s| s as &dyn rusqlite::ToSql));
    let mut rows = c
        .prepare(&format!(
            "SELECT id FROM jobs WHERE contractor = ?1 AND state IN ({marks}) ORDER BY id"
        ))
        .q()?;
    let taken = rows
        .query_map(rusqlite::params_from_iter(args), |r| r.get::<_, i64>(0))
        .q()?;
    let mut out = Vec::new();
    for row in taken {
        out.push(row.q()?);
    }
    Ok(out)
}

/// The sweep of the spec's recovery and "Durability" 5: the jobs that died
/// while the factory was down, and the ones that die now. One call, run once
/// on start with --now and again every poll, so the jobs that expired in the
/// outage are sorted out by the same call that sorts out the ones that expire
/// now and no verb can lose a job by forgetting to.
///
///   an `offered` job past its expiry is lost with reason `expired`;
///   a `running` or `stopping` job whose last check-in is at or before now
///   minus the timeout is lost with reason `timeout`; a job that never
///   checked in is measured from the second it was offered;
///   the open jobs of a revoked contractor are lost with reason `revoked`,
///   which is what keeps a contractor taken away between two polls from
///   holding a job until a person notices;
///   --now closes out the invite codes nobody joined with, so the start of the
///   factory is the one moment the codes of an outage are settled.
///
/// A lost job is given no rest period here: the rest period is the instance's
/// setting, and the command that loaded it passes it as the --rest of
/// `job-lose`. The sweep counts what was lost, as every loss does.
fn sweep(home: &Path, timeout: i64, at_start: bool, now: i64) -> Result<String, Fail> {
    work(home, move |c| {
        let mut dead = Vec::new();
        {
            let mut rows = c
                .prepare(
                    "SELECT id, state FROM jobs WHERE
                       (state = 'offered' AND expires_at <= ?1)
                       OR (state IN ('running','stopping')
                             AND coalesce(last_check_in_at, offered_at) <= ?2)
                     ORDER BY id",
                )
                .q()?;
            let taken = rows
                .query_map(params![now, now - timeout], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
                })
                .q()?;
            for row in taken {
                dead.push(row.q()?);
            }
        }
        let mut lost = 0;
        for (id, state) in dead {
            let reason = if state == "offered" {
                "expired"
            } else {
                "timeout"
            };
            if lose_one(c, id, reason, FACTORY, 0, now)? {
                lost += 1;
            }
        }
        let mut names = Vec::new();
        {
            let mut rows = c
                .prepare("SELECT name FROM contractors WHERE revoked_at IS NOT NULL ORDER BY name")
                .q()?;
            let taken = rows.query_map([], |r| r.get::<_, String>(0)).q()?;
            for row in taken {
                names.push(row.q()?);
            }
        }
        let mut ended = 0;
        for name in names {
            for id in open_of(c, &name)? {
                if lose_one(c, id, "revoked", FACTORY, 0, now)? {
                    ended += 1;
                }
            }
        }
        let codes = if at_start { expire_codes(c, now)? } else { 0 };
        Ok(line(
            "swept",
            &[
                ("lost", j(lost)),
                ("revoked", j(ended)),
                ("codes", j(codes)),
            ],
        ))
    })
}

/// What the outsourced attempts of an issue have cost, which is what a later
/// part reads for "this issue has spent its outsourced attempts". The retry
/// cap is the instance's setting and the caller that loaded it makes the
/// comparison; this only says what was spent, and `open` says whether a job of
/// the issue is still going.
fn attempts(home: &Path, issue: i64) -> Result<String, Fail> {
    look(home, move |c| {
        let held: Option<(i64, i64, Option<i64>)> = c
            .query_row(
                "SELECT given, lost, last_given_at FROM issue_attempts WHERE issue = ?1",
                params![issue],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .q()?;
        let (given, lost, last) = held.unwrap_or((0, 0, None));
        let open = current(c, issue)?.is_some();
        Ok(line(
            "attempts",
            &[
                ("issue", j(issue)),
                ("given", j(given)),
                ("lost", j(lost)),
                ("open", j(open)),
                ("last_given_at", j(last)),
            ],
        ))
    })
}

/// The refusal of a name the store holds no contractor for.
fn unknown(name: &str) -> Fail {
    (1, format!("store: unknown {name}"))
}

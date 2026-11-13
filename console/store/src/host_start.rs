//! The starts approved from the host (#1967), one row of `host_starts` per
//! issue:
//!
//!   host-start-set <issue> --at <unix>
//!   host-start-clear <issue>
//!   host-start-list
//!
//! A row is one start still owed, given by `tehas start <n>` and not by a
//! GitHub login. The poll starts the issue under its usual rules and records
//! the start as an approval of how `host`; `cycle-start` takes the row off
//! with any start of the issue, so one approval starts one cycle.
//!
//! `host-start-set` keeps a row that is there, its time included, so asking
//! twice changes nothing; `new` says whether this call wrote it. Each answers
//! {"api":1,"verb":...,"ok":true,...}.

use crate::verbs::{self, j, line, list, obj, stub, Args, Fail, Kind, Q};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub enum Call {
    Set { issue: i64, at: i64 },
    Clear { issue: i64 },
    List,
}

pub fn parse(name: &str, rest: &[String]) -> Option<Result<Call, String>> {
    Some(match name {
        "host-start-set" => Args::parse("host-start-set", rest, &[("--at", Kind::Value)], true)
            .and_then(|a| {
                Ok(Call::Set {
                    issue: a.issue()?,
                    at: a.strict_need("--at", "a Unix time", verbs::unix)?,
                })
            }),
        "host-start-clear" => Args::parse("host-start-clear", rest, &[], true)
            .and_then(|a| Ok(Call::Clear { issue: a.issue()? })),
        "host-start-list" => Args::parse("host-start-list", rest, &[], false).map(|_| Call::List),
        _ => return None,
    })
}

fn row(issue: i64, at: i64) -> String {
    obj(&[("issue", j(issue)), ("at", j(at))])
}

fn held(c: &Connection, issue: i64) -> Result<Option<i64>, Fail> {
    c.query_row(
        "SELECT at FROM host_starts WHERE issue = ?1",
        [issue],
        |r| r.get(0),
    )
    .optional()
    .q()
}

pub fn run(home: &Path, call: &Call) -> Result<String, Fail> {
    match call {
        Call::Set { issue, at } => verbs::write(home, |c| {
            let (at, new) = match held(c, *issue)? {
                Some(was) => (was, false),
                None => {
                    stub(c, *issue)?;
                    c.execute(
                        "INSERT INTO host_starts (issue, at) VALUES (?1, ?2)",
                        params![issue, at],
                    )
                    .q()?;
                    (*at, true)
                }
            };
            Ok(line(
                "host-start-set",
                &[("host_start", row(*issue, at)), ("new", j(new))],
            ))
        }),
        Call::Clear { issue } => verbs::write(home, |c| {
            let was = held(c, *issue)?;
            c.execute("DELETE FROM host_starts WHERE issue = ?1", [issue])
                .q()?;
            Ok(line(
                "host-start-clear",
                &[
                    ("cleared", j(was.is_some())),
                    (
                        "host_start",
                        was.map_or("null".into(), |at| row(*issue, at)),
                    ),
                ],
            ))
        }),
        Call::List => verbs::look(home, |c| {
            let mut q = c
                .prepare("SELECT issue, at FROM host_starts ORDER BY issue")
                .q()?;
            let rows = q
                .query_map([], |r| Ok(row(r.get(0)?, r.get(1)?)))
                .q()?
                .collect::<Result<Vec<_>, _>>()
                .q()?;
            Ok(line("host-start-list", &[("host_starts", list(&rows))]))
        }),
    }
}

/// Run by cycle-start: whatever started the issue spent the host's approval.
pub fn spend(c: &Connection, issue: i64) -> Result<(), Fail> {
    c.execute("DELETE FROM host_starts WHERE issue = ?1", [issue])
        .q()
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_verbs_read_their_words() {
        assert_eq!(
            parse("host-start-set", &w(&["12", "--at", "99"])),
            Some(Ok(Call::Set { issue: 12, at: 99 }))
        );
        assert_eq!(
            parse("host-start-clear", &w(&["12"])),
            Some(Ok(Call::Clear { issue: 12 }))
        );
        assert_eq!(parse("host-start-list", &[]), Some(Ok(Call::List)));
        for (verb, bad) in [
            ("host-start-set", &["12"][..]),
            ("host-start-set", &["--at", "99"]),
            ("host-start-set", &["12", "--at", "soon"]),
            ("host-start-set", &["x", "--at", "99"]),
            ("host-start-clear", &[]),
            ("host-start-list", &["12"]),
        ] {
            assert!(
                matches!(parse(verb, &w(bad)), Some(Err(_))),
                "{verb} {bad:?}"
            );
        }
        assert_eq!(parse("host-start", &[]), None);
    }
}

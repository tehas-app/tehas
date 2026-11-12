//! The start order, one row of `start_order` (tehas_core::order says what it
//! holds and how a reading reads):
//!
//!   order-show
//!   order-set --issues N[,N...] [--only] [--until <unix>] [--reason TEXT]
//!   order-clear
//!
//! Each answers {"api":1,"verb":...,"ok":true,"order":{...}}, the order as it
//! then is, read at the moment of the call. `order-set` replaces the whole
//! row; the same order twice leaves the row as it was, `set_at` included.

use crate::verbs::{self, line, Args, Fail, Kind, Q};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use tehas_core::order::{list, Order, Reading};

#[derive(Debug, PartialEq, Eq)]
pub enum Call {
    Show,
    Set(Order),
    Clear,
}

pub fn parse(name: &str, rest: &[String]) -> Option<Result<Call, String>> {
    Some(match name {
        "order-show" => Args::parse("order-show", rest, &[], false).map(|_| Call::Show),
        "order-clear" => Args::parse("order-clear", rest, &[], false).map(|_| Call::Clear),
        "order-set" => Args::parse(
            "order-set",
            rest,
            &[
                ("--issues", Kind::Value),
                ("--only", Kind::Switch),
                ("--until", Kind::Value),
                ("--reason", Kind::Value),
            ],
            false,
        )
        .and_then(|a| {
            let issues = list(a.need("--issues")?)
                .ok_or_else(|| a.usage("--issues", "is a list such as 12,8"))?;
            let until = match a.value("--until") {
                None => None,
                Some(u) => Some(
                    verbs::number(u)
                        .filter(|n| *n > 0)
                        .ok_or_else(|| a.usage("--until", "is a Unix time"))?,
                ),
            };
            let o = Order {
                issues,
                only: a.has("--only"),
                until,
                reason: a.value("--reason").map(str::to_string),
            };
            o.check().map_err(|why| a.usage("--issues", &why))?;
            Ok(Call::Set(o))
        }),
        _ => return None,
    })
}

pub fn run(home: &Path, call: &Call, now: i64) -> Result<String, Fail> {
    let (verb, reading) = match call {
        Call::Show => ("order-show", verbs::look(home, |c| read(c, now))?),
        Call::Clear => (
            "order-clear",
            verbs::write(home, |c| {
                c.execute("DELETE FROM start_order", []).q()?;
                read(c, now)
            })?,
        ),
        Call::Set(o) => (
            "order-set",
            verbs::write(home, |c| {
                let issues = serde_json::to_string(&o.issues).unwrap_or_default();
                let only = i64::from(o.only);
                let same: Option<i64> = c
                    .query_row(
                        "SELECT 1 FROM start_order WHERE id = 1 AND issues_json = ?1 AND only = ?2
                         AND until_at IS ?3 AND reason IS ?4",
                        params![issues, only, o.until, o.reason],
                        |r| r.get(0),
                    )
                    .optional()
                    .q()?;
                if same.is_none() {
                    c.execute(
                        "INSERT OR REPLACE INTO start_order (id, issues_json, only, until_at, reason, set_at)
                         VALUES (1, ?1, ?2, ?3, ?4, ?5)",
                        params![issues, only, o.until, o.reason, now],
                    )
                    .q()?;
                }
                read(c, now)
            })?,
        ),
    };
    Ok(line(verb, &[("order", reading.json())]))
}

/// The row as a reading at `now`. A row the store cannot make an order of is
/// a damaged store.
pub fn read(c: &Connection, now: i64) -> Result<Reading, Fail> {
    let row: Option<(String, i64, Option<i64>, Option<String>)> = c
        .query_row(
            "SELECT issues_json, only, until_at, reason FROM start_order WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .q()?;
    let Some((issues, only, until, reason)) = row else {
        return Ok(Reading::at(None, now));
    };
    let issues: Vec<u64> = serde_json::from_str(&issues)
        .map_err(|_| verbs::damaged("the start order's issues are not a list of numbers"))?;
    let o = Order {
        issues,
        only: only == 1,
        until,
        reason,
    };
    o.check()
        .map_err(|why| verbs::damaged(&format!("the start order on record: {why}")))?;
    Ok(Reading::at(Some(o), now))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn set_reads_its_words_and_refuses_what_it_cannot_keep() {
        assert_eq!(
            parse(
                "order-set",
                &w(&["--issues", "12,8", "--only", "--until", "99", "--reason", "x"])
            ),
            Some(Ok(Call::Set(Order {
                issues: vec![12, 8],
                only: true,
                until: Some(99),
                reason: Some("x".into()),
            })))
        );
        assert_eq!(
            parse("order-set", &w(&["--issues", "", "--only"])),
            Some(Ok(Call::Set(Order {
                only: true,
                ..Order::default()
            })))
        );
        for bad in [
            &[][..],
            &["--issues", "1,1"],
            &["--issues", "0"],
            &["--issues", "a"],
            &["--issues", "1", "--until", "soon"],
            &["--issues", "1", "--until", "0"],
            &["--issues", "1", "--reason", " x"],
        ] {
            assert!(
                matches!(parse("order-set", &w(bad)), Some(Err(_))),
                "{bad:?}"
            );
        }
        assert_eq!(parse("order-show", &[]), Some(Ok(Call::Show)));
        assert!(matches!(parse("order-show", &w(&["x"])), Some(Err(_))));
        assert_eq!(parse("order-sort", &[]), None);
    }
}

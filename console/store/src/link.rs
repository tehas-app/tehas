//! The links of a ticket to its blueprints (ports spec, "The links", T4.8):
//! the one writer of the table `links`.
//!
//!   store link-record <blueprint> --ticket <key> --by <account>
//!                     --realm code-host|tickets --how opened|linked
//!     The link of the blueprint to the ticket, made by the account of that
//!     realm, now. `opened`: tehas opened the blueprint for the ticket;
//!     `linked`: an allowed account's act, or the operator's `tehas link`.
//!     A blueprint has one ticket. A link already on record for the same
//!     ticket stands as it was written, and the answer says nothing changed;
//!     one for another ticket is refused, and stays.
//!   store link-record <blueprint> --ticket <key> --by <account>
//!                     --realm code-host|tickets --remove
//!     The same act taking the link away: the row goes. No link is nothing
//!     to take away; a link to another ticket is refused, and stays.
//!
//! The answer is the row on record after the call, null for none, and
//! whether the call changed it. Where ticket and blueprint are one item, as
//! on every setup 2.0 runs, the link is answered without a row
//! (`tehas_ports::SameItem`), so `same-item` is no how this verb takes.

use crate::verbs::{self, j, line, obj, refuse, row_json, Args, Fail, Kind, Q};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use tehas_ports::KeyText;

/// The realms an account that made a link is of, as the table spells them.
const REALMS: [(&str, &str); 2] = [("code-host", "code-host"), ("tickets", "tickets")];

/// The ways a row is made, as the table spells them.
const HOWS: [(&str, &str); 2] = [("opened", "opened"), ("linked", "linked")];

#[derive(Debug, PartialEq, Eq)]
pub struct Call {
    pub blueprint: i64,
    pub ticket: String,
    pub by: String,
    pub realm: &'static str,
    /// None takes the link away.
    pub how: Option<&'static str>,
}

pub fn parse(name: &str, rest: &[String]) -> Option<Result<Call, String>> {
    (name == "link-record").then(|| record_args(rest))
}

/// A ticket's key as the ports check it: kept whole, never split.
fn ticket_key(s: &str) -> Option<String> {
    KeyText::new(s).ok().map(|k| k.as_str().to_string())
}

fn record_args(rest: &[String]) -> Result<Call, String> {
    let a = Args::parse(
        "link-record",
        rest,
        &[
            ("--ticket", Kind::Value),
            ("--by", Kind::Value),
            ("--realm", Kind::Value),
            ("--how", Kind::Value),
            ("--remove", Kind::Switch),
        ],
        true,
    )?;
    let blueprint = a.issue()?;
    let ticket = a.strict_need("--ticket", "a ticket's key", ticket_key)?;
    let by = a.strict_need("--by", "an account", verbs::account)?;
    a.need("--realm")?;
    let realm = a.word("--realm", &REALMS)?.expect("--realm was given");
    let how = a.word("--how", &HOWS)?;
    match (how, a.has("--remove")) {
        (Some(_), true) => return Err(a.usage("--how", "is not taken with --remove")),
        (None, false) => return Err(a.usage("--how", "is required, or --remove")),
        _ => {}
    }
    Ok(Call {
        blueprint,
        ticket,
        by,
        realm,
        how,
    })
}

pub fn run(home: &Path, call: &Call, now: i64) -> Result<String, Fail> {
    verbs::write(home, |c| record(c, call, now))
}

const NAMES: [&str; 6] = [
    "ticket",
    "blueprint",
    "made_by",
    "made_by_realm",
    "made_at",
    "how",
];

/// The blueprint's row on record, as the answer gives it, and its ticket.
fn on_record(c: &Connection, blueprint: i64) -> Result<Option<(String, String)>, Fail> {
    c.query_row(
        "SELECT ticket, blueprint, made_by, made_by_realm, made_at, how
         FROM links WHERE blueprint = ?1",
        [blueprint],
        |r| Ok((obj(&row_json(r, &NAMES)?), r.get(0)?)),
    )
    .optional()
    .q()
}

fn record(c: &Connection, call: &Call, now: i64) -> Result<String, Fail> {
    let held = on_record(c, call.blueprint)?;
    if let Some((_, ticket)) = &held {
        if *ticket != call.ticket {
            return Err(refuse(
                "link-record",
                call.blueprint,
                "the blueprint is linked to another ticket; take that link away first",
            ));
        }
    }
    let changed = match (call.how, &held) {
        (Some(_), Some(_)) | (None, None) => false,
        (Some(how), None) => {
            c.execute(
                "INSERT INTO links (ticket, blueprint, made_by, made_by_realm, made_at, how)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![call.ticket, call.blueprint, call.by, call.realm, now, how],
            )
            .q()?;
            true
        }
        (None, Some(_)) => {
            c.execute("DELETE FROM links WHERE blueprint = ?1", [call.blueprint])
                .q()?;
            true
        }
    };
    let now_held = on_record(c, call.blueprint)?.map_or("null".to_string(), |(row, _)| row);
    Ok(line(
        "link-record",
        &[("link", now_held), ("changed", j(changed))],
    ))
}

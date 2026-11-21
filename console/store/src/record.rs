//! A plugin's records, the rows of `plugin_records` (plugin spec 1.0.0, the
//! v3 part, "Records"). A plugin asks; tehas checks the grant and writes:
//!
//!   record-put --plugin <name> --id <id> --kind <kind> --by <login>
//!   record-file --plugin <name> --id <id> --issue N --files-sha256 <sha256> --by <login> --by-id N
//!   record-close --plugin <name> --id <id>
//!   records [--plugin <name>] [--issue N] [--open]
//!
//! Each answers {"api":1,"verb":...,"ok":true,...}: the three writers with
//! the record as it then is, `records` with a list in the order the records
//! were made. A record is made once: `record-put` of a record that is there
//! with the same kind answers it as it is. It is filed once, with the issue
//! and the hash of its files together, and closed once; a write that would
//! change a filed or closed record is 1 `conflict`, and a record that is not
//! there is 1 `norecord`.
//!
//! `--by` of record-file is the person who filed the issue: tehas gives it,
//! from the gate's signed-in session at the press or a person at the shell,
//! never from what a plugin asked or a run wrote.

use crate::verbs::{self, line, Args, Fail, Kind, Q};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::{json, Value};
use std::path::Path;
use tehas_core::plugin::is_name as plugin_name;

#[derive(Debug, PartialEq, Eq)]
pub enum Call {
    Put {
        plugin: String,
        id: String,
        kind: String,
        by: String,
    },
    File {
        plugin: String,
        id: String,
        issue: i64,
        sha256: String,
        by: String,
        by_id: i64,
    },
    Close {
        plugin: String,
        id: String,
    },
    List {
        plugin: Option<String>,
        issue: Option<i64>,
        open: bool,
    },
}

/// A record's id: `a-z`, `0-9` and `-`, 1 to 32, not starting or ending
/// with `-`, so it is safe as a directory name.
pub fn record_id(s: &str) -> bool {
    (1..=32).contains(&s.len())
        && !s.starts_with('-')
        && !s.ends_with('-')
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// A record's kind, as the manifest names it: `a-z` and `-`, 1 to 16.
pub fn record_kind(s: &str) -> bool {
    (1..=16).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
}

fn key(a: &Args) -> Result<(String, String), String> {
    let plugin = a.need("--plugin")?;
    if !plugin_name(plugin) {
        return Err(a.usage(
            "--plugin",
            "is a plugin's name: a-z and 0-9, 1 to 12, a letter first",
        ));
    }
    let id = a.need("--id")?;
    if !record_id(id) {
        return Err(a.usage("--id", "is a-z, 0-9 and -, 1 to 32"));
    }
    Ok((plugin.to_string(), id.to_string()))
}

pub fn parse(name: &str, rest: &[String]) -> Option<Result<Call, String>> {
    Some(match name {
        "record-put" => Args::parse(
            "record-put",
            rest,
            &[
                ("--plugin", Kind::Value),
                ("--id", Kind::Value),
                ("--kind", Kind::Value),
                ("--by", Kind::Value),
            ],
            false,
        )
        .and_then(|a| {
            let (plugin, id) = key(&a)?;
            let kind = a.need("--kind")?;
            if !record_kind(kind) {
                return Err(a.usage("--kind", "is a-z and -, 1 to 16"));
            }
            let by = verbs::account(a.need("--by")?)
                .ok_or_else(|| a.usage("--by", "is a GitHub login"))?;
            Ok(Call::Put {
                plugin,
                id,
                kind: kind.to_string(),
                by,
            })
        }),
        "record-file" => Args::parse(
            "record-file",
            rest,
            &[
                ("--plugin", Kind::Value),
                ("--id", Kind::Value),
                ("--issue", Kind::Value),
                ("--files-sha256", Kind::Value),
                ("--by", Kind::Value),
                ("--by-id", Kind::Value),
            ],
            false,
        )
        .and_then(|a| {
            let (plugin, id) = key(&a)?;
            let issue = verbs::number(a.need("--issue")?)
                .filter(|n| *n > 0)
                .ok_or_else(|| a.usage("--issue", "is an issue number"))?;
            let sha256 = verbs::sha256(a.need("--files-sha256")?)
                .ok_or_else(|| a.usage("--files-sha256", "is 64 lowercase hex"))?;
            let by = verbs::account(a.need("--by")?)
                .ok_or_else(|| a.usage("--by", "is a GitHub login"))?;
            let by_id = verbs::number(a.need("--by-id")?)
                .filter(|n| *n > 0)
                .ok_or_else(|| a.usage("--by-id", "is a GitHub user id"))?;
            Ok(Call::File {
                plugin,
                id,
                issue,
                sha256,
                by,
                by_id,
            })
        }),
        "record-close" => Args::parse(
            "record-close",
            rest,
            &[("--plugin", Kind::Value), ("--id", Kind::Value)],
            false,
        )
        .and_then(|a| {
            let (plugin, id) = key(&a)?;
            Ok(Call::Close { plugin, id })
        }),
        "records" => Args::parse(
            "records",
            rest,
            &[
                ("--plugin", Kind::Value),
                ("--issue", Kind::Value),
                ("--open", Kind::Switch),
            ],
            false,
        )
        .and_then(|a| {
            let plugin = match a.value("--plugin") {
                None => None,
                Some(p) if plugin_name(p) => Some(p.to_string()),
                Some(_) => return Err(a.usage("--plugin", "is a plugin's name")),
            };
            let issue = match a.value("--issue") {
                None => None,
                Some(n) => Some(
                    verbs::number(n)
                        .filter(|n| *n > 0)
                        .ok_or_else(|| a.usage("--issue", "is an issue number"))?,
                ),
            };
            Ok(Call::List {
                plugin,
                issue,
                open: a.has("--open"),
            })
        }),
        _ => return None,
    })
}

const COLUMNS: &str =
    "plugin, id, kind, issue, files_sha256, created_at, created_by, filed_at, closed_at, filed_by, filed_by_id";

fn row(r: &Row) -> rusqlite::Result<Value> {
    Ok(json!({
        "plugin": r.get::<_, String>(0)?,
        "id": r.get::<_, String>(1)?,
        "kind": r.get::<_, String>(2)?,
        "issue": r.get::<_, Option<i64>>(3)?,
        "files_sha256": r.get::<_, Option<String>>(4)?,
        "created_at": r.get::<_, i64>(5)?,
        "created_by": r.get::<_, String>(6)?,
        "filed_at": r.get::<_, Option<i64>>(7)?,
        "closed_at": r.get::<_, Option<i64>>(8)?,
        "filed_by": r.get::<_, Option<String>>(9)?,
        "filed_by_id": r.get::<_, Option<i64>>(10)?,
    }))
}

fn one(c: &Connection, plugin: &str, id: &str) -> Result<Option<Value>, Fail> {
    c.query_row(
        &format!("SELECT {COLUMNS} FROM plugin_records WHERE plugin = ?1 AND id = ?2"),
        params![plugin, id],
        row,
    )
    .optional()
    .q()
}

fn there(c: &Connection, plugin: &str, id: &str) -> Result<Value, Fail> {
    one(c, plugin, id)?.ok_or_else(|| (1, format!("norecord: no record {plugin}/{id}")))
}

fn conflict(plugin: &str, id: &str, why: &str) -> Fail {
    (1, format!("conflict: record {plugin}/{id} {why}"))
}

pub fn run(home: &Path, call: &Call, now: i64) -> Result<String, Fail> {
    match call {
        Call::Put {
            plugin,
            id,
            kind,
            by,
        } => {
            let rec = verbs::write(home, |c| {
                if let Some(rec) = one(c, plugin, id)? {
                    if rec["kind"] != json!(kind) {
                        return Err(conflict(plugin, id, "is of another kind"));
                    }
                    return Ok(rec);
                }
                c.execute(
                    "INSERT INTO plugin_records (plugin, id, kind, created_at, created_by)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![plugin, id, kind, now, by],
                )
                .q()?;
                there(c, plugin, id)
            })?;
            Ok(line("record-put", &[("record", rec.to_string())]))
        }
        Call::File {
            plugin,
            id,
            issue,
            sha256,
            by,
            by_id,
        } => {
            let rec = verbs::write(home, |c| {
                let rec = there(c, plugin, id)?;
                if !rec["closed_at"].is_null() {
                    return Err(conflict(plugin, id, "is closed"));
                }
                if !rec["issue"].is_null() {
                    if rec["issue"] == json!(issue)
                        && rec["files_sha256"] == json!(sha256)
                        && rec["filed_by"] == json!(by)
                        && rec["filed_by_id"] == json!(by_id)
                    {
                        return Ok(rec);
                    }
                    return Err(conflict(plugin, id, "is filed already"));
                }
                c.execute(
                    "UPDATE plugin_records SET issue = ?3, files_sha256 = ?4, filed_at = ?5, filed_by = ?6,
                     filed_by_id = ?7 WHERE plugin = ?1 AND id = ?2",
                    params![plugin, id, issue, sha256, now, by, by_id],
                )
                .q()?;
                there(c, plugin, id)
            })?;
            Ok(line("record-file", &[("record", rec.to_string())]))
        }
        Call::Close { plugin, id } => {
            let rec = verbs::write(home, |c| {
                let rec = there(c, plugin, id)?;
                if !rec["closed_at"].is_null() {
                    return Ok(rec);
                }
                c.execute(
                    "UPDATE plugin_records SET closed_at = ?3 WHERE plugin = ?1 AND id = ?2",
                    params![plugin, id, now],
                )
                .q()?;
                there(c, plugin, id)
            })?;
            Ok(line("record-close", &[("record", rec.to_string())]))
        }
        Call::List {
            plugin,
            issue,
            open,
        } => {
            let list = verbs::look(home, |c| {
                let mut q = c
                    .prepare(&format!(
                        "SELECT {COLUMNS} FROM plugin_records
                         WHERE (?1 IS NULL OR plugin = ?1) AND (?2 IS NULL OR issue = ?2)
                           AND (?3 = 0 OR closed_at IS NULL)
                         ORDER BY created_at, plugin, id"
                    ))
                    .q()?;
                let rows = q
                    .query_map(params![plugin, issue, i64::from(*open)], row)
                    .q()?;
                rows.collect::<rusqlite::Result<Vec<Value>>>().q()
            })?;
            Ok(line(
                "records",
                &[("records", Value::from(list).to_string())],
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn names_ids_and_kinds_keep_to_their_grammar() {
        for good in ["pageticket", "specs", "a", "floorplan2"] {
            assert!(plugin_name(good), "{good}");
        }
        for bad in ["", "page-ticket", "Page", "2plan", "abcdefghijklm"] {
            assert!(!plugin_name(bad), "{bad}");
        }
        for good in ["p-1a2b", "a", "x9"] {
            assert!(record_id(good), "{good}");
        }
        for bad in ["", "-a", "a-", "a/b", "..", "A", &"a".repeat(33)] {
            assert!(!record_id(bad), "{bad}");
        }
        assert!(record_kind("prototype"));
        assert!(!record_kind("proto1"));
        assert!(!record_kind(&"a".repeat(17)));
    }

    #[test]
    fn each_verb_reads_its_words_and_refuses_what_it_cannot_keep() {
        assert_eq!(
            parse(
                "record-put",
                &w(&[
                    "--plugin",
                    "pageticket",
                    "--id",
                    "p-1",
                    "--kind",
                    "prototype",
                    "--by",
                    "octo-cat"
                ])
            ),
            Some(Ok(Call::Put {
                plugin: "pageticket".into(),
                id: "p-1".into(),
                kind: "prototype".into(),
                by: "octo-cat".into(),
            }))
        );
        let sha = "a".repeat(64);
        assert_eq!(
            parse(
                "record-file",
                &w(&[
                    "--plugin",
                    "pageticket",
                    "--id",
                    "p-1",
                    "--issue",
                    "12",
                    "--files-sha256",
                    &sha,
                    "--by",
                    "octo-cat",
                    "--by-id",
                    "583231"
                ])
            ),
            Some(Ok(Call::File {
                plugin: "pageticket".into(),
                id: "p-1".into(),
                issue: 12,
                sha256: sha.clone(),
                by: "octo-cat".into(),
                by_id: 583231,
            }))
        );
        assert_eq!(
            parse("records", &w(&["--open"])),
            Some(Ok(Call::List {
                plugin: None,
                issue: None,
                open: true
            }))
        );
        for (verb, bad) in [
            (
                "record-put",
                &[
                    "--plugin",
                    "page-ticket",
                    "--id",
                    "p",
                    "--kind",
                    "k",
                    "--by",
                    "x",
                ][..],
            ),
            (
                "record-put",
                &["--plugin", "p", "--id", "../x", "--kind", "k", "--by", "x"],
            ),
            (
                "record-put",
                &["--plugin", "p", "--id", "x", "--kind", "k1", "--by", "x"],
            ),
            (
                "record-put",
                &["--plugin", "p", "--id", "x", "--kind", "k", "--by", "a b"],
            ),
            ("record-put", &["--plugin", "p", "--id", "x", "--kind", "k"]),
            (
                "record-file",
                &[
                    "--plugin",
                    "p",
                    "--id",
                    "x",
                    "--issue",
                    "0",
                    "--files-sha256",
                    &sha,
                    "--by",
                    "octo-cat",
                ],
            ),
            (
                "record-file",
                &[
                    "--plugin",
                    "p",
                    "--id",
                    "x",
                    "--issue",
                    "1",
                    "--files-sha256",
                    &sha,
                ],
            ),
            (
                "record-file",
                &[
                    "--plugin",
                    "p",
                    "--id",
                    "x",
                    "--issue",
                    "1",
                    "--files-sha256",
                    &sha,
                    "--by",
                    "not a login",
                ],
            ),
            (
                "record-file",
                &[
                    "--plugin",
                    "p",
                    "--id",
                    "x",
                    "--issue",
                    "1",
                    "--files-sha256",
                    "abc",
                    "--by",
                    "octo-cat",
                ],
            ),
            ("record-close", &["--plugin", "p"]),
            ("records", &["--issue", "x"]),
        ] {
            assert!(
                matches!(parse(verb, &w(bad)), Some(Err(_))),
                "{verb} {bad:?}"
            );
        }
        assert_eq!(parse("record-show", &[]), None);
    }
}

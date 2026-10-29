//! `analytics orders`: what the factory spent on each issue the dashboard
//! lists, for the work order of the front page. It reads live/status.json,
//! the queue snapshot `tehasbot status` wrote, and writes live/orders.json:
//!
//!   {"orders": 1, "generatedAt": "2026-10-29T10:31:00Z",
//!    "issues": {"962": {"runs": 3, "seconds": 5400, "tokens": 60800000}}}
//!
//! One entry for each issue of the snapshot's lists that has a run in the
//! database, work runs and planning turns both, as far back as it holds.
//! `seconds` is the sum of the runs' own time and `tokens` the sum of their
//! input, output and cache reads, the sum the report counts. Either is left
//! out when no run of the issue recorded it. The file is public, as
//! status.json is: it names only issues the snapshot lists already, with
//! counts beside them.
//!
//! A snapshot that is missing or no JSON is a file of no issues. The file is
//! written through a name of this run and moved into place, mode 644.

use crate::run::read_json;
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use tehas_core::time::iso;

/// The shape of the file.
pub const ORDERS: i64 = 1;
pub const FILE: &str = "orders.json";
/// The lists of live/status.json that hold issues.
const LISTS: [&str; 6] = [
    "running",
    "queued",
    "awaiting",
    "stopped",
    "planned",
    "completed",
];

/// The numbers of the issues the snapshot lists.
fn listed(status: &Value) -> BTreeSet<i64> {
    LISTS
        .iter()
        .filter_map(|k| status.get(*k)?.as_array())
        .flatten()
        .filter_map(|i| i.get("number")?.as_i64())
        .filter(|n| *n > 0)
        .collect()
}

/// The file's object for the issues in `wanted`.
pub fn build(conn: &Connection, wanted: &BTreeSet<i64>, now: i64) -> rusqlite::Result<Value> {
    let mut stmt = conn.prepare(
        "SELECT issue, count(*), sum(seconds),
                sum(CASE WHEN input IS NULL AND output IS NULL AND cache_read IS NULL THEN NULL
                    ELSE coalesce(input, 0) + coalesce(output, 0) + coalesce(cache_read, 0) END)
         FROM runs GROUP BY issue",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, Option<i64>>(2)?,
            r.get::<_, Option<i64>>(3)?,
        ))
    })?;
    let mut issues = Map::new();
    for row in rows {
        let (issue, runs, seconds, tokens) = row?;
        if !wanted.contains(&issue) {
            continue;
        }
        let mut o = Map::new();
        o.insert("runs".into(), json!(runs));
        if let Some(s) = seconds {
            o.insert("seconds".into(), json!(s));
        }
        if let Some(t) = tokens {
            o.insert("tokens".into(), json!(t));
        }
        issues.insert(issue.to_string(), Value::Object(o));
    }
    Ok(json!({"orders": ORDERS, "generatedAt": iso(now), "issues": issues}))
}

/// Reads `<live>/status.json` and writes `<live>/orders.json`.
pub fn write(live: &Path, conn: &Connection, now: i64) -> Result<(), (u8, String)> {
    let wanted = read_json(&live.join("status.json"))
        .map(|s| listed(&s))
        .unwrap_or_default();
    let body =
        build(conn, &wanted, now).map_err(|e| (70, format!("the runs could not be read: {e}")))?;
    let why = |e: std::io::Error| (73, format!("{FILE} could not be written: {e}"));
    let to = live.join(FILE);
    // A name of this run: a cycle's end and a poll may run status at once.
    // The edge serves no *.new.
    let temp = to.with_extension(format!("{}.new", std::process::id()));
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .open(&temp)
        .map_err(why)?;
    let written = out
        .write_all(format!("{body}\n").as_bytes())
        .and_then(|()| out.sync_all())
        .and_then(|()| fs::set_permissions(&temp, fs::Permissions::from_mode(0o644)));
    if let Err(e) = written.and_then(|()| fs::rename(&temp, &to)) {
        let _ = fs::remove_file(&temp);
        return Err(why(e));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("schema.sql")).unwrap();
        conn
    }

    fn run(
        conn: &Connection,
        id: &str,
        kind: &str,
        issue: i64,
        secs: Option<i64>,
        t: [Option<i64>; 3],
    ) {
        conn.execute(
            "INSERT INTO runs (id, kind, issue, started, ended, seconds, input, output, cache_read)
             VALUES (?1, ?2, ?3, 1, 2, ?4, ?5, ?6, ?7)",
            rusqlite::params![id, kind, issue, secs, t[0], t[1], t[2]],
        )
        .unwrap();
    }

    #[test]
    fn the_listed_issues_are_every_list_of_the_snapshot() {
        let s = json!({
            "running": [{"number": 1}], "queued": [{"number": 2}], "awaiting": [{"number": 3}],
            "stopped": [{"number": 4}], "planned": [{"number": 5}], "completed": [{"number": 6}],
            "planning": 9, "buzzwords": [{"number": 7}],
        });
        assert_eq!(listed(&s), (1..=6).collect());
        let odd = json!({"running": [{"number": -1}, {"number": "8"}, {}, 3], "queued": "x"});
        assert!(listed(&odd).is_empty());
    }

    #[test]
    fn each_listed_issue_sums_its_runs_and_turns() {
        let conn = db();
        run(
            &conn,
            "a",
            "work",
            7,
            Some(600),
            [Some(100), Some(20), Some(3)],
        );
        run(&conn, "b", "work", 7, Some(300), [Some(1000), None, None]);
        run(&conn, "c", "plan", 7, Some(60), [None, None, None]);
        run(&conn, "d", "work", 8, None, [None, None, None]);
        run(&conn, "e", "work", 9, Some(5), [Some(1), Some(1), Some(1)]);
        let v = build(&conn, &[7, 8, 10].into(), 1_793_270_000).unwrap();
        assert_eq!(v["orders"], 1);
        assert_eq!(v["generatedAt"], "2026-10-29T10:33:20Z");
        assert_eq!(
            v["issues"],
            json!({"7": {"runs": 3, "seconds": 960, "tokens": 1123}, "8": {"runs": 1}})
        );
    }

    #[test]
    fn the_file_is_public_and_whole() {
        let dir = std::env::temp_dir().join(format!("orders-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let conn = db();
        run(&conn, "a", "work", 7, Some(600), [Some(1), None, None]);
        write(&dir, &conn, 0).unwrap();
        let f = dir.join(FILE);
        let v: Value = serde_json::from_str(&fs::read_to_string(&f).unwrap()).unwrap();
        assert_eq!(v["issues"], json!({}), "no snapshot is no issues");
        fs::write(dir.join("status.json"), r#"{"running":[{"number":7}]}"#).unwrap();
        write(&dir, &conn, 0).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&f).unwrap()).unwrap();
        assert_eq!(v["issues"]["7"]["seconds"], 600);
        assert_eq!(
            fs::metadata(&f).unwrap().permissions().mode() & 0o777,
            0o644
        );
        let left: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| n.ends_with(".new"))
            .collect();
        assert!(left.is_empty(), "{left:?}");
        fs::remove_dir_all(&dir).unwrap();
    }
}

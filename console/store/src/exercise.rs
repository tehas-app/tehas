//! `store test-write`: the writer that tests/concurrency.rs runs as many
//! processes at once. No verb of this part of the store writes rows, and the
//! tests must spawn the store binary itself, so it has this one for them.
//!
//! Only a build with debug assertions has it. `cargo test` makes one; install.sh
//! and the release workflow build with --release, which has none, so the store
//! a host runs answers 64 to this verb as to any it does not know.
//!
//!   store test-write --batch <n> --rows <r> [--hold]
//!     Writes r rows of branch_checks, named test-write-<n> with head_sha 0 to
//!     r-1 and a note of 1 KiB, in one transaction under the write lock and busy
//!     timeout every verb that writes takes. With --hold it prints
//!     {"held":true} once the rows are written and waits for stdin to close
//!     before it commits, so a test can kill it mid-transaction.

use crate::db;
use rusqlite::params;
use std::io::{Read, Write as _};
use std::path::Path;

pub const VERB: &str = "test-write";

const MAX_ROWS: u32 = 100_000;

#[derive(Debug, PartialEq, Eq)]
pub struct Write {
    batch: u32,
    rows: u32,
    hold: bool,
}

pub fn parse(args: &[String]) -> Option<Write> {
    let (mut batch, mut rows, mut hold) = (None, None, false);
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => i += 1,
            "--hold" if !hold => {
                hold = true;
                i += 1;
            }
            "--batch" if batch.is_none() => {
                batch = Some(args.get(i + 1)?.parse().ok()?);
                i += 2;
            }
            "--rows" if rows.is_none() => {
                rows = Some(args.get(i + 1)?.parse().ok()?);
                i += 2;
            }
            _ => return None,
        }
    }
    let rows = rows.filter(|r| (1..=MAX_ROWS).contains(r))?;
    Some(Write {
        batch: batch?,
        rows,
        hold,
    })
}

pub fn write(file: &Path, w: &Write, now: i64) -> Result<String, (u8, String)> {
    let fail = |e| crate::line(db::classify(e));
    // `lock` leaves BEGIN IMMEDIATE open; a return before COMMIT rolls it back.
    let conn = db::lock(file).map_err(crate::line)?;
    let name = format!("test-write-{}", w.batch);
    let note = "x".repeat(1024);
    let mut insert = conn
        .prepare(
            "INSERT INTO branch_checks (name, head_sha, at, state, note) \
             VALUES (?1, ?2, ?3, 'pending', ?4)",
        )
        .map_err(fail)?;
    for row in 0..w.rows {
        insert
            .execute(params![name, row.to_string(), now, note])
            .map_err(fail)?;
    }
    drop(insert);
    if w.hold {
        let mut out = std::io::stdout();
        let _ = writeln!(out, "{{\"api\":1,\"verb\":\"{VERB}\",\"held\":true}}");
        let _ = out.flush();
        let _ = std::io::stdin().read_to_end(&mut Vec::new());
    }
    conn.execute_batch("COMMIT").map_err(fail)?;
    Ok(format!(
        "{{\"api\":1,\"verb\":\"{VERB}\",\"ok\":true,\"batch\":{},\"rows\":{}}}",
        w.batch, w.rows
    ))
}

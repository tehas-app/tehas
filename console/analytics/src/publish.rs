//! `analytics publish`: the private JSON Lines files of the MCP server, under
//! `~/tehas/mcp` - `sessions.jsonl`, the snapshot of the last thirty days, and
//! `outcomes.jsonl`, the counts of how the runs ended for the retention back.
//! `tehasbot status` asks for both on every poll, and a cycle's end may ask at
//! the same moment, so the name of the file in progress carries this run.
//! Neither file is ever published: the directory is mode 700 and every file
//! mode 600.
//!
//! `tehasbot status` asks for both on every poll, and usually no run has
//! started or ended since the last write. `fingerprint` answers whether
//! anything a file of ours holds could have changed, `unchanged` compares it
//! with the one line `<dir>/<name>.stamp` holds, and `stamp` writes that line
//! after a write. A match leaves the file and its mtime alone.

use rusqlite::Connection;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use tehas_core::time::civil_from_days;

/// The private file `<dir>/<name>`, written from `lines`: through a name of
/// this run, mode 600, and moved into place so a reader never sees half a
/// file. What is written is what the caller was given, so the host that asks
/// for the file needs no reader of its own. The directory is made mode 700 if
/// it is not there.
pub fn put(dir: &Path, name: &str, lines: &[String]) -> Result<(), String> {
    let why = |what: &str, e: std::io::Error| format!("{what}: {e}");
    // The directory is the private one of the MCP server, mode 700. Make it
    // with that mode rather than 755 and a later chmod, which would leave it
    // readable in between.
    fs::create_dir_all(dir).map_err(|e| why("the directory could not be made", e))?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
        .map_err(|e| why("the directory could not be given its mode", e))?;
    let to = dir.join(name);
    // A name of this run, so two writers of the same file cannot meet in it.
    let temp = to.with_extension(format!("{}.new", std::process::id()));
    let made = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .map_err(|e| why("could not be written", e))?;
    let mut out = std::io::BufWriter::new(made);
    let body = format!("{}\n", lines.join("\n"));
    let written = out
        .write_all(body.as_bytes())
        .and_then(|()| out.flush())
        .and_then(|()| out.get_mut().sync_all());
    // The mode of a file made by create_new is the one asked for, masked by
    // the umask, which a service may widen. Set it again.
    let mode = fs::set_permissions(&temp, fs::Permissions::from_mode(0o600));
    if let Err(e) = written.and(mode) {
        let _ = fs::remove_file(&temp);
        return Err(why("could not be written", e));
    }
    fs::rename(&temp, &to).map_err(|e| {
        let _ = fs::remove_file(&temp);
        why("could not be put in place", e)
    })
}

/// The name of the line that says a file was written: `<name>.stamp`, beside
/// the file it speaks for.
pub fn stamp_name(name: &str) -> String {
    format!("{name}.stamp")
}

/// What the two files answer to: the runs of the database, and the day their
/// window ends on. The count and the largest `ended` answer a run that starts
/// or ends, and today's date because the 30-day window of the sessions moves
/// on its own: a snapshot of a day is not a snapshot of the next one.
///
/// It answers nothing when any run has no `ended`, because the size of a live
/// run's logs changes with no row of the database changing to say so - there
/// is nothing to compare then, so the file is written again. A row it cannot
/// read is an error, answered as any unreadable database is.
pub fn fingerprint(conn: &Connection, now: i64) -> rusqlite::Result<Option<String>> {
    let counts = |sql: &str| -> rusqlite::Result<i64> {
        conn.query_row(sql, [], |r| r.get::<_, Option<i64>>(0))
            .map(|v| v.unwrap_or(0))
    };
    // Three reads of one connection, so no run counted in one answer is
    // missing from another. max() of no row and count() of nothing are both
    // null, and both read as no runs.
    let rows = counts("SELECT count(*) FROM runs")?;
    let ended = counts("SELECT max(ended) FROM runs")?;
    let live = counts("SELECT count(*) FROM runs WHERE ended IS NULL")?;
    let (y, m, d) = civil_from_days(now.div_euclid(86_400));
    Ok((live == 0).then(|| format!("runs={rows};ended={ended};day={y:04}-{m:02}-{d:02}")))
}

/// True when `dir` already holds the file `name` as the fingerprint describes
/// it: the line of `<dir>/<name>.stamp` is exactly that fingerprint, and the
/// file it speaks for is there. A missing stamp, a missing file or a
/// fingerprint of none each says write it.
pub fn unchanged(dir: &Path, name: &str, fingerprint: &Option<String>) -> bool {
    let Some(line) = fingerprint else {
        return false;
    };
    // The line of the stamp, which put ends with a newline: the words of the
    // fingerprint and that mark, and nothing else. A file written by an older
    // program, or one with another line after this, is not believed.
    let held = fs::read_to_string(dir.join(stamp_name(name))).ok();
    held.as_deref() == Some(format!("{line}\n").as_str()) && dir.join(name).is_file()
}

/// The line of `<dir>/<name>.stamp`, written after the file itself: through a
/// name of this run and moved into place, as the file is. A stamp that does
/// not exist says write again, so a stamp left by a write of another name
/// costs one rewrite and holds nothing back.
pub fn stamp(dir: &Path, name: &str, fingerprint: &Option<String>) -> Result<(), String> {
    let Some(line) = fingerprint else {
        return Ok(());
    };
    put(dir, &stamp_name(name), std::slice::from_ref(line))
}

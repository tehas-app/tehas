//! `analytics publish`: the private JSON Lines files of the MCP server, under
//! `~/tehas/mcp` - `sessions.jsonl`, the snapshot of the last thirty days, and
//! `outcomes.jsonl`, the counts of how the runs ended for the retention back.
//! `tehasbot status` asks for both on every poll, and a cycle's end may ask at
//! the same moment, so the name of the file in progress carries this run.
//! Neither file is ever published: the directory is mode 700 and every file
//! mode 600.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

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

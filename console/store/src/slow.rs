//! The record writer's one selection that is not a copy: the driver's slow
//! notices of a cycle, as they go into the record that survives the workspace
//! sweep.
//!
//!   slow <file> --out <file>
//!     The `dispatch-slow` lines of the cycle's stream, oldest first, at most
//!     20 of them and never over 64 KiB, each line whole. The rule is
//!     tehas_core::slow, where it is tested; this verb is the way the shell
//!     reaches it. The file is read as the store reads a host file: a plain
//!     file, no link, under a bound. The lines are written to <out>, which the
//!     caller names, and the object this verb answers with is the count of
//!     them: the caller is the record writer, and the file it names holds
//!     nothing but notices, so the answer is not mixed into its content. A
//!     cycle whose children all finished inside the bound leaves no file, which
//!     is what the record wants: no file at all.
//!
//! Exit: 0 read, none of the lines among them; 1 the file is not a plain file
//! under the bound, or <out> could not be written; 64 a bad argument.

use crate::verbs::read_file;
use std::fs;
use std::path::Path;

/// A cycle's stream is a few thousand lines. This is far above it, and bounds
/// what the verb is ever asked to read: the same way the store reads a host
/// file, at a size the caller agrees to.
const MAX_BYTES: u64 = 16 << 20;

pub fn run(file: &Path, out: &Path) -> Result<String, (u8, String)> {
    let (bytes, _) = read_file(file, MAX_BYTES).map_err(|_| {
        (
            1,
            format!(
                "slow: {} is not a plain file under the bound",
                file.display()
            ),
        )
    })?;
    let text = String::from_utf8_lossy(&bytes);
    let kept = tehas_core::slow::lines(&text);
    // The file is made only when there is something to put in it: a record of
    // a cycle whose children all finished inside the bound holds no such file.
    if !kept.is_empty() {
        // Both paths are the caller's: the record writer names the stream it
        // owns and the record it is building, as it names every file of a record.
        fs::write(out, kept.as_bytes())
            .map_err(|e| (1, format!("slow: write {}: {e}", out.display())))?;
    }
    Ok(format!(
        "{{\"api\":1,\"verb\":\"slow\",\"ok\":true,\"lines\":{}}}",
        kept.lines().count()
    ))
}

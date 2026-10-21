//! The sessions snapshot the host writes to ~/tehas/mcp/sessions.jsonl: a
//! header line, then one session per line, newest first. Never published.

use crate::tools::{epoch, ToolError, STALE_SECS};
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub const FILE: &str = "sessions.jsonl";

pub struct Snapshot {
    pub days: i64,
    pub generated_at: String,
    /// The host has not written it for ten minutes.
    pub stale: bool,
    path: PathBuf,
    now: i64,
}

fn bad(why: &str) -> ToolError {
    ToolError::File {
        file: FILE.into(),
        why: why.into(),
    }
}

impl Snapshot {
    /// Reads and checks the header only.
    pub fn open(dir: &Path, now: i64) -> Result<Snapshot, ToolError> {
        let path = dir.join(FILE);
        let f = File::open(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                bad("not written")
            } else {
                bad("could not be read")
            }
        })?;
        let mut first = String::new();
        BufReader::new(f)
            .read_line(&mut first)
            .map_err(|_| bad("could not be read"))?;
        let head: Value = serde_json::from_str(&first).map_err(|_| bad("has no header"))?;
        if head["api"] != 1 {
            return Err(bad("has a header of another api"));
        }
        let days = head["days"]
            .as_i64()
            .filter(|d| *d >= 1)
            .ok_or_else(|| bad("has a header without days"))?;
        let generated_at = head["generatedAt"].as_str().unwrap_or("").to_string();
        let stale = epoch(&generated_at).is_none_or(|t| now - t > STALE_SECS);
        Ok(Snapshot {
            days,
            generated_at,
            stale,
            path,
            now,
        })
    }

    /// The sessions within the window, one line read at a time. A line that is
    /// not an object, or has no time in the window, is skipped.
    pub fn sessions(&self) -> impl Iterator<Item = Value> {
        let from = self.now - self.days * 86400;
        File::open(&self.path)
            .ok()
            .into_iter()
            .flat_map(|f| BufReader::new(f).lines().skip(1))
            .map_while(Result::ok)
            .filter_map(|l| serde_json::from_str::<Value>(&l).ok())
            .filter(move |v| {
                v.is_object()
                    && v["started"]
                        .as_str()
                        .and_then(epoch)
                        .is_some_and(|t| t >= from)
            })
    }
}

//! `factory_decisions`: the dispatcher's last decisions, start, skip, wait
//! and drop (of a refresh request), read from the end of `dispatch.log`.
//!
//! The log is opened by its one fixed name in the logs directory; a caller
//! gives no path. Only its last `TAIL_BYTES` are read. A decision's time, word
//! and issue are checked shapes; its reason is free text, masked, then cut.

use crate::tools::{Dirs, ToolError};
use serde_json::{json, Map, Value};
use std::collections::VecDeque;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use tehas_core::mask::mask;

pub const FILE: &str = "dispatch.log";
/// The most decisions one answer holds.
pub const LIMIT_MAX: u64 = 200;
const LIMIT_DEFAULT: u64 = 50;
/// The most of the log's end one call reads. On a host a decision line is
/// about 80 bytes, so this holds far more than `LIMIT_MAX` of them.
pub const TAIL_BYTES: u64 = 1024 * 1024;
/// A reason is cut to this many characters.
const REASON_MAX: usize = 400;
pub const WORDS: [&str; 4] = ["start", "skip", "wait", "drop"];

/// One decision line, as the dispatcher writes it:
/// `<time> start #<n> (<reason>)`, `<time> skip|wait #<n>: <reason>` or
/// `<time> drop the refresh of #<n>: <reason>`.
#[derive(Debug, PartialEq, Eq)]
pub struct Decision<'a> {
    pub time: &'a str,
    pub word: &'a str,
    pub issue: u64,
    /// As logged, not masked.
    pub reason: &'a str,
}

/// The one parse of a decision line. None for any other line of the log.
pub fn parse(line: &str) -> Option<Decision<'_>> {
    let line = line.strip_suffix('\r').unwrap_or(line);
    let (time, rest) = line.split_once(' ')?;
    crate::tools::epoch(time)?;
    let (word, rest) = rest.split_once(" #")?;
    // The dispatcher drops only refresh requests, and logs them so.
    let word = match word {
        "start" | "skip" | "wait" => word,
        "drop the refresh of" => "drop",
        _ => return None,
    };
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let issue = rest[..digits].parse::<u64>().ok().filter(|n| *n > 0)?;
    let tail = &rest[digits..];
    let reason = match tail.strip_prefix(": ") {
        Some(r) => r,
        None if tail.is_empty() => "",
        None => tail.strip_prefix(" (")?.strip_suffix(')')?,
    };
    Some(Decision {
        time,
        word,
        issue,
        reason,
    })
}

pub fn tool() -> Value {
    let text = |desc: &str| json!({"type": "string", "description": desc});
    json!({
        "name": "factory_decisions",
        "description": "The dispatcher's last decisions, newest last: each start, skip, wait and drop with its time, issue and reason as the dispatcher logs it. A drop is a refresh request the dispatcher dropped. Read from the last 1 MiB of the dispatch log, masked; a reason is cut to 400 characters.",
        "inputSchema": {"type": "object", "properties": {
            "limit": {"type": "integer", "minimum": 1, "maximum": LIMIT_MAX, "description": "How many decisions, from 1 to 200. 50 without it."},
        }, "additionalProperties": false},
        "outputSchema": {"type": "object", "properties": {
            "api": {"type": "integer", "description": "The API version, 1."},
            "decisions": {"type": "array", "description": "The last decisions, oldest first.", "items": {
                "type": "object", "description": "One decision.", "properties": {
                    "time": text("When it was made, UTC."),
                    "decision": {"type": "string", "enum": WORDS, "description": "start, skip, wait or drop (of a refresh request)."},
                    "issue": {"type": "integer", "description": "The issue number."},
                    "reason": text("Why, as the dispatcher logs it, masked and cut to 400 characters. For a start, who started it and how."),
                }}},
            "whole": {"type": "boolean", "description": "True when the read reached the start of the log, so no older decision was left unread."},
        }, "required": ["api"]},
    })
}

fn bad(why: &str) -> ToolError {
    ToolError::BadArgs(why.to_string())
}

fn file_error(why: &str) -> ToolError {
    ToolError::File {
        file: FILE.into(),
        why: why.into(),
    }
}

pub(crate) fn call(dirs: &Dirs, args: &Value) -> Result<Map<String, Value>, ToolError> {
    let limit = match args {
        Value::Null => LIMIT_DEFAULT,
        Value::Object(m) => {
            if let Some(k) = m.keys().find(|k| *k != "limit") {
                return Err(bad(&format!(
                    "{k} is not an argument; limit is the only one"
                )));
            }
            match m.get("limit") {
                None => LIMIT_DEFAULT,
                Some(v) => v
                    .as_u64()
                    .filter(|n| (1..=LIMIT_MAX).contains(n))
                    .ok_or_else(|| bad(&format!("limit is from 1 to {LIMIT_MAX}")))?,
            }
        }
        _ => return Err(bad("the arguments are an object")),
    };
    let (text, whole) = tail(&dirs.logs.join(FILE))?;
    let mut last: VecDeque<Decision> = VecDeque::new();
    for d in text.split('\n').filter_map(parse) {
        if last.len() as u64 == limit {
            last.pop_front();
        }
        last.push_back(d);
    }
    let decisions: Vec<Value> = last
        .iter()
        .map(|d| {
            // Masked before the cut, so a cut never leaves part of a secret.
            let reason: String = mask(d.reason).chars().take(REASON_MAX).collect();
            json!({"time": d.time, "decision": d.word, "issue": d.issue, "reason": reason})
        })
        .collect();
    let mut out = Map::new();
    out.insert("decisions".into(), Value::Array(decisions));
    out.insert("whole".into(), json!(whole));
    Ok(out)
}

/// The last `TAIL_BYTES` of a plain file, never one through a link, from its
/// first whole line, and whether that is the whole file.
fn tail(path: &std::path::Path) -> Result<(String, bool), ToolError> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            file_error("is not written yet")
        } else {
            file_error("could not be read")
        }
    })?;
    if !meta.file_type().is_file() {
        return Err(file_error("is not a plain file"));
    }
    let mut f = std::fs::File::open(path).map_err(|_| file_error("could not be read"))?;
    // The file opened is the one checked, not a link put in its place since.
    let opened = f.metadata().map_err(|_| file_error("could not be read"))?;
    if opened.dev() != meta.dev() || opened.ino() != meta.ino() || !opened.is_file() {
        return Err(file_error("is not a plain file"));
    }
    let start = opened.len().saturating_sub(TAIL_BYTES);
    f.seek(SeekFrom::Start(start))
        .map_err(|_| file_error("could not be read"))?;
    let mut bytes = Vec::with_capacity(opened.len().min(TAIL_BYTES) as usize);
    f.take(TAIL_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|_| file_error("could not be read"))?;
    let from = if start == 0 {
        0
    } else {
        bytes
            .iter()
            .position(|b| *b == b'\n')
            .map_or(bytes.len(), |i| i + 1)
    };
    Ok((
        String::from_utf8_lossy(&bytes[from..]).into_owned(),
        start == 0,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d<'a>(time: &'a str, word: &'a str, issue: u64, reason: &'a str) -> Option<Decision<'a>> {
        Some(Decision {
            time,
            word,
            issue,
            reason,
        })
    }

    #[test]
    fn the_lines_a_host_writes() {
        let t = "2026-10-29T16:23:14Z";
        assert_eq!(
            parse(&format!(
                "{t} skip #316: its question is not among the last 100 comments"
            )),
            d(
                t,
                "skip",
                316,
                "its question is not among the last 100 comments"
            )
        );
        assert_eq!(
            parse(&format!("{t} start #12 (labelled by alice)")),
            d(t, "start", 12, "labelled by alice")
        );
        assert_eq!(
            parse(&format!("{t} wait #5: tehas:blocked holds it: #4 is open")),
            d(t, "wait", 5, "tehas:blocked holds it: #4 is open")
        );
        assert_eq!(
            parse(&format!(
                "{t} drop the refresh of #8: pull request #9 moved since the request\r"
            )),
            d(t, "drop", 8, "pull request #9 moved since the request")
        );
        assert_eq!(parse(&format!("{t} skip #9")), d(t, "skip", 9, ""));
    }

    #[test]
    fn other_lines_are_none() {
        let t = "2026-10-29T16:23:14Z";
        for line in [
            format!("{t} holding: the limit allows 0 cycles at once and 5 are running: draining"),
            format!("{t} start planning #7 (labelled by alice)"),
            format!("{t} drop #9: no writer logs this shape"),
            format!("{t} drop the refresh #8: no of"),
            format!("{t} done #7 rc=0 status=merged"),
            format!("{t} skip #0: zero"),
            format!("{t} skip #: none"),
            format!("{t} skip #7x: glued"),
            format!("{t} start #7 (open"),
            format!("{t} skip #99999999999999999999999: too long"),
            "2026-10-29 16:23:14 skip #7: no T".to_string(),
            "skip #7: no time".to_string(),
            String::new(),
        ] {
            assert_eq!(parse(&line), None, "{line}");
        }
    }
}

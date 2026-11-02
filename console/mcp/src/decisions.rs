//! `factory_decisions`: the dispatcher's last decisions, start, skip, wait,
//! hold and drop (of a refresh request), read from the end of `dispatch.log`.
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
/// The most of the log's end one call reads. The longest decision line a host
/// writes today is about 250 bytes, so this holds far more than `LIMIT_MAX`
/// of them.
pub const TAIL_BYTES: u64 = 1024 * 1024;
/// A reason is cut to this many characters.
const REASON_MAX: usize = 400;
/// Where a decision's reason sits in its line: in brackets after the issue,
/// as only a start's is, or after `: `. A line of the second shape with no
/// reason after the issue is no decision the dispatcher made, except a start,
/// which has no reason to give: it says who labelled the issue. A `skip` is
/// the one decision that may be logged without its reason at all.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ReasonStyle {
    Brackets,
    /// The reason may be absent: a bare `skip #9` is a decision with none.
    Bare,
    /// A bare `wait #9` is no decision: the reason is what the caller wants.
    Colon,
}

/// One row of [`DECISIONS`]. Its fields are named, not positional, because
/// three of them are words and only their names say which is which: `word` is
/// what the tool answers and the schema's enum holds, `logged` is the word the
/// dispatcher writes at the start of its line, and `sample` is a line of that
/// shape.
#[derive(Clone, Copy)]
pub struct DecisionKind {
    pub word: &'static str,
    pub logged: &'static str,
    /// The kind `kind_of` in `events` reads this decision's line as.
    pub kind: &'static str,
    pub style: ReasonStyle,
    pub sample: &'static str,
    pub number: u64,
}

/// The decisions a caller can read from the log. One row per word holds what
/// both readers need: the word the tool answers, the word the dispatcher
/// logs, the kind `kind_of` in `events` reads such a line as, where its
/// reason sits, and one sample line with the issue it names. `parse` answers
/// a line only for the words of this table and reads it by the row it
/// matched, the schema's enum is its `word` column, and `kind_of` is tested
/// of its samples, so the two readers of these words cannot drift apart. A
/// hold says what it holds, so its line starts `hold #`; a drop names what it
/// drops, which is a refresh request. A `lift #<n>:` line, the release of a
/// hold from the same code, is no decision of its own: `factory_events`
/// shows it.
pub const DECISIONS: [DecisionKind; 5] = [
    DecisionKind {
        word: "start",
        logged: "start",
        kind: "start",
        style: ReasonStyle::Brackets,
        sample: "2026-10-29T16:23:14Z start #12 (labelled by alice)",
        number: 12,
    },
    DecisionKind {
        word: "skip",
        logged: "skip",
        kind: "skip",
        style: ReasonStyle::Bare,
        sample: "2026-10-29T16:23:14Z skip #316: its question is not among the last 100 comments",
        number: 316,
    },
    DecisionKind {
        word: "wait",
        logged: "wait",
        kind: "wait",
        style: ReasonStyle::Colon,
        sample: "2026-10-29T16:23:14Z wait #5: tehas:blocked holds it: #4 is open",
        number: 5,
    },
    DecisionKind {
        word: "hold",
        logged: "hold",
        kind: "wait",
        style: ReasonStyle::Colon,
        sample: "2026-10-29T16:23:14Z hold #688: its last cycle parked on intent-park and the body has not changed since",
        number: 688,
    },
    DecisionKind {
        word: "drop",
        logged: "drop the refresh of",
        kind: "drop",
        style: ReasonStyle::Colon,
        sample: "2026-10-29T16:23:14Z drop the refresh of #8: pull request #9 moved since the request",
        number: 8,
    },
];

/// The words the writers of `dispatch.log` use today, each a `logged` of
/// `DECISIONS` or the first word of a `WRITER_NON_DECISIONS`. A word either
/// writer logs that is in neither list, or a list that keeps a word no writer
/// logs, fails the comparison in `tests/decisions.rs` rather than leaving a
/// line of the log quietly unanswered.
pub const WRITER_WORDS: [&str; 12] = [
    "clear",
    "drop the refresh of",
    "hold",
    "hold refreshes",
    "holding",
    "ignore",
    "lift",
    "park",
    "released",
    "skip",
    "start",
    "wait",
];

/// A word the writers of `dispatch.log` log today that this tool answers as no
/// decision, each with its line and the kind `kind_of` in `events` reads such
/// a line as, so the vocabulary is pinned on both sides of the log: a `lift`
/// line, the release of a hold, a `park` line and the poll's `holding:` and
/// `released:` lines are events of `factory_events`, never decisions.
pub const WRITER_NON_DECISIONS: [(&str, &str, &str); 7] = [
    (
        "clear",
        "2026-10-29T16:23:14Z clear #7: tehas:running off, applied by 'mallory' who may not authorise work here",
        "tidy",
    ),
    (
        "hold refreshes",
        "2026-10-29T16:23:14Z hold refreshes: two pull requests are being read",
        "wait",
    ),
    (
        "holding",
        "2026-10-29T16:23:14Z holding: the limit allows 0 cycles at once and 1 are running: draining",
        "wait",
    ),
    (
        "ignore",
        "2026-10-29T16:23:14Z ignore #7: tehas:blocked was taken off by 'mallory' since it was applied",
        "skip",
    ),
    (
        "lift",
        "2026-10-29T16:23:14Z lift #7: its last cycle parked on intent-park and the body has not changed since, lifted on the fixer's guidance comment",
        "warn",
    ),
    (
        "park",
        "2026-10-29T16:23:14Z park #7: it is marked tehas:running and no cycle holds its lock",
        "tidy",
    ),
    (
        "released",
        "2026-10-29T16:23:14Z released: cycles may start again",
        "wait",
    ),
];

// The words a host's dispatcher writes for a hold, in the words it logs today,
// kept beside the table so both readers are tested of the same lines: `parse`
// in its own tests, `kind_of` in `events`, and the integration oracle of
// `tests/decisions.rs` through the fixture's log. The three long reasons of a
// hold are written once, here.
pub const HOST_HOLD_LINES: [&str; 3] = [
    "2026-10-29T16:23:14Z hold #7: its text links an attachment and TEHAS_WORK_IMAGES is 1, but test-model does not declare image input in models.json",
    "2026-10-29T16:23:14Z hold #7: its text says SCREENSHOT GOES HERE and links no attachment",
    "2026-10-29T16:23:14Z hold #7: its last cycle parked on intent-park and the body has not changed since, and the fixer's comments could not be read",
];

/// One decision line, as the dispatcher writes it:
/// `<time> start #<n> (<reason>)`, `<time> skip|wait|hold #<n>: <reason>` or
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
    // The words of `DECISIONS` alone: a line of the log that only looks like
    // a decision, such as the poll's `holding:` line, is none.
    let kind = DECISIONS.iter().find(|kind| kind.logged == word)?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let issue = rest[..digits].parse::<u64>().ok().filter(|n| *n > 0)?;
    let tail = &rest[digits..];
    let reason = match kind.style {
        ReasonStyle::Brackets => tail.strip_prefix(" (")?.strip_suffix(')')?,
        ReasonStyle::Bare => match tail {
            "" => "",
            _ => tail.strip_prefix(": ")?,
        },
        ReasonStyle::Colon => tail.strip_prefix(": ")?,
    };
    Some(Decision {
        time,
        word: kind.word,
        issue,
        reason,
    })
}

/// The words of `DECISIONS`, in its order: the schema's enum.
fn words() -> Vec<&'static str> {
    DECISIONS.iter().map(|kind| kind.word).collect()
}

pub fn tool() -> Value {
    let text = |desc: &str| json!({"type": "string", "description": desc});
    json!({
        "name": "factory_decisions",
        "description": "The dispatcher's last decisions, newest last: each start, skip, wait, hold and drop with its time, issue and reason as the dispatcher logs it. A drop is a refresh request the dispatcher dropped. Read from the last 1 MiB of the dispatch log, masked; a reason is cut to 400 characters.",
        "inputSchema": {"type": "object", "properties": {
            "limit": {"type": "integer", "minimum": 1, "maximum": LIMIT_MAX, "description": "How many decisions, from 1 to 200. 50 without it."},
        }, "additionalProperties": false},
        "outputSchema": {"type": "object", "properties": {
            "api": {"type": "integer", "description": "The API version, 1."},
            "decisions": {"type": "array", "description": "The last decisions, oldest first.", "items": {
                "type": "object", "description": "One decision.", "properties": {
                    "time": text("When it was made, UTC."),
                    "decision": {"type": "string", "enum": words(), "description": "start, skip, wait, hold (an issue the dispatcher keeps parked without starting it, such as one whose last cycle parked on the same text, or one whose attachment the model cannot read) or drop (of a refresh request)."},
                    "issue": {"type": "integer", "description": "The issue number."},
                    "reason": text("Why, as the dispatcher logs it, masked and cut to 400 characters. For a start, who started it and how; for a hold, why the parked issue does not start."),
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
    fn every_word_of_the_schema_is_read_from_its_own_line() {
        let schema = tool();
        let served = schema["outputSchema"]["properties"]["decisions"]["items"]["properties"]
            ["decision"]["enum"]
            .as_array()
            .expect("the enum of the decision field");
        for row in DECISIONS {
            assert!(
                served.contains(&json!(row.word)),
                "{} is not in the schema",
                row.word
            );
            let d = parse(row.sample).unwrap_or_else(|| panic!("{} is not a decision", row.sample));
            assert_eq!((d.word, d.issue), (row.word, row.number), "{}", row.sample);
            assert!(!d.reason.is_empty(), "{} gives no reason", row.sample);
        }
        assert_eq!(
            served,
            &DECISIONS.iter().map(|r| json!(r.word)).collect::<Vec<_>>()
        );
    }

    // A line the dispatcher writes that is no decision: `parse` answers none of
    // them, so a word added to the writers and not to the table is caught here.
    #[test]
    fn the_lines_a_writer_says_that_are_no_decision() {
        for (_, line, _) in WRITER_NON_DECISIONS {
            assert_eq!(parse(line), None, "{line}");
        }
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
        // Every reason a host's dispatcher logs for a hold, from one list.
        for line in HOST_HOLD_LINES {
            let d = parse(line).unwrap_or_else(|| panic!("{line} is not a decision"));
            assert_eq!((d.word, d.issue), ("hold", 7), "{line}");
            assert!(!d.reason.is_empty(), "{line} gives no reason");
        }
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
            format!("{t} hold #9 (a hold is not logged in a start's brackets)"),
            format!("{t} hold: no issue"),
            format!("{t} start planning #7 (labelled by alice)"),
            format!("{t} drop #9: no writer logs this shape"),
            format!("{t} drop the refresh #8: no of"),
            format!("{t} done #7 rc=0 status=merged"),
            format!("{t} skip #0: zero"),
            format!("{t} skip #: none"),
            format!("{t} skip #7x: glued"),
            format!("{t} start #7 (open"),
            format!("{t} hold #7 open"),
            format!("{t} skip #99999999999999999999999: too long"),
            "2026-10-29 16:23:14 skip #7: no T".to_string(),
            "skip #7: no time".to_string(),
            String::new(),
        ] {
            assert_eq!(parse(&line), None, "{line}");
        }
    }
}

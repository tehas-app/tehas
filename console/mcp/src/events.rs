//! `factory_events`: the lines of `dispatch.log` and `refresh.log` after a
//! cursor, masked, each with a kind, merged by time.
//!
//! The tool opens four fixed names in the logs directory and no other: the two
//! logs and their rotated copies, `<log>.1`. A caller never names a file.
//!
//! The cursor holds, per log, the inode of the file it was read from, the byte
//! offset read to, whether that offset is inside a line too long for one read,
//! and a hash of the file's first bytes up to that offset. The hash tells a file
//! that was replaced or truncated and written again from the one that was read,
//! whatever its inode. When the file read is no longer the log, the rest is
//! read from the rotated copy when that copy is the file read, and then the new
//! log from its start. When neither is, the new log is read from its start and
//! the answer names the log in `gaps`.
//!
//! A caller can forge a cursor, so no text is answered from the middle of a
//! line, where the mask would miss a secret: an offset must be a line's start,
//! and one inside a long line skips to the next.

use crate::tools::{bad, epoch, ToolError};
use regex::Regex;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::fs::File;
use std::os::unix::fs::{FileExt, MetadataExt};
use std::path::Path;
use std::sync::LazyLock;
use tehas_core::mask::mask;

/// The logs, by the word an event names them with, and their file names.
const LOGS: [(&str, &str); 2] = [("dispatch", "dispatch.log"), ("refresh", "refresh.log")];
/// The suffix of the one rotated copy that is read.
const ROTATED: &str = ".1";
pub const KINDS: [&str; 9] = [
    "start", "done", "wait", "skip", "drop", "tidy", "refresh", "warn", "error",
];
/// The events of an answer without `limit`, and the most with it.
pub const DEFAULT_EVENTS: u64 = 100;
pub const EVENTS_MAX: u64 = 500;
/// The most bytes read of each log in one call, its rotated copy included.
pub const READ_MAX: u64 = 256 * 1024;
/// The most characters of a line an event gives.
pub const TEXT_CUT: usize = 1000;
/// The bytes of a file's start the cursor's hash covers.
const HEAD: u64 = 128;
const CURSOR_MAX: usize = 128;
/// A version, then per log: inode, offset, skip, hash.
static CURSOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^1(?:\.[0-9]{1,20}\.[0-9]{1,20}\.[01]\.[0-9a-f]{16}){2}$").unwrap()
});

#[derive(Clone, Debug, PartialEq, Eq)]
struct Pos {
    ino: u64,
    off: u64,
    /// Inside a line longer than a read: read on from the next line.
    skip: bool,
    head: String,
}

impl Pos {
    fn none() -> Pos {
        Pos {
            ino: 0,
            off: 0,
            skip: false,
            head: hash(&[]),
        }
    }
}

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn parse_cursor(s: &str) -> Result<[Pos; 2], ToolError> {
    let refuse = || bad("after is a cursor from an earlier answer's next");
    if s.len() > CURSOR_MAX || !CURSOR.is_match(s) {
        return Err(refuse());
    }
    let parts: Vec<&str> = s.split('.').skip(1).collect();
    let pos = |i: usize| -> Result<Pos, ToolError> {
        let p = Pos {
            ino: parts[i * 4].parse().map_err(|_| refuse())?,
            off: parts[i * 4 + 1].parse().map_err(|_| refuse())?,
            skip: parts[i * 4 + 2] == "1",
            head: parts[i * 4 + 3].to_string(),
        };
        Ok(p)
    };
    Ok([pos(0)?, pos(1)?])
}

fn format_cursor(p: &[Pos; 2]) -> String {
    let mut s = "1".to_string();
    for p in p {
        s.push_str(&format!(
            ".{}.{}.{}.{}",
            p.ino,
            p.off,
            u8::from(p.skip),
            p.head
        ));
    }
    s
}

struct Opened {
    file: File,
    name: String,
    ino: u64,
    len: u64,
}

fn file_error(name: &str, why: &str) -> ToolError {
    ToolError::File {
        file: name.into(),
        why: why.into(),
    }
}

/// A plain file of the logs directory, never one through a link. None when it
/// is not there.
fn open_plain(dir: &Path, name: &str) -> Result<Option<Opened>, ToolError> {
    let path = dir.join(name);
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(file_error(name, "could not be read")),
    };
    if !meta.file_type().is_file() {
        return Err(file_error(name, "is not a plain file"));
    }
    let file = match File::open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(file_error(name, "could not be read")),
    };
    let opened = file
        .metadata()
        .map_err(|_| file_error(name, "could not be read"))?;
    if opened.dev() != meta.dev() || opened.ino() != meta.ino() || !opened.is_file() {
        return Err(file_error(name, "is not a plain file"));
    }
    Ok(Some(Opened {
        file,
        name: name.into(),
        ino: opened.ino(),
        len: opened.len(),
    }))
}

impl Opened {
    fn read(&self, at: u64, n: u64) -> Result<Vec<u8>, ToolError> {
        let mut buf = vec![0; n as usize];
        self.file
            .read_exact_at(&mut buf, at)
            .map_err(|_| file_error(&self.name, "could not be read"))?;
        Ok(buf)
    }

    /// A skip hashes no bytes short of the full head, so a forged one cannot
    /// test a guess of the bytes before it one at a time.
    fn pos(&self, off: u64, skip: bool) -> Result<Pos, ToolError> {
        let upto = if skip && off < HEAD { 0 } else { off };
        Ok(Pos {
            ino: self.ino,
            off,
            skip,
            head: hash(&self.read(0, upto.min(HEAD))?),
        })
    }

    /// Whether this is the file the cursor read to `pos.off`, and the offset
    /// a line's start unless it skips to one.
    fn holds(&self, pos: &Pos) -> Result<bool, ToolError> {
        if self.len < pos.off {
            return Ok(false);
        }
        if !pos.skip && pos.off > 0 && self.read(pos.off - 1, 1)? != b"\n" {
            return Ok(false);
        }
        Ok(self.pos(pos.off, pos.skip)?.head == pos.head)
    }
}

struct Seg {
    file: Opened,
    from: u64,
    /// `from` is inside a line: read on from the next.
    skip: bool,
    /// Read up to here: the end of the last whole line taken.
    end: u64,
    /// `end` is inside a line too long for one read.
    end_skip: bool,
    /// A rotated copy: a last line without its newline was cut by the copy,
    /// and its rest starts the next file.
    old: bool,
}

impl Seg {
    fn new(file: Opened, from: u64, skip: bool, old: bool) -> Seg {
        Seg {
            file,
            from,
            skip,
            end: from,
            end_skip: skip,
            old,
        }
    }
}

struct Line {
    /// The time it is merged by: its own, or that of the line before it.
    key: String,
    time: Option<String>,
    /// As read: masked when it is answered.
    text: String,
    seg: usize,
    end: u64,
    /// The start of a line too long for one read: `end` is inside it.
    skip: bool,
}

struct Stream {
    log: &'static str,
    segs: Vec<Seg>,
    lines: VecDeque<Line>,
    /// Bytes remain that this call did not read.
    open: bool,
    gap: bool,
    start: Pos,
    /// The last segment read.
    last: usize,
}

impl Stream {
    fn new(log: &'static str, segs: Vec<Seg>, gap: bool) -> Result<Stream, ToolError> {
        let start = match segs.first() {
            Some(s) => s.file.pos(s.from, s.skip)?,
            None => Pos::none(),
        };
        Ok(Stream {
            log,
            segs,
            lines: VecDeque::new(),
            open: false,
            gap,
            start,
            last: 0,
        })
    }

    /// Reads the segments in order, at most `READ_MAX` bytes in all, and
    /// keeps at most `keep` lines: the first, or in the tail of a log the
    /// last. A segment that starts inside a line reads on from the next.
    fn read(&mut self, tail: bool, keep: usize) -> Result<(), ToolError> {
        let mut budget = READ_MAX;
        let mut key = String::new();
        let mut skip = false;
        // The start of a line a rotated copy cut, and where it starts.
        let mut carry: Vec<u8> = Vec::new();
        let mut carry_at = None;
        for i in 0..self.segs.len() {
            let seg = &self.segs[i];
            let (from, old, len) = (seg.from, seg.old, seg.file.len);
            skip |= seg.skip;
            let n = len.saturating_sub(from).min(budget);
            let buf = seg.file.read(from, n)?;
            budget -= n;
            let eof = from + n == len;
            let mut at = 0usize;
            if skip {
                match buf.iter().position(|b| *b == b'\n') {
                    Some(p) => {
                        at = p + 1;
                        skip = false;
                    }
                    None => at = buf.len(),
                }
            }
            let mut full = false;
            while let Some(p) = buf[at..].iter().position(|b| *b == b'\n') {
                let mut raw = std::mem::take(&mut carry);
                raw.extend_from_slice(&buf[at..at + p]);
                carry_at = None;
                self.push(&raw, &mut key, i, from + (at + p + 1) as u64, false);
                at += p + 1;
                if tail && self.lines.len() > keep {
                    self.lines.pop_front();
                }
                if !tail && self.lines.len() >= keep {
                    full = true;
                    break;
                }
            }
            // A line longer than a whole read gives its start, and the rest
            // is skipped, so a poll never stalls on it.
            if !full && !eof && at == 0 && n == READ_MAX && !skip && carry.is_empty() {
                self.push(&buf, &mut key, i, from + n, true);
                at = buf.len();
                skip = true;
            }
            if !full && eof && old && !skip && at < buf.len() && i + 1 < self.segs.len() {
                carry = buf[at..].to_vec();
                carry_at = Some((i, from + at as u64));
                at = buf.len();
            }
            self.segs[i].end = from + at as u64;
            self.segs[i].end_skip = skip;
            self.last = i;
            if full || !eof {
                self.open = true;
                break;
            }
            // Only a rotated copy's cut line goes on in the next file.
            skip &= old;
        }
        if let Some((i, end)) = carry_at {
            self.segs[i].end = end;
            self.segs[i].end_skip = false;
            self.last = i;
        }
        Ok(())
    }

    fn push(&mut self, raw: &[u8], key: &mut String, seg: usize, end: u64, skip: bool) {
        let s = String::from_utf8_lossy(raw);
        let s = s.strip_suffix('\r').unwrap_or(&s);
        let (time, text) = match (s.get(..20), s.get(20..21)) {
            (Some(t), Some(" ")) if epoch(t).is_some() => (Some(t.to_string()), &s[21..]),
            _ => (None, s),
        };
        if text.trim().is_empty() {
            return;
        }
        if let Some(t) = &time {
            key.clone_from(t);
        }
        self.lines.push_back(Line {
            key: key.clone(),
            time,
            text: text.to_string(),
            seg,
            end,
            skip,
        });
    }

    /// Where the next call reads on after the first `k` lines.
    fn after(&self, k: usize) -> Result<Pos, ToolError> {
        if k == 0 && !self.lines.is_empty() || self.segs.is_empty() {
            return Ok(self.start.clone());
        }
        if k < self.lines.len() {
            let l = &self.lines[k - 1];
            return self.segs[l.seg].file.pos(l.end, l.skip);
        }
        // Every line was taken: past what was read, blank lines included.
        let s = &self.segs[self.last];
        s.file.pos(s.end, s.end_skip)
    }
}

/// The stream of one log from a cursor's place in it.
fn follow(
    dir: &Path,
    log: &'static str,
    name: &str,
    pos: &Pos,
    keep: usize,
) -> Result<Stream, ToolError> {
    let cur = open_plain(dir, name)?;
    let old = open_plain(dir, &format!("{name}{ROTATED}"))?;
    let mut segs = Vec::new();
    let mut gap = false;
    if pos.off == 0 {
        // Nothing of the log was read. Its rotated copy is read first only
        // when it is the very file the cursor names.
        let renamed = pos.ino != 0
            && old.as_ref().is_some_and(|o| o.ino == pos.ino)
            && cur.as_ref().is_none_or(|c| c.ino != pos.ino);
        if let (true, Some(o)) = (renamed, old) {
            segs.push(Seg::new(o, 0, false, true));
        }
        if let Some(c) = cur {
            segs.push(Seg::new(c, 0, false, false));
        }
    } else {
        let cur_holds = match &cur {
            Some(c) => c.holds(pos)?,
            None => false,
        };
        let old_holds = match &old {
            Some(o) => o.holds(pos)?,
            None => false,
        };
        let old_is_it = old.as_ref().is_some_and(|o| o.ino == pos.ino)
            && cur.as_ref().is_none_or(|c| c.ino != pos.ino);
        if cur_holds && !(old_holds && old_is_it) {
            segs.push(Seg::new(cur.expect("it holds"), pos.off, pos.skip, false));
        } else {
            if old_holds {
                segs.push(Seg::new(old.expect("it holds"), pos.off, pos.skip, true));
            } else {
                gap = true;
            }
            if let Some(c) = cur {
                segs.push(Seg::new(c, 0, false, false));
            }
        }
    }
    let mut s = Stream::new(log, segs, gap)?;
    s.read(false, keep)?;
    Ok(s)
}

/// The stream of a log's last `READ_MAX` bytes, the end of its rotated copy
/// first when the log is shorter.
fn tail(dir: &Path, log: &'static str, name: &str, keep: usize) -> Result<Stream, ToolError> {
    let cur = open_plain(dir, name)?;
    let short = READ_MAX - cur.as_ref().map_or(0, |c| c.len.min(READ_MAX));
    let mut segs = Vec::new();
    if short > 0 {
        if let Some(o) = open_plain(dir, &format!("{name}{ROTATED}"))? {
            if cur.as_ref().is_none_or(|c| c.ino != o.ino) {
                let from = o.len.saturating_sub(short);
                segs.push(Seg::new(o, from, from > 0, true));
            }
        }
    }
    if let Some(c) = cur {
        let from = c.len.saturating_sub(READ_MAX);
        segs.push(Seg::new(c, from, from > 0, false));
    }
    let mut s = Stream::new(log, segs, false)?;
    s.read(true, keep)?;
    s.open = false;
    Ok(s)
}

/// The kind of a line: the closest of `KINDS` to its first words. A line of
/// no known shape, a tool's own output among them, is a warn.
pub fn kind_of(log: &str, text: &str) -> &'static str {
    let first = text.split_whitespace().next().unwrap_or("");
    match (log, first) {
        (_, "ERROR:" | "ERROR") => "error",
        (_, "WARN:" | "WARN") => "warn",
        ("refresh", _) => "refresh",
        (_, "start" | "tehas-cycle:") => "start",
        (_, "done" | "stop") => "done",
        (_, "planning" | "refining") if text.contains(" waits") => "wait",
        (_, "planning") if text.contains(" runs as ") || text.contains(" continues") => "start",
        (_, "planning") if text.contains(" ended") => "done",
        (_, "refining") => "start",
        (_, "wait" | "holding:" | "hold" | "released:" | "low" | "another" | "nothing") => "wait",
        (_, "the") if text.starts_with("the code index stays") => "wait",
        (_, "skip" | "ignore") => "skip",
        (_, "drop") => "drop",
        (_, "tidy" | "removed" | "clear" | "park") => "tidy",
        (_, "refuse") => "error",
        _ => "warn",
    }
}

/// The issue a line is about: a `#N` among its first five words.
pub fn issue_of(text: &str) -> Option<u64> {
    text.split_whitespace().take(5).find_map(|w| {
        let n = w.strip_prefix('#')?.trim_end_matches([':', ',', ')']);
        n.parse::<u64>().ok().filter(|n| *n > 0)
    })
}

/// Merges the streams by time, each in its own order, a tie to the first.
/// Stops at `limit`, or where a stream ran out of what this call read of it
/// while more remains, since its next line may be older than the others'.
fn merge(streams: &[Stream], limit: usize) -> (Vec<(usize, usize)>, [usize; 2]) {
    let mut taken = [0usize; 2];
    let mut out = Vec::new();
    while out.len() < limit {
        if (0..2).any(|s| taken[s] == streams[s].lines.len() && streams[s].open) {
            break;
        }
        let pick = (0..2)
            .filter(|s| taken[*s] < streams[*s].lines.len())
            .min_by(|a, b| {
                let ka = &streams[*a].lines[taken[*a]].key;
                let kb = &streams[*b].lines[taken[*b]].key;
                ka.cmp(kb).then(a.cmp(b))
            });
        let Some(s) = pick else { break };
        out.push((s, taken[s]));
        taken[s] += 1;
    }
    (out, taken)
}

fn event(s: &Stream, l: &Line) -> Value {
    let text = mask(&l.text);
    let mut e = Map::new();
    if let Some(t) = &l.time {
        e.insert("time".into(), json!(t));
    }
    e.insert("kind".into(), json!(kind_of(s.log, &text)));
    if let Some(n) = issue_of(&text) {
        e.insert("issue".into(), json!(n));
    }
    e.insert("log".into(), json!(s.log));
    e.insert(
        "text".into(),
        json!(text.chars().take(TEXT_CUT).collect::<String>()),
    );
    Value::Object(e)
}

pub(crate) fn call(logs: &Path, args: &Value) -> Result<Map<String, Value>, ToolError> {
    let empty = Map::new();
    let args = match args {
        Value::Null => &empty,
        Value::Object(m) => m,
        _ => return Err(bad("the arguments are an object")),
    };
    if let Some(k) = args.keys().find(|k| *k != "after" && *k != "limit") {
        return Err(bad(&format!(
            "{k} is not an argument: factory_events takes after and limit"
        )));
    }
    let limit = match args.get("limit") {
        None | Some(Value::Null) => DEFAULT_EVENTS,
        Some(v) => v
            .as_u64()
            .filter(|n| (1..=EVENTS_MAX).contains(n))
            .ok_or_else(|| bad(&format!("limit is from 1 to {EVENTS_MAX}")))?,
    } as usize;
    let after = match args.get("after") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(parse_cursor(s)?),
        Some(_) => return Err(bad("after is a cursor from an earlier answer's next")),
    };

    let mut streams = Vec::new();
    for (i, (log, name)) in LOGS.iter().enumerate() {
        streams.push(match &after {
            Some(p) => follow(logs, log, name, &p[i], limit)?,
            None => tail(logs, log, name, limit)?,
        });
    }
    let (picked, taken, more) = match after {
        Some(_) => {
            let (picked, taken) = merge(&streams, limit);
            let more = (0..2).any(|s| taken[s] < streams[s].lines.len() || streams[s].open);
            (picked, taken, more)
        }
        None => {
            let (mut picked, _) = merge(&streams, usize::MAX);
            picked.drain(..picked.len().saturating_sub(limit));
            let all = [streams[0].lines.len(), streams[1].lines.len()];
            (picked, all, false)
        }
    };
    let next = [streams[0].after(taken[0])?, streams[1].after(taken[1])?];
    let events: Vec<Value> = picked
        .iter()
        .map(|(s, i)| event(&streams[*s], &streams[*s].lines[*i]))
        .collect();
    let mut out = Map::new();
    out.insert("events".into(), Value::Array(events));
    out.insert("next".into(), json!(format_cursor(&next)));
    let gaps: Vec<&str> = streams.iter().filter(|s| s.gap).map(|s| s.log).collect();
    if !gaps.is_empty() {
        out.insert("gaps".into(), json!(gaps));
    }
    out.insert("more".into(), json!(more));
    Ok(out)
}

pub fn tool() -> Value {
    let text = |desc: &str| json!({"type": "string", "description": desc});
    json!({
        "name": "factory_events",
        "description": "The dispatcher's and refresh's events, from dispatch.log and refresh.log, masked, oldest first: each with its time, kind, issue when the line names one, the log it is from and its text. Without after: the last 100 events. With after, the next of an earlier answer: the events written since, each once, across a copy-and-truncate rotation of either log. Polled, not streamed: call again with next. At most 256 KiB of each log is read and limit events returned in a call; more is true when there is more to read now.",
        "inputSchema": {"type": "object", "properties": {
            "after": {"type": "string", "maxLength": CURSOR_MAX, "pattern": CURSOR.as_str(), "description": "The next of an earlier answer. The last events without it."},
            "limit": {"type": "integer", "minimum": 1, "maximum": EVENTS_MAX, "description": "The most events to return, from 1 to 500. 100 without it."},
        }, "additionalProperties": false},
        "outputSchema": {"type": "object", "properties": {
            "api": {"type": "integer", "description": "The API version, 1."},
            "events": {"type": "array", "description": "The events, oldest first. Each log keeps its own order; the two are merged by time.", "items": {"type": "object", "properties": {
                "time": text("When it was written, UTC. Left out for a line with no time, such as a tool's own error output."),
                "kind": {"type": "string", "enum": KINDS, "description": "The closest kind: start, done, wait, skip, drop, tidy, refresh, warn or error. A line of no known shape is a warn."},
                "issue": {"type": "integer", "description": "The issue the line names among its first words. Left out when it names none."},
                "log": {"type": "string", "enum": ["dispatch", "refresh"], "description": "The log it is from."},
                "text": text("The line after its time, masked, cut to 1000 characters."),
            }}},
            "next": text("The cursor to pass as after: just past the last event returned."),
            "gaps": {"type": "array", "items": {"type": "string", "enum": ["dispatch", "refresh"]}, "description": "The logs that were replaced or truncated with no rotated copy of what was read: their events since the last call may be missing. Left out when there is none."},
            "more": {"type": "boolean", "description": "True when the call stopped early and more can be read now."},
        }, "required": ["api"]},
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_decision_line_of_the_table_is_a_kind() {
        // The same sample lines `parse` is tested of, so the two readers of
        // these words cannot drift apart on a word the dispatcher writes. The
        // kind each row is read as is named in the table itself.
        for row in crate::decisions::DECISIONS {
            let text = row.sample.split_once(' ').expect("a timed line").1;
            let kind = kind_of("dispatch", text);
            assert!(KINDS.contains(&kind), "{}", row.word);
            assert_eq!(kind, row.kind, "{}", row.sample);
        }
        // The writers' own words for a line that is no decision of this tool
        // are events all the same, each of a kind `factory_events` names.
        for (word, line, kind) in crate::decisions::WRITER_NON_DECISIONS {
            let text = line.split_once(' ').expect("a timed line").1;
            let read = kind_of("dispatch", text);
            assert!(KINDS.contains(&read), "{word}");
            assert_eq!(read, kind, "{line}");
        }
    }

    #[test]
    fn kinds_of_the_real_line_shapes() {
        let d = |t| kind_of("dispatch", t);
        assert_eq!(d("start #1022 (labelled by alice)"), "start");
        assert_eq!(d("start planning #44"), "start");
        assert_eq!(d("tehas-cycle: #1022 runs from /x"), "start");
        assert_eq!(
            d("planning #1015: turn 1 runs as tehas-plan-1015.service"),
            "start"
        );
        assert_eq!(
            d("refining #5 (its cycle parked as x, started by y)"),
            "start"
        );
        assert_eq!(d("done #1012 rc=21 status=merged"), "done");
        assert_eq!(d("done planning #1015 turn=1 rc=0"), "done");
        assert_eq!(d("stop #975: its issue is closed"), "done");
        assert_eq!(d("planning #3 ended"), "done");
        assert_eq!(
            d("planning #7 continues (answer from mill-bot, the fixer)"),
            "start"
        );
        assert_eq!(d("wait #462: tehas:blocked holds it"), "wait");
        assert_eq!(d("holding: the limit allows 0 cycles at once"), "wait");
        assert_eq!(d("hold #688: its last cycle parked"), "wait");
        // The release of a hold is no decision of `factory_decisions`, but it
        // is an event; the poll's own words for its slots are a wait.
        assert_eq!(
            d("lift #7: its last cycle parked on intent-park and the body has not changed since, lifted on the fixer's guidance comment"),
            "warn"
        );
        assert_eq!(
            d("lift #7: its last cycle parked on intent-park and the body has not changed since, lifted on the fixer's guidance comment"),
            "warn"
        );
        assert_eq!(
            d("hold #7: its text links an attachment and TEHAS_WORK_IMAGES is 1, but test-model does not declare image input in models.json"),
            "wait"
        );
        assert_eq!(
            d("hold #7: its text says SCREENSHOT GOES HERE and links no attachment"),
            "wait"
        );
        assert_eq!(
            d("hold #7: its last cycle parked on intent-park and the body has not changed since, and the fixer's comments could not be read"),
            "wait"
        );
        assert_eq!(d("released: cycles may start again"), "wait");
        assert_eq!(
            d("planning #1015 waits: the turn of #1014 is still running"),
            "wait"
        );
        assert_eq!(
            d("refining #5 waits: the limit holds new planning turns"),
            "wait"
        );
        assert_eq!(
            d("the code index stays as it is: a cycle is running on #9"),
            "wait"
        );
        assert_eq!(
            d("skip #316: its question is not among the last 100 comments"),
            "skip"
        );
        assert_eq!(d("ignore #508: tehas:blocked was taken off"), "skip");
        assert_eq!(
            d("drop the refresh of #947: pull request #950 moved"),
            "drop"
        );
        assert_eq!(d("tidy #962: removed tehas:question"), "tidy");
        assert_eq!(d("removed what #410 left on this host"), "tidy");
        assert_eq!(d("clear #784: it is closed"), "tidy");
        assert_eq!(d("park #408: it is marked tehas:running"), "tidy");
        assert_eq!(d("WARN: tidy #962: no merged pull request"), "warn");
        assert_eq!(d("WARN status: the read could not be read"), "warn");
        assert_eq!(d("model endpoint: provider fleet is on its backup"), "warn");
        assert_eq!(d("ERROR: gh api graphql failed"), "error");
        assert_eq!(d("tehas: improve: no log of issue 283"), "warn");
        assert_eq!(d("something new"), "warn");
        let r = |t| kind_of("refresh", t);
        assert_eq!(
            r("read 1 pull requests, 1 conflict, wrote 1 requests"),
            "refresh"
        );
        assert_eq!(
            r("ERROR: GitHub did not list the open pull requests"),
            "error"
        );
        assert_eq!(r("WARN: could not write x"), "warn");
    }

    #[test]
    fn a_copy_cut_in_a_line_keeps_the_bound_of_lines() {
        let dir = std::env::temp_dir().join(format!("mcp-events-keep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("dispatch.log");
        std::fs::write(&log, "2026-10-29T10:00:00Z start #1\n").unwrap();
        let o = open_plain(&dir, "dispatch.log").unwrap().unwrap();
        let pos = o.pos(o.len, false).unwrap();
        let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
        std::io::Write::write_all(&mut f, b"2026-10-29T10:00:01Z WARN: half").unwrap();
        std::fs::copy(&log, dir.join("dispatch.log.1")).unwrap();
        let lines: String = (0..50)
            .map(|i| format!("2026-10-29T10:01:00Z wait #{i}: x\n"))
            .collect();
        std::fs::write(&log, lines).unwrap();
        let s = follow(&dir, "dispatch", "dispatch.log", &pos, 1).unwrap();
        assert_eq!(s.lines.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_issue_is_among_the_first_words() {
        assert_eq!(issue_of("start #1022 (labelled by alice)"), Some(1022));
        assert_eq!(issue_of("skip #316: its question"), Some(316));
        assert_eq!(
            issue_of("drop the refresh of #947: pull request #950"),
            Some(947)
        );
        assert_eq!(issue_of("tehas-cycle: #1022 runs from /x"), Some(1022));
        assert_eq!(
            issue_of("the code index stays as it is: a cycle is running on #9"),
            None
        );
        assert_eq!(issue_of("holding: the limit allows 0"), None);
        assert_eq!(issue_of("skip #0: x"), None);
        assert_eq!(issue_of("skip #12a: x"), None);
    }

    #[test]
    fn a_cursor_reads_back_as_it_was_written() {
        let p = [
            Pos {
                ino: 18446744073709551615,
                off: 7,
                skip: false,
                head: "0123456789abcdef".into(),
            },
            Pos::none(),
        ];
        let s = format_cursor(&p);
        assert!(s.len() <= CURSOR_MAX);
        assert_eq!(parse_cursor(&s).unwrap(), p);
        let mut q = p.clone();
        q[1] = Pos {
            ino: 3,
            off: READ_MAX,
            skip: true,
            head: "0123456789abcdef".into(),
        };
        assert_eq!(parse_cursor(&format_cursor(&q)).unwrap(), q);
        for bad in [
            "",
            "1",
            "2.1.1.0.0123456789abcdef.1.1.0.0123456789abcdef",
            "1.1.1.0.0123456789abcdef",
            "1.1.1.0123456789abcdef.1.1.0123456789abcdef",
            "1.1.1.0.0123456789ABCDEF.1.1.0.0123456789abcdef",
            "1.1.1.0.0123456789abcdef.1.1.0.0123456789abcdef.",
            "1.18446744073709551616.1.0.0123456789abcdef.1.1.0.0123456789abcdef",
            "1.-1.1.0.0123456789abcdef.1.1.0.0123456789abcdef",
            "1.1.1.2.0123456789abcdef.1.1.0.0123456789abcdef",
            "1.1.1.0.0123456789abcdef.1.1.0.0123456789abcdef\n",
        ] {
            assert!(parse_cursor(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_merge_waits_for_a_log_read_only_in_part() {
        let line = |key: &str| Line {
            key: key.into(),
            time: Some(key.into()),
            text: "x".into(),
            seg: 0,
            end: 0,
            skip: false,
        };
        let stream = |keys: &[&str], open| Stream {
            log: "dispatch",
            segs: Vec::new(),
            lines: keys.iter().map(|k| line(k)).collect(),
            open,
            gap: false,
            start: Pos::none(),
            last: 0,
        };
        let a = stream(&["1", "3"], true);
        let b = stream(&["2", "4", "5"], false);
        let (out, taken) = merge(&[a, b], 10);
        assert_eq!(out, [(0, 0), (1, 0), (0, 1)]);
        assert_eq!(taken, [2, 1]);
        let a = stream(&["1", "2"], false);
        let b = stream(&["1", "2"], false);
        let (out, _) = merge(&[a, b], 3);
        assert_eq!(out, [(0, 0), (1, 0), (0, 1)]);
    }
}

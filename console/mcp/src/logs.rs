//! `factory_log`: one log file of one session, masked, plain in pieces or
//! whole gzipped, and the search over those logs.
//!
//! The logs directory also holds the host's own logs. A file is opened only
//! by a name the session's snapshot line holds, after that name matches the
//! one pattern its word allows for the session's kind and issue. A caller
//! never gives a path, and the search opens no file the snapshot does not
//! name.

use crate::sessions;
use crate::snapshot::Snapshot;
use crate::tools::{self, Dirs, Output, ToolError};
use flate2::write::GzEncoder;
use flate2::Compression;
use regex::Regex;
use serde_json::{json, Map, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use tehas_core::mask::mask;

/// A larger file is not served.
pub const LOG_MAX: u64 = 16 * 1024 * 1024;
/// The most of a log a plain call returns, and the default.
pub const PLAIN_MAX: u64 = 1024 * 1024;
/// Logs sent at the same time. Each holds a whole masked file in memory.
pub const LOGS_AT_ONCE: usize = 2;
/// The shortest and longest text a search asks for: a shorter one matches too
/// much of a log to be worth reading it for, and a longer one is a quote, not
/// a search.
const TEXT_MIN: usize = 3;
const TEXT_MAX: usize = 200;
/// The most lines one search answer holds, and the default.
pub const MATCHES_MAX: u64 = 200;
/// The most bytes one search reads through, and the default.
pub const SEARCH_BYTES_MAX: u64 = 8 * 1024 * 1024;
/// The most a match's line is cut to, in characters. A transcript line is
/// long, and an answer of whole lines would be the log again.
const MATCH_LINE: usize = 400;

pub const WORDS: [&str; 7] = [
    "transcript",
    "err",
    "preflight",
    "console",
    "result",
    "dispatch",
    "progress",
];
const WORK: [&str; 6] = [
    "transcript",
    "err",
    "preflight",
    "console",
    "result",
    "dispatch",
];
const PLAN: [&str; 3] = ["progress", "result", "dispatch"];

/// A log of a session, as the snapshot names it: the word of `WORDS` it was
/// written as, the one file name the snapshot's line holds for it, and where to
/// start reading it, in masked bytes - 0, or the `offset` of the record's
/// `searchResume` of this word.
struct LogFile<'a> {
    word: &'a str,
    name: &'a str,
    offset: u64,
}

/// Where the reading of one log got to, as a search gives it back in `next`:
/// the session it was reading, the word of the log and the offset, in masked
/// bytes, of the rest - the one number that says where both the search and
/// `factory_log` read on. Give it back as that record's `searchResume`.
#[derive(Debug, PartialEq, Eq)]
pub struct Resume<'a> {
    pub id: &'a str,
    pub word: &'a str,
    pub offset: u64,
}

/// The logs of one session that the tool serves: the words its kind has, each
/// once, with a name its pattern allows. A word the snapshot lists twice, or
/// under a name it does not allow, answers as the reader of one log does: a
/// file error that names the snapshot or the word, never the bad name.
fn logs_of(s: &Value) -> Result<Vec<LogFile<'_>>, ToolError> {
    let id = s["id"].as_str().unwrap_or("");
    let kind = s["kind"].as_str().unwrap_or("");
    let issue = match (words_of(kind), s["issue"].as_u64().filter(|n| *n > 0)) {
        (w, Some(n)) if !w.is_empty() && id.starts_with(&format!("{kind}-{n}-")) => n,
        _ => {
            return Err(file_error(
                crate::snapshot::FILE,
                &format!("has no session {id} with a kind and issue to find its logs by"),
            ))
        }
    };
    let listed = listed_of(s);
    let mut seen: Vec<&str> = listed.iter().filter_map(|l| l["word"].as_str()).collect();
    seen.sort_unstable();
    if let Some(w) = seen.windows(2).find(|p| p[0] == p[1]).map(|p| p[0]) {
        return Err(file_error(
            crate::snapshot::FILE,
            &format!("lists the {w} log of {id} twice"),
        ));
    }
    let mut out = Vec::new();
    for word in words_of(kind) {
        let Some(entry) = listed.iter().find(|l| l["word"].as_str() == Some(*word)) else {
            continue;
        };
        let name = entry["name"].as_str().unwrap_or("");
        if !name_ok(kind, issue, word, name) {
            return Err(file_error(
                word,
                "has a name in the snapshot that is not a log's",
            ));
        }
        out.push(LogFile {
            word,
            name,
            offset: 0,
        });
    }
    Ok(out)
}

static LOGS: AtomicUsize = AtomicUsize::new(0);

/// One of the logs being sent. Dropping it frees the place, so it is held
/// until the answer is written.
#[derive(Debug)]
pub struct LogGuard(());

impl Drop for LogGuard {
    fn drop(&mut self) {
        LOGS.fetch_sub(1, Ordering::SeqCst);
    }
}

/// A place to send a log, or None when `LOGS_AT_ONCE` are being sent.
pub fn acquire() -> Option<LogGuard> {
    LOGS.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
        (n < LOGS_AT_ONCE).then_some(n + 1)
    })
    .ok()
    .map(|_| LogGuard(()))
}

fn words_of(kind: &str) -> &'static [&'static str] {
    match kind {
        "work" => &WORK,
        "plan" => &PLAN,
        _ => &[],
    }
}

/// The one file name pattern `word` allows for a session.
fn pattern(kind: &str, issue: u64, word: &str) -> Option<String> {
    let base = format!("^{kind}-{issue}-[0-9]{{6}}-[0-9]{{6}}");
    let suffix = match (kind, word) {
        ("work", "transcript") | ("plan", "progress") => r"\.jsonl",
        ("work", "err") => r"\.jsonl\.err",
        ("work", "preflight") => r"\.jsonl\.preflight",
        ("work", "console") => r"\.jsonl\.console",
        ("work", "result") => r"\.jsonl\.result\.json",
        ("plan", "result") => r"\.json",
        ("work" | "plan", "dispatch") => return Some(format!(r"^dispatch-{kind}-{issue}\.log$")),
        _ => return None,
    };
    Some(format!("{base}{suffix}$"))
}

fn name_ok(kind: &str, issue: u64, word: &str, name: &str) -> bool {
    pattern(kind, issue, word)
        .and_then(|p| Regex::new(&p).ok())
        .is_some_and(|re| re.is_match(name))
}

fn file_error(file: &str, why: &str) -> ToolError {
    ToolError::File {
        file: file.into(),
        why: why.into(),
    }
}

/// The file masked line by line, its newlines kept as they were. Only a plain
/// file of at most `LOG_MAX` bytes is read, never one through a link.
pub fn masked(path: &Path) -> Result<String, ToolError> {
    let err = |why: &str| file_error("log", why);
    let meta = std::fs::symlink_metadata(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            err("is gone")
        } else {
            err("could not be read")
        }
    })?;
    if !meta.file_type().is_file() {
        return Err(err("is not a plain file"));
    }
    if meta.len() > LOG_MAX {
        return Err(err("is over 16 MiB and is not served"));
    }
    let f = std::fs::File::open(path).map_err(|_| err("could not be read"))?;
    // The file opened is the one checked, not a link put in its place since.
    let opened = f.metadata().map_err(|_| err("could not be read"))?;
    if opened.dev() != meta.dev() || opened.ino() != meta.ino() || !opened.is_file() {
        return Err(err("is not a plain file"));
    }
    let mut r = BufReader::new(f.take(LOG_MAX + 1));
    let mut out = String::with_capacity(meta.len() as usize);
    let (mut line, mut read) = (Vec::new(), 0u64);
    loop {
        line.clear();
        let n = r
            .read_until(b'\n', &mut line)
            .map_err(|_| err("could not be read"))?;
        if n == 0 {
            break;
        }
        read += n as u64;
        if read > LOG_MAX {
            return Err(err("is over 16 MiB and is not served"));
        }
        let end = line.strip_suffix(b"\n");
        out.push_str(&mask(&String::from_utf8_lossy(end.unwrap_or(&line))));
        if end.is_some() {
            out.push('\n');
        }
    }
    Ok(out)
}

/// Standard base64 with padding.
pub fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn bad(why: &str) -> ToolError {
    ToolError::BadArgs(why.to_string())
}

/// The tool, for `tools::list`.
pub fn tool() -> Value {
    let int = |desc: &str| json!({"type": "integer", "description": desc});
    let text = |desc: &str| json!({"type": "string", "description": desc});
    json!({
        "name": "factory_log",
        "description": "One log file of a session of the last 30 days, masked. Plain: text from offset, at most bytes, 1 MiB at most; follow next for the rest. gzip: the whole masked file compressed, as an embedded application/gzip resource. Work sessions have transcript, err, preflight, console, result and dispatch; planning sessions have progress, result and dispatch. dispatch is per issue and holds every run of it. Files over 16 MiB are not served.",
        "inputSchema": {"type": "object", "properties": {
            "id": {"type": "string", "pattern": sessions::ID.as_str(), "description": "The session id from factory_sessions."},
            "file": {"type": "string", "enum": WORDS, "description": "Which log, a word factory_session lists."},
            "gzip": {"type": "boolean", "description": "The whole file gzipped. Not with offset or bytes."},
            "offset": {"type": "integer", "minimum": 0, "description": "Where to start, in bytes of the masked text. 0 without it."},
            "bytes": {"type": "integer", "minimum": 1, "maximum": PLAIN_MAX, "description": "How many bytes of masked text, 1 MiB without it."},
        }, "required": ["id", "file"], "additionalProperties": false},
        "outputSchema": {"type": "object", "properties": {
            "api": int("The API version, 1."),
            "id": text("The session id."),
            "file": text("The word."),
            "size": int("The masked file's size in bytes."),
            "offset": int("Plain: where the text starts, at the start of a character."),
            "next": int("Plain: the offset of the rest. Left out at the end."),
            "text": text("Plain: the masked text."),
            "gzipBytes": int("gzip: the size of the compressed file."),
        }, "required": ["api"]},
    })
}

pub(crate) fn call(dirs: &Dirs, args: &Value) -> Result<(Output, LogGuard), ToolError> {
    let id = args["id"]
        .as_str()
        .ok_or_else(|| bad("id is a string like work-12-1792485967"))?;
    if !sessions::ID.is_match(id) {
        return Err(bad("id is like work-12-1792485967"));
    }
    let word = match args["file"].as_str() {
        Some(w) if WORDS.contains(&w) => w,
        _ => return Err(bad(&format!("file is one of {}", WORDS.join(", ")))),
    };
    let gzip = match &args["gzip"] {
        Value::Null => false,
        Value::Bool(b) => *b,
        _ => return Err(bad("gzip is true or false")),
    };
    if gzip && (!args["offset"].is_null() || !args["bytes"].is_null()) {
        return Err(bad("offset and bytes are for plain reads, not gzip"));
    }
    let offset = match &args["offset"] {
        Value::Null => 0,
        v => v
            .as_u64()
            .ok_or_else(|| bad("offset is an integer of at least 0"))?,
    };
    let bytes = match &args["bytes"] {
        Value::Null => PLAIN_MAX,
        v => v
            .as_u64()
            .filter(|n| (1..=PLAIN_MAX).contains(n))
            .ok_or_else(|| bad(&format!("bytes is from 1 to {PLAIN_MAX}")))?,
    };

    let place =
        acquire().ok_or_else(|| bad("two logs are being sent; try again in a few seconds"))?;
    let snap = Snapshot::open(&dirs.sessions, tools::now())?;
    let s = sessions::find(&snap, id)?;
    let kind = s["kind"].as_str().unwrap_or("");
    let issue = s["issue"].as_u64().filter(|n| *n > 0);
    let (words, issue) = match (words_of(kind), issue) {
        (w, Some(n)) if !w.is_empty() && id.starts_with(&format!("{kind}-{n}-")) => (w, n),
        _ => return Err(bad("the session has no kind and issue to find its logs by")),
    };
    let listed = listed_of(&s);
    // Two files under one word would be two runs' logs: neither is served.
    let mut seen: Vec<&str> = listed.iter().filter_map(|l| l["word"].as_str()).collect();
    seen.sort_unstable();
    if let Some(w) = seen.windows(2).find(|p| p[0] == p[1]).map(|p| p[0]) {
        return Err(file_error(
            crate::snapshot::FILE,
            &format!("lists the {w} log of {id} twice"),
        ));
    }
    let entry = listed
        .iter()
        .find(|l| l["word"].as_str() == Some(word))
        .filter(|_| words.contains(&word));
    let Some(entry) = entry else {
        let has: Vec<&str> = words
            .iter()
            .copied()
            .filter(|w| listed.iter().any(|l| l["word"].as_str() == Some(w)))
            .collect();
        let logs = if has.is_empty() {
            "none".to_string()
        } else {
            has.join(", ")
        };
        return Err(bad(&format!(
            "the session has no {word} log; its logs are {logs}"
        )));
    };
    let name = entry["name"].as_str().unwrap_or("");
    if !name_ok(kind, issue, word, name) {
        return Err(file_error(
            word,
            "has a name in the snapshot that is not a log's",
        ));
    }
    let text = masked(&dirs.logs.join(name)).map_err(|e| match e {
        ToolError::File { why, .. } => file_error(word, &why),
        e => e,
    })?;
    let size = text.len() as u64;

    let mut out = Map::new();
    out.insert("api".into(), json!(1));
    out.insert("id".into(), json!(id));
    out.insert("file".into(), json!(word));
    out.insert("size".into(), json!(size));
    if gzip {
        let mut z = GzEncoder::new(Vec::new(), Compression::default());
        z.write_all(text.as_bytes())
            .map_err(|_| file_error(word, "could not be compressed"))?;
        drop(text);
        let z = z
            .finish()
            .map_err(|_| file_error(word, "could not be compressed"))?;
        out.insert("gzipBytes".into(), json!(z.len()));
        let blob = Output::Blob {
            meta: Value::Object(out),
            uri: format!("tehas://sessions/{id}/{word}.gz"),
            mime: "application/gzip",
            blob: base64(&z),
        };
        return Ok((blob, place));
    }
    if offset > size {
        return Err(bad(&format!("offset is past the end; the size is {size}")));
    }
    let (start, end) = piece(&text, offset as usize, bytes as usize);
    out.insert("offset".into(), json!(start));
    if end < text.len() {
        out.insert("next".into(), json!(end));
    }
    out.insert("text".into(), json!(&text[start..end]));
    Ok((Output::Json(Value::Object(out)), place))
}

/// The logs a snapshot line lists: `word`, `name` and `bytes` each. Neither the
/// reader of one log nor the search opens a file that is not on the list.
fn listed_of(s: &Value) -> Vec<&Value> {
    s["logs"].as_array().into_iter().flatten().collect()
}

/// One line of a masked log: its text, and where it stands in the masked text
/// of the file, whose newline each line keeps as the whole-file reader keeps it.
/// `start` is an offset a match gives and one to give `factory_log`; `end` is
/// past the line, where the next one starts.
struct MaskedLine {
    start: u64,
    end: u64,
    text: String,
}

/// The file read line by line and masked as the whole-file reader masks - the
/// mask of the line, and its newline kept as it was - so a line holds what of
/// the secrets the patterns know and nothing more, and a match's offset is one
/// to give `factory_log`. `from` is one of those offsets: a line that begins
/// before it is read and masked, but not kept, because the search reads on from
/// where a `Resume` left off and a line is one unit of it - the line that holds
/// a `from` in the middle of it is read and masked for nothing. A resumed
/// reading that kept no line at all read the file for nothing: the cap was
/// spent before its reading got past the offset, and what lies past it is
/// beyond this search, which is what says the log is done. The reading stops at
/// the last line that ends within `stop` raw bytes, which it says with
/// `stopped`, and gives the bytes it read so the cap of a whole search can be
/// shared out. Nothing past the cap is read, so a cap never cuts a line in two,
/// which would hide a secret that spans the cut. The file must be a plain file
/// of at most `LOG_MAX` bytes, never one through a link.
fn scan(path: &Path, from: u64, stop: u64) -> Result<(Vec<MaskedLine>, u64, bool), ToolError> {
    let err = |why: &str| file_error("log", why);
    let meta = std::fs::symlink_metadata(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            err("is gone")
        } else {
            err("could not be read")
        }
    })?;
    if !meta.file_type().is_file() {
        return Err(err("is not a plain file"));
    }
    if meta.len() > LOG_MAX {
        return Err(err("is over 16 MiB and is not served"));
    }
    let f = std::fs::File::open(path).map_err(|_| err("could not be read"))?;
    // The file opened is the one checked, not a link put in its place since.
    let opened = f.metadata().map_err(|_| err("could not be read"))?;
    if opened.dev() != meta.dev() || opened.ino() != meta.ino() || !opened.is_file() {
        return Err(err("is not a plain file"));
    }
    let mut r = BufReader::new(f);
    let mut lines = Vec::new();
    let (mut raw_read, mut masked_read) = (0u64, 0u64);
    loop {
        // Over `LOG_MAX` is not served, whoever asks for the lines.
        if raw_read > LOG_MAX {
            return Err(err("is over 16 MiB and is not served"));
        }
        // One line of more than what is left of the cap is cut where the
        // reading stops: the bytes past it are never looked at.
        if raw_read >= stop {
            break;
        }
        let mut raw = Vec::new();
        let n = r
            .read_until(b'\n', &mut raw)
            .map_err(|_| err("could not be read"))?;
        if n == 0 {
            break;
        }
        raw_read += n as u64;
        let end = raw.strip_suffix(b"\n");
        let (had_newline, body) = match end {
            Some(body) => (true, body),
            None => (false, &raw[..]),
        };
        let mut text = mask(&String::from_utf8_lossy(body));
        if had_newline {
            text.push('\n');
        }
        // The newline belongs to the masked text as the whole-file reader keeps
        // it, so the offset of the next line counts it too.
        let masked_end = masked_read + text.len() as u64;
        // The line is kept when it begins at `from` or later; the one that
        // holds a `from` in the middle of it is read and masked for nothing.
        if masked_read >= from {
            lines.push(MaskedLine {
                start: masked_read,
                end: masked_end,
                text,
            });
        }
        masked_read = masked_end;
    }
    Ok((lines, raw_read, raw_read >= stop))
}

/// `line` cut to `MATCH_LINE` characters, never inside one. The newline of the
/// line is kept, as it is in the masked text the offset counts, so the answer
/// of a match is the line the log holds.
fn cut(line: &str) -> String {
    match line.char_indices().nth(MATCH_LINE) {
        Some((at, _)) => line[..at].to_string(),
        None => line.to_string(),
    }
}

/// The `logs` of one session record, in the order its kind lists the words,
/// with the one place a stopped search reads that log on from: the record's own
/// `searchResume`, or the start of it.
fn logs_with_resume(s: &Value) -> Result<Vec<LogFile<'_>>, ToolError> {
    let mut out = logs_of(s)?;
    let r = &s["searchResume"];
    for l in &mut out {
        if r["word"].as_str() == Some(l.word) {
            l.offset = r["offset"].as_u64().unwrap_or(0);
        }
    }
    Ok(out)
}

/// One line a search found, and where it is: the word of the log it was read
/// from, its offset in masked bytes - one to give `factory_log` - and the line
/// itself, cut to `MATCH_LINE` characters with its newline kept as the masked
/// text keeps it.
pub struct Found<'a> {
    pub word: &'a str,
    pub offset: u64,
    pub line: String,
}

/// The lines of the logs of `sessions` that hold `text`, a fixed string, not a
/// pattern, and where the reading got to. Each session is one snapshot record;
/// nothing here opens a file a record's `logs` list does not name.
///
/// A log is masked line by line as `factory_log` masks a file, so a search
/// cannot find what the mask takes out. It stops when `limit` matches are found
/// or `byte_cap` raw bytes have been read, whichever comes first, and says so
/// with the `Resume` of where it got to - the record, the word of its log and
/// the offset of the rest in masked bytes - and gives `None` when every log was
/// read to its end. `files`, when given, narrows the search to the words named.
///
/// The way on of a search that stopped is the same call with the `Resume` put on
/// the record it names as that record's `searchResume`: of that record then only
/// that one log is read, from that offset on, and every other record keeps its
/// own way on, so the rest of one log never hides another session's lines. The
/// offset is the end of the last line the answer holds, so no line is in two
/// answers at once. The answer of a reading the cap spent before it reached a
/// line of its own is that the log is done: it holds none, and the way on of a
/// log reads it no further.
pub fn search<'a>(
    dirs: &Dirs,
    sessions: &'a [Value],
    text: &str,
    files: Option<&'a [&'a str]>,
    limit: u64,
    byte_cap: u64,
) -> Result<(Vec<Found<'a>>, Option<Resume<'a>>), ToolError> {
    let len = text.chars().count();
    if !(TEXT_MIN..=TEXT_MAX).contains(&len) {
        return Err(bad(&format!(
            "text is {TEXT_MIN} to {TEXT_MAX} characters, not {len}"
        )));
    }
    if !(1..=MATCHES_MAX).contains(&limit) {
        return Err(bad(&format!(
            "limit is from 1 to {MATCHES_MAX}, not {limit}"
        )));
    }
    if byte_cap == 0 || byte_cap > SEARCH_BYTES_MAX {
        return Err(bad(&format!(
            "bytes is from 1 to {SEARCH_BYTES_MAX}, not {byte_cap}"
        )));
    }
    if let Some(words) = files {
        for w in words {
            if !WORDS.contains(w) {
                return Err(bad(&format!("file {w} is not one of {}", WORDS.join(", "))));
            }
        }
    }

    // The logs of every record first, all of them checked, so a snapshot that
    // names one file twice or under a name it does not allow is refused before
    // a byte of any log is read.
    let mut logs = Vec::with_capacity(sessions.len());
    for s in sessions {
        let id = s["id"].as_str().unwrap_or("");
        if !sessions::ID.is_match(id) {
            return Err(bad("id is like work-12-1792485967"));
        }
        // Of a record that carries a `searchResume` only the log it names is
        // read, from its offset on; of the rest, every log `files` names.
        let resumed = s["searchResume"]["word"].as_str();
        let wanted = |word: &str| match resumed {
            Some(w) => w == word,
            None => files.is_none_or(|fs| fs.contains(&word)),
        };
        let kept = logs_with_resume(s)?
            .into_iter()
            .filter(|l| wanted(l.word))
            .collect::<Vec<_>>();
        logs.push((id, kept));
    }

    // The cap is the whole search's: what one log costs is off what the rest of
    // it has left, whatever session the rest belongs to.
    let mut left = byte_cap;
    let mut found = Vec::new();
    let mut next = None;
    for (id, logs) in &logs {
        for l in logs {
            let (lines, read, stopped) =
                scan(&dirs.logs.join(l.name), l.offset, left).map_err(|e| match e {
                    ToolError::File { why, .. } => file_error(l.word, &why),
                    e => e,
                })?;
            left = left.saturating_sub(read);
            for line in &lines {
                if !line.text.contains(text) {
                    continue;
                }
                found.push(Found {
                    word: l.word,
                    offset: line.start,
                    line: cut(&line.text),
                });
                if found.len() as u64 == limit {
                    // The match just taken is in the answer, so the rest starts
                    // at the line after it.
                    next = Some(Resume {
                        id,
                        word: l.word,
                        offset: line.end,
                    });
                    return Ok((found, next));
                }
            }
            if stopped {
                // Where a stopped reading leaves this log: the end of the last
                // line the answer holds, and the rest of the log follows from
                // there, past that line and so never the offset it came from.
                // A resumed reading that kept no line read the file for nothing:
                // the cap was spent on bytes the answer already holds, so the
                // log is done, and a `next` naming the offset the caller resumed
                // from would be a way on that is no way on.
                let to = match lines.last() {
                    Some(last) => last.end,
                    // The reading holds no line: past the offset it came from
                    // there is nothing of this log left to answer, and at it the
                    // answer would say nothing new.
                    None if l.offset > 0 => continue,
                    None => l.offset,
                };
                return Ok((
                    found,
                    Some(Resume {
                        id,
                        word: l.word,
                        offset: to,
                    }),
                ));
            }
        }
    }
    Ok((found, next))
}

/// From `offset` moved on to a character, at most `n` bytes cut back to one,
/// but never empty before the end.
fn piece(text: &str, offset: usize, n: usize) -> (usize, usize) {
    let mut start = offset.min(text.len());
    while !text.is_char_boundary(start) {
        start += 1;
    }
    let mut end = start.saturating_add(n).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    if end == start && start < text.len() {
        end += 1;
        while !text.is_char_boundary(end) {
            end += 1;
        }
    }
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_word_allows_one_name() {
        let ok = [
            ("work", "transcript", "work-7-261021-100000.jsonl"),
            ("work", "err", "work-7-261021-100000.jsonl.err"),
            ("work", "result", "work-7-261021-100000.jsonl.result.json"),
            ("work", "dispatch", "dispatch-work-7.log"),
            ("plan", "progress", "plan-7-261021-100000.jsonl"),
            ("plan", "result", "plan-7-261021-100000.json"),
            ("plan", "dispatch", "dispatch-plan-7.log"),
        ];
        for (k, w, n) in ok {
            assert!(name_ok(k, 7, w, n), "{k} {w} {n}");
        }
        let no = [
            ("work", "transcript", "work-77-261021-100000.jsonl"),
            ("work", "transcript", "work-7-261021-100000.jsonl.err"),
            ("work", "transcript", "work-7-261021-100000.jsonl\n"),
            ("work", "err", "work-7-261021-100000.jsonl"),
            ("work", "dispatch", "dispatch-plan-7.log"),
            ("work", "dispatch", "dispatch-work-7.log/x"),
            ("work", "progress", "work-7-261021-100000.jsonl"),
            ("plan", "transcript", "plan-7-261021-100000.jsonl"),
            ("plan", "result", "plan-7-261021-100000.jsonl.result.json"),
            ("work", "transcript", "../work-7-261021-100000.jsonl"),
            ("other", "dispatch", "dispatch-other-7.log"),
        ];
        for (k, w, n) in no {
            assert!(!name_ok(k, 7, w, n), "{k} {w} {n:?}");
        }
    }
}

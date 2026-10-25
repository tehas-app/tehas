//! `factory_log`: one log file of one session, masked, plain in pieces or
//! whole gzipped.
//!
//! The logs directory also holds the host's own logs. A file is opened only
//! by a name the session's snapshot line holds, after that name matches the
//! one pattern its word allows for the session's kind and issue. A caller
//! never gives a path.

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
    let listed: Vec<&Value> = s["logs"].as_array().into_iter().flatten().collect();
    // Two files under one word would be two runs' logs: neither is served.
    let mut seen: Vec<&str> = listed.iter().filter_map(|l| l["word"].as_str()).collect();
    seen.sort_unstable();
    if let Some(w) = seen.windows(2).find(|p| p[0] == p[1]).map(|p| p[0]) {
        return Err(file_error(
            crate::snapshot::FILE,
            &format!("lists the {w} log of {id} twice"),
        ));
    }
    let has: Vec<&str> = words
        .iter()
        .copied()
        .filter(|w| listed.iter().any(|l| l["word"].as_str() == Some(w)))
        .collect();
    let entry = listed
        .iter()
        .find(|l| l["word"].as_str() == Some(word))
        .filter(|_| words.contains(&word));
    let Some(entry) = entry else {
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

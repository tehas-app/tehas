// factory_log over a snapshot and a logs directory written by the tests.

use flate2::read::GzDecoder;
use mcp::keys::{parse_keys, Setting};
use mcp::logs::{acquire, base64, masked, search, MATCHES_MAX, PLAIN_MAX, SEARCH_BYTES_MAX};
use mcp::rpc::{handle, Request};
use mcp::tools::{call, Dirs, Output, ToolError};
use serde_json::{json, Value};
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, MutexGuard};
use tehas_core::time::{iso, now};

// The number of logs being sent is one for the process: the tests take turns.
static TURN: Mutex<()> = Mutex::new(());

fn turn() -> MutexGuard<'static, ()> {
    TURN.lock().unwrap_or_else(|e| e.into_inner())
}

// Built here, not written out: scripts/check-generic.sh reads this file too.
fn ip(parts: [u8; 4]) -> String {
    parts.map(|p| p.to_string()).join(".")
}

fn token() -> String {
    format!("ghp_{}", "abcdefghijklmnopqrstuvwxyzABCDEFGHIJ")
}

const STAMP: &str = "261021-100000";

// The one instant the session fixtures of this process are stamped at. `now()`
// moves between two calls of it, so the id a record holds and the id a test asks
// the tool with are one id only when every record comes of one instant, taken
// once. Ten digits, which is what `ID` allows an id's last part to be, and a
// hundred seconds before the run, so a record is inside the window of a snapshot
// written at the instant the environment is made.
fn start() -> i64 {
    static START: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *START.get_or_init(|| now() - 100)
}

struct Env {
    root: PathBuf,
    dirs: Dirs,
}

impl Env {
    // `logs` is (word, name, content) per file. A session with no files
    // still lists the words, so a test can plant a name.
    fn new(name: &str, kind: &str, issue: i64, logs: &[(&str, &str, &[u8])]) -> Env {
        let root = std::env::temp_dir().join(format!("mcp-logs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (live, snap, dir) = (root.join("live"), root.join("mcp"), root.join("logs"));
        for d in [&live, &snap, &dir] {
            std::fs::create_dir_all(d).unwrap();
        }
        let listed: Vec<Value> = logs
            .iter()
            .map(|(w, n, c)| {
                let p = dir.join(n);
                if !n.contains('/') && !n.is_empty() {
                    std::fs::write(&p, c).unwrap();
                }
                json!({"word": w, "name": n, "bytes": c.len()})
            })
            .collect();
        let t = start();
        let s = json!({
            "id": format!("{kind}-{issue}-{t}"), "kind": kind, "issue": issue,
            "started": iso(t), "ended": iso(t + 60), "logs": listed,
        });
        let text = format!(
            "{}\n{s}\n",
            json!({"api": 1, "days": 30, "generatedAt": iso(now()), "sessions": 1})
        );
        std::fs::write(snap.join("sessions.jsonl"), &text).unwrap();
        Env {
            root,
            dirs: Dirs {
                live,
                sessions: snap.clone(),
                logs: dir,
            },
        }
    }

    fn id(&self) -> String {
        let text = std::fs::read_to_string(self.dirs.sessions.join("sessions.jsonl")).unwrap();
        let line: Value = serde_json::from_str(text.lines().nth(1).unwrap()).unwrap();
        line["id"].as_str().unwrap().to_string()
    }

    fn log(&self, args: Value) -> Result<Output, ToolError> {
        let mut a = args;
        a["id"] = json!(self.id());
        call(&self.dirs, "factory_log", &a)
    }

    fn plain(&self, args: Value) -> Value {
        match self.log(args) {
            Ok(Output::Json(v)) => v,
            other => panic!("not plain: {other:?}"),
        }
    }

    fn error(&self, args: Value) -> String {
        match self.log(args) {
            Err(ToolError::BadArgs(why)) => why,
            Err(ToolError::File { file, why }) => format!("{file} {why}"),
            other => panic!("not a tool error: {other:?}"),
        }
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

// A snapshot of the sessions of `sessions`, and a logs directory holding
// `logs` by name: the ground the search is tried on, removed when dropped.
struct Search {
    root: PathBuf,
    dirs: Dirs,
}

impl Search {
    // The environment of a test: a snapshot of `sessions`, and a logs directory
    // holding `logs` by name. The records a test hands to `ask` are the snapshot
    // own lines, as the caller of a tool holds them, and the tool reads the
    // sessions of `issue` or `ids` out of that same snapshot.
    fn new(name: &str, sessions: &[Value], logs: &[(&str, &[u8])]) -> Search {
        Self::built(name, sessions, logs)
    }

    // As `new`, for a test of a tool that names sessions by id: the lines of the
    // snapshot it reads are given back with it, so the test asks with the ids the
    // tool can find and not with the ids of records it wrote for itself.
    fn with(name: &str, sessions: &[Value], logs: &[(&str, &[u8])]) -> (Search, Vec<Value>) {
        let env = Self::built(name, sessions, logs);
        (env, sessions.to_vec())
    }

    fn built(name: &str, served: &[Value], logs: &[(&str, &[u8])]) -> Search {
        // The number of the environment is taken by the one call that gives it,
        // so two environments built in parallel cannot read the same one and
        // share a directory: the process id alone would let them, and the name
        // of the test is unique only among the tests.
        let n = SEARCH_NO.fetch_add(1, Ordering::SeqCst).to_string();
        let root =
            std::env::temp_dir().join(format!("mcp-search-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (live, snap, dir) = (root.join("live"), root.join("mcp"), root.join("logs"));
        for d in [&live, &snap, &dir] {
            std::fs::create_dir_all(d).unwrap();
        }
        for (n, c) in logs {
            std::fs::write(dir.join(n), c).unwrap();
        }
        let mut text = header(served.len()).to_string();
        for s in served {
            text.push('\n');
            text.push_str(&s.to_string());
        }
        std::fs::write(snap.join("sessions.jsonl"), text + "\n").unwrap();
        Search {
            root,
            dirs: Dirs {
                live,
                sessions: snap,
                logs: dir,
            },
        }
    }

    fn records(&self) -> Vec<Value> {
        let text = std::fs::read_to_string(self.dirs.sessions.join("sessions.jsonl")).unwrap();

        text.lines()
            .skip(1)
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    // The snapshot the tools read, with these records. The search of the rest
    // reads the record of the session it stopped in as its caller holds it, with
    // the `next` of the answer put on it as its `searchResume`, so a test of the
    // way on gives the tool that record and not the one the snapshot held before.
    fn wrote(&self, records: &[Value]) {
        let path = self.dirs.sessions.join("sessions.jsonl");
        let text = std::fs::read_to_string(&path).unwrap();
        let header = text.lines().next().unwrap();
        let mut out = format!("{header}\n");
        for r in records {
            out.push_str(&r.to_string());
            out.push('\n');
        }
        std::fs::write(&path, out).unwrap();
    }

    // The answer of a search: `Found` as JSON, with the id of the session the
    // word belongs to put back on it. A test asserts the words of an answer
    // with `found`, `rest` or `error`, and one of those fails when the answer
    // is not the kind it asks for, so no test has to name the kind twice.
    fn ask(
        &self,
        records: &[Value],
        text: &str,
        files: Option<&[&str]>,
        limit: u64,
        byte_cap: u64,
    ) -> Result<Value, String> {
        // `Found` is a line of one log of one session. The id of the session it
        // came from is put back by the line's own words: the caller of the tool
        // has the snapshot records, so it names them as `next` does.
        // Of a resumed record the log the `searchResume` names is the one that
        // was stopped before it kept a line: no line of it is in the answer, so
        // of a search that reads it again the caller holds nothing, and a `next`
        // naming the offset it resumed from would be a way on that is no way on.
        let id_of_found = |word: &str, line: &str, from: u64| {
            records
                .iter()
                .find(|r| {
                    r["logs"].as_array().into_iter().flatten().any(|l| {
                        l["word"].as_str() == Some(word)
                            && l["name"].as_str().is_some_and(|n| {
                                std::fs::read_to_string(self.dirs.logs.join(n))
                                    .is_ok_and(|t| t.contains(line))
                            })
                    })
                })
                .and_then(|r| {
                    let resume = &r["searchResume"];
                    if resume["word"].as_str() == Some(word)
                        && resume["offset"].as_u64() != Some(from)
                    {
                        return None;
                    }
                    r["id"].as_str()
                })
                .unwrap_or("")
                .to_string()
        };
        match search(&self.dirs, records, text, files, limit, byte_cap) {
            Ok((found, next)) => {
                let found: Vec<Value> = found
                    .iter()
                    .map(|f| {
                        json!({
                            "id": id_of_found(f.word, &f.line, f.offset),
                            "file": f.word,
                            "offset": f.offset,
                            "line": f.line,
                        })
                    })
                    .collect();
                let mut o = json!({"matches": found});
                if let Some(n) = next {
                    o["next"] = json!({"id": n.id, "file": n.word, "offset": n.offset});
                }
                Ok(o)
            }
            Err(ToolError::BadArgs(why)) => Err(why),
            Err(ToolError::File { file, why }) => Err(format!("{file} {why}")),
            Err(other) => panic!("not a tool error: {other:?}"),
        }
    }

    // The answer of a search, which the test then reads.
    fn found(
        &self,
        records: &[Value],
        text: &str,
        files: Option<&[&str]>,
        limit: u64,
        byte_cap: u64,
    ) -> Value {
        // An answer, or the words of a refusal: both are the test's to read, and
        // `error` is the one that expects the second.
        match self.ask(records, text, files, limit, byte_cap) {
            Ok(v) => v,
            Err(why) => panic!("no answer: {why}"),
        }
    }

    // The rest of the log `next` names, from its offset: the way on of an
    // answer that stopped, asked by putting the `next` on the record it came
    // from as its `searchResume`, which is all the search of the rest needs.
    fn rest(&self, records: &[Value], text: &str, next: &Value) -> Value {
        self.rest_of(records, text, next, SEARCH_BYTES_MAX)
    }

    // The way on again, with the cap of the test's choosing: a `next` given back
    // with the cap that made it is the way on a caller takes, so the tests of
    // the cap ask it that way and not only with the greatest one.
    fn rest_of(&self, records: &[Value], text: &str, next: &Value, byte_cap: u64) -> Value {
        let on = records
            .iter()
            .map(|r| {
                let mut r = r.clone();
                if r["id"] == next["id"] {
                    r["searchResume"] = json!({"word": next["file"], "offset": next["offset"]});
                }
                r
            })
            .collect::<Vec<Value>>();
        match self.ask(&on, text, None, MATCHES_MAX, byte_cap) {
            Ok(v) => v,
            Err(why) => panic!("{why}"),
        }
    }

    // The words a search that refuses gives back: the `Err` side of `ask`.
    fn error(&self, records: &[Value], text: &str, files: Option<&[&str]>) -> String {
        match self.ask(records, text, files, MATCHES_MAX, SEARCH_BYTES_MAX) {
            Err(why) => why,
            Ok(v) => panic!("not an error: {v}"),
        }
    }

    // The whole masked text of one log of the session, as `factory_log` gives
    // it: the ground an offset of a search answer is read back against.
    // The whole masked text of one log of the session, as `factory_log` gives
    // it: the ground an offset of a search answer is read back against.
    fn text(&self, id: &str, word: &str) -> String {
        self.plain(id, json!({"file": word}))["text"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn plain(&self, id: &str, args: Value) -> Value {
        let mut a = args;
        a["id"] = json!(id);
        match call(&self.dirs, "factory_log", &a) {
            Ok(Output::Json(v)) => v,
            other => panic!("not plain: {other:?}"),
        }
    }

    // `factory_search` over the whole snapshot: the same reading as `ask`, told
    // by the arguments of the tool rather than by records handed to it. A test
    // of the tool asserts its answer with `searched` or `refused`, so one of
    // those fails when the answer is not the kind it asks for.
    fn asked(&self, args: Value) -> Result<Value, String> {
        match call(&self.dirs, "factory_search", &args) {
            Ok(Output::Json(v)) => Ok(v),
            Ok(other) => panic!("not JSON: {other:?}"),
            Err(ToolError::BadArgs(why)) => Err(why),
            Err(ToolError::File { file, why }) => Err(format!("{file} {why}")),
            Err(other) => panic!("not a tool error: {other:?}"),
        }
    }

    fn searched(&self, args: Value) -> Value {
        match self.asked(args) {
            Ok(v) => v,
            Err(why) => panic!("no answer: {why}"),
        }
    }

    // The words a call that must be refused gives back. The argument it names
    // are kept as the caller wrote them, so the `ids` of the refusal are the
    // caller's and not the snapshot's.
    fn refused(&self, want: Value) -> String {
        let args = want.clone();
        match self.asked(args) {
            Err(why) => why,
            Ok(v) => panic!("{want} answered: {v}"),
        }
    }

    // The words of a call whose arguments are not an object of arguments at all:
    // `refused` reads them as the caller wrote them, which for these is no
    // member of an object, so they go to the call directly.
    fn refused_at(&self, args: Value) -> String {
        match self.asked(args) {
            Err(why) => why,
            Ok(v) => panic!("answered: {v}"),
        }
    }

    // The way on of the tool's own answer, as a caller takes it: the same call
    // with the session it stopped in named alone, and the answer's `file` and
    // `offset` put back on that session's record as its `searchResume`, which is
    // all the reading of the rest needs. The other sessions keep their records.
    fn on(&self, args: Value, records: &[Value], next: &Value) -> Value {
        let mut a = args;
        a["ids"] = json!([next["id"]]);
        a["files"] = json!([next["file"]]);
        let lines: Vec<Value> = records
            .iter()
            .map(|r| {
                if r["id"] == next["id"] {
                    resumed(r, next)
                } else {
                    r.clone()
                }
            })
            .collect();
        self.wrote(&lines);
        self.searched(a)
    }
}

static SEARCH_NO: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl Drop for Search {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn header(served: usize) -> Value {
    json!({"api": 1, "days": 30, "generatedAt": iso(now()), "sessions": served})
}

// One session's snapshot record, with the logs of `logs` - and nothing else,
// which is what makes the host's own logs unreachable to a search. `started` is
// the instant of its id, so the order of the snapshot is a test's to choose, and
// a test that asks the tools for a session by id holds the record whose id it is.
fn record_at(kind: &str, issue: i64, started: i64, logs: &[(&str, &str)]) -> Value {
    json!({
        "id": format!("{kind}-{issue}-{started}"), "kind": kind, "issue": issue,
        "started": iso(started), "ended": iso(started + 60),
        "logs": logs.iter().map(|(w, n)| json!({"word": w, "name": n, "bytes": 1}))
                    .collect::<Vec<Value>>(),
    })
}

// One session's snapshot record, with the logs of `logs` - and nothing else,
// which is what makes the host's own logs unreachable to a search. Every record
// of one test is stamped at the same instant, so the ids a test reads out of the
// snapshot are the ids its records hold: `now()` moves between two calls of it.
fn record(kind: &str, issue: i64, logs: &[(&str, &str)]) -> Value {
    record_at(kind, issue, start(), logs)
}

// The record of a search that stopped, as its caller gives it back: the place
// `next` names, put on the session it stopped in as that session's
// `searchResume`, which is all the reading of the rest needs.
fn resumed(s: &Value, next: &Value) -> Value {
    let mut s = s.clone();
    s["searchResume"] = json!({"word": next["file"], "offset": next["offset"]});
    s
}

fn id_of(s: &Value) -> String {
    s["id"].as_str().unwrap().to_string()
}

// A search answer as a string, for asserting what no match may hold.
fn text_of(v: &Value) -> String {
    v.to_string()
}

fn transcript(issue: i64) -> String {
    format!("work-{issue}-{STAMP}.jsonl")
}

fn secrets() -> Vec<String> {
    vec![token(), "hunter2".into(), "u:pw@".into(), ip([10, 1, 2, 3])]
}

fn secret_log() -> String {
    format!(
        "{{\"line\":\"pushed with {}\"}}\nPASSWORD=hunter2\nfetch https://u:pw@example.com/\nmodel at {}:8086\n",
        token(),
        ip([10, 1, 2, 3])
    )
}

fn gunzip(b64: &str) -> String {
    let bytes = decode64(b64);
    let mut out = String::new();
    GzDecoder::new(&bytes[..]).read_to_string(&mut out).unwrap();
    out
}

fn decode64(s: &str) -> Vec<u8> {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes().filter(|c| *c != b'=') {
        acc = acc << 6 | A.iter().position(|a| *a == c).unwrap() as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

#[test]
fn base64_known_vectors() {
    for (i, o) in [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ] {
        assert_eq!(base64(i.as_bytes()), o, "{i}");
        assert_eq!(decode64(o), i.as_bytes());
    }
}

#[test]
fn masks_plain_and_gzip() {
    let _t = turn();
    let log = secret_log();
    let env = Env::new(
        "mask",
        "work",
        7,
        &[("transcript", &transcript(7), log.as_bytes())],
    );
    let plain = env.plain(json!({"file": "transcript"}));
    let text = plain["text"].as_str().unwrap();
    assert!(text.contains("PASSWORD="), "{text}");
    assert_eq!(plain["size"], text.len());
    assert_eq!(plain["offset"], 0);
    assert!(plain.get("next").is_none());
    assert_eq!(plain["api"], 1);
    assert_eq!(plain["file"], "transcript");

    let Ok(Output::Blob {
        meta,
        uri,
        mime,
        blob,
    }) = env.log(json!({"file": "transcript", "gzip": true}))
    else {
        panic!("not a blob")
    };
    let unzipped = gunzip(&blob);
    assert_eq!(unzipped, text);
    assert_eq!(mime, "application/gzip");
    assert_eq!(uri, format!("tehas://sessions/{}/transcript.gz", env.id()));
    assert_eq!(meta["size"], text.len());
    assert_eq!(meta["gzipBytes"], decode64(&blob).len());
    assert_eq!(meta["api"], 1);
    for s in secrets() {
        assert!(!text.contains(&s), "plain has {s}");
        assert!(!unzipped.contains(&s), "gzip has {s}");
    }
}

#[test]
fn offset_inside_a_token() {
    let _t = turn();
    let log = secret_log();
    let env = Env::new(
        "offset",
        "work",
        7,
        &[("transcript", &transcript(7), log.as_bytes())],
    );
    let at = log.find(&token()).unwrap() + 10;
    let out = env.plain(json!({"file": "transcript", "offset": at}));
    let text = out["text"].as_str().unwrap();
    let tail = &token()[10..];
    for w in tail.as_bytes().windows(6) {
        let w = std::str::from_utf8(w).unwrap();
        assert!(!text.contains(w), "{w} in {text}");
    }
}

#[test]
fn last_line_without_newline() {
    let _t = turn();
    let log = format!("first\nlast {}", token());
    let env = Env::new(
        "last",
        "work",
        7,
        &[("transcript", &transcript(7), log.as_bytes())],
    );
    let text = env.plain(json!({"file": "transcript"}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(!text.contains(&token()), "{text}");
    assert!(text.starts_with("first\nlast ghp_"), "{text}");
    assert!(!text.ends_with('\n'));
    let path = env.dirs.logs.join(transcript(7));
    assert_eq!(masked(&path).unwrap(), text);
}

// A plain read masks the whole file and then cuts one piece from it, so the
// wait of this test is the mask of the whole log times the number of pieces,
// not the size of a piece. In the debug profile the mask of a megabyte of
// these lines costs about a second, so the log this test had - two and a half
// megabytes - cost near two seconds a read, and the four reads of it (one to
// know the whole, then the pieces) were most of the eight seconds of this
// file. The mask stays where the source puts it: a read of a piece has to know
// the size and the end of the whole masked text, and `joined` is the right
// answer only if each read masks again. What went was the size of the log, so
// it is now the smallest that still needs two pieces. A piece is counted in
// bytes of the masked text and the mask makes these lines about 40 percent
// shorter, so a file of about 1.75 MB masks to just over the 1 MiB of one
// plain read: two pieces, two masks, and a quarter of the wait. The other
// tests of this file read a log of a few hundred bytes and each waited about
// six seconds only because they take turns, so they waited on this one.
#[test]
fn pieces_join() {
    let _t = turn();
    const MIN_LOG: usize = 1_750_000;
    const MAX_LOG: usize = 1_760_000;
    let mut log = String::new();
    let mut i = 0;
    while log.len() < MIN_LOG {
        log.push_str(&format!(
            "{{\"n\":{i},\"t\":\"{} ä {}\"}}\n",
            token(),
            ip([192, 168, 1, (i % 250) as u8])
        ));
        i += 1;
    }
    // Each line is 70 bytes, so the log overshoots the floor by one line at
    // most and is nowhere near a third piece.
    assert!(log.len() <= MAX_LOG, "the log grew to {} B", log.len());
    let env = Env::new(
        "pieces",
        "work",
        7,
        &[("transcript", &transcript(7), log.as_bytes())],
    );
    let whole = masked(&env.dirs.logs.join(transcript(7))).unwrap();
    let mut joined = String::new();
    let mut offset = 0u64;
    let mut pieces = 0;
    loop {
        let out = env.plain(json!({"file": "transcript", "offset": offset}));
        assert_eq!(out["size"], whole.len());
        assert_eq!(out["offset"], offset);
        let text = out["text"].as_str().unwrap();
        assert!(text.len() as u64 <= PLAIN_MAX);
        joined.push_str(text);
        pieces += 1;
        match out["next"].as_u64() {
            Some(n) => offset = n,
            None => break,
        }
    }
    assert!(pieces >= 2, "{pieces}");
    assert_eq!(joined, whole);
}

#[test]
fn utf8_boundary() {
    let _t = turn();
    let log = "aä€b\n";
    let env = Env::new(
        "utf8",
        "work",
        7,
        &[("transcript", &transcript(7), log.as_bytes())],
    );
    // ä is bytes 1..3, € 3..6: four bytes from 0 would cut €.
    let out = env.plain(json!({"file": "transcript", "offset": 0, "bytes": 4}));
    assert_eq!(out["text"], "aä");
    assert_eq!(out["next"], 3);
    // An offset inside ä starts at the next character.
    let out = env.plain(json!({"file": "transcript", "offset": 2, "bytes": 3}));
    assert_eq!(out["text"], "€");
    assert_eq!(out["offset"], 3);
    assert_eq!(out["next"], 6);
    // A piece too small for one character still moves on.
    let out = env.plain(json!({"file": "transcript", "offset": 3, "bytes": 1}));
    assert_eq!(out["text"], "€");
    assert_eq!(out["next"], 6);
}

#[test]
fn link_and_growth() {
    let _t = turn();
    let env = Env::new(
        "link",
        "work",
        7,
        &[
            ("transcript", &transcript(7), b"x\n"),
            ("err", &format!("{}.err", transcript(7)), b"e\n"),
            ("console", &format!("{}.console", transcript(7)), b"c\n"),
        ],
    );
    let outside = env.root.join("outside");
    std::fs::write(&outside, secret_log()).unwrap();
    let t = env.dirs.logs.join(transcript(7));
    std::fs::remove_file(&t).unwrap();
    std::os::unix::fs::symlink(&outside, &t).unwrap();
    let e = env.error(json!({"file": "transcript"}));
    assert!(e.starts_with("transcript "), "{e}");
    let e = env.error(json!({"file": "transcript", "gzip": true}));
    assert!(e.starts_with("transcript "), "{e}");

    let err = env.dirs.logs.join(format!("{}.err", transcript(7)));
    std::fs::File::options()
        .write(true)
        .open(&err)
        .unwrap()
        .set_len(16 * 1024 * 1024 + 1)
        .unwrap();
    let e = env.error(json!({"file": "err"}));
    assert!(e.starts_with("err ") && e.contains("16 MiB"), "{e}");

    std::fs::remove_file(env.dirs.logs.join(format!("{}.console", transcript(7)))).unwrap();
    let e = env.error(json!({"file": "console"}));
    assert!(e.starts_with("console "), "{e}");
}

#[test]
fn bad_names_never_open() {
    let _t = turn();
    let env = Env::new(
        "names",
        "work",
        7,
        &[
            ("transcript", "../x", b""),
            ("err", &format!("{}.err", transcript(8)), b"e\n"),
            ("dispatch", "dispatch-work-7.log/x", b""),
            ("console", &transcript(7), b"t\n"),
        ],
    );
    // Planted where the names point: none may be read.
    std::fs::write(env.root.join("x"), "outside").unwrap();
    std::fs::create_dir(env.dirs.logs.join("dispatch-work-7.log")).unwrap();
    std::fs::write(env.dirs.logs.join("dispatch-work-7.log/x"), "inside").unwrap();
    for word in ["transcript", "err", "dispatch", "console"] {
        let e = env.error(json!({"file": word}));
        assert!(e.starts_with(&format!("{word} ")), "{e}");
        for name in ["../x", "work-8", "dispatch-work-7.log", "work-7"] {
            assert!(!e.contains(name), "{e}");
        }
    }
}

#[test]
fn a_word_listed_twice_is_not_served() {
    let _t = turn();
    let env = Env::new(
        "twice",
        "work",
        7,
        &[
            ("transcript", "work-7-261021-095900.jsonl", b"older\n"),
            ("transcript", &transcript(7), b"newer\n"),
            ("err", &format!("{}.err", transcript(7)), b"e\n"),
        ],
    );
    for word in ["transcript", "err"] {
        let e = env.error(json!({"file": word}));
        assert!(
            e.starts_with("sessions.jsonl lists the transcript log of work-7-"),
            "{e}"
        );
        assert!(e.ends_with(" twice"), "{e}");
    }
}

#[test]
fn the_id_is_checked_before_a_place_and_the_snapshot() {
    let _t = turn();
    let dirs = Dirs {
        live: "/nonexistent/live".into(),
        sessions: "/nonexistent/mcp".into(),
        logs: "/nonexistent/logs".into(),
    };
    let a = acquire().unwrap();
    let b = acquire().unwrap();
    for id in ["../x", "work-01-123456789", ""] {
        match call(
            &dirs,
            "factory_log",
            &json!({"id": id, "file": "transcript"}),
        ) {
            Err(ToolError::BadArgs(why)) => assert_eq!(why, "id is like work-12-1792485967"),
            other => panic!("{id:?}: {other:?}"),
        }
    }
    drop(a);
    drop(b);
}

#[test]
fn wrong_word_for_kind() {
    let _t = turn();
    let env = Env::new(
        "kind",
        "plan",
        5,
        &[
            ("progress", &format!("plan-5-{STAMP}.jsonl"), b"p\n"),
            ("result", &format!("plan-5-{STAMP}.json"), b"{}\n"),
            ("dispatch", "dispatch-plan-5.log", b"d\n"),
        ],
    );
    let e = env.error(json!({"file": "transcript"}));
    assert!(e.contains("progress, result, dispatch"), "{e}");
    assert_eq!(env.plain(json!({"file": "result"}))["text"], "{}\n");
    assert_eq!(env.plain(json!({"file": "dispatch"}))["text"], "d\n");

    let work = Env::new(
        "kind2",
        "work",
        7,
        &[("transcript", &transcript(7), b"t\n")],
    );
    let e = work.error(json!({"file": "err"}));
    assert!(e.contains("transcript") && !e.contains("err,"), "{e}");
    let e = work.error(json!({"file": "nope"}));
    assert!(e.contains("transcript, err, preflight"), "{e}");
}

#[test]
fn arguments_are_checked() {
    let _t = turn();
    let env = Env::new(
        "args",
        "work",
        7,
        &[("transcript", &transcript(7), b"0123456789\n")],
    );
    for a in [
        json!({"file": "transcript", "gzip": true, "offset": 0}),
        json!({"file": "transcript", "gzip": true, "bytes": 5}),
        json!({"file": "transcript", "bytes": 0}),
        json!({"file": "transcript", "bytes": PLAIN_MAX + 1}),
        json!({"file": "transcript", "offset": 12}),
        json!({"file": "transcript", "offset": -1}),
        json!({"file": "transcript", "gzip": "yes"}),
    ] {
        env.error(a);
    }
    let out = env.plain(json!({"file": "transcript", "offset": 11}));
    assert_eq!(out["text"], "");
    assert!(out.get("next").is_none());
}

#[test]
fn two_at_once() {
    let _t = turn();
    let env = Env::new("two", "work", 7, &[("transcript", &transcript(7), b"t\n")]);
    let a = acquire().unwrap();
    let b = acquire().unwrap();
    assert!(acquire().is_none());
    assert_eq!(
        env.error(json!({"file": "transcript"})),
        "two logs are being sent; try again in a few seconds"
    );
    drop(a);
    drop(b);
    assert_eq!(env.plain(json!({"file": "transcript"}))["text"], "t\n");
}

#[test]
fn blob_shape() {
    let _t = turn();
    let env = Env::new("blob", "work", 7, &[("transcript", &transcript(7), b"t\n")]);
    let k = "abcdefghijklmnopqrstuvwxyz012345";
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {
        "name": "factory_log",
        "arguments": {"id": env.id(), "file": "transcript", "gzip": true},
        "_meta": {"io.modelcontextprotocol/protocolVersion": "2026-07-28"},
    }})
    .to_string();
    let auth = format!("Bearer {k}");
    let headers = [
        ("Authorization", auth.as_str()),
        ("MCP-Protocol-Version", "2026-07-28"),
        ("Mcp-Method", "tools/call"),
        ("Mcp-Name", "factory_log"),
    ];
    let req = Request {
        method: "POST",
        headers: headers
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect(),
        body: body.as_bytes(),
    };
    let r = handle(
        &req,
        &parse_keys([(&Setting::Full, k), (&Setting::Dashboard, "")]).unwrap(),
        &env.dirs,
        "v9.9.9",
    );
    let v: Value = serde_json::from_slice(&r.body).unwrap();
    let res = &v["result"];
    assert_eq!(res["content"][1]["type"], "resource", "{v}");
    let resource = &res["content"][1]["resource"];
    assert_eq!(resource["mimeType"], "application/gzip");
    assert_eq!(gunzip(resource["blob"].as_str().unwrap()), "t\n");
    assert!(res["structuredContent"].get("blob").is_none());
    assert_eq!(res["structuredContent"]["file"], "transcript");

    // The answer holds its place until it is dropped, after it is written.
    let other = acquire().expect("one place left");
    assert!(acquire().is_none(), "the answer gave up its place");
    drop(other);
    drop(r);
    let (a, b) = (acquire(), acquire());
    assert!(a.is_some() && b.is_some(), "the place was not freed");
}

// The search over a session's masked logs. The mask is the one `factory_log`
// uses, so what it takes out of a log no search finds either, and a match's
// offset is one to give `factory_log` to read the line back.

#[test]
fn a_match_is_read_back_by_the_log_reader() {
    let _t = turn();
    let log = "first\nthe pids limit was hit here\nlast\n";
    let s = record("work", 7, &[("transcript", &transcript(7))]);
    let id = id_of(&s);
    let env = Search::new(
        "found",
        std::slice::from_ref(&s),
        &[(&transcript(7), log.as_bytes())],
    );
    let r = env.records();
    let out = env.found(&r, "pids limit", None, MATCHES_MAX, SEARCH_BYTES_MAX);
    assert_eq!(out["matches"].as_array().unwrap().len(), 1, "{out}");
    let m = &out["matches"][0];
    assert_eq!(m["id"], id);
    assert_eq!(m["file"], "transcript");
    assert!(out.get("next").is_none(), "{out}");

    // The offset is in masked bytes: the plain read of it starts at the line.
    let text = env.text(&id, "transcript");
    let at = m["offset"].as_u64().unwrap() as usize;
    assert!(text.is_char_boundary(at), "{at} is inside a character");
    let piece = env.plain(&id, json!({"file": "transcript", "offset": at}));
    assert_eq!(piece["offset"], at);
    assert_eq!(piece["text"], "the pids limit was hit here\nlast\n");
    assert_eq!(at, 6, "first\n is six bytes of the masked text");
    assert_eq!(m["line"], "the pids limit was hit here\n");
}

#[test]
fn a_secret_the_mask_removes_is_not_found() {
    let _t = turn();
    let secret = token();
    let log = format!(
        "pushed with {secret}\nPASSWORD=hunter2\nread at {}:8086\n",
        ip([10, 1, 2, 3])
    );
    let s = record("work", 7, &[("transcript", &transcript(7))]);
    let env = Search::new("secret", &[s], &[(&transcript(7), log.as_bytes())]);
    let r = env.records();
    for want in [&secret, "hunter2", "u:pw@", &ip([10, 1, 2, 3])] {
        let out = env.found(&r, want, None, MATCHES_MAX, SEARCH_BYTES_MAX);
        assert_eq!(out["matches"], json!([]), "{want} found in {log}");
    }
    // What the mask leaves is found, and no match holds a secret.
    let out = env.found(&r, "pushed with", None, MATCHES_MAX, SEARCH_BYTES_MAX);
    assert_eq!(out["matches"].as_array().unwrap().len(), 1, "{out}");
    for s in secrets() {
        assert!(!text_of(&out).contains(&s), "{s} in {out}");
    }
}

#[test]
fn the_byte_cap_stops_with_next_and_the_rest_follows() {
    let _t = turn();
    // Four lines of nine bytes each, all of them a match: the cap of eighteen
    // bytes reads the first two and stops before the third.
    let log = "aaaa hit\nbbbb hit\ncccc hit\ndddd hit\n";
    let s = record("work", 7, &[("transcript", &transcript(7))]);
    let env = Search::new("cap", &[s], &[(&transcript(7), log.as_bytes())]);
    let r = env.records();
    let out = env.found(&r, "hit", None, MATCHES_MAX, 18);
    let m = out["matches"].as_array().unwrap();
    assert_eq!(m.len(), 2, "{out}");
    assert_eq!(m[0]["offset"], 0);
    assert_eq!(m[1]["offset"], 9);
    // A cap that stops the reading always says where the rest starts, in the
    // masked text the offsets of the matches are counted in.
    assert_eq!(out["next"]["file"], "transcript");
    assert_eq!(out["next"]["offset"], 18);

    // The way on reads the log from there and finds the rest, once each.
    let on = env.rest(&r, "hit", &out["next"]);
    let rest = on["matches"].as_array().unwrap();
    assert_eq!(rest.len(), 2, "{on}");
    assert_eq!(rest[0]["offset"], 18);
    assert_eq!(rest[1]["offset"], 27);
    assert!(on.get("next").is_none(), "{on}");
    // Nothing from before the offset is answered twice.
    let whole = out["matches"]
        .as_array()
        .unwrap()
        .iter()
        .chain(on["matches"].as_array().unwrap())
        .map(|m| m["offset"].as_u64().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(whole, vec![0, 9, 18, 27]);
}

#[test]
fn a_cap_stops_and_the_way_on_with_the_same_cap_ends_the_log() {
    let _t = turn();
    // The way on of a cap-bound answer asked again with the very same cap:
    // `rest` raises the cap to the greatest one, so no test follows a `next` of
    // a small cap with that same cap, and a `next` naming the offset the caller
    // resumed from - a way on that is no way on - passed here unseen.
    let log = "aaaa hit\nbbbb hit\ncccc hit\ndddd hit\n";
    let s = record("work", 7, &[("transcript", &transcript(7))]);
    let env = Search::new("same", &[s], &[(&transcript(7), log.as_bytes())]);
    let r = env.records();
    let out = env.found(&r, "hit", None, MATCHES_MAX, 18);
    assert_eq!(out["matches"].as_array().unwrap().len(), 2, "{out}");
    assert_eq!(out["next"]["offset"], 18);
    // The rest starts past the offset of the answer: the reading that held no
    // line of its own is the end of the log, and its `next` is no `next`.
    let on = env.rest_of(&r, "hit", &out["next"], 18);
    assert_eq!(on["matches"], json!([]), "{on}");
    assert!(on.get("next").is_none(), "{on}");
}

#[test]
fn a_cap_spent_before_a_line_ends_the_log_and_nothing_is_lost() {
    let _t = turn();
    // One line of more bytes than the whole cap: the cap is spent reading it,
    // so the way on reads nothing more of the log. The answer of that reading is
    // that the log is done, not the same offset over again, which is what the
    // reading that holds no line answers.
    let line = format!("{} hit {}", "x".repeat(300), "y");
    // The number of the line goes in a field of its own, so the mask shortens
    // every line the same way: `05` is not a number the mask reads as one.
    let log: String = (0..50)
        .map(|i| format!("{i:02} {}\n", &line[3..]))
        .collect();
    let s = record("work", 7, &[("transcript", &transcript(7))]);
    let env = Search::new("spent", &[s], &[(&transcript(7), log.as_bytes())]);
    // The masked text of the first line with its newline - the step from one
    // line's offset to the next - as the whole-file reader of `factory_log`
    // keeps it, which is what an offset of an answer counts.
    let step = masked(&env.dirs.logs.join(transcript(7)))
        .unwrap()
        .find('\n')
        .unwrap()
        + 1;
    let r = env.records();
    let out = env.found(&r, " hit ", None, MATCHES_MAX, 200);
    assert_eq!(out["matches"].as_array().unwrap().len(), 1, "{out}");
    // The one match is the line the reading paid for, at its own offset, and the
    // way on starts at the line after it.
    assert_eq!(out["matches"][0]["offset"], 0);
    assert_eq!(out["next"]["offset"], step as u64);
    // The same call again, on the way on, ends the log: no matches, no `next`,
    // and the offsets asked for never come back the same.
    let on = env.rest_of(&r, " hit ", &out["next"], 200);
    assert_eq!(on["matches"], json!([]), "{on}");
    assert!(on.get("next").is_none(), "{on}");
    // With the greatest cap the log gives all fifty lines, so none of them was
    // lost to the reading that stopped: the cap spent the log, not hid it.
    let all = env.rest_of(&r, " hit ", &out["next"], SEARCH_BYTES_MAX);
    assert_eq!(all["matches"].as_array().unwrap().len(), 49, "{all}");
    let offs = all["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["offset"].as_u64().unwrap())
        .collect::<Vec<_>>();
    let want: Vec<u64> = (1..50).map(|i| (i * step) as u64).collect();
    assert_eq!(offs, want);
}

#[test]
fn a_line_the_cap_crosses_is_answered_and_its_bytes_are_paid_for() {
    let _t = turn();
    // The cap stops the reading with a line whose beginning it paid for and
    // whose end it did not: 8 bytes cover the first line, 18 the second, so a
    // cap of 12 reads the second line whole and keeps it - the reading paid for
    // some of its bytes, so it is one the answer may hold - and stops there.
    let log = "hit one\nabcdefghij hit two\nlast hit\n";
    let s = record("work", 7, &[("transcript", &transcript(7))]);
    let env = Search::new("crossed", &[s], &[(&transcript(7), log.as_bytes())]);
    let r = env.records();
    let out = env.found(&r, "hit", None, MATCHES_MAX, 12);
    let m = out["matches"].as_array().unwrap();
    assert_eq!(m.len(), 2, "{out}");
    assert_eq!(m[0]["offset"], 0);
    assert_eq!(m[1]["offset"], 8);
    // The line is answered whole, its newline kept, though the cap ran out
    // inside it: a cap never cuts a line in two.
    assert_eq!(m[1]["line"], "abcdefghij hit two\n");
    assert_eq!(
        out["next"]["offset"], 27,
        "the end of the line the answer holds"
    );
    // The way on from there holds no line of its own - the cap is spent on
    // bytes already read - so the log is done and the last line follows only
    // when the cap is raised, once, from the same offset.
    let on = env.rest_of(&r, "hit", &out["next"], 12);
    assert_eq!(on["matches"], json!([]), "{on}");
    assert!(on.get("next").is_none(), "{on}");
    let on = env.rest_of(&r, "hit", &out["next"], SEARCH_BYTES_MAX);
    assert_eq!(on["matches"].as_array().unwrap().len(), 1, "{on}");
    assert_eq!(on["matches"][0]["offset"], 27);
    assert_eq!(on["matches"][0]["line"], "last hit\n");
}

#[test]
fn the_limit_stops_with_next_and_the_rest_follows() {
    let _t = turn();
    let log = "one hit\ntwo miss\nthree hit\nfour hit\n";
    let s = record("work", 7, &[("transcript", &transcript(7))]);
    let env = Search::new("limit", &[s], &[(&transcript(7), log.as_bytes())]);
    let r = env.records();
    let out = env.found(&r, "hit", None, 2, SEARCH_BYTES_MAX);
    let m = out["matches"].as_array().unwrap();
    assert_eq!(m.len(), 2, "{out}");
    assert_eq!(m[0]["line"], "one hit\n");
    assert_eq!(m[0]["offset"], 0);
    assert_eq!(m[1]["line"], "three hit\n");
    // "one hit\n" and "two miss\n" come to 17 bytes before the third line.
    assert_eq!(m[1]["offset"], 17);
    assert_eq!(out["next"]["file"], "transcript");
    assert_eq!(out["next"]["offset"], 27);

    let on = env.rest(&r, "hit", &out["next"]);
    let rest = on["matches"].as_array().unwrap();
    assert_eq!(rest.len(), 1, "{on}");
    assert_eq!(rest[0]["line"], "four hit\n");
    assert!(on.get("next").is_none(), "{on}");
}

#[test]
fn a_text_of_too_few_or_too_many_characters_is_refused() {
    let _t = turn();
    let s = record("work", 7, &[("transcript", &transcript(7))]);
    let env = Search::new("length", &[s], &[(&transcript(7), b"hit hit hit\n")]);
    let r = env.records();
    // 3 and 200 are allowed, 2 and 201 are not; a character is what counts, so
    // a multi-byte line counts its characters and not its bytes.
    for (n, ok) in [(2, false), (3, true), (200, true), (201, false)] {
        let text = "ä".repeat(n);
        let got = env.ask(&r, &text, None, MATCHES_MAX, SEARCH_BYTES_MAX);
        if ok {
            assert!(got.is_ok(), "{n}: {got:?}");
        } else {
            let why = got.unwrap_err();
            assert!(why.starts_with("text is 3 to 200 characters"), "{n}: {why}");
        }
    }
    // Of an empty logs directory the search asks nothing, so a refused text is
    // refused whatever the logs hold.
    let none = Search::new("length2", &[], &[]);
    let empty: Vec<Value> = Vec::new();
    assert!(none
        .error(&empty, "ab", None)
        .starts_with("text is 3 to 200"));
    assert!(none
        .error(&empty, &"x".repeat(201), None)
        .starts_with("text is 3 to 200"));
}

#[test]
fn the_files_narrow_the_search_and_a_bad_word_is_refused() {
    let _t = turn();
    let s = record(
        "work",
        7,
        &[
            ("transcript", &transcript(7)),
            ("err", &format!("{}.err", transcript(7))),
        ],
    );
    let env = Search::new(
        "words",
        &[s],
        &[
            (&transcript(7), b"a hit in the transcript\n"),
            (
                &format!("{}.err", transcript(7)),
                b"a hit in the error output\n",
            ),
        ],
    );
    let r = env.records();
    for (word, want) in [("transcript", "transcript"), ("err", "error output")] {
        let out = env.found(
            &r,
            "a hit",
            Some(&[word][..]),
            MATCHES_MAX,
            SEARCH_BYTES_MAX,
        );
        let m = out["matches"].as_array().unwrap();
        assert_eq!(m.len(), 1, "{word}: {out}");
        assert_eq!(m[0]["file"], word);
        assert!(m[0]["line"].as_str().unwrap().contains(want), "{out}");
    }
    let both = env.found(
        &r,
        "a hit",
        Some(&["transcript", "err"][..]),
        MATCHES_MAX,
        SEARCH_BYTES_MAX,
    );
    assert_eq!(both["matches"].as_array().unwrap().len(), 2, "{both}");
    let why = env.error(&r, "a hit", Some(&["nope"][..]));
    assert!(why.starts_with("file nope is not one of"), "{why}");
}

#[test]
fn only_the_logs_the_snapshot_names_are_opened() {
    let _t = turn();
    // The host's own log of the same issue, planted where a name of the
    // snapshot's own would not be allowed, and named by no session.
    let host = "dispatch-work-7.log";
    let s = record("work", 7, &[("transcript", &transcript(7))]);
    let env = Search::new(
        "host",
        &[s],
        &[
            (&transcript(7), b"the run of the cycle\n"),
            (host, b"the host's own hit line\n"),
        ],
    );
    let r = env.records();
    let out = env.found(&r, "hit", None, MATCHES_MAX, SEARCH_BYTES_MAX);
    assert_eq!(out["matches"], json!([]), "{out}");
    // A word the snapshot does not list is left out without an error: there is
    // nothing of it to read.
    let out = env.found(&r, "hit", Some(&["err"][..]), MATCHES_MAX, SEARCH_BYTES_MAX);
    assert_eq!(out["matches"], json!([]), "{out}");
}

#[test]
fn a_log_that_is_gone_or_a_link_is_a_file_error_of_its_word() {
    let _t = turn();
    let s = record(
        "work",
        7,
        &[
            ("transcript", &transcript(7)),
            ("err", &format!("{}.err", transcript(7))),
            ("console", &format!("{}.console", transcript(7))),
        ],
    );
    let env = Search::new(
        "gone",
        &[s],
        &[
            (&transcript(7), b"here\n"),
            (&format!("{}.err", transcript(7)), b"there\n"),
        ],
    );
    // The console is named by the snapshot and not in the logs directory.
    let e = env.error(&env.records(), "here", Some(&["console"][..]));
    assert!(e.starts_with("console ") && e.contains("is gone"), "{e}");
    // A log put in place by a link to a file outside the logs directory.
    let outside = env.root.join("outside");
    std::fs::write(&outside, b"a hit outside\n").unwrap();
    let err = env.dirs.logs.join(format!("{}.err", transcript(7)));
    std::fs::remove_file(&err).unwrap();
    std::os::unix::fs::symlink(&outside, &err).unwrap();
    let e = env.error(&env.records(), "a hit", Some(&["err"][..]));
    assert!(e.starts_with("err ") && e.contains("plain file"), "{e}");
    // The error names the word, never the name it opened.
    for name in [transcript(7), "outside".to_string(), "../".to_string()] {
        assert!(!e.contains(&name), "{e}");
    }
}

#[test]
fn a_bad_name_in_the_snapshot_is_refused_before_any_read() {
    let _t = turn();
    let s = record(
        "work",
        7,
        &[
            ("transcript", &transcript(7)),
            ("err", &format!("work-8-{STAMP}.jsonl.err")),
        ],
    );
    let env = Search::new(
        "badname",
        &[s],
        &[
            (&transcript(7), b"a hit here\n"),
            (&format!("work-8-{STAMP}.jsonl.err"), b"a hit there\n"),
        ],
    );
    let r = env.records();
    // Every log the session lists is checked before one byte is read, so a
    // search that would read them all is refused by the one bad name, whether
    // or not the words asked for it: `files` leaves no log unchecked.
    let mut last = String::new();
    for files in [None, Some(&["transcript"][..]), Some(&["err"][..])] {
        let e = env.error(&r, "a hit", files);
        assert!(e.starts_with("err "), "{e}");
        assert!(e.contains("not a log's"), "{e}");
        last = e;
    }
    // The file is never opened for it: the words of the error name the word and
    // the snapshot, never the name it refused.
    for name in [format!("work-8-{STAMP}.jsonl.err"), "outside".to_string()] {
        assert!(!last.contains(&name), "{last}");
    }
}

#[test]
fn a_search_spans_the_sessions_it_is_given() {
    let _t = turn();
    let (a, b) = (
        record("work", 7, &[("transcript", &transcript(7))]),
        record("work", 8, &[("transcript", &transcript(8))]),
    );
    let env = Search::new(
        "span",
        &[a.clone(), b.clone()],
        &[
            (&transcript(7), b"one hit\n"),
            (&transcript(8), b"two hit\n"),
        ],
    );
    let r = env.records();
    let out = env.found(&r, "hit", None, MATCHES_MAX, SEARCH_BYTES_MAX);
    let m = out["matches"].as_array().unwrap();
    assert_eq!(m.len(), 2, "{out}");
    assert_eq!(m[0]["id"], id_of(&a));
    assert_eq!(m[0]["line"], "one hit\n");
    assert_eq!(m[1]["id"], id_of(&b));
    assert_eq!(m[1]["line"], "two hit\n");
    assert!(out.get("next").is_none(), "{out}");

    // The cap is the whole search's, not one log's: it stops in the first
    // session's log and the rest is answered from there, so no line is missed
    // and none is given twice.
    let stopped = env.found(&r, "hit", None, MATCHES_MAX, 8);
    assert_eq!(stopped["matches"].as_array().unwrap().len(), 1, "{stopped}");
    assert_eq!(stopped["matches"][0]["id"], id_of(&a), "{stopped}");
    assert_eq!(stopped["next"]["offset"], 8);
    let on = env.rest(&r, "hit", &stopped["next"]);
    assert_eq!(on["matches"].as_array().unwrap().len(), 1, "{on}");
    assert_eq!(on["matches"][0]["id"], id_of(&b), "{on}");
}

#[test]
fn a_log_the_cap_spends_before_it_keeps_a_line_leaves_no_next() {
    let _t = turn();
    // Two sessions, each with one transcript. The first is 1000 filler bytes and
    // no match in any of its lines; the second holds the match. The cap of 100
    // bytes is spent inside the first log, whose reading therefore keeps lines
    // but answers none of them, so the call has nothing to say of it: `next`
    // names the place past the last byte the answer answered, as `factory_log`'s
    // `next` does, and a `next` naming bytes the answer holds nothing of is a way
    // on that is no way on - a caller that follows it is answered the same `next`
    // again, call after call, and the sessions behind it are never read. So the
    // reading moves on with what the cap has left, which is none, and the rest of
    // the search waits for a call of its own, as the manual says. The filler is
    // letters, never a form the mask knows: the mask shortens what it masks, and
    // the offsets of an answer count the masked text.
    let (a, b) = (
        record("work", 7, &[("transcript", &transcript(7))]),
        record("work", 8, &[("transcript", &transcript(8))]),
    );
    let big = "xxxxxxxxxx\n".repeat(100);
    let env = Search::new(
        "spent_log",
        &[a.clone(), b.clone()],
        &[
            (&transcript(7), big.as_bytes()),
            (&transcript(8), b"a needle of another session\n"),
        ],
    );
    let r = env.records();
    // The cap is spent inside the first log, which holds no match. The answer
    // holds no line, and the `next` it gives is a step forward - past the bytes
    // the reading really read, never the start of the log it read them from, and
    // never the offset a caller resumed from, which is a way on that is no way
    // on. The search does not stop there as though it had answered the log: the
    // session behind it, named alone, gives the line the stopped call never read.
    let out = env.found(&r, "needle", None, MATCHES_MAX, 100);
    assert_eq!(out["matches"], json!([]), "{out}");
    assert_eq!(out["next"]["id"], id_of(&a), "{out}");
    assert_eq!(out["next"]["file"], "transcript", "{out}");
    assert!(
        out["next"]["offset"].as_u64().is_some_and(|o| o > 0),
        "{out} says nothing was read"
    );
    // The log that spent the cap does not end the search as an error or as a
    // `next` that goes nowhere: named alone, the session behind it answers the
    // line the stopped call never read. The cap costs the caller its reach, never
    // its matches.
    let on = env.found(
        std::slice::from_ref(&b),
        "needle",
        None,
        MATCHES_MAX,
        SEARCH_BYTES_MAX,
    );
    let m = on["matches"].as_array().unwrap();
    assert_eq!(m.len(), 1, "{on}");
    assert_eq!(m[0]["id"], id_of(&b), "{on}");
    assert_eq!(m[0]["offset"], 0, "{on}");
    // The way on of that answer, taken as a caller takes it, is not the same
    // place again: a reading that kept no line of the log it resumed in has no
    // way on left, and the log is done. What lies past it is answered by a call
    // of the sessions behind it, as the one above shows.
    let on = env.rest_of(&r, "needle", &out["next"], 100);
    assert_eq!(on["matches"], json!([]), "{on}");
    assert!(on.get("next").is_none(), "{on}");
    // The same call again asks the same question and is answered the same way:
    // no `next` that leads a caller in a circle.
    let again = env.found(&r, "needle", None, MATCHES_MAX, 100);
    assert_eq!(again, out, "{again} vs {out}");
}

#[test]
fn a_line_is_cut_to_four_hundred_characters() {
    let _t = turn();
    let long = format!("hit {} tail", "äx".repeat(500));
    let log = format!("{long}\n");
    let s = record("work", 7, &[("transcript", &transcript(7))]);
    let id = id_of(&s);
    let env = Search::new("cut", &[s], &[(&transcript(7), log.as_bytes())]);
    let r = env.records();
    let out = env.found(&r, "tail", None, MATCHES_MAX, SEARCH_BYTES_MAX);
    let m = &out["matches"][0];
    let line = m["line"].as_str().unwrap();
    assert_eq!(line.chars().count(), 400, "{} chars", line.chars().count());
    assert!(line.starts_with("hit "), "{line}");
    // The cut is at a character: the line is text, and its offset still reads
    // back to the whole line.
    assert!(std::str::from_utf8(line.as_bytes()).is_ok());
    let at = m["offset"].as_u64().unwrap();
    let piece = env.plain(&id, json!({"file": "transcript", "offset": at}));
    assert_eq!(piece["text"], log);
}

#[test]
fn a_line_of_a_utf8_log_is_matched_whole_and_offset_right() {
    let _t = turn();
    let log = "aä€b\nhit äöü üäö\n";
    let s = record("work", 7, &[("transcript", &transcript(7))]);
    let id = id_of(&s);
    let env = Search::new("utf8search", &[s], &[(&transcript(7), log.as_bytes())]);
    let r = env.records();
    let out = env.found(&r, "äöü", None, MATCHES_MAX, SEARCH_BYTES_MAX);
    let m = &out["matches"][0];
    // A match's line is the line of the log, its newline kept, as the offsets
    // of the matches count it.
    assert_eq!(m["line"], "hit äöü üäö\n");
    // aä€b masks to itself: 7 bytes and its newline make the second line's
    // offset 8.
    assert_eq!(m["offset"], 8);
    let piece = env.plain(&id, json!({"file": "transcript", "offset": 8}));
    assert_eq!(piece["text"], "hit äöü üäö\n");
}

#[test]
fn a_search_of_no_session_finds_nothing() {
    let _t = turn();
    let empty: Vec<Value> = Vec::new();
    let env = Search::new("none", &[], &[(&transcript(7), b"a hit\n")]);
    let out = env.found(&empty, "a hit", None, MATCHES_MAX, SEARCH_BYTES_MAX);
    assert_eq!(out["matches"], json!([]), "{out}");
    assert!(out.get("next").is_none(), "{out}");
}

#[test]
fn a_limit_or_a_cap_outside_its_range_is_refused() {
    let _t = turn();
    let empty: Vec<Value> = Vec::new();
    let env = Search::new("bounds", &[], &[]);
    for (limit, bytes) in [
        (0, SEARCH_BYTES_MAX),
        (MATCHES_MAX + 1, SEARCH_BYTES_MAX),
        (1, 0),
        (1, SEARCH_BYTES_MAX + 1),
    ] {
        let why = env.ask(&empty, "hit", None, limit, bytes).unwrap_err();
        let want = if limit == 0 || limit > MATCHES_MAX {
            "limit is from 1 to 200"
        } else {
            "bytes is from 1 to"
        };
        assert!(why.starts_with(want), "{why}");
    }
}

#[test]
fn a_record_without_a_kind_or_issue_is_a_file_error() {
    let _t = turn();
    let s = json!({"id": "work-7-26102110000", "logs": []});
    let env = Search::new("nokind", &[s], &[]);
    let r = env.records();
    let e = env.error(&r, "hit", None);
    assert!(e.contains("sessions.jsonl"), "{e}");
    // An id that is not an id is refused before any file is opened.
    let bad = json!({"id": "../x", "kind": "work", "issue": 7, "logs": []});
    let r = vec![bad];
    let e = env.error(&r, "hit", None);
    assert_eq!(e, "id is like work-12-1792485967");
}

// `factory_search`: the tool over the two arguments that say what to search,
// `issue` and `ids`, and the two that say how far to read, `files` and `limit`.
// It resolves its sessions as `factory_log` does, holds the same two places,
// and answers the matches of the search beside it and, when the reading stops,
// the place it stopped at.

// The arguments of a call by issue, which `ids` of a test replace.
fn by_issue(text: &str) -> Value {
    json!({"text": text, "issue": 7})
}

#[test]
fn the_tool_searches_the_sessions_of_an_issue() {
    let _t = turn();
    let (a, b, other) = (
        record("work", 7, &[("transcript", &transcript(7))]),
        record("work", 7, &[("err", &format!("{}.err", transcript(7)))]),
        record("work", 8, &[("transcript", &transcript(8))]),
    );
    let env = Search::new(
        "tool_issue",
        &[a.clone(), b.clone(), other.clone()],
        &[
            (&transcript(7), b"the pids limit was hit here\n"),
            (
                &format!("{}.err", transcript(7)),
                b"a second hit in the error output\n",
            ),
            (&transcript(8), b"a hit on another issue\n"),
        ],
    );
    let out = env.searched(by_issue("hit"));
    assert_eq!(out["api"], 1);
    assert_eq!(out["days"], 30);
    assert_eq!(out["stale"], false);
    assert_eq!(out["sessions"], 2, "{out}");
    let m = out["matches"].as_array().unwrap();
    assert_eq!(m.len(), 2, "{out}");
    assert_eq!(m[0]["id"], id_of(&a));
    assert_eq!(m[0]["file"], "transcript");
    assert_eq!(m[0]["line"], "the pids limit was hit here\n");
    assert_eq!(m[1]["id"], id_of(&b));
    assert_eq!(m[1]["file"], "err");
    assert!(out.get("next").is_none(), "{out}");
    // Nothing of the issue the call did not name is in the answer.
    assert!(!text_of(&out).contains(&id_of(&other)), "{out}");
    // The offset of a match is one to give `factory_log` for that log.
    let at = m[0]["offset"].as_u64().unwrap();
    let piece = env.plain(&id_of(&a), json!({"file": "transcript", "offset": at}));
    assert_eq!(piece["text"], "the pids limit was hit here\n");
}

#[test]
fn the_tool_searches_the_sessions_it_is_given_by_id() {
    let _t = turn();
    // The snapshot answers newest first, so the three records are stamped a
    // thousand seconds apart from the one instant of the run.
    let (env, recs) = Search::with(
        "tool_ids",
        &[
            record_at("work", 7, start(), &[("transcript", &transcript(7))]),
            record_at(
                "plan",
                7,
                start() - 1000,
                &[("progress", &format!("plan-7-{STAMP}.jsonl"))],
            ),
            record_at("work", 9, start() - 2000, &[("transcript", &transcript(9))]),
        ],
        &[
            (
                &transcript(7),
                b"the plan of the run hit one step\nanother hit of the run\n",
            ),
            (
                &format!("plan-7-{STAMP}.jsonl"),
                b"the plan hit its second step\n",
            ),
            (&transcript(9), b"a hit of a session no id named\n"),
        ],
    );
    let (a, b, other) = (recs[0].clone(), recs[1].clone(), recs[2].clone());
    let out = env.searched(json!({
        "text": "hit",
        "ids": [id_of(&b), id_of(&a)],
    }));
    assert_eq!(out["sessions"], 2, "{out}");
    let m = out["matches"].as_array().unwrap();
    // The snapshot's own order, not the order of the ids: the work session's two
    // lines first, then the planning session's one.
    assert_eq!(m.len(), 3, "{out}");
    assert_eq!(m[0]["id"], id_of(&a));
    assert_eq!(m[1]["id"], id_of(&a));
    assert_eq!(m[2]["id"], id_of(&b));
    assert!(!text_of(&out).contains(&id_of(&other)), "{out}");
    // One session of the two, named alone, and a name given twice is one
    // reading: the matches of a session are answered once, however often its
    // id is given - the two lines of its transcript, and not two of each.
    let once = env.searched(json!({
        "text": "hit",
        "ids": [id_of(&a), id_of(&a)],
        "files": ["transcript"],
    }));
    assert_eq!(once["sessions"], 1, "{once}");
    assert_eq!(once["matches"].as_array().unwrap().len(), 2, "{once}");
    // `files` narrows the search to the words it names: of the three matches
    // above, the one read from the planning session's progress log.
    let one = env.searched(json!({
        "text": "hit",
        "ids": [id_of(&a), id_of(&b)],
        "files": ["progress"],
    }));
    let m = one["matches"].as_array().unwrap();
    assert_eq!(m.len(), 1, "{one}");
    assert_eq!(m[0]["file"], "progress");
    assert_eq!(m[0]["id"], id_of(&b), "{one}");
    // `limit` says how many matches come back - one here, of the three - and
    // the rest of the sessions follows from the `next` the answer gives.
    let few = env.searched(json!({
        "text": "hit",
        "ids": [id_of(&a), id_of(&b)],
        "limit": 1,
    }));
    assert_eq!(few["matches"].as_array().unwrap().len(), 1, "{few}");
    assert_eq!(few["next"]["id"], few["matches"][0]["id"], "{few}");
    assert_eq!(few["next"]["file"], "transcript", "{few}");
    assert_eq!(few["next"]["offset"], 33, "{few}");
    // The way on reads the log the stop named, from its offset, and holds the
    // line that follows the one the answer above holds: no line of the log is
    // in both answers.
    let on = env.on(
        json!({"text": "hit", "ids": [id_of(&a), id_of(&b)]}),
        &recs,
        &few["next"],
    );
    assert_eq!(on["matches"].as_array().unwrap().len(), 1, "{on}");
    assert_eq!(on["matches"][0]["id"], id_of(&a), "{on}");
    assert_eq!(on["matches"][0]["offset"], 33, "{on}");
    assert_eq!(on["matches"][0]["line"], "another hit of the run\n", "{on}");
    assert!(on.get("next").is_none(), "{on}");
    let offs = few["matches"]
        .as_array()
        .unwrap()
        .iter()
        .chain(on["matches"].as_array().unwrap())
        .map(|m| m["offset"].as_u64().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(offs, vec![0, 33], "{few} {on}");
}

#[test]
fn issue_and_ids_together_or_neither_are_tool_errors() {
    let _t = turn();
    // A session of the issue the calls name, and one of another, so an answer of
    // nothing for 42 is the answer of an issue with no session and not the answer
    // of a search that read nothing at all.
    let env = Search::new(
        "tool_args",
        &[
            record("work", 7, &[("transcript", &transcript(7))]),
            record("work", 8, &[("transcript", &transcript(8))]),
        ],
        &[
            (&transcript(7), b"the pids limit was hit here\n"),
            (&transcript(8), b"a hit of another issue\n"),
        ],
    );
    // The id of the session the snapshot holds, read back as its caller has it:
    // `now()` moves between two calls of `record`, so the id a test names is one
    // it read out of the snapshot and not one it wrote itself.
    let id = env.records()[0]["id"].as_str().unwrap().to_string();
    for args in [
        // Both of the two that say what to search, and neither of them.
        json!({"text": "hit", "issue": 7, "ids": [id.clone()]}),
        json!({"text": "hit"}),
        // The words of the other arguments, in the shapes that are not theirs.
        json!({"issue": 7}),
        json!({"text": "hi", "issue": 7}),
        json!({"text": "x".repeat(201), "issue": 7}),
        json!({"text": "hit", "issue": 0}),
        json!({"text": "hit", "issue": "7"}),
        json!({"text": "hit", "ids": ["../x"]}),
        json!({"text": "hit", "ids": [12]}),
        json!({"text": "hit", "ids": "work-7-1792485967"}),
        json!({"text": "hit", "ids": ["work-7-261021100000"]}),
        json!({"text": "hit", "issue": 7, "files": ["nope"]}),
        json!({"text": "hit", "issue": 7, "files": "transcript"}),
        json!({"text": "hit", "issue": 7, "limit": 0}),
        json!({"text": "hit", "issue": 7, "limit": 201}),
        json!({"text": "hit", "issue": 7, "bytes": 1024}),
    ] {
        let why = env.refused(args.clone());
        assert!(!why.is_empty(), "{args} was refused by nothing");
    }
    // The two that say what to search are refused whichever way they are put
    // together, and the answer of a good call is an answer of matches, so the
    // refusals above are not the shape of every answer.
    assert!(env
        .refused(json!({"text": "hit", "issue": 7, "ids": [id.clone()]}))
        .starts_with("give issue or ids, not both"));
    assert!(env
        .refused(json!({"text": "hit"}))
        .starts_with("give issue or ids"));
    // Bytes that are not an object at all are a refusal of the arguments, not a
    // panic: `factory_log` refuses them by indexing, which reads no member of
    // them, and a client that sends `"arguments": null` gets a tool error.
    for args in [
        json!("hit"),
        json!(42),
        json!(true),
        json!(["text"]),
        json!(null),
    ] {
        let why = env.refused_at(args.clone());
        assert!(
            why.starts_with("arguments are an object"),
            "{args} answered: {why}"
        );
    }
    // An id of no session of the window names the id it could not find, as the
    // other session tools do.
    let why = env.refused(json!({"text": "hit", "ids": ["plan-42-1792485967"]}));
    assert!(
        why.starts_with("no session plan-42-1792485967 in the last 30 days"),
        "{why}"
    );
    // Of an issue with no session in the window, an answer of nothing: there is
    // no file to read, and the tool has no reason to refuse.
    let out = env.searched(json!({"text": "hit", "issue": 42}));
    assert_eq!(out["matches"], json!([]), "{out}");
    assert_eq!(out["sessions"], 0, "{out}");
}

// The tool takes `factory_log`'s two places at a time, and holds the one it
// took until its answer is dropped. A third call meanwhile is the error the
// log reader gives.
#[test]
fn a_third_search_waits_as_a_third_log_does() {
    let _t = turn();
    let s = record("work", 7, &[("transcript", &transcript(7))]);
    let env = Search::new(
        "tool_two",
        &[s],
        &[(&transcript(7), b"the pids limit was hit here\n")],
    );
    let (a, b) = (acquire().unwrap(), acquire().unwrap());
    assert_eq!(
        env.refused(by_issue("hit")),
        "two logs are being sent; try again in a few seconds"
    );
    drop(a);
    drop(b);
    assert_eq!(
        env.searched(by_issue("hit"))["matches"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // Through the protocol, as a client asks it: the answer keeps its place
    // until it is written, so a search that holds one leaves one for the next.
    let k = "abcdefghijklmnopqrstuvwxyz012345";
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {
        "name": "factory_search",
        "arguments": {"text": "hit", "issue": 7},
        "_meta": {"io.modelcontextprotocol/protocolVersion": "2026-07-28"},
    }})
    .to_string();
    let auth = format!("Bearer {k}");
    let headers = [
        ("Authorization", auth.as_str()),
        ("MCP-Protocol-Version", "2026-07-28"),
        ("Mcp-Method", "tools/call"),
        ("Mcp-Name", "factory_search"),
    ];
    let req = Request {
        method: "POST",
        headers: headers
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect(),
        body: body.as_bytes(),
    };
    let r = handle(
        &req,
        &parse_keys([(&Setting::Full, k), (&Setting::Dashboard, "")]).unwrap(),
        &env.dirs,
        "v9.9.9",
    );
    let v: Value = serde_json::from_slice(&r.body).unwrap();
    assert_eq!(v["result"]["structuredContent"]["api"], 1);
    assert_eq!(
        v["result"]["structuredContent"]["matches"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let other = acquire().expect("one place left");
    assert!(acquire().is_none(), "the answer gave up its place");
    drop(other);
    drop(r);
    let (a, b) = (acquire(), acquire());
    assert!(a.is_some() && b.is_some(), "the place was not freed");
}

// The text a search serves is masked as the log reader masks it, so a secret
// the mask takes out is in no answer of the tool either.
#[test]
fn a_search_of_the_tool_masks_what_the_log_reader_masks() {
    let _t = turn();
    let secret = token();
    let log = format!(
        "pushed with {secret}\nPASSWORD=hunter2\nread at {}:8086\nthe pids limit was hit\n",
        ip([10, 1, 2, 3])
    );
    let s = record("work", 7, &[("transcript", &transcript(7))]);
    let env = Search::new("tool_mask", &[s], &[(&transcript(7), log.as_bytes())]);
    for want in ["hit", "pushed with"] {
        let out = env.searched(json!({"text": want, "issue": 7}));
        assert_eq!(out["matches"].as_array().unwrap().len(), 1, "{out}");
        for s in secrets() {
            assert!(!text_of(&out).contains(&s), "{s} in {out}");
        }
    }
    for want in [&secret, "hunter2", "u:pw@", &ip([10, 1, 2, 3])] {
        let out = env.searched(json!({"text": want, "issue": 7}));
        assert_eq!(out["matches"], json!([]), "{want} found in {log}");
    }
}

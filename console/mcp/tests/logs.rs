// factory_log over a snapshot and a logs directory written by the tests.

use flate2::read::GzDecoder;
use mcp::keys::parse_keys;
use mcp::logs::{acquire, base64, masked, PLAIN_MAX};
use mcp::rpc::{handle, Request};
use mcp::tools::{call, Dirs, Output, ToolError};
use serde_json::{json, Value};
use std::io::Read;
use std::path::PathBuf;
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
        let t = now() - 100;
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

#[test]
fn pieces_join() {
    let _t = turn();
    let mut log = String::new();
    let mut i = 0;
    while log.len() < 2_621_440 {
        log.push_str(&format!(
            "{{\"n\":{i},\"t\":\"{} ä {}\"}}\n",
            token(),
            ip([192, 168, 1, (i % 250) as u8])
        ));
        i += 1;
    }
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
    let r = handle(&req, &parse_keys(k).unwrap(), &env.dirs, "v9.9.9");
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

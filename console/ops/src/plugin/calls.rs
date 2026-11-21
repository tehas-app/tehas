//! The broker of plugin calls for the MCP server (docs/specs/2026-11-21-plugins-mcp.md):
//! `tehasbot plugin calls` serves `~/tehas/run/plugin-calls/calls.sock`, the
//! one socket `tehas-mcp` is given. It answers two requests and nothing else:
//!
//!   GET /tools   the tools granted `tool:<id>` and `mcp:<id>` at the pin
//!   POST /call   {"plugin", "tool", "args", "key"}: one call of a listed tool
//!
//! A call is the call of `tehas plugin call`, made by [`Calls`]; this module
//! adds the request rules and the rate per key. `key` is the key's id, the
//! first 8 hex characters of its sha256: the broker never sees a key.

use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const HEAD_MAX: usize = 8192;
/// A call's body: 2 KiB of arguments and the four fields around them.
const BODY_MAX: usize = 4096;
const READ_WAIT: Duration = Duration::from_secs(5);
/// Calls a key may make in a minute, across all plugins.
pub const PER_MINUTE: usize = 60;
/// Connections at once. Each call holds one while its container runs.
const MAX_CONNECTIONS: usize = 8;

/// A tool the broker serves: granted `tool:<id>` and `mcp:<id>` at the
/// current pin, of a plugin that is ready and not held.
#[derive(Clone, Debug, PartialEq)]
pub struct Served {
    pub plugin: String,
    pub id: String,
    pub label: String,
    pub about: String,
    pub params: Value,
}

/// Why a call gave no answer, as the call function says it.
#[derive(Debug, PartialEq)]
pub enum Failed {
    /// The arguments break the tool's `params` or are over 2 KiB.
    Args(String),
    /// Not granted at the pin, held, not ready, or busy.
    Refused(String),
    /// tehas's reason: over 16 KiB, not one object, not UTF-8, a non-zero
    /// exit, or past the timeout.
    Tool(String),
}

/// What the broker serves and how a call is made: the plugin settings and
/// states on a host, a stub in a test.
pub trait Calls: Send + Sync {
    fn served(&self) -> Vec<Served>;
    /// One call for the key `key`. Ok is the one JSON object the tool wrote.
    fn call(&self, plugin: &str, tool: &str, args: &Value, key: &str) -> Result<Value, Failed>;
}

/// The calls of each key in the last minute.
#[derive(Default)]
pub struct Rate {
    seen: HashMap<String, VecDeque<Instant>>,
}

impl Rate {
    /// Whether `key` may make a call at `now`, and counts it if so.
    pub fn admit(&mut self, key: &str, now: Instant) -> bool {
        let minute = Duration::from_secs(60);
        self.seen.retain(|_, times| {
            times
                .back()
                .is_some_and(|t| now.duration_since(*t) < minute)
        });
        let times = self.seen.entry(key.to_string()).or_default();
        while times
            .front()
            .is_some_and(|t| now.duration_since(*t) >= minute)
        {
            times.pop_front();
        }
        if times.len() >= PER_MINUTE {
            return false;
        }
        times.push_back(now);
        true
    }
}

/// One answer: a status and a JSON body, or none.
#[derive(Debug, PartialEq)]
pub struct Answer {
    pub status: u16,
    pub body: Option<Value>,
}

fn status(status: u16) -> Answer {
    Answer { status, body: None }
}

fn ok(body: Value) -> Answer {
    Answer {
        status: 200,
        body: Some(body),
    }
}

/// The answer to one request. `line` takes a line for the journal, naming
/// a plugin and a tool only as the served list names them.
pub fn answer(
    calls: &dyn Calls,
    rate: &Mutex<Rate>,
    now: Instant,
    method: &str,
    path: &str,
    body: &[u8],
    line: &mut dyn FnMut(String),
) -> Answer {
    match (method, path) {
        ("GET", "/tools") => {
            let tools: Vec<Value> = calls
                .served()
                .iter()
                .map(|t| {
                    json!({"plugin": t.plugin, "id": t.id, "label": t.label,
                           "about": t.about, "params": t.params})
                })
                .collect();
            ok(json!({ "tools": tools }))
        }
        ("POST", "/call") => call(calls, rate, now, body, line),
        _ => status(404),
    }
}

fn call(
    calls: &dyn Calls,
    rate: &Mutex<Rate>,
    now: Instant,
    body: &[u8],
    line: &mut dyn FnMut(String),
) -> Answer {
    let Ok(v) = serde_json::from_slice::<Value>(body) else {
        return status(400);
    };
    let (Some(plugin), Some(tool), Some(key)) =
        (v["plugin"].as_str(), v["tool"].as_str(), v["key"].as_str())
    else {
        return status(400);
    };
    if key.len() != 8
        || !key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return status(400);
    }
    let served = calls.served();
    let Some(t) = served.iter().find(|t| t.plugin == plugin && t.id == tool) else {
        line("refused a tool that is not served".to_string());
        return status(403);
    };
    let admitted = rate.lock().map(|mut r| r.admit(key, now)).unwrap_or(false);
    if !admitted {
        line(format!("key {key}: over {PER_MINUTE} calls a minute"));
        return status(429);
    }
    let args = v.get("args").cloned().unwrap_or(json!({}));
    match calls.call(&t.plugin, &t.id, &args, key) {
        Ok(answer) => ok(json!({ "answer": answer })),
        Err(Failed::Args(why)) | Err(Failed::Tool(why)) => ok(json!({ "reason": why })),
        Err(Failed::Refused(why)) if why == "busy" => status(429),
        Err(Failed::Refused(why)) => {
            line(format!("{}/{}: refused: {why}", t.plugin, t.id));
            status(403)
        }
    }
}

/// A place among the connections, given back when its thread ends.
struct Slot(Arc<AtomicUsize>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Serves the socket until the listener fails.
pub fn serve(listener: UnixListener, calls: Arc<dyn Calls>) {
    let rate = Arc::new(Mutex::new(Rate::default()));
    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        if active.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            active.fetch_sub(1, Ordering::SeqCst);
            write(&mut stream, &status(503));
            continue;
        }
        let slot = Slot(Arc::clone(&active));
        let (calls, rate) = (Arc::clone(&calls), Arc::clone(&rate));
        // A thread that cannot start drops its closure, and the slot with it.
        let _ = std::thread::Builder::new().spawn(move || {
            let _slot = slot;
            let a = match read(&mut stream) {
                Ok((method, path, body)) => answer(
                    calls.as_ref(),
                    &rate,
                    Instant::now(),
                    &method,
                    &path,
                    &body,
                    &mut |l| eprintln!("tehasbot plugin calls: {l}"),
                ),
                Err(code) => status(code),
            };
            write(&mut stream, &a);
        });
    }
}

/// The method, path and body of one request, or the status that refuses it.
fn read(stream: &mut UnixStream) -> Result<(String, String, Vec<u8>), u16> {
    stream
        .set_read_timeout(Some(READ_WAIT))
        .map_err(|_| 500u16)?;
    let mut got = Vec::new();
    let mut buf = [0u8; 1024];
    let end = loop {
        if let Some(at) = got.windows(4).position(|w| w == b"\r\n\r\n") {
            break at;
        }
        if got.len() > HEAD_MAX {
            return Err(431);
        }
        let n = stream.read(&mut buf).map_err(|_| 408u16)?;
        if n == 0 {
            return Err(400);
        }
        got.extend_from_slice(&buf[..n]);
    };
    request(&got, end, stream)
}

fn request(
    got: &[u8],
    end: usize,
    stream: &mut dyn Read,
) -> Result<(String, String, Vec<u8>), u16> {
    let head = std::str::from_utf8(&got[..end]).map_err(|_| 400u16)?;
    let mut lines = head.split("\r\n");
    let first: Vec<&str> = lines.next().unwrap_or("").split(' ').collect();
    let [method, path, version] = first[..] else {
        return Err(400);
    };
    if !version.starts_with("HTTP/1.") {
        return Err(400);
    }
    let mut length = 0usize;
    for l in lines {
        if let Some((k, v)) = l.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                length = v.trim().parse().map_err(|_| 400u16)?;
            }
            if k.eq_ignore_ascii_case("transfer-encoding") {
                return Err(501);
            }
        }
    }
    if length > BODY_MAX {
        return Err(413);
    }
    let mut body = got[end + 4..].to_vec();
    if body.len() > length {
        return Err(400);
    }
    let mut rest = vec![0u8; length - body.len()];
    stream.read_exact(&mut rest).map_err(|_| 408u16)?;
    body.extend_from_slice(&rest);
    Ok((method.to_string(), path.to_string(), body))
}

fn write(stream: &mut UnixStream, a: &Answer) {
    let body = a.body.as_ref().map(Value::to_string).unwrap_or_default();
    let head = format!(
        "HTTP/1.1 {} -\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        a.status,
        body.len()
    );
    let _ = stream.set_write_timeout(Some(READ_WAIT));
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Stub {
        served: Vec<Served>,
        result: fn() -> Result<Value, Failed>,
        asked: Mutex<Vec<(String, String, Value, String)>>,
    }

    impl Calls for Stub {
        fn served(&self) -> Vec<Served> {
            self.served.clone()
        }
        fn call(&self, plugin: &str, tool: &str, args: &Value, key: &str) -> Result<Value, Failed> {
            self.asked.lock().unwrap().push((
                plugin.to_string(),
                tool.to_string(),
                args.clone(),
                key.to_string(),
            ));
            (self.result)()
        }
    }

    fn stub(result: fn() -> Result<Value, Failed>) -> Stub {
        Stub {
            served: vec![Served {
                plugin: "docs".to_string(),
                id: "find-doc".to_string(),
                label: "Docs".to_string(),
                about: "Find a doc".to_string(),
                params: json!({"type": "object"}),
            }],
            result,
            asked: Mutex::new(Vec::new()),
        }
    }

    const KEY: &str = "3511bfc5";

    fn ask(
        s: &Stub,
        rate: &Mutex<Rate>,
        method: &str,
        path: &str,
        body: Value,
    ) -> (Answer, Vec<String>) {
        let mut lines = Vec::new();
        let a = answer(
            s,
            rate,
            Instant::now(),
            method,
            path,
            body.to_string().as_bytes(),
            &mut |l| lines.push(l),
        );
        (a, lines)
    }

    fn a_call(plugin: &str, tool: &str) -> Value {
        json!({"plugin": plugin, "tool": tool, "args": {"q": "x"}, "key": KEY})
    }

    #[test]
    fn the_list_is_the_served_tools() {
        let s = stub(|| Ok(json!({})));
        let (a, _) = ask(&s, &Mutex::default(), "GET", "/tools", json!(null));
        assert_eq!(a.status, 200);
        let t = &a.body.unwrap()["tools"][0];
        assert_eq!(
            (t["plugin"].as_str(), t["id"].as_str()),
            (Some("docs"), Some("find-doc"))
        );
    }

    #[test]
    fn a_call_of_a_served_tool_reaches_the_call_with_the_key_id() {
        let s = stub(|| Ok(json!({"doc": "a.md"})));
        let (a, _) = ask(
            &s,
            &Mutex::default(),
            "POST",
            "/call",
            a_call("docs", "find-doc"),
        );
        assert_eq!(a, ok(json!({"answer": {"doc": "a.md"}})));
        assert_eq!(
            s.asked.lock().unwrap()[0],
            (
                "docs".to_string(),
                "find-doc".to_string(),
                json!({"q": "x"}),
                KEY.to_string()
            )
        );
    }

    #[test]
    fn a_tool_that_is_not_served_is_refused_and_not_called() {
        let s = stub(|| Ok(json!({})));
        let rate = Mutex::default();
        for (plugin, tool) in [("docs", "other"), ("other", "find-doc")] {
            let (a, lines) = ask(&s, &rate, "POST", "/call", a_call(plugin, tool));
            assert_eq!(a.status, 403);
            // The journal names no word of the request.
            assert_eq!(lines, ["refused a tool that is not served"]);
        }
        assert!(s.asked.lock().unwrap().is_empty());
    }

    #[test]
    fn any_other_request_is_refused() {
        let s = stub(|| Ok(json!({})));
        let rate = Mutex::default();
        assert_eq!(ask(&s, &rate, "GET", "/call", json!({})).0.status, 404);
        assert_eq!(ask(&s, &rate, "POST", "/tools", json!({})).0.status, 404);
        assert_eq!(ask(&s, &rate, "GET", "/", json!({})).0.status, 404);
        let mut bad_key = a_call("docs", "find-doc");
        bad_key["key"] = json!("abcdefghijklmnopqrstuvwxyz012345");
        assert_eq!(ask(&s, &rate, "POST", "/call", bad_key).0.status, 400);
        assert_eq!(
            ask(&s, &rate, "POST", "/call", json!({"plugin": "docs"}))
                .0
                .status,
            400
        );
        assert!(s.asked.lock().unwrap().is_empty());
    }

    #[test]
    fn a_failure_says_tehass_reason_and_busy_is_429() {
        let rate = Mutex::default();
        let tool = stub(|| Err(Failed::Tool("past the timeout".to_string())));
        let (a, _) = ask(&tool, &rate, "POST", "/call", a_call("docs", "find-doc"));
        assert_eq!(a, ok(json!({"reason": "past the timeout"})));
        let args = stub(|| Err(Failed::Args("q is over 200".to_string())));
        let (a, _) = ask(&args, &rate, "POST", "/call", a_call("docs", "find-doc"));
        assert_eq!(a, ok(json!({"reason": "q is over 200"})));
        let busy = stub(|| Err(Failed::Refused("busy".to_string())));
        assert_eq!(
            ask(&busy, &rate, "POST", "/call", a_call("docs", "find-doc"))
                .0
                .status,
            429
        );
        let held = stub(|| Err(Failed::Refused("held".to_string())));
        let (a, lines) = ask(&held, &rate, "POST", "/call", a_call("docs", "find-doc"));
        assert_eq!(a.status, 403);
        assert_eq!(lines, ["docs/find-doc: refused: held"]);
    }

    #[test]
    fn a_key_has_60_calls_a_minute() {
        let mut rate = Rate::default();
        let t0 = Instant::now();
        for i in 0..PER_MINUTE {
            assert!(rate.admit(KEY, t0 + Duration::from_millis(i as u64)));
        }
        assert!(!rate.admit(KEY, t0 + Duration::from_secs(1)));
        // Another key has its own.
        assert!(rate.admit("00000000", t0 + Duration::from_secs(1)));
        // A minute after the first, one more.
        assert!(rate.admit(KEY, t0 + Duration::from_secs(60)));
        assert!(!rate.admit(KEY, t0 + Duration::from_secs(60)));
    }

    #[test]
    fn a_key_over_its_rate_is_refused_before_the_call() {
        let s = stub(|| Ok(json!({})));
        let rate = Mutex::default();
        for _ in 0..PER_MINUTE {
            assert_eq!(
                ask(&s, &rate, "POST", "/call", a_call("docs", "find-doc"))
                    .0
                    .status,
                200
            );
        }
        let (a, lines) = ask(&s, &rate, "POST", "/call", a_call("docs", "find-doc"));
        assert_eq!(a.status, 429);
        assert_eq!(lines, [format!("key {KEY}: over 60 calls a minute")]);
        assert_eq!(s.asked.lock().unwrap().len(), PER_MINUTE);
    }

    #[test]
    fn a_request_is_read_with_limits() {
        let read_of = |raw: &str| {
            let got = raw.as_bytes();
            let end = got.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
            request(got, end, &mut std::io::empty())
        };
        assert_eq!(
            read_of("POST /call HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}"),
            Ok(("POST".to_string(), "/call".to_string(), b"{}".to_vec()))
        );
        assert_eq!(
            read_of("POST /call HTTP/1.1\r\nContent-Length: 5000\r\n\r\n"),
            Err(413)
        );
        assert_eq!(
            read_of("POST /call HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n"),
            Err(501)
        );
        assert_eq!(read_of("POST /call\r\n\r\n"), Err(400));
        assert_eq!(read_of("POST /call SMTP\r\n\r\n"), Err(400));
        // A body shorter than its length is a request that never came whole.
        assert_eq!(
            read_of("POST /call HTTP/1.1\r\nContent-Length: 9\r\n\r\n{}"),
            Err(408)
        );
    }

    #[test]
    fn the_socket_answers_a_call_whole() {
        let dir = tehas_core::testdir::TestDir::new("plugin-calls-serve");
        let sock = dir.join("calls.sock");
        let listener = UnixListener::bind(&sock).unwrap();
        let calls: Arc<dyn Calls> = Arc::new(stub(|| Ok(json!({"n": 1}))));
        std::thread::spawn(move || serve(listener, calls));
        let body = a_call("docs", "find-doc").to_string();
        let mut c = UnixStream::connect(&sock).unwrap();
        write!(
            c,
            "POST /call HTTP/1.1\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let mut got = String::new();
        c.read_to_string(&mut got).unwrap();
        assert!(got.starts_with("HTTP/1.1 200 "), "{got}");
        assert!(got.ends_with(r#"{"answer":{"n":1}}"#), "{got}");
    }
}

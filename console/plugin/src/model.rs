//! The model socket of a plugin's run, `/run/tehas/model.sock` in its
//! container (docs/specs/2026-11-21-plugins-security.md, "The model socket").
//!
//! A plugin has no network and never holds the model's key. With `model`
//! granted, this serves one run on a unix socket of mode 0666 in a directory
//! the caller made for it, and stops when the caller says. It answers
//! `GET /v1/models`, which lists the one id the run's context names, and
//! `POST /v1/chat/completions`, checked field by field, held to the run's and
//! the day's budget before it is sent, and sent one at a time through curl to
//! the endpoint of [`Upstream`]. Anything else is 404.
//!
//! Counts are kept, never a body: nothing here logs.

use crate::day::Day;
use crate::upstream::Upstream;
use serde_json::{json, Map, Value};
use std::io::{self, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// The socket's name in the run's directory.
pub const SOCKET: &str = "model.sock";
/// The largest request body of text, [`Content::Text`].
pub const MAX_BODY: usize = 1 << 20;
/// The largest answer passed back.
pub const MAX_ANSWER: usize = 4 << 20;
/// The largest `max_tokens`.
pub const MAX_TOKENS: u64 = 16_384;
/// curl's bound on one request, in seconds.
pub const UPSTREAM_SECONDS: u64 = 300;
const MAX_HEAD: usize = 8192;
const IO_TIMEOUT: Duration = Duration::from_secs(10);
// Connections served at once; one more is 503. Only one is sent at a time.
const MAX_OPEN: usize = 16;

const FIELDS: [&str; 10] = [
    "model",
    "messages",
    "max_tokens",
    "temperature",
    "top_p",
    "stop",
    "tools",
    "tool_choice",
    "response_format",
    "seed",
];
const MESSAGE_FIELDS: [&str; 5] = ["role", "content", "tool_calls", "tool_call_id", "name"];
const ROLES: [&str; 4] = ["system", "user", "assistant", "tool"];

/// What one run is given.
pub struct Run {
    /// The model's id as the run's context names it, the one a request sends.
    pub id: String,
    /// The run's budget, the value of its `model:<tokens>` grant.
    pub tokens: u64,
    /// `TEHAS_PLUGIN_MODEL_DAY`.
    pub day_limit: u64,
    pub day: Arc<dyn Day>,
    pub upstream: Upstream,
    /// The curl program: `curl` on a host.
    pub curl: String,
    /// What a message may hold, by the grant.
    pub content: Content,
}

/// What a run's requests cost: the requests sent and the tokens charged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub requests: u64,
    pub tokens: u64,
}

struct Shared {
    run: Run,
    // Held while a request is sent: one at a time.
    counts: Mutex<Counts>,
    open: AtomicUsize,
    stop: AtomicBool,
}

/// A socket being served. [`Server::stop`] ends it.
pub struct Server {
    path: PathBuf,
    shared: Arc<Shared>,
    accept: Option<JoinHandle<()>>,
}

impl Server {
    /// Serves `run` on `<dir>/model.sock`. The socket must not be there yet.
    pub fn start(dir: &Path, run: Run) -> io::Result<Server> {
        let path = dir.join(SOCKET);
        let listener = UnixListener::bind(&path)?;
        // The plugin's uid is not this program's.
        if let Err(e) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)) {
            let _ = std::fs::remove_file(&path);
            return Err(e);
        }
        let shared = Arc::new(Shared {
            run,
            counts: Mutex::new(Counts::default()),
            open: AtomicUsize::new(0),
            stop: AtomicBool::new(false),
        });
        let s = Arc::clone(&shared);
        let accept = std::thread::spawn(move || accept(&listener, &s));
        Ok(Server {
            path,
            shared,
            accept: Some(accept),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The counts so far. Waits for a request being sent.
    pub fn counts(&self) -> Counts {
        *lock(&self.shared.counts)
    }

    /// Stops taking connections, removes the socket, waits for a request
    /// being sent and gives the run's counts.
    pub fn stop(mut self) -> Counts {
        self.halt();
        self.counts()
    }

    fn halt(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.accept.take() {
            // The accept loop sees the flag on its next connection.
            let _ = UnixStream::connect(&self.path);
            let _ = t.join();
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.halt();
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn accept(listener: &UnixListener, shared: &Arc<Shared>) {
    for conn in listener.incoming() {
        if shared.stop.load(Ordering::SeqCst) {
            return;
        }
        let Ok(mut stream) = conn else { continue };
        if shared.open.fetch_add(1, Ordering::SeqCst) >= MAX_OPEN {
            shared.open.fetch_sub(1, Ordering::SeqCst);
            let busy = Answer::error(503, "too many connections at once");
            write(&mut stream, &busy, Instant::now() + Duration::from_secs(1));
            continue;
        }
        let s = Arc::clone(shared);
        std::thread::spawn(move || {
            let answer = exchange(&mut stream, &s);
            write(&mut stream, &answer, Instant::now() + IO_TIMEOUT);
            drain(&mut stream, s.run.content.max_body());
            s.open.fetch_sub(1, Ordering::SeqCst);
        });
    }
}

/// An answer to the plugin: a status and JSON.
#[derive(Debug)]
struct Answer {
    status: u16,
    body: Vec<u8>,
}

impl Answer {
    fn json(status: u16, v: &Value) -> Answer {
        Answer {
            status,
            body: v.to_string().into_bytes(),
        }
    }

    // The words are this program's own, never the request's or the endpoint's.
    fn error(status: u16, message: &str) -> Answer {
        Answer::json(status, &json!({ "error": { "message": message } }))
    }
}

fn exchange(stream: &mut UnixStream, shared: &Shared) -> Answer {
    let deadline = Instant::now() + IO_TIMEOUT;
    let (head, body) = match read_head(stream, deadline) {
        Ok(v) => v,
        Err(status) => return Answer::error(status, "the request could not be read"),
    };
    let Some((method, path, headers)) = parse_head(&head) else {
        return Answer::error(400, "the request's head is not HTTP/1.1");
    };
    match (method.as_str(), path.as_str()) {
        ("GET", "/v1/models") => Answer::json(
            200,
            &json!({
                "object": "list",
                "data": [{ "id": shared.run.id, "object": "model", "owned_by": "tehas" }]
            }),
        ),
        ("POST", "/v1/chat/completions") => match read_body(
            stream,
            deadline,
            &headers,
            body,
            shared.run.content.max_body(),
        ) {
            Ok(body) => chat(&body, shared),
            Err(a) => a,
        },
        _ => Answer::error(404, "not found"),
    }
}

fn read_body(
    stream: &mut UnixStream,
    deadline: Instant,
    headers: &[(String, String)],
    mut body: Vec<u8>,
    max: usize,
) -> Result<Vec<u8>, Answer> {
    let value = |name: &str| {
        headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    };
    let length = || Answer::error(411, "a body needs a Content-Length");
    if value("Transfer-Encoding").is_some() {
        return Err(length());
    }
    let Some(len) = value("Content-Length").and_then(|v| v.parse::<u64>().ok()) else {
        return Err(length());
    };
    if len > max as u64 {
        let why = format!("the body passes {} MiB", max >> 20);
        return Err(Answer::error(413, &why));
    }
    let len = len as usize;
    let mut chunk = vec![0u8; 64 * 1024];
    while body.len() < len {
        match read_by(stream, deadline, &mut chunk) {
            Some(n) => body.extend_from_slice(&chunk[..n]),
            None => return Err(Answer::error(408, "the body did not come in time")),
        }
    }
    body.truncate(len);
    Ok(body)
}

fn chat(body: &[u8], shared: &Shared) -> Answer {
    let run = &shared.run;
    let (sent, max_tokens) = match check(body, &run.id, &run.upstream.model, run.content) {
        Ok(v) => v,
        Err(why) => return Answer::error(400, &why),
    };
    let sent = sent.to_string();
    let reserve = sent.len() as u64 / 3 + max_tokens;
    let mut counts = lock(&shared.counts);
    if shared.stop.load(Ordering::SeqCst) {
        return Answer::error(503, "the run has ended");
    }
    if counts.tokens.saturating_add(reserve) > run.tokens {
        return Answer::error(429, "the run's model budget would be passed");
    }
    if !run.day.reserve(reserve, run.day_limit) {
        return Answer::error(429, "the day's model budget would be passed");
    }
    let answer = forward(&run.upstream, &run.curl, &sent);
    let used = match &answer {
        Ok((_, body)) => usage(body).unwrap_or(reserve),
        Err(_) => reserve,
    };
    run.day.settle(reserve, used);
    counts.requests += 1;
    counts.tokens = counts.tokens.saturating_add(used);
    drop(counts);
    match answer {
        Ok((status, body)) => Answer { status, body },
        Err(Fail::Timeout) => Answer::error(504, "the model did not answer in time"),
        Err(Fail::TooBig) => Answer::error(502, "the model's answer passes 4 MiB"),
        Err(Fail::NotJson) => Answer::error(502, "the model's answer is not JSON"),
        Err(Fail::NoAnswer) => Answer::error(502, "the model could not be asked"),
    }
}

/// The request to send, the model rewritten to the endpoint's own, and its
/// `max_tokens`; or why it is refused.
fn check(body: &[u8], id: &str, model: &str, content: Content) -> Result<(Value, u64), String> {
    let Ok(Value::Object(req)) = serde_json::from_slice::<Value>(body) else {
        return Err("the body is not a JSON object".into());
    };
    let mut out = Map::new();
    for (k, v) in &req {
        match k.as_str() {
            "stream" if v == &Value::Bool(false) => {}
            "stream" => return Err("stream is not served: an answer comes whole".into()),
            "n" if v.as_u64() == Some(1) => {}
            "n" => return Err("n must be 1".into()),
            f if FIELDS.contains(&f) => {
                out.insert(k.clone(), v.clone());
            }
            _ => return Err(format!("the field {k} is not allowed")),
        }
    }
    if req.get("model").and_then(Value::as_str) != Some(id) {
        return Err(format!("model must be {id}"));
    }
    out.insert("model".into(), Value::String(model.into()));
    let max_tokens = req
        .get("max_tokens")
        .and_then(Value::as_u64)
        .filter(|n| (1..=MAX_TOKENS).contains(n))
        .ok_or(format!(
            "max_tokens is required, a whole number from 1 to {MAX_TOKENS}"
        ))?;
    match req.get("messages").and_then(Value::as_array) {
        Some(ms) if !ms.is_empty() => {
            for (i, m) in ms.iter().enumerate() {
                message(m).map_err(|why| format!("message {i}: {why}"))?;
            }
            let parts: Vec<&Value> = ms
                .iter()
                .filter_map(|m| m.get("content").and_then(Value::as_array))
                .flatten()
                .collect();
            content.parts(&parts)?;
        }
        _ => return Err("messages must be a list of at least one message".into()),
    }
    let typed = |f: &str, ok: fn(&Value) -> bool, what: &str| match req.get(f) {
        Some(v) if !ok(v) => Err(format!("{f} must be {what}")),
        _ => Ok(()),
    };
    typed("temperature", Value::is_number, "a number")?;
    typed("top_p", Value::is_number, "a number")?;
    typed("seed", Value::is_i64, "a whole number")?;
    typed("stop", stop, "a string or a list of strings")?;
    typed(
        "tools",
        |v| list_of(v, Value::is_object),
        "a list of objects",
    )?;
    typed(
        "tool_choice",
        |v| v.is_string() || v.is_object(),
        "a string or an object",
    )?;
    typed("response_format", Value::is_object, "an object")?;
    Ok((Value::Object(out), max_tokens))
}

fn list_of(v: &Value, each: fn(&Value) -> bool) -> bool {
    v.as_array().is_some_and(|a| a.iter().all(each))
}

fn calls(v: &Value) -> bool {
    v.as_array().is_some_and(|a| !a.is_empty()) && list_of(v, tool_call)
}

fn stop(v: &Value) -> bool {
    v.is_string() || list_of(v, Value::is_string)
}

fn only(obj: &Map<String, Value>, fields: &[&str]) -> Result<(), String> {
    match obj.keys().find(|k| !fields.contains(&k.as_str())) {
        Some(k) => Err(format!("the field {k} is not allowed")),
        None => Ok(()),
    }
}

// A message: its role, text content, and the fields of a tool turn.
fn message(m: &Value) -> Result<(), String> {
    let Some(m) = m.as_object() else {
        return Err("not an object".into());
    };
    only(m, &MESSAGE_FIELDS)?;
    let role = m.get("role").and_then(Value::as_str).unwrap_or("");
    if !ROLES.contains(&role) {
        return Err(format!("role must be one of {}", ROLES.join(", ")));
    }
    let calls = match m.get("tool_calls") {
        None => false,
        Some(c) if role == "assistant" && calls(c) => true,
        Some(_) => return Err("tool_calls must be an assistant's list of function calls".into()),
    };
    match m.get("tool_call_id") {
        None => {}
        Some(Value::String(_)) if role == "tool" => {}
        Some(_) => return Err("tool_call_id must be a tool turn's string".into()),
    }
    if m.get("name").is_some_and(|n| !n.is_string()) {
        return Err("name must be a string".into());
    }
    match m.get("content") {
        Some(Value::String(_)) | Some(Value::Array(_)) => Ok(()),
        Some(Value::Null) | None if calls => Ok(()),
        _ => Err("content must be text".into()),
    }
}

/// What the grant lets a request's content hold. v1 has text alone. A later
/// grant adds a kind here, with its parts and its body bound, and nothing
/// else changes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Content {
    #[default]
    Text,
}

impl Content {
    /// The largest request body.
    pub fn max_body(self) -> usize {
        match self {
            Content::Text => MAX_BODY,
        }
    }

    /// Every content part of the request, of all its messages at once, or
    /// why they are refused. No image or file part in text: the model is
    /// never asked to fetch an address.
    fn parts(self, parts: &[&Value]) -> Result<(), String> {
        match self {
            Content::Text if parts.iter().all(|p| text_part(p)) => Ok(()),
            Content::Text => Err("content parts must be of type text".into()),
        }
    }
}

fn text_part(p: &Value) -> bool {
    p.as_object().is_some_and(|p| {
        only(p, &["type", "text"]).is_ok()
            && p.get("type").and_then(Value::as_str) == Some("text")
            && p.get("text").is_some_and(Value::is_string)
    })
}

fn tool_call(c: &Value) -> bool {
    let Some(c) = c.as_object() else {
        return false;
    };
    let f = c.get("function").and_then(Value::as_object);
    only(c, &["id", "type", "function"]).is_ok()
        && c.get("id").is_some_and(Value::is_string)
        && c.get("type").and_then(Value::as_str) == Some("function")
        && f.is_some_and(|f| {
            only(f, &["name", "arguments"]).is_ok()
                && f.get("name").is_some_and(Value::is_string)
                && f.get("arguments").is_some_and(Value::is_string)
        })
}

/// The tokens an answer's `usage` gives: `total_tokens`, or the prompt's and
/// the completion's.
fn usage(body: &[u8]) -> Option<u64> {
    let v: Value = serde_json::from_slice(body).ok()?;
    let u = v.get("usage")?;
    let figure = |name: &str| u.get(name).and_then(Value::as_u64);
    figure("total_tokens")
        .or_else(|| Some(figure("prompt_tokens")?.saturating_add(figure("completion_tokens")?)))
}

/// Why no answer came back to pass on.
#[derive(Debug, PartialEq, Eq)]
enum Fail {
    NoAnswer,
    Timeout,
    TooBig,
    NotJson,
}

// curl's config: what goes on its standard input with `-K -`, so neither the
// key nor the body is on a command line.
fn config(key: &str, body: &str) -> String {
    let mut c = tehas_core::models::auth(key);
    c.push_str("header = \"Content-Type: application/json\"\n");
    // curl would wait a second for a 100 Continue on a long body.
    c.push_str("header = \"Expect:\"\n");
    // A body of JSON holds no control character. It starts with `{`, never
    // with the `@` that would make curl read a file.
    let quoted = body.replace('\\', "\\\\").replace('"', "\\\"");
    c.push_str(&format!("data-binary = \"{quoted}\"\n"));
    c
}

/// Sends `body` to the endpoint's chat completions: its status and answer.
fn forward(up: &Upstream, curl: &str, body: &str) -> Result<(u16, Vec<u8>), Fail> {
    let url = format!("{}/chat/completions", up.url);
    let seconds = UPSTREAM_SECONDS.to_string();
    let args = [
        "-sS",
        "-K",
        "-",
        "--proto",
        "=http,https",
        "-m",
        &seconds,
        "-w",
        "\n%{http_code}",
        &url,
    ];
    let Ok(mut child) = Command::new(curl)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Err(Fail::NoAnswer);
    };
    // curl reads all of its config before it sends.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(config(&up.key, body).as_bytes());
    }
    // The answer, a line end and three digits of status.
    let bound = MAX_ANSWER + 4;
    let mut out = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        let _ = stdout.take(bound as u64 + 1).read_to_end(&mut out);
    }
    if out.len() > bound {
        let _ = child.kill();
        let _ = child.wait();
        return Err(Fail::TooBig);
    }
    match child.wait().ok().and_then(|s| s.code()) {
        Some(0) => {}
        Some(28) => return Err(Fail::Timeout),
        _ => return Err(Fail::NoAnswer),
    }
    let Some(at) = out.iter().rposition(|b| *b == b'\n') else {
        return Err(Fail::NoAnswer);
    };
    let status = std::str::from_utf8(&out[at + 1..])
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .filter(|s| (200..600).contains(s))
        .ok_or(Fail::NoAnswer)?;
    out.truncate(at);
    if out.len() > MAX_ANSWER {
        return Err(Fail::TooBig);
    }
    if serde_json::from_slice::<Value>(&out).is_err() {
        return Err(Fail::NotJson);
    }
    Ok((status, out))
}

/// One read that ends by the deadline: the bytes read, or None at the end of
/// the stream, an error or the deadline.
fn read_by(stream: &mut UnixStream, deadline: Instant, buf: &mut [u8]) -> Option<usize> {
    let left = deadline.checked_duration_since(Instant::now())?;
    stream
        .set_read_timeout(Some(left.max(Duration::from_millis(1))))
        .ok()?;
    match stream.read(buf) {
        Ok(0) | Err(_) => None,
        Ok(n) => Some(n),
    }
}

/// The head up to its blank line, and whatever of the body came with it.
fn read_head(stream: &mut UnixStream, deadline: Instant) -> Result<(Vec<u8>, Vec<u8>), u16> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 4096];
    loop {
        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            if end > MAX_HEAD {
                return Err(431);
            }
            let body = buf.split_off(end + 4);
            buf.truncate(end);
            return Ok((buf, body));
        }
        if buf.len() > MAX_HEAD {
            return Err(431);
        }
        match read_by(stream, deadline, &mut chunk) {
            Some(n) => buf.extend_from_slice(&chunk[..n]),
            None => return Err(408),
        }
    }
}

type Head = (String, String, Vec<(String, String)>);

fn parse_head(head: &[u8]) -> Option<Head> {
    let text = std::str::from_utf8(head).ok()?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let (method, path, proto) = (first.next()?, first.next()?, first.next()?);
    if first.next().is_some() || !proto.starts_with("HTTP/1.") {
        return None;
    }
    let mut headers = vec![];
    for line in lines {
        let (k, v) = line.split_once(':')?;
        headers.push((k.trim().to_string(), v.trim().to_string()));
    }
    Some((method.to_string(), path.to_string(), headers))
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        408 => "Request Timeout",
        411 => "Length Required",
        413 => "Content Too Large",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Status",
    }
}

fn write(stream: &mut UnixStream, a: &Answer, deadline: Instant) {
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        a.status,
        reason(a.status),
        a.body.len()
    );
    if send(stream, head.as_bytes(), deadline) && send(stream, &a.body, deadline) {
        let _ = stream.shutdown(std::net::Shutdown::Write);
    } else {
        let _ = stream.shutdown(std::net::Shutdown::Both);
    }
}

/// Reads what the client sent and was not read, for a second at most: a
/// socket closed with unread bytes resets, and the client loses the answer.
fn drain(stream: &mut UnixStream, max_body: usize) {
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut left = MAX_HEAD + max_body;
    let mut chunk = [0u8; 4096];
    while left > 0 {
        match read_by(stream, deadline, &mut chunk) {
            Some(n) => left = left.saturating_sub(n),
            None => return,
        }
    }
}

/// Writes `bytes` by `deadline`; false when the deadline or an error came
/// first.
fn send(stream: &mut UnixStream, mut bytes: &[u8], deadline: Instant) -> bool {
    while !bytes.is_empty() {
        let Some(left) = deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
        else {
            return false;
        };
        if stream.set_write_timeout(Some(left)).is_err() {
            return false;
        }
        match stream.write(bytes) {
            Ok(0) => return false,
            Ok(n) => bytes = &bytes[n..],
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests;

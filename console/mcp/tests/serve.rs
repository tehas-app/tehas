// The binary: settings, the HTTP loop and the access log.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, ChildStderr, Command, Stdio};
use std::time::Duration;
use tehas_core::time::iso;

const BIN: &str = env!("CARGO_BIN_EXE_tehas-mcp");
const K1: &str = "abcdefghijklmnopqrstuvwxyz012345";

fn fixture(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
        .display()
        .to_string()
}

fn run_once(keys: Option<&str>, args: &[&str]) -> (i32, String) {
    let mut c = Command::new(BIN);
    c.args(args).env_remove("TEHAS_MCP_KEYS");
    if let Some(k) = keys {
        c.env("TEHAS_MCP_KEYS", k);
    }
    let out = c.output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

struct Server {
    child: Child,
    port: u16,
    log: BufReader<ChildStderr>,
    first: String,
}

impl Server {
    // Port 0: the server says which port it was given, so tests that run
    // side by side never race for one.
    fn start() -> Self {
        Self::start_with(
            &fixture("live"),
            &fixture("sessions"),
            &fixture("logs"),
            &[],
        )
    }

    fn start_with(live: &str, sessions: &str, logs: &str, extra: &[&str]) -> Self {
        let mut child = Command::new(BIN)
            .args([
                "--live",
                live,
                "--sessions",
                sessions,
                "--logs",
                logs,
                "--listen",
                "127.0.0.1:0",
                "--server-version",
                "v1.2.3",
            ])
            .args(extra)
            .env("TEHAS_MCP_KEYS", K1)
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut log = BufReader::new(child.stderr.take().unwrap());
        let mut first = String::new();
        log.read_line(&mut first).unwrap();
        let port = first
            .split(" listening on ")
            .nth(1)
            .and_then(|s| s.split(',').next())
            .and_then(|a| a.rsplit(':').next())
            .and_then(|p| p.parse().ok());
        let Some(port) = port else {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the server did not start: {first}");
        };
        Server {
            child,
            port,
            log,
            first,
        }
    }

    fn post(&self, auth: Option<&str>, body: &[u8]) -> (u16, String) {
        let mut s = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        let mut req = format!(
            "POST /mcp HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        );
        if let Some(a) = auth {
            req.push_str(&format!("Authorization: {a}\r\n"));
        }
        req.push_str("\r\n");
        s.write_all(req.as_bytes()).unwrap();
        let _ = s.write_all(body);
        let mut out = String::new();
        let _ = s.read_to_string(&mut out);
        let status = out
            .split_whitespace()
            .nth(1)
            .and_then(|c| c.parse().ok())
            .unwrap_or(0);
        (status, out)
    }

    /// Sends raw bytes, then reads the answer for at most `wait`.
    fn raw(&self, bytes: &[u8], wait: Duration) -> (u16, String) {
        let mut s = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        s.set_read_timeout(Some(wait)).unwrap();
        let _ = s.write_all(bytes);
        let mut buf = vec![0; 65536];
        let n = s.read(&mut buf).unwrap_or(0);
        let out = String::from_utf8_lossy(&buf[..n]).into_owned();
        let status = out
            .split_whitespace()
            .nth(1)
            .and_then(|c| c.parse().ok())
            .unwrap_or(0);
        (status, out)
    }

    fn alive(&mut self) -> bool {
        self.child.try_wait().unwrap().is_none()
    }

    fn stop(self) -> String {
        self.stop_after(0)
    }

    // A request's log line is written after its answer is sent, so a client
    // can see the answer before the line exists. Read the `lines` the test
    // needs before the kill, which would lose them.
    fn stop_after(mut self, lines: usize) -> String {
        let mut err = std::mem::take(&mut self.first);
        for _ in 0..lines {
            if self.log.read_line(&mut err).unwrap() == 0 {
                break;
            }
        }
        self.child.kill().unwrap();
        self.log.read_to_string(&mut err).unwrap();
        let _ = self.child.wait();
        err
    }
}

// A test that panics still stops its server, which holds the test's output.
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

const LEGACY_LIST: &[u8] = br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#;

#[test]
fn no_keys_is_78_naming_the_setting() {
    let (rc, err) = run_once(
        None,
        &[
            "--live",
            "/x",
            "--sessions",
            "/x",
            "--logs",
            "/x",
            "--listen",
            "127.0.0.1:1",
            "--server-version",
            "v",
        ],
    );
    assert_eq!(rc, 78);
    assert!(err.contains("TEHAS_MCP_KEYS"), "{err}");
}

#[test]
fn a_bad_key_is_78_and_not_printed() {
    let bad = "abcdefghijklmnopqrstuvwxyz01234+";
    let (rc, err) = run_once(
        Some(bad),
        &[
            "--live",
            "/x",
            "--sessions",
            "/x",
            "--logs",
            "/x",
            "--listen",
            "127.0.0.1:1",
            "--server-version",
            "v",
        ],
    );
    assert_eq!(rc, 78);
    assert!(!err.contains(bad), "{err}");
}

#[test]
fn a_missing_argument_is_78() {
    let (rc, err) = run_once(Some(K1), &["--live", "/x"]);
    assert_eq!(rc, 78);
    assert!(err.contains("--sessions is missing"), "{err}");
    let (rc, err) = run_once(
        Some(K1),
        &["--live", "/x", "--sessions", "/x", "--logs", "/x"],
    );
    assert_eq!(rc, 78);
    assert!(err.contains("--listen is missing"), "{err}");
}

#[test]
fn it_serves_and_logs_by_fingerprint() {
    let s = Server::start();
    let (status, out) = s.post(Some(&format!("Bearer {K1}")), LEGACY_LIST);
    assert_eq!(status, 200, "{out}");
    assert!(out.contains("\"protocolVersion\":\"2025-11-25\""), "{out}");
    assert!(out.contains("\"version\":\"v1.2.3\""), "{out}");
    assert_eq!(s.post(None, LEGACY_LIST).0, 401);
    assert_eq!(
        s.post(Some(&format!("Bearer {K1}")), &vec![b' '; 70000]).0,
        413
    );
    let log = s.stop_after(3);
    assert!(!log.contains(K1), "{log}");
    assert!(
        log.lines()
            .any(|l| l.contains("200") && l.contains("key=") && !l.contains("key=-")),
        "{log}"
    );
    assert!(
        log.lines()
            .any(|l| l.contains("401") && l.contains("key=-")),
        "{log}"
    );
}

fn keyed_list(s: &Server) -> u16 {
    s.post(Some(&format!("Bearer {K1}")), LEGACY_LIST).0
}

// The body of a request with no key is never read: it is refused at once,
// however slowly it arrives, and the requests with a key go on.
#[test]
fn slow_bodies_without_a_key_block_nothing() {
    let s = Server::start();
    let mut open = vec![];
    for _ in 0..8 {
        let mut c = TcpStream::connect(("127.0.0.1", s.port)).unwrap();
        c.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        c.write_all(b"POST /mcp HTTP/1.1\r\nHost: x\r\nContent-Length: 5000\r\n\r\nabc")
            .unwrap();
        open.push(c);
    }
    for c in &mut open {
        let mut buf = [0; 64];
        let n = c.read(&mut buf).unwrap_or(0);
        assert!(String::from_utf8_lossy(&buf[..n]).starts_with("HTTP/1.1 401"));
    }
    let t = std::time::Instant::now();
    assert_eq!(keyed_list(&s), 200);
    assert!(t.elapsed() < Duration::from_secs(1));
    s.stop();
}

#[test]
fn a_huge_length_is_refused_and_the_server_stays_up() {
    let mut s = Server::start();
    let wait = Duration::from_secs(3);
    let huge = b"POST /mcp HTTP/1.1\r\nHost: x\r\nContent-Length: 18446744073709551000\r\n\r\n{}";
    for _ in 0..8 {
        assert_eq!(s.raw(huge, wait).0, 401);
    }
    let keyed = format!(
        "POST /mcp HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {K1}\r\nContent-Length: 18446744073709551000\r\n\r\n{{}}"
    );
    for _ in 0..8 {
        assert_eq!(s.raw(keyed.as_bytes(), wait).0, 413);
    }
    assert!(s.alive());
    assert_eq!(keyed_list(&s), 200);
    s.stop();
}

#[test]
fn a_chunked_body_or_none_at_all_needs_a_length() {
    let s = Server::start();
    let wait = Duration::from_secs(3);
    let chunked = format!(
        "POST /mcp HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {K1}\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{{}}\r\n0\r\n\r\n"
    );
    assert_eq!(s.raw(chunked.as_bytes(), wait).0, 411);
    let none = format!("POST /mcp HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {K1}\r\n\r\n");
    assert_eq!(s.raw(none.as_bytes(), wait).0, 411);
    s.stop();
}

#[test]
fn only_the_one_endpoint() {
    let s = Server::start();
    let wait = Duration::from_secs(3);
    for path in ["/", "/mcpx", "/mcp/more", "/mcp?x=1"] {
        let req = format!(
            "POST {path} HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {K1}\r\nContent-Length: 2\r\n\r\n{{}}"
        );
        assert_eq!(s.raw(req.as_bytes(), wait).0, 404, "{path}");
    }
    s.stop();
}

#[test]
fn a_header_that_never_ends_is_refused() {
    let s = Server::start();
    let mut big = b"POST /mcp HTTP/1.1\r\nX: ".to_vec();
    big.extend(vec![b'a'; 20000]);
    assert_eq!(s.raw(&big, Duration::from_secs(3)).0, 431);
    s.stop();
}

// Idle connections take their places for a few seconds at most, and the server
// says it is full instead of hanging or dying.
#[test]
fn a_flood_of_idle_connections_is_answered_and_survived() {
    let mut s = Server::start();
    let idle: Vec<TcpStream> = (0..80)
        .map(|_| TcpStream::connect(("127.0.0.1", s.port)).unwrap())
        .collect();
    std::thread::sleep(Duration::from_millis(200));
    let (status, _) = s.raw(LEGACY_LIST, Duration::from_secs(2));
    assert!(status == 503 || status == 401, "{status}");
    assert!(s.alive());
    drop(idle);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(keyed_list(&s), 200);
    s.stop();
}

// A client that sends a byte now and then is cut off at the deadline for the
// whole request, not kept for as long as it keeps sending.
#[test]
fn a_request_has_a_deadline_however_it_trickles() {
    let s = Server::start();
    let mut c = TcpStream::connect(("127.0.0.1", s.port)).unwrap();
    c.write_all(b"POST /mcp HTTP/1.1\r\nX: ").unwrap();
    let started = std::time::Instant::now();
    let mut closed = false;
    while started.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_secs(2));
        if c.write_all(b"a").is_err() {
            closed = true;
            break;
        }
        c.set_read_timeout(Some(Duration::from_millis(10))).unwrap();
        let mut buf = [0; 64];
        if let Ok(n) = c.read(&mut buf) {
            closed = true;
            assert!(n == 0 || String::from_utf8_lossy(&buf[..n]).starts_with("HTTP/1.1 408"));
            break;
        }
    }
    assert!(closed, "still open after 20 s");
    assert!(started.elapsed() < Duration::from_secs(15));
    s.stop();
}

// The log takes the method as a word of letters only: what a client puts there
// cannot start a line of its own or reach a terminal as a control sequence.
#[test]
fn a_method_cannot_write_into_the_log() {
    let s = Server::start();
    let (status, _) = s.raw(
        b"POST\nFORGED\x1b[31m /mcp HTTP/1.1\r\nContent-Length: 0\r\n\r\n",
        Duration::from_secs(3),
    );
    assert_eq!(status, 400);
    let log = s.stop_after(1);
    assert!(!log.contains("FORGED"), "{log}");
    assert!(!log.contains('\x1b'), "{log}");
}

fn post_raw(
    method: &str,
    path: &str,
    auth: Option<&str>,
    headers: &[(&str, &str)],
    body: &str,
) -> Vec<u8> {
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
        body.len()
    );
    if let Some(a) = auth {
        req.push_str(&format!("Authorization: {a}\r\n"));
    }
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(body);
    req.into_bytes()
}

fn assert_refused(out: &str, what: &str) {
    let (head, body) = out.split_once("\r\n\r\n").unwrap_or((out, ""));
    let head = format!("{head}\r\n");
    assert!(head.starts_with("HTTP/1.1 401 "), "{what}: {out}");
    assert!(
        head.contains("\r\nWWW-Authenticate: Bearer\r\n"),
        "{what}: {out}"
    );
    assert!(head.contains("\r\nContent-Length: 0\r\n"), "{what}: {out}");
    assert!(body.is_empty(), "{what}: {out}");
}

// What it is, its body and its headers beyond the version.
type Unkeyed = (String, String, Vec<(&'static str, String)>);

// The sessions and logs are behind the key: no method and no tool answers
// without one, and the tools are taken from the list so a new one is covered.
#[test]
fn every_method_and_tool_needs_a_key() {
    let s = Server::start();
    let wait = Duration::from_secs(3);
    let meta = serde_json::json!({"io.modelcontextprotocol/protocolVersion": "2026-07-28"});
    let mut requests: Vec<Unkeyed> = vec![];
    for method in ["server/discover", "initialize", "ping", "tools/list"] {
        let body = serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": method,
            "params": {"_meta": meta}});
        requests.push((
            method.to_string(),
            body.to_string(),
            vec![("Mcp-Method", method.to_string())],
        ));
    }
    let tools = mcp::tools::list();
    let names: Vec<&str> = tools
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(!names.is_empty());
    for name in names {
        let body = serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": name, "arguments": {}, "_meta": meta}});
        requests.push((
            name.to_string(),
            body.to_string(),
            vec![
                ("Mcp-Method", "tools/call".to_string()),
                ("Mcp-Name", name.to_string()),
            ],
        ));
    }
    let wrong = format!("Bearer {}x", &K1[..31]);
    let basic = format!("Basic {K1}");
    for (what, body, extra) in &requests {
        let mut h: Vec<(&str, &str)> = vec![("MCP-Protocol-Version", "2026-07-28")];
        h.extend(extra.iter().map(|(k, v)| (*k, v.as_str())));
        for auth in [None, Some(wrong.as_str()), Some(basic.as_str())] {
            let (_, out) = s.raw(&post_raw("POST", "/mcp", auth, &h, body), wait);
            assert_refused(&out, &format!("{what} with {auth:?}"));
        }
    }
    let (_, out) = s.raw(&post_raw("GET", "/mcp", None, &[], ""), wait);
    assert_refused(&out, "GET /mcp");
    let (_, out) = s.raw(&post_raw("POST", "/other", None, &[], "{}"), wait);
    assert_refused(&out, "POST /other");
    s.stop();
}

// The text factory_log serves of a transcript that names a host, from a
// server given `extra` arguments.
fn served_transcript(name: &str, extra: &[&str]) -> String {
    let root = std::env::temp_dir().join(format!("mcp-serve-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (live, snap, logs) = (root.join("live"), root.join("mcp"), root.join("logs"));
    for d in [&live, &snap, &logs] {
        std::fs::create_dir_all(d).unwrap();
    }
    let file = "work-7-261021-100000.jsonl";
    std::fs::write(logs.join(file), "asked MODELS.lan:8086 for a model\n").unwrap();
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        - 100;
    let id = format!("work-7-{t}");
    let session = serde_json::json!({"id": id, "kind": "work", "issue": 7,
        "started": iso(t), "ended": iso(t + 60),
        "logs": [{"word": "transcript", "name": file, "bytes": 1}]});
    let head =
        serde_json::json!({"api": 1, "days": 30, "generatedAt": iso(t + 100), "sessions": 1});
    std::fs::write(snap.join("sessions.jsonl"), format!("{head}\n{session}\n")).unwrap();
    let path = |p: &Path| p.display().to_string();
    let s = Server::start_with(&path(&live), &path(&snap), &path(&logs), extra);
    let body = serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {
        "name": "factory_log", "arguments": {"id": id, "file": "transcript"},
        "_meta": {"io.modelcontextprotocol/protocolVersion": "2026-07-28"}}})
    .to_string();
    let auth = format!("Bearer {K1}");
    let h = [
        ("MCP-Protocol-Version", "2026-07-28"),
        ("Mcp-Method", "tools/call"),
        ("Mcp-Name", "factory_log"),
        ("Connection", "close"),
    ];
    let mut c = TcpStream::connect(("127.0.0.1", s.port)).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    c.write_all(&post_raw("POST", "/mcp", Some(&auth), &h, &body))
        .unwrap();
    let mut out = String::new();
    let _ = c.read_to_string(&mut out);
    let status = out.split_whitespace().nth(1).unwrap_or_default();
    s.stop();
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(status, "200", "{out}");
    let (_, body) = out.split_once("\r\n\r\n").unwrap();
    let v: serde_json::Value = serde_json::from_str(body).unwrap();
    v["result"]["structuredContent"]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no text: {v}"))
        .to_string()
}

#[test]
fn a_served_log_masks_the_private_names_given() {
    assert_eq!(
        served_transcript("plain", &[]),
        "asked MODELS.lan:8086 for a model\n"
    );
    assert_eq!(
        served_transcript("private", &["--private", "gb", "--private", "models.lan"]),
        "asked [private name]:8086 for a model\n"
    );
}

#[test]
fn private_needs_a_name() {
    let (rc, err) = run_once(
        Some(K1),
        &[
            "--live",
            "/x",
            "--sessions",
            "/x",
            "--logs",
            "/x",
            "--listen",
            "127.0.0.1:0",
            "--server-version",
            "v",
            "--private",
        ],
    );
    assert_eq!(rc, 78);
    assert!(err.contains("--private needs a name"), "{err}");
}

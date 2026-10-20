// The binary: settings, the HTTP loop and the access log.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

const BIN: &str = env!("CARGO_BIN_EXE_tehas-mcp");
const K1: &str = "abcdefghijklmnopqrstuvwxyz012345";

fn live() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/live")
        .display()
        .to_string()
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
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
}

impl Server {
    fn start() -> Self {
        let port = free_port();
        let listen = format!("127.0.0.1:{port}");
        let mut child = Command::new(BIN)
            .args([
                "--live",
                &live(),
                "--listen",
                &listen,
                "--server-version",
                "v1.2.3",
            ])
            .env("TEHAS_MCP_KEYS", K1)
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        for _ in 0..100 {
            if TcpStream::connect(&listen).is_ok() {
                return Server { child, port };
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("the server did not start");
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

    fn stop(mut self) -> String {
        self.child.kill().unwrap();
        let mut err = String::new();
        self.child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut err)
            .unwrap();
        let _ = self.child.wait();
        err
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
    assert!(err.contains("--listen"), "{err}");
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
    let log = s.stop();
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

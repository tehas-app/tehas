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

// Plugin tools through the whole request: listed and called for a full key,
// never for a dashboard key, and named in the access log by the server.

use mcp::keys::{parse_keys, Keys, Setting};
use mcp::rpc::{answer_in, gate, Request, Response};
use mcp::tools::Dirs;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use tehas_core::testdir::TestDir;

const FULL: &str = "abcdefghijklmnopqrstuvwxyz012345";
const DASH: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ012345";

fn keys() -> Keys {
    parse_keys([(&Setting::Full, FULL), (&Setting::Dashboard, DASH)]).unwrap()
}

fn dirs() -> Dirs {
    let f = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    Dirs {
        live: f.join("live"),
        sessions: f.join("sessions"),
        logs: f.join("logs"),
    }
}

const TOOLS: &str = r#"{"tools": [{"plugin": "docs", "id": "find-doc", "label": "Docs",
  "about": "Find the doc that answers a question",
  "params": {"type": "object", "properties": {"q": {"type": "string", "maxLength": 200}}}}]}"#;

/// A broker that answers every `GET /tools` with TOOLS and every
/// `POST /call` with <call>, and sends each call's body.
fn broker(name: &str, call: &'static str) -> (TestDir, PathBuf, mpsc::Receiver<Value>) {
    let dir = TestDir::new(&format!("mcp-plugins-{name}"));
    let sock = dir.join("calls.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for c in listener.incoming() {
            let Ok(mut c) = c else { return };
            let mut buf = vec![0u8; 16384];
            let n = c.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let answer = if req.starts_with("GET /tools ") {
                TOOLS
            } else {
                let body = req.split("\r\n\r\n").nth(1).unwrap_or("");
                let _ = tx.send(serde_json::from_str(body).unwrap_or(Value::Null));
                call
            };
            let _ = write!(c, "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{answer}");
        }
    });
    (dir, sock, rx)
}

fn ask(key: &str, method: &str, params: Value, socket: Option<&Path>) -> Response {
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
    let body = body.to_string();
    let auth = format!("Bearer {key}");
    let req = Request {
        method: "POST",
        headers: vec![
            ("Authorization".to_string(), auth),
            ("MCP-Protocol-Version".to_string(), "2025-11-25".to_string()),
        ],
        body: body.as_bytes(),
    };
    let keys = keys();
    let key = gate(&req, &keys).ok().unwrap();
    answer_in(&req, &key, &dirs(), "v9", socket)
}

fn names(r: &Response) -> Vec<String> {
    let v: Value = serde_json::from_slice(&r.body).unwrap();
    v["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_full_key_is_listed_the_plugin_tool_after_the_servers_own() {
    let (_d, sock, _) = broker("list", "{}");
    let without = names(&ask(FULL, "tools/list", json!({}), None));
    let with = names(&ask(FULL, "tools/list", json!({}), Some(&sock)));
    assert_eq!(with.len(), without.len() + 1);
    assert_eq!(with[..without.len()], without[..]);
    assert_eq!(with.last().unwrap(), "plugin_docs_find_doc");
}

#[test]
fn a_dashboard_key_is_never_listed_or_served_a_plugin_tool() {
    let (_d, sock, calls) = broker("dashboard", r#"{"answer": {"doc": "a.md"}}"#);
    let listed = names(&ask(DASH, "tools/list", json!({}), Some(&sock)));
    assert!(!listed.iter().any(|n| n.starts_with("plugin_")));
    let r = ask(
        DASH,
        "tools/call",
        json!({"name": "plugin_docs_find_doc", "arguments": {"q": "x"}}),
        Some(&sock),
    );
    let v: Value = serde_json::from_slice(&r.body).unwrap();
    assert_eq!(v["error"]["message"], "Unknown tool");
    assert_eq!(r.tool.as_deref(), Some("?"));
    assert!(calls.try_recv().is_err());
}

#[test]
fn a_full_keys_call_reaches_the_broker_and_comes_back_marked() {
    let (_d, sock, calls) = broker("call", r#"{"answer": {"doc": "a.md"}}"#);
    let r = ask(
        FULL,
        "tools/call",
        json!({"name": "plugin_docs_find_doc", "arguments": {"q": "x"}}),
        Some(&sock),
    );
    let v: Value = serde_json::from_slice(&r.body).unwrap();
    assert_eq!(
        v["result"]["structuredContent"],
        json!({"untrusted": true, "source": "plugin:docs", "answer": {"doc": "a.md"}})
    );
    assert_eq!(r.tool.as_deref(), Some("plugin_docs_find_doc"));
    let sent = calls.recv().unwrap();
    assert_eq!(sent["plugin"], "docs");
    assert_eq!(sent["tool"], "find-doc");
    assert_eq!(sent["args"], json!({"q": "x"}));
    // The key's fingerprint, never the key.
    let key = sent["key"].as_str().unwrap();
    assert_eq!(key.len(), 8);
    assert!(!FULL.contains(key));
}

#[test]
fn a_failed_tool_is_a_tool_error_with_the_brokers_reason() {
    let (_d, sock, _) = broker("failed", r#"{"reason": "past the timeout"}"#);
    let r = ask(
        FULL,
        "tools/call",
        json!({"name": "plugin_docs_find_doc", "arguments": {}}),
        Some(&sock),
    );
    let v: Value = serde_json::from_slice(&r.body).unwrap();
    assert_eq!(v["result"]["isError"], true);
    assert_eq!(v["result"]["content"][0]["text"], "past the timeout");
}

#[test]
fn a_plugin_name_without_a_broker_is_a_tool_the_server_has_not() {
    let r = ask(
        FULL,
        "tools/call",
        json!({"name": "plugin_docs_find_doc", "arguments": {}}),
        None,
    );
    let v: Value = serde_json::from_slice(&r.body).unwrap();
    assert_eq!(v["error"]["message"], "Unknown tool");
    assert_eq!(r.tool.as_deref(), Some("?"));
}

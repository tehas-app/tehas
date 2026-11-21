//! Plugin tools on the server (docs/specs/2026-11-21-plugins-mcp.md): the
//! client of the broker's socket, `tehasbot plugin calls`.
//!
//! The broker runs on the host and starts each call's container; this server
//! runs none. It answers two requests: `GET /tools`, the tools granted
//! `mcp:<id>` at the current pin, and `POST /call`. Only a full key reaches
//! either: `for_scope` is the one place that decides it.

use crate::tools::Scope;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

/// The start of every plugin tool's name. The server's own are `factory_*`.
pub const PREFIX: &str = "plugin_";
/// A call's arguments, as JSON, at most.
pub const ARGS_MAX: usize = 2048;
/// The tools of one list, at most: 4 plugins of 8 tools.
const TOOLS_MAX: usize = 32;
/// What the broker may answer, at most: a list, or a call's 16 KiB answer.
const ANSWER_MAX: usize = 64 * 1024;
const LIST_WAIT: Duration = Duration::from_secs(2);
/// A call's tool has at most 10 seconds; the rest is the container's start.
const CALL_WAIT: Duration = Duration::from_secs(25);
const REASON_MAX: usize = 200;

static BROKER: OnceLock<PathBuf> = OnceLock::new();

/// Serves the plugin tools of the broker at `socket`. `tehas-mcp
/// --plugin-calls` calls it once, at start.
pub fn serve_from(socket: PathBuf) {
    let _ = BROKER.set(socket);
}

/// The broker's socket, when the server was given one.
pub fn broker() -> Option<&'static Path> {
    BROKER.get().map(PathBuf::as_path)
}

/// One plugin tool as the broker lists it. `name` is made here from the
/// plugin and the id, never read, so a name in the access log is the
/// server's own.
#[derive(Clone, Debug, PartialEq)]
pub struct Tool {
    pub name: String,
    pub plugin: String,
    pub id: String,
    label: String,
    about: String,
    params: Value,
}

impl Tool {
    /// The tool's entry in `tools/list`.
    pub fn entry(&self) -> Value {
        json!({
            "name": self.name,
            "description": format!("Plugin {}, untrusted: {}", self.label, self.about),
            "inputSchema": self.params,
        })
    }
}

/// The plugin tools a key of this scope is served: none for a dashboard key,
/// and none without a broker.
pub fn for_scope(scope: Scope, socket: Option<&Path>) -> Vec<Tool> {
    match (scope, socket) {
        (Scope::Full, Some(s)) => list(s),
        _ => Vec::new(),
    }
}

/// The broker's list. A broker that is not there, or that answers what is not
/// a list, serves nothing, and the journal says why.
pub fn list(socket: &Path) -> Vec<Tool> {
    match exchange(socket, "GET", "/tools", None, LIST_WAIT) {
        Ok((200, body)) => read_tools(&body),
        Ok((status, _)) => {
            eprintln!("tehas-mcp: plugin tools: the broker answered {status}");
            Vec::new()
        }
        Err(why) => {
            eprintln!("tehas-mcp: plugin tools: {why}");
            Vec::new()
        }
    }
}

/// The tools of a `GET /tools` answer. An entry that breaks a rule of the
/// manifest is left out, and the rest are served.
pub fn read_tools(body: &[u8]) -> Vec<Tool> {
    let Ok(v) = serde_json::from_slice::<Value>(body) else {
        return Vec::new();
    };
    let Some(rows) = v["tools"].as_array() else {
        return Vec::new();
    };
    let mut out: Vec<Tool> = Vec::new();
    for row in rows {
        let Some(t) = read_tool(row) else { continue };
        if out.len() == TOOLS_MAX {
            break;
        }
        if !out.iter().any(|o| o.name == t.name) {
            out.push(t);
        }
    }
    out
}

fn read_tool(row: &Value) -> Option<Tool> {
    let text = |k: &str| row[k].as_str();
    let plugin = text("plugin")?;
    let id = text("id")?;
    let label = text("label")?;
    let about = text("about")?;
    let params = &row["params"];
    let name_ok = (1..=12).contains(&plugin.len())
        && plugin.starts_with(|c: char| c.is_ascii_lowercase())
        && plugin
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
    let id_ok =
        (1..=16).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_lowercase() || b == b'-');
    if !name_ok || !id_ok || !one_line(label, 24) || !one_line(about, 120) {
        return None;
    }
    if params["type"] != "object" {
        return None;
    }
    let name = format!("{PREFIX}{plugin}_{}", id.replace('-', "_"));
    if name.len() > 64 {
        return None;
    }
    Some(Tool {
        name,
        plugin: plugin.to_string(),
        id: id.to_string(),
        label: label.to_string(),
        about: about.to_string(),
        params: params.clone(),
    })
}

fn one_line(s: &str, max: usize) -> bool {
    !s.is_empty() && s.chars().count() <= max && !s.chars().any(char::is_control)
}

/// Runs one call for the key `key_id`, the key's fingerprint. Ok is the
/// result to give the client, marked untrusted; Err is the reason a tool
/// error says, tehas's words and none of the tool's output.
pub fn call(socket: &Path, tool: &Tool, args: &Value, key_id: &str) -> Result<Value, String> {
    if !args.is_object() || args.to_string().len() > ARGS_MAX {
        return Err(format!(
            "the arguments are not an object of at most {ARGS_MAX} bytes"
        ));
    }
    let body = json!({"plugin": tool.plugin, "tool": tool.id, "args": args, "key": key_id});
    match exchange(socket, "POST", "/call", Some(&body), CALL_WAIT) {
        Ok((200, body)) => answered(&tool.plugin, &body),
        Ok((429, _)) => Err("busy".to_string()),
        Ok((403, _)) => Err("the plugin refused the call".to_string()),
        Ok((status, _)) => Err(format!("the plugin's broker answered {status}")),
        Err(why) => {
            eprintln!("tehas-mcp: plugin call: {why}");
            Err("the plugin's broker did not answer".to_string())
        }
    }
}

/// A `POST /call` answer of 200: `{"answer": {...}}` or `{"reason": "..."}`.
pub fn answered(plugin: &str, body: &[u8]) -> Result<Value, String> {
    let v: Value = serde_json::from_slice(body)
        .map_err(|_| "the plugin's broker answered no JSON".to_string())?;
    if let Some(answer) = v.get("answer").filter(|a| a.is_object()) {
        return Ok(
            json!({"untrusted": true, "source": format!("plugin:{plugin}"), "answer": answer}),
        );
    }
    match v["reason"].as_str() {
        Some(r) => Err(reason(r)),
        None => Err("the plugin's broker answered no answer".to_string()),
    }
}

/// A reason of the broker's, as one short line of printable text.
fn reason(r: &str) -> String {
    let line: String = r
        .chars()
        .filter(|c| !c.is_control())
        .take(REASON_MAX)
        .collect();
    if line.is_empty() {
        "the tool failed".to_string()
    } else {
        line
    }
}

/// One HTTP/1.1 request to the broker and its status and body. The broker is
/// tehas's own and closes each connection, so the answer is read to its end.
fn exchange(
    socket: &Path,
    method: &str,
    path: &str,
    body: Option<&Value>,
    wait: Duration,
) -> Result<(u16, Vec<u8>), String> {
    let mut s = UnixStream::connect(socket).map_err(|e| format!("{}: {e}", socket.display()))?;
    s.set_read_timeout(Some(wait)).map_err(|e| e.to_string())?;
    s.set_write_timeout(Some(wait)).map_err(|e| e.to_string())?;
    let body = body.map(Value::to_string).unwrap_or_default();
    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: broker\r\nConnection: close\r\n");
    if method == "POST" {
        req.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        ));
    }
    req.push_str("\r\n");
    req.push_str(&body);
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut got = Vec::new();
    s.take((ANSWER_MAX + 4096) as u64)
        .read_to_end(&mut got)
        .map_err(|e| e.to_string())?;
    parse(&got)
}

/// The status and body of an answer read whole.
fn parse(got: &[u8]) -> Result<(u16, Vec<u8>), String> {
    let end = got
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("the broker's answer has no head")?;
    let head = std::str::from_utf8(&got[..end]).map_err(|_| "the broker's head is not text")?;
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .filter(|_| head.starts_with("HTTP/1."))
        .ok_or("the broker's answer has no status")?;
    let body = &got[end + 4..];
    if body.len() > ANSWER_MAX {
        return Err(format!("the broker's answer is over {ANSWER_MAX} bytes"));
    }
    Ok((status, body.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use std::sync::mpsc;

    fn row(plugin: &str, id: &str) -> Value {
        json!({"plugin": plugin, "id": id, "label": "Docs", "about": "Find a doc",
               "params": {"type": "object", "properties": {"q": {"type": "string", "maxLength": 200}}}})
    }

    fn tools_of(rows: Vec<Value>) -> Vec<Tool> {
        read_tools(json!({ "tools": rows }).to_string().as_bytes())
    }

    #[test]
    fn a_tool_is_named_by_its_plugin_and_id() {
        let t = tools_of(vec![row("docs", "find-doc")]);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].name, "plugin_docs_find_doc");
        let e = t[0].entry();
        assert_eq!(e["description"], "Plugin Docs, untrusted: Find a doc");
        assert_eq!(e["inputSchema"]["type"], "object");
    }

    #[test]
    fn an_entry_that_breaks_a_rule_is_left_out() {
        let mut long = row("docs", "a");
        long["about"] = json!("x".repeat(121));
        let mut no_object = row("docs", "b");
        no_object["params"] = json!({"type": "string"});
        let mut control = row("docs", "c");
        control["label"] = json!("Do\ncs");
        let bad = vec![
            row("Docs", "find"),
            row("1docs", "find"),
            row("docs-x", "find"),
            row("thirteenchars", "find"),
            row("docs", "find_doc"),
            row("docs", "seventeen-chars-x"),
            long,
            no_object,
            control,
            json!({"plugin": "docs"}),
        ];
        assert_eq!(tools_of(bad), vec![]);
        assert_eq!(read_tools(b"not json"), vec![]);
        assert_eq!(read_tools(b"{\"tools\": 1}"), vec![]);
    }

    #[test]
    fn a_list_holds_no_name_twice_and_at_most_32() {
        let twice = tools_of(vec![row("docs", "find"), row("docs", "find")]);
        assert_eq!(twice.len(), 1);
        let many: Vec<Value> = (0..40).map(|i| row(&format!("p{i}"), "find")).collect();
        assert_eq!(tools_of(many).len(), TOOLS_MAX);
    }

    #[test]
    fn a_dashboard_key_and_a_server_without_a_broker_get_no_tool() {
        let (dir, _) = broker_answering(vec![ok_list()]);
        let sock = dir.join("calls.sock");
        assert_eq!(for_scope(Scope::Dashboard, Some(&sock)), vec![]);
        assert_eq!(for_scope(Scope::Full, None), vec![]);
        assert_eq!(for_scope(Scope::Full, Some(&sock)).len(), 1);
    }

    #[test]
    fn an_answer_is_marked_untrusted_with_its_source() {
        let got = answered("docs", br#"{"answer": {"doc": "a.md"}}"#).unwrap();
        assert_eq!(
            got,
            json!({"untrusted": true, "source": "plugin:docs", "answer": {"doc": "a.md"}})
        );
        assert_eq!(
            answered("docs", br#"{"reason": "past the timeout\n"}"#),
            Err("past the timeout".to_string())
        );
        assert!(answered("docs", br#"{"answer": "text"}"#).is_err());
        assert!(answered("docs", b"<html>").is_err());
    }

    #[test]
    fn arguments_over_2_kib_or_not_an_object_are_not_sent() {
        let (dir, asked) = broker_answering(vec![]);
        let t = &tools_of(vec![row("docs", "find")])[0];
        let sock = dir.join("calls.sock");
        let big = json!({"q": "x".repeat(ARGS_MAX)});
        assert!(call(&sock, t, &big, "3511bfc5")
            .unwrap_err()
            .contains("2048"));
        assert!(call(&sock, t, &json!([1]), "3511bfc5").is_err());
        assert!(asked.try_recv().is_err());
    }

    #[test]
    fn a_call_sends_the_plugin_the_tool_the_args_and_the_key_id() {
        let (dir, asked) = broker_answering(vec![
            "HTTP/1.1 200 OK\r\n\r\n{\"answer\": {\"n\": 1}}".to_string(),
            "HTTP/1.1 429 Too Many\r\n\r\n".to_string(),
            "HTTP/1.1 403 Forbidden\r\n\r\n".to_string(),
        ]);
        let t = &tools_of(vec![row("docs", "find-doc")])[0];
        let sock = dir.join("calls.sock");
        let got = call(&sock, t, &json!({"q": "x"}), "3511bfc5").unwrap();
        assert_eq!(got["answer"], json!({"n": 1}));
        let req = asked.recv().unwrap();
        assert!(req.starts_with("POST /call HTTP/1.1\r\n"));
        let body: Value = serde_json::from_str(req.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(
            body,
            json!({"plugin": "docs", "tool": "find-doc", "args": {"q": "x"}, "key": "3511bfc5"})
        );
        assert_eq!(call(&sock, t, &json!({}), "k"), Err("busy".to_string()));
        assert_eq!(
            call(&sock, t, &json!({}), "k"),
            Err("the plugin refused the call".to_string())
        );
    }

    #[test]
    fn a_broker_that_is_not_there_serves_nothing() {
        let dir = tehas_core::testdir::TestDir::new("mcp-plugins-none");
        let sock = dir.join("calls.sock");
        assert_eq!(list(&sock), vec![]);
        let t = &tools_of(vec![row("docs", "find")])[0];
        assert_eq!(
            call(&sock, t, &json!({}), "k"),
            Err("the plugin's broker did not answer".to_string())
        );
    }

    #[test]
    fn an_answer_with_no_head_or_status_is_refused() {
        assert!(parse(b"garbage").is_err());
        assert!(parse(b"SMTP 200\r\n\r\n").is_err());
        assert_eq!(
            parse(b"HTTP/1.1 200 OK\r\nX: y\r\n\r\nab"),
            Ok((200, b"ab".to_vec()))
        );
    }

    fn ok_list() -> String {
        format!(
            "HTTP/1.1 200 OK\r\n\r\n{}",
            json!({"tools": [row("docs", "find")]})
        )
    }

    /// A broker on a socket in a new directory that answers each connection
    /// with the next of <answers> and sends what it was asked.
    fn broker_answering(
        answers: Vec<String>,
    ) -> (tehas_core::testdir::TestDir, mpsc::Receiver<String>) {
        let dir = tehas_core::testdir::TestDir::new("mcp-plugins");
        let listener = UnixListener::bind(dir.join("calls.sock")).unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for answer in answers {
                let Ok((mut c, _)) = listener.accept() else {
                    return;
                };
                let mut buf = [0u8; 8192];
                let n = c.read(&mut buf).unwrap_or(0);
                let _ = tx.send(String::from_utf8_lossy(&buf[..n]).to_string());
                let _ = c.write_all(answer.as_bytes());
            }
        });
        (dir, rx)
    }
}

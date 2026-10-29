// The protocol: MCP 2026-07-28 over Streamable HTTP, and the 2025-11-25
// handshake for older clients.

use mcp::keys::parse_keys;
use mcp::rpc::{handle, Request, Response};
use mcp::tools::Dirs;
use serde_json::{json, Value};
use std::path::Path;

const K1: &str = "abcdefghijklmnopqrstuvwxyz012345";
const MODERN: &str = "2026-07-28";

fn dirs() -> Dirs {
    let f = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    Dirs {
        live: f.join("live"),
        sessions: f.join("sessions"),
        logs: f.join("logs"),
    }
}

fn send(method: &str, headers: &[(&str, &str)], body: &[u8]) -> Response {
    let keys = parse_keys(K1).unwrap();
    let req = Request {
        method,
        headers: headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        body,
    };
    handle(&req, &keys, &dirs(), "v9.9.9")
}

fn json_of(r: &Response) -> Value {
    serde_json::from_slice(&r.body).unwrap()
}

fn header<'a>(r: &'a Response, name: &str) -> Option<&'a str> {
    r.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

// A modern request with the headers that mirror its body.
fn modern(method: &str, mut params: Value) -> Response {
    params["_meta"] = json!({"io.modelcontextprotocol/protocolVersion": MODERN});
    let name = params["name"].as_str().map(str::to_string);
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
    let auth = format!("Bearer {K1}");
    let mut h = vec![
        ("Authorization", auth.as_str()),
        ("MCP-Protocol-Version", MODERN),
        ("Mcp-Method", method),
    ];
    if let Some(n) = name.as_deref() {
        h.push(("Mcp-Name", n));
    }
    send("POST", &h, body.to_string().as_bytes())
}

// A legacy request: no _meta version.
fn legacy(id: Option<i64>, method: &str, params: Value) -> Response {
    let mut body = json!({"jsonrpc": "2.0", "method": method, "params": params});
    if let Some(id) = id {
        body["id"] = json!(id);
    }
    let auth = format!("Bearer {K1}");
    let h = [
        ("Authorization", auth.as_str()),
        ("MCP-Protocol-Version", "2025-11-25"),
    ];
    send("POST", &h, body.to_string().as_bytes())
}

#[test]
fn no_key_or_a_wrong_key_is_401_with_an_empty_body() {
    let body = br#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
    let wrong = format!("Bearer {}x", &K1[..31]);
    for h in [vec![], vec![("Authorization", wrong.as_str())]] {
        let r = send("POST", &h, body);
        assert_eq!(r.status, 401);
        assert!(r.body.is_empty());
        assert_eq!(header(&r, "WWW-Authenticate"), Some("Bearer"));
        assert_eq!(r.key, None);
        assert!(!r.headers.iter().any(|(_, v)| v.contains(K1)));
    }
}

#[test]
fn only_post() {
    let auth = format!("Bearer {K1}");
    for m in ["GET", "DELETE"] {
        assert_eq!(send(m, &[("Authorization", &auth)], b"").status, 405);
    }
}

#[test]
fn a_body_over_64_kb_is_413() {
    let auth = format!("Bearer {K1}");
    let big = vec![b' '; 65537];
    assert_eq!(send("POST", &[("Authorization", &auth)], &big).status, 413);
}

#[test]
fn a_browser_origin_is_403() {
    let auth = format!("Bearer {K1}");
    let r = send(
        "POST",
        &[("Authorization", &auth), ("Origin", "https://evil.example")],
        b"{}",
    );
    assert_eq!(r.status, 403);
}

#[test]
fn not_json_rpc_is_invalid_request() {
    let auth = format!("Bearer {K1}");
    for body in [&b"nope"[..], br#"{"id":1,"method":"tools/list"}"#, b"[]"] {
        let r = send("POST", &[("Authorization", &auth)], body);
        assert_eq!(r.status, 400);
        let code = json_of(&r)["error"]["code"].as_i64().unwrap();
        assert!(code == -32600 || code == -32700, "{code}");
    }
}

#[test]
fn discover_names_both_versions_and_the_server() {
    let r = modern("server/discover", json!({}));
    assert_eq!(r.status, 200);
    let res = &json_of(&r)["result"];
    assert_eq!(res["resultType"], "complete");
    assert_eq!(
        res["supportedVersions"],
        json!(["2026-07-28", "2025-11-25"])
    );
    assert_eq!(res["capabilities"], json!({"tools": {}}));
    assert_eq!(
        res["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "tehas"
    );
    assert_eq!(
        res["_meta"]["io.modelcontextprotocol/serverInfo"]["version"],
        "v9.9.9"
    );
    assert!(res["ttlMs"].is_number());
    assert_eq!(res["cacheScope"], "private");
    assert_eq!(header(&r, "Content-Type"), Some("application/json"));
}

#[test]
fn tools_list_is_cacheable_and_in_order() {
    let res = json_of(&modern("tools/list", json!({})))["result"].clone();
    assert_eq!(res["resultType"], "complete");
    assert_eq!(res["ttlMs"], 30000);
    assert_eq!(res["cacheScope"], "private");
    assert_eq!(res["tools"][0]["name"], "factory_status");
    assert_eq!(res["tools"].as_array().unwrap().len(), 12);
}

#[test]
fn a_tool_call_gives_structured_and_text_content() {
    let r = modern(
        "tools/call",
        json!({"name": "factory_status", "arguments": {}}),
    );
    assert_eq!(r.status, 200);
    let res = &json_of(&r)["result"];
    assert_eq!(res["resultType"], "complete");
    assert_eq!(res["structuredContent"]["api"], 1);
    let text: Value = serde_json::from_str(res["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text, res["structuredContent"]);
    assert!(res.get("isError").is_none() || res["isError"] == false);
    assert_eq!(r.key.as_deref().map(str::len), Some(8));
}

#[test]
fn a_tool_error_is_a_result_that_names_the_problem() {
    let r = modern(
        "tools/call",
        json!({"name": "factory_issues", "arguments": {"stage": "nope"}}),
    );
    assert_eq!(r.status, 200);
    let res = &json_of(&r)["result"];
    assert_eq!(res["isError"], true);
    assert!(res["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("stage"));
}

// A `tools/call` whose arguments are not an object of arguments at all: the
// server answers a tool error, as it does for a bad argument of a tool, and
// does not unwind the request. `factory_log` indexes its arguments, which reads
// nothing of a string or a number, and `factory_search` checks their shape.
#[test]
fn arguments_that_are_not_an_object_are_a_tool_error_not_a_crash() {
    for arguments in [json!("hit"), json!(42), json!(true), json!(""), json!(null)] {
        for name in ["factory_search", "factory_log"] {
            let r = modern("tools/call", json!({"name": name, "arguments": arguments}));
            assert_eq!(r.status, 200, "{name} {arguments}");
            let res = &json_of(&r)["result"];
            assert_eq!(res["isError"], true, "{name} {arguments}: {res}");
            assert!(!res["content"][0]["text"].as_str().unwrap().is_empty());
        }
    }
    // The legacy handshake, whose body is the only one whose name reaches the
    // log, answers the same way.
    let r = legacy(
        Some(1),
        "tools/call",
        json!({"name": "factory_search", "arguments": "hit"}),
    );
    assert_eq!(r.status, 200);
    assert_eq!(json_of(&r)["result"]["isError"], true);
    assert_eq!(r.tool.as_deref(), Some("factory_search"));
}

#[test]
fn an_unknown_tool_is_invalid_params() {
    let r = modern(
        "tools/call",
        json!({"name": "factory_delete", "arguments": {}}),
    );
    assert_eq!(json_of(&r)["error"]["code"], -32602);
}

#[test]
fn an_unknown_method_is_404_method_not_found() {
    let r = modern("resources/list", json!({}));
    assert_eq!(r.status, 404);
    assert_eq!(json_of(&r)["error"]["code"], -32601);
}

// What the access line adds for a tools/call: the name of the tool, and
// nothing of the arguments. A name the server has not is `?`; a name that is
// not text at all is `-`.
#[test]
fn a_call_names_its_tool_and_no_argument_in_the_line() {
    let secret = "issue-987-please-do-not-log-this-text";
    let r = modern(
        "tools/call",
        json!({"name": "factory_issues", "arguments": {"stage": "queued", "note": secret}}),
    );
    assert_eq!(r.status, 200);
    assert_eq!(r.tool.as_deref(), Some("factory_issues"));
    let line = r.tool.as_deref().unwrap();
    assert!(!line.contains(secret), "{line}");
    assert!(!line.contains("queued"), "{line}");

    // A name the server does not have is logged as ?, and the answer gives
    // the caller nothing back to enumerate with: no name of its own.
    let r = modern(
        "tools/call",
        json!({"name": "factory_delete", "arguments": {"issue": 987}}),
    );
    assert_eq!(json_of(&r)["error"]["code"], -32602);
    assert_eq!(json_of(&r)["error"]["message"], "Unknown tool");
    assert!(!String::from_utf8_lossy(&r.body).contains("factory_delete"));
    assert_eq!(r.tool.as_deref(), Some("?"));
}

// The value the line carries for a call is one the server wrote, never the
// client's text: a name it serves, or one of the two sentinels. Every shape
// below goes through the legacy handshake, the only one that lets the body's
// name reach the log.
#[test]
fn the_logged_name_is_a_word_of_the_server_or_a_sentinel() {
    for name in [
        json!("factory_log\r\nFORGED \x1b[31m"),
        json!("factory_status\n\x1b]0;pwn\x07"),
        json!("\r\n"),
        json!(" "),
        json!("factory status"),
        json!("&"),
        json!(null),
        json!(7),
        json!({"a": 1}),
        json!(["factory_status"]),
        json!(""),
        json!("x".repeat(4000)),
    ] {
        let r = legacy(Some(1), "tools/call", json!({"name": name}));
        let logged = r.tool.clone().unwrap_or_default();
        assert!(
            ["?", "-", "factory_status", "factory_log"].contains(&logged.as_str()),
            "{name} logged {logged:?}"
        );
        assert!(
            !logged.contains(['\r', '\n', '\t', ' ']) && !logged.contains('\x1b'),
            "{name} logged {logged:?}"
        );
    }
}

// A name of another shape than a string is refused by the mirror of its
// header, which compares text, so no tool is logged for it.
#[test]
fn a_name_that_is_not_text_is_refused_by_the_mirror() {
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": 7, "arguments": {"issue": 987},
                   "_meta": {"io.modelcontextprotocol/protocolVersion": MODERN}}})
    .to_string();
    let auth = format!("Bearer {K1}");
    let h = [
        ("Authorization", auth.as_str()),
        ("MCP-Protocol-Version", MODERN),
        ("Mcp-Method", "tools/call"),
        ("Mcp-Name", "factory_status"),
    ];
    let r = send("POST", &h, body.as_bytes());
    assert_eq!(json_of(&r)["error"]["code"], -32020);
    assert_eq!(r.tool, None);
}

// Only a tools/call names a tool. The other methods, and a request refused
// before its body is read, log as they did.
#[test]
fn only_a_call_names_a_tool_in_the_line() {
    // ping is answered to a legacy client, not to a modern one.
    for method in ["server/discover", "tools/list"] {
        let r = modern(method, json!({}));
        assert_eq!(r.status, 200, "{method}");
        assert_eq!(r.tool, None, "{method}");
    }
    assert_eq!(legacy(Some(1), "ping", json!({})).tool, None);
    let auth = format!("Bearer {K1}");
    for m in ["GET", "DELETE"] {
        let r = send(m, &[("Authorization", &auth)], b"");
        assert_eq!(r.status, 405, "{m}");
        assert_eq!(r.tool, None, "{m}");
    }
    let r = legacy(
        Some(1),
        "tools/call",
        json!({"name": "factory_status", "arguments": {"note": "private text"}}),
    );
    assert_eq!(r.status, 200);
    assert_eq!(r.tool.as_deref(), Some("factory_status"));
}

// A `tools/call` whose name is not text at all is a name the server has not,
// told from a probe by the dash it logs: the legacy handshake carries no
// `Mcp-Name` to mirror, so the body is what the server reads.
#[test]
fn a_call_whose_name_is_not_text_logs_a_dash() {
    let r = legacy(Some(1), "tools/call", json!({"name": 7}));
    assert_eq!(r.status, 200);
    assert_eq!(r.tool.as_deref(), Some("-"));
    let r = legacy(Some(1), "tools/call", json!({"arguments": {}}));
    assert_eq!(r.tool.as_deref(), Some("-"));
}

#[test]
fn headers_must_mirror_the_body() {
    let auth = format!("Bearer {K1}");
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "factory_status", "arguments": {},
                   "_meta": {"io.modelcontextprotocol/protocolVersion": MODERN}}})
    .to_string();
    let cases: [Vec<(&str, &str)>; 4] = [
        vec![("Mcp-Method", "tools/call"), ("Mcp-Name", "factory_stats")],
        vec![("Mcp-Method", "tools/list"), ("Mcp-Name", "factory_status")],
        vec![("Mcp-Name", "factory_status")],
        vec![("Mcp-Method", "tools/call")],
    ];
    for extra in cases {
        let mut h = vec![
            ("Authorization", auth.as_str()),
            ("MCP-Protocol-Version", MODERN),
        ];
        h.extend(extra.iter().copied());
        let r = send("POST", &h, body.as_bytes());
        assert_eq!(r.status, 400, "{extra:?}");
        assert_eq!(json_of(&r)["error"]["code"], -32020, "{extra:?}");
    }
    let mut h = vec![
        ("Authorization", auth.as_str()),
        ("MCP-Protocol-Version", "2025-11-25"),
    ];
    h.extend([("Mcp-Method", "tools/call"), ("Mcp-Name", "factory_status")]);
    assert_eq!(
        json_of(&send("POST", &h, body.as_bytes()))["error"]["code"],
        -32020
    );
}

#[test]
fn a_base64_name_header_is_decoded_before_the_comparison() {
    let auth = format!("Bearer {K1}");
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "factory_status", "arguments": {},
                   "_meta": {"io.modelcontextprotocol/protocolVersion": MODERN}}})
    .to_string();
    // "factory_status" in base64.
    let h = [
        ("Authorization", auth.as_str()),
        ("MCP-Protocol-Version", MODERN),
        ("Mcp-Method", "tools/call"),
        ("Mcp-Name", "=?base64?ZmFjdG9yeV9zdGF0dXM=?="),
    ];
    assert_eq!(send("POST", &h, body.as_bytes()).status, 200);
}

#[test]
fn another_version_is_unsupported_and_says_which_are() {
    let auth = format!("Bearer {K1}");
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list",
        "params": {"_meta": {"io.modelcontextprotocol/protocolVersion": "1900-01-01"}}})
    .to_string();
    let h = [
        ("Authorization", auth.as_str()),
        ("MCP-Protocol-Version", "1900-01-01"),
        ("Mcp-Method", "tools/list"),
    ];
    let r = send("POST", &h, body.as_bytes());
    assert_eq!(r.status, 400);
    let e = &json_of(&r)["error"];
    assert_eq!(e["code"], -32022);
    assert_eq!(e["data"]["supported"], json!(["2026-07-28", "2025-11-25"]));
    assert_eq!(e["data"]["requested"], "1900-01-01");
}

#[test]
fn a_legacy_client_initializes_and_calls_tools() {
    let auth = format!("Bearer {K1}");
    let init = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2025-11-25", "capabilities": {},
                   "clientInfo": {"name": "old", "version": "1"}}})
    .to_string();
    // initialize carries no MCP-Protocol-Version header.
    let r = send("POST", &[("Authorization", &auth)], init.as_bytes());
    assert_eq!(r.status, 200);
    let res = &json_of(&r)["result"];
    assert_eq!(res["protocolVersion"], "2025-11-25");
    assert_eq!(res["capabilities"], json!({"tools": {}}));
    assert_eq!(res["serverInfo"]["name"], "tehas");
    assert_eq!(header(&r, "Mcp-Session-Id"), None);

    let n = legacy(None, "notifications/initialized", json!({}));
    assert_eq!(n.status, 202);
    assert!(n.body.is_empty());
    assert_eq!(
        json_of(&legacy(Some(2), "ping", json!({})))["result"],
        json!({})
    );
    let list = json_of(&legacy(Some(3), "tools/list", json!({})));
    assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 12);
    let call = json_of(&legacy(
        Some(4),
        "tools/call",
        json!({"name": "factory_status", "arguments": {}}),
    ));
    assert_eq!(call["result"]["structuredContent"]["api"], 1);
}

#[test]
fn a_legacy_request_on_an_older_version_is_unsupported() {
    let auth = format!("Bearer {K1}");
    let body = br#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#;
    let r = send(
        "POST",
        &[
            ("Authorization", &auth),
            ("MCP-Protocol-Version", "2025-06-18"),
        ],
        body,
    );
    assert_eq!(r.status, 400);
    assert_eq!(json_of(&r)["error"]["code"], -32022);
}

#[test]
fn instructions_name_the_tools_and_the_window() {
    let auth = format!("Bearer {K1}");
    let init = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2025-11-25", "capabilities": {},
                   "clientInfo": {"name": "old", "version": "1"}}})
    .to_string();
    let legacy = json_of(&send("POST", &[("Authorization", &auth)], init.as_bytes()));
    let modern = json_of(&modern("server/discover", json!({})));
    for res in [&legacy["result"], &modern["result"]] {
        let text = res["instructions"].as_str().unwrap();
        for word in [
            "factory_failures",
            "factory_sessions",
            "factory_session",
            "factory_log",
            "30 days",
            "gzip",
        ] {
            assert!(text.contains(word), "{word}: {text}");
        }
        assert!(text.len() < 600, "{}", text.len());
    }
}

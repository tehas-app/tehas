//! JSON-RPC for MCP over Streamable HTTP: revision 2026-07-28, and the
//! 2025-11-25 handshake for clients that have not moved yet.
//!
//! A request whose params carry `_meta` with a protocol version is modern and
//! is checked against its mirrored headers. One without is legacy: an
//! `initialize` is answered, then the client's requests carry the header
//! `MCP-Protocol-Version: 2025-11-25`. No session is kept for either.

use crate::keys::Keys;
use crate::tools::{self, ToolError};
use serde_json::{json, Value};
use std::path::Path;

pub const MODERN: &str = "2026-07-28";
pub const LEGACY: &str = "2025-11-25";
pub const MAX_BODY: usize = 65536;
const VERSION_KEY: &str = "io.modelcontextprotocol/protocolVersion";
const SERVER_INFO_KEY: &str = "io.modelcontextprotocol/serverInfo";
const LIST_TTL_MS: u64 = 30000;

pub struct Request<'a> {
    pub method: &'a str,
    pub headers: Vec<(String, String)>,
    pub body: &'a [u8],
}

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// The fingerprint of the key that was used, for the access log.
    pub key: Option<String>,
}

impl Response {
    pub fn empty(status: u16) -> Self {
        Response {
            status,
            headers: vec![],
            body: vec![],
            key: None,
        }
    }

    fn json(status: u16, v: Value) -> Self {
        Response {
            status,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: v.to_string().into_bytes(),
            key: None,
        }
    }
}

fn header<'a>(req: &'a Request, name: &str) -> Option<&'a str> {
    req.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

fn error(status: u16, id: &Value, code: i64, message: &str, data: Option<Value>) -> Response {
    let mut e = json!({"code": code, "message": message});
    if let Some(d) = data {
        e["data"] = d;
    }
    Response::json(status, json!({"jsonrpc": "2.0", "id": id, "error": e}))
}

fn mismatch(id: &Value, what: &str) -> Response {
    error(400, id, -32020, &format!("Header mismatch: {what}"), None)
}

fn unsupported(id: &Value, requested: &str) -> Response {
    error(
        400,
        id,
        -32022,
        "Unsupported protocol version",
        Some(json!({"supported": [MODERN, LEGACY], "requested": requested})),
    )
}

pub fn handle(req: &Request, keys: &Keys, live: &Path, server_version: &str) -> Response {
    let key = match gate(req, keys) {
        Ok(k) => k,
        Err(r) => return r,
    };
    let mut r = answer(req, live, server_version);
    r.key = Some(key);
    r
}

/// What is decided from the headers alone, before any byte of the body is
/// read: the fingerprint of the request's key, or the answer that refuses it.
pub fn gate(req: &Request, keys: &Keys) -> Result<String, Response> {
    // No browser is a client of this server: an Origin means a page is
    // trying to reach it (DNS rebinding), whatever the origin is.
    if header(req, "Origin").is_some() {
        return Err(Response::empty(403));
    }
    if req.method != "POST" {
        return Err(Response::empty(405));
    }
    keys.check(header(req, "Authorization")).ok_or_else(|| {
        let mut r = Response::empty(401);
        r.headers.push(("WWW-Authenticate".into(), "Bearer".into()));
        r
    })
}

/// The answer to a request that passed the gate.
pub fn answer(req: &Request, live: &Path, server_version: &str) -> Response {
    if req.body.len() > MAX_BODY {
        return Response::empty(413);
    }
    let Ok(msg) = serde_json::from_slice::<Value>(req.body) else {
        return error(400, &Value::Null, -32700, "Parse error", None);
    };
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    let method = match (&msg["jsonrpc"], &msg["method"]) {
        (Value::String(v), Value::String(m)) if v == "2.0" => m.as_str(),
        _ => return error(400, &id, -32600, "Invalid Request", None),
    };
    let params = msg.get("params").cloned().unwrap_or(json!({}));
    let modern = match params["_meta"][VERSION_KEY].as_str() {
        Some(v) => {
            if let Some(r) = check_modern(req, &id, v, method, &params) {
                return r;
            }
            true
        }
        None => {
            if let Some(r) = check_legacy(req, &id, method) {
                return r;
            }
            false
        }
    };
    if msg.get("id").is_none() {
        return Response::empty(202);
    }
    let info = json!({"name": "tehas", "version": server_version});
    let mut result = match (method, modern) {
        ("server/discover", _) => json!({
            "resultType": "complete",
            "supportedVersions": [MODERN, LEGACY],
            "capabilities": {"tools": {}},
            "ttlMs": LIST_TTL_MS,
            "cacheScope": "private",
        }),
        ("initialize", false) => {
            return Response::json(
                200,
                json!({"jsonrpc": "2.0", "id": id, "result": {
                    "protocolVersion": LEGACY,
                    "capabilities": {"tools": {}},
                    "serverInfo": info,
                }}),
            )
        }
        ("ping", false) => json!({}),
        ("tools/list", _) => json!({
            "resultType": "complete",
            "tools": tools::list(),
            "ttlMs": LIST_TTL_MS,
            "cacheScope": "private",
        }),
        ("tools/call", _) => {
            let name = params["name"].as_str().unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match tools::call(live, name, &args) {
                Ok(v) => json!({
                    "resultType": "complete",
                    "content": [{"type": "text", "text": v.to_string()}],
                    "structuredContent": v,
                }),
                Err(ToolError::Unknown(n)) => {
                    return error(200, &id, -32602, &format!("Unknown tool: {n}"), None)
                }
                Err(ToolError::BadArgs(why)) => tool_error(&why),
                Err(ToolError::File { file, why }) => tool_error(&format!("{file} {why}")),
            }
        }
        _ => return error(404, &id, -32601, "Method not found", None),
    };
    if modern {
        result["_meta"] = json!({SERVER_INFO_KEY: info});
    }
    Response::json(200, json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

fn tool_error(text: &str) -> Value {
    json!({
        "resultType": "complete",
        "content": [{"type": "text", "text": text}],
        "isError": true,
    })
}

/// The modern request's version and the headers that mirror its body.
fn check_modern(
    req: &Request,
    id: &Value,
    version: &str,
    method: &str,
    params: &Value,
) -> Option<Response> {
    match header(req, "MCP-Protocol-Version") {
        Some(h) if h == version => {}
        Some(_) => return Some(mismatch(id, "MCP-Protocol-Version does not match the body")),
        None => return Some(mismatch(id, "MCP-Protocol-Version is missing")),
    }
    if version != MODERN {
        return Some(unsupported(id, version));
    }
    if header(req, "Mcp-Method") != Some(method) {
        return Some(mismatch(id, "Mcp-Method does not match the body"));
    }
    if method == "tools/call" {
        let name = header(req, "Mcp-Name").and_then(decode_name);
        if name.is_none() || name.as_deref() != params["name"].as_str() {
            return Some(mismatch(id, "Mcp-Name does not match the body"));
        }
    }
    None
}

/// A legacy request is on 2025-11-25; only its initialize comes without the
/// header, as that revision has it.
fn check_legacy(req: &Request, id: &Value, method: &str) -> Option<Response> {
    match header(req, "MCP-Protocol-Version") {
        Some(LEGACY) => None,
        None if method == "initialize" => None,
        // A request with no header is 2025-03-26 by the transport's rule.
        None => Some(unsupported(id, "2025-03-26")),
        Some(other) => Some(unsupported(id, other)),
    }
}

/// An Mcp-Name value, decoding the `=?base64?...?=` form.
fn decode_name(v: &str) -> Option<String> {
    match v
        .strip_prefix("=?base64?")
        .and_then(|s| s.strip_suffix("?="))
    {
        Some(b64) => String::from_utf8(base64(b64)?).ok(),
        None => Some(v.to_string()),
    }
}

fn base64(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        Some(v as u32)
    };
    let s = s.trim_end_matches('=').as_bytes();
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for &c in s {
        acc = (acc << 6) | val(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

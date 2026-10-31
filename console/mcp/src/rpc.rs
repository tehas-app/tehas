//! JSON-RPC for MCP over Streamable HTTP: revision 2026-07-28, and the
//! 2025-11-25 handshake for clients that have not moved yet.
//!
//! A request whose params carry `_meta` with a protocol version is modern and
//! is checked against its mirrored headers. One without is legacy: an
//! `initialize` is answered, then the client's requests carry the header
//! `MCP-Protocol-Version: 2025-11-25`. No session is kept for either.

use crate::keys::{Keys, Scope};
use crate::logs::LogGuard;
use crate::tools::{self, Dirs, Output, ToolError};
use serde_json::{json, Value};

pub const MODERN: &str = "2026-07-28";
pub const LEGACY: &str = "2025-11-25";
pub const MAX_BODY: usize = 65536;
const VERSION_KEY: &str = "io.modelcontextprotocol/protocolVersion";
const SERVER_INFO_KEY: &str = "io.modelcontextprotocol/serverInfo";
const LIST_TTL_MS: u64 = 30000;

/// Sent in both handshakes: how to use the tools.
pub const INSTRUCTIONS: &str =
    "This is a tehas software factory, read only: no tool changes anything. \
To see what went wrong, call factory_failures, then factory_sessions with outcome \"failed\", \
then factory_session for one of them, then factory_log for one of its logs. \
Sessions cover the last 30 days, and logs are masked. \
Ask for gzip: true to get a transcript whole; read a plain log in pieces with offset. \
The manual is at /mcp-server on the factory's site.";

pub struct Request<'a> {
    pub method: &'a str,
    pub headers: Vec<(String, String)>,
    pub body: &'a [u8],
}

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// The scope of the key that was used, for the access log: `full` or
    /// `dashboard`, the server's own words. A refused request has none.
    pub key: Option<String>,
    pub scope: Option<String>,
    /// The tool of a `tools/call`, for the access log: its name only, never
    /// its arguments, which can carry an issue number or text from a private
    /// repository. The value is one the server wrote, never the client's: a
    /// name of a tool it serves, `?` for a text name it has not (a probe), and
    /// `-` for a `tools/call` whose name is not text at all, which only the
    /// 2025-11-25 handshake can carry, as a modern request must mirror its
    /// name in `Mcp-Name` and no text equals a number. So `-` covers a broken
    /// client and a probe whose name never reached the server as text alike.
    /// A request that is not a `tools/call` has none.
    pub tool: Option<String>,
    /// The place of a log being sent, freed when the answer is dropped.
    pub hold: Option<LogGuard>,
}

impl Response {
    pub fn empty(status: u16) -> Self {
        Response {
            status,
            headers: vec![],
            body: vec![],
            key: None,
            scope: None,
            tool: None,
            hold: None,
        }
    }

    fn json(status: u16, v: Value) -> Self {
        Response {
            status,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: v.to_string().into_bytes(),
            key: None,
            scope: None,
            tool: None,
            hold: None,
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

pub fn handle(req: &Request, keys: &Keys, dirs: &Dirs, server_version: &str) -> Response {
    let (key, scope) = match gate(req, keys) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let mut r = answer(req, scope, dirs, server_version);
    r.key = Some(key);
    r.scope = Some(scope.as_str().to_string());
    r
}

/// What is decided from the headers alone, before any byte of the body is
/// read: the fingerprint of the request's key with its scope, or the answer
/// that refuses it. The refusal is held in a box: a `Response` carries a body,
/// and the refusal of the headers never has one, so the box keeps the size of
/// this function's error nothing but a pointer where an answer is a body's
/// length away.
pub fn gate(req: &Request, keys: &Keys) -> Result<(String, Scope), Box<Response>> {
    // No browser is a client of this server: an Origin means a page is
    // trying to reach it (DNS rebinding), whatever the origin is.
    if header(req, "Origin").is_some() {
        return Err(Box::new(Response::empty(403)));
    }
    // The key before the method: without one, nothing about the server is
    // said, not even which methods it takes.
    let (key, scope) = keys.check(header(req, "Authorization")).ok_or_else(|| {
        Box::new({
            let mut r = Response::empty(401);
            r.headers.push(("WWW-Authenticate".into(), "Bearer".into()));
            r
        })
    })?;
    if req.method != "POST" {
        let mut r = Response::empty(405);
        r.key = Some(key);
        r.scope = Some(scope.as_str().to_string());
        return Err(Box::new(r));
    }
    Ok((key, scope))
}

/// The answer to a request that passed the gate. `scope` is the scope of the
/// key that made the request: it decides which tools the request may see and
/// call.
pub fn answer(req: &Request, scope: Scope, dirs: &Dirs, server_version: &str) -> Response {
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
    let mut hold = None;
    let mut tool = None;
    let mut result = match (method, modern) {
        ("server/discover", _) => json!({
            "resultType": "complete",
            "supportedVersions": [MODERN, LEGACY],
            "capabilities": {"tools": {}},
            "instructions": INSTRUCTIONS,
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
                    "instructions": INSTRUCTIONS,
                }}),
            )
        }
        ("ping", false) => json!({}),
        ("tools/list", _) => json!({
            "resultType": "complete",
            "tools": tools::list_scoped(scope),
            "ttlMs": LIST_TTL_MS,
            "cacheScope": "private",
        }),
        ("tools/call", _) => {
            // One read of the body's name, shared by the dispatch and the log
            // so the two cannot name different tools. Only the name goes to
            // the log: the arguments can carry an issue number or text from a
            // private repository, and are never copied.
            let name = params["name"].as_str();
            // The line says which tool the request asked for even when the
            // call is refused: the server's own name for one it serves, `?`
            // for a text name it has not, and `-` for a name that is not
            // text. A name it serves is a word of `CALLED`, copied from the
            // body only ever as one, so no byte of the client's reaches the
            // line.
            tool = Some(match name {
                Some(n) if tools::known(n) => n.to_string(),
                Some(_) => "?".into(),
                None => "-".into(),
            });
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            // A tool outside the key's scope is refused before its arguments
            // are looked at, and is refused as an unknown one is below: the
            // answer is the same, so the scope cannot be probed tool by tool.
            let served = name.is_some_and(|n| Keys::serves(scope, n));
            // A tool the key may not see is answered as one the server does
            // not have, so no argument of it is read and no other answer is
            // ever given.
            let called = if served {
                tools::call_held(dirs, name.unwrap_or(""), &args)
            } else {
                Err(ToolError::Unknown(name.unwrap_or("").to_string()))
            };
            match called.map(|(out, place)| {
                hold = place;
                out
            }) {
                Ok(Output::Json(v)) => json!({
                    "resultType": "complete",
                    "content": [{"type": "text", "text": v.to_string()}],
                    "structuredContent": v,
                }),
                Ok(Output::Blob {
                    meta,
                    uri,
                    mime,
                    blob,
                }) => json!({
                    "resultType": "complete",
                    "content": [
                        {"type": "text", "text": meta.to_string()},
                        {"type": "resource", "resource": {"uri": uri, "mimeType": mime, "blob": blob}},
                    ],
                    "structuredContent": meta,
                }),
                Err(ToolError::Unknown(_)) => {
                    // The name the client sent is not echoed: the answer is
                    // the same for any name the server has not, so it gives a
                    // caller no way to tell a tool it has from one it has not.
                    // The journal holds the distinction, as `tool=?`.
                    let mut r = error(200, &id, -32602, "Unknown tool", None);
                    r.tool = tool.take();
                    return r;
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
    let mut r = Response::json(200, json!({"jsonrpc": "2.0", "id": id, "result": result}));
    r.hold = hold;
    r.tool = tool;
    r
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

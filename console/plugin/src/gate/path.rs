//! A request's path, normalised before it is matched, and what it names
//! (docs/specs/2026-11-21-plugins-web.md, "Paths").
//!
//! Percent escapes are decoded first, so `%2e%2e` is `..` and `%5f` is `_`.
//! An escaped `/` or `\`, a `\`, a control byte or text that is not UTF-8 is
//! no path. Empty and `.` segments go, `..` takes the one before it, and a
//! `..` past the root leaves `view/`: no path. Paths starting `/_` are the
//! gate's, whatever the view holds.

/// A normalised path: its segments, none empty, `.` or `..`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path(pub Vec<String>);

impl Path {
    /// The path as `/a/b`, `/` for none.
    pub fn text(&self) -> String {
        format!("/{}", self.0.join("/"))
    }
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Decodes one segment's escapes. None: a bad escape, an escaped separator,
/// a control byte or not UTF-8.
fn decode(raw: &str) -> Option<String> {
    let b = raw.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let v = hex(*b.get(i + 1)?)? << 4 | hex(*b.get(i + 2)?)?;
            if v == b'/' || v == b'\\' {
                return None;
            }
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    let s = String::from_utf8(out).ok()?;
    (!s.chars().any(|c| c.is_control() || c == '\\')).then_some(s)
}

/// The path of a request target, without its query. None: no path the gate
/// serves, which is 404.
pub fn normalise(target: &str) -> Option<Path> {
    let path = target.split_once('?').map_or(target, |(p, _)| p);
    let rest = path.strip_prefix('/')?;
    if path.contains('#') {
        return None;
    }
    let mut out: Vec<String> = Vec::new();
    for raw in rest.split('/') {
        match decode(raw)?.as_str() {
            "" | "." => {}
            ".." => {
                out.pop()?;
            }
            s => out.push(s.to_string()),
        }
    }
    Some(Path(out))
}

/// The value of `name` in a target's query, decoded. None when it is not
/// there once or does not decode.
pub fn query(target: &str, name: &str) -> Option<String> {
    let (_, q) = target.split_once('?')?;
    let mut found = q
        .split('&')
        .filter_map(|kv| kv.split_once('=').or(Some((kv, ""))))
        .filter(|(k, _)| *k == name)
        .map(|(_, v)| v);
    let v = found.next()?;
    if found.next().is_some() {
        return None;
    }
    let b = v.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => {
                out.push(hex(*b.get(i + 1)?)? << 4 | hex(*b.get(i + 2)?)?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// Percent-encodes everything but the unreserved characters and `/`.
pub fn encode(s: &str) -> String {
    escape(s, b"-._~/")
}

/// Percent-encodes a value of a query: `/` too.
pub fn component(s: &str) -> String {
    escape(s, b"-._~")
}

fn escape(s: &str, keep: &[u8]) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || keep.contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// What a path of a plugin's name is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// `/_kit/<file>`, the file's path in the kit.
    Kit(String),
    /// `GET /_actions`.
    Actions,
    /// `POST /_actions/<id>`.
    Action(String),
    /// `/_hooks/<pr>`.
    Hooks(u64),
    Session,
    Signout,
    /// `/_auth/<step>` of the hand-off on a plugin's name: `ask`, `take`.
    Auth(String),
    /// A path of `/_` the gate does not answer. v3's `/_frame/<path>` and
    /// uploads, and the WebSocket paths `/_relay/<id>` and `/_console/<id>`,
    /// are matched here when they are built.
    Reserved,
    /// A file of `view/`, its segments.
    File(Vec<String>),
}

/// What `path` names on a plugin's name.
pub fn route(path: &Path) -> Route {
    let s: Vec<&str> = path.0.iter().map(String::as_str).collect();
    match s.as_slice() {
        [first, ..] if !first.starts_with('_') => Route::File(path.0.clone()),
        [] => Route::File(Vec::new()),
        ["_kit", rest @ ..] if !rest.is_empty() => Route::Kit(rest.join("/")),
        ["_actions"] => Route::Actions,
        ["_actions", id] if tehas_core::plugin::is_action_id(id) => Route::Action(id.to_string()),
        ["_hooks", pr] => match pr.parse::<u64>() {
            Ok(n) if n > 0 && pr.len() <= 9 && !pr.starts_with('0') => Route::Hooks(n),
            _ => Route::Reserved,
        },
        ["_session"] => Route::Session,
        ["_signout"] => Route::Signout,
        ["_auth", step] => Route::Auth(step.to_string()),
        _ => Route::Reserved,
    }
}

/// A `back` path of the hand-off: a path only, normalised, never one of the
/// gate's own. None for anything else, which the hand-off takes as `/`.
pub fn back(raw: &str) -> Option<String> {
    if !raw.starts_with('/') || raw.starts_with("//") || raw.contains('?') {
        return None;
    }
    let p = normalise(raw)?;
    let slash = if raw.ends_with('/') && !p.0.is_empty() {
        "/"
    } else {
        ""
    };
    match route(&p) {
        Route::File(_) => Some(format!("{}{slash}", encode(&p.text()))),
        _ => None,
    }
}

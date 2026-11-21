//! The browser's policy of a plugin's page (docs/specs/2026-11-21-plugins-web.md,
//! "The browser's policy"): a pure function from a document's bytes to its
//! `Content-Security-Policy`. Each inline `<script>` and `<style>` block is
//! hashed with sha256 and named in that document's policy; nothing of the
//! plugin's can add a source. A document with an inline event handler
//! (`onclick=`) or `<meta http-equiv="refresh">` gets no policy: the gate
//! serves an error page in its place.

use sha2::{Digest, Sha256};

/// `Permissions-Policy` of every answer of the gate.
pub const PERMISSIONS: &str = "camera=(), microphone=(), geolocation=()";
/// `Permissions-Policy` of a page.
pub const PAGE_PERMISSIONS: &str =
    "camera=(), microphone=(), geolocation=(), usb=(), payment=(), clipboard-read=()";
pub const OPENER: &str = "same-origin-allow-popups";

/// Why a document gets no policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// An attribute such as `onclick`, by its name.
    Handler(String),
    Refresh,
}

impl Refused {
    pub fn why(&self) -> String {
        match self {
            Refused::Handler(name) => format!("it has an inline event handler ({name})"),
            Refused::Refresh => "it has a meta refresh".to_string(),
        }
    }
}

/// The inline blocks a document holds.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Blocks {
    pub scripts: Vec<String>,
    pub styles: Vec<String>,
}

fn base64(data: &[u8]) -> String {
    const LETTERS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let at = |i: usize| chunk.get(i).copied().unwrap_or(0) as u32;
        let n = at(0) << 16 | at(1) << 8 | at(2);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(LETTERS[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The source expression of a block's text.
pub fn hash(text: &[u8]) -> String {
    format!("'sha256-{}'", base64(&Sha256::digest(text)))
}

fn lower(b: &[u8]) -> Vec<u8> {
    b.to_ascii_lowercase()
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from > hay.len() {
        return None;
    }
    hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// An attribute's name, in lower case, and its value.
type Attr = (Vec<u8>, Vec<u8>);

/// A tag's name and its attributes, read from just after `<`. Returns where
/// the tag ends, after its `>`.
fn tag(doc: &[u8], at: usize) -> (Vec<u8>, Vec<Attr>, usize) {
    let mut i = at;
    let name_start = i;
    while i < doc.len() && !doc[i].is_ascii_whitespace() && doc[i] != b'>' && doc[i] != b'/' {
        i += 1;
    }
    let name = lower(&doc[name_start..i]);
    let mut attrs = Vec::new();
    loop {
        while i < doc.len() && (doc[i].is_ascii_whitespace() || doc[i] == b'/') {
            i += 1;
        }
        if i >= doc.len() {
            return (name, attrs, i);
        }
        if doc[i] == b'>' {
            return (name, attrs, i + 1);
        }
        let k = i;
        while i < doc.len() && !doc[i].is_ascii_whitespace() && !b"=>/".contains(&doc[i]) {
            i += 1;
        }
        if i == k {
            i += 1;
            continue;
        }
        let key = lower(&doc[k..i]);
        while i < doc.len() && doc[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut value = Vec::new();
        if i < doc.len() && doc[i] == b'=' {
            i += 1;
            while i < doc.len() && doc[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < doc.len() && (doc[i] == b'"' || doc[i] == b'\'') {
                let q = doc[i];
                let end = doc[i + 1..]
                    .iter()
                    .position(|&b| b == q)
                    .map_or(doc.len(), |p| i + 1 + p);
                value = doc[i + 1..end].to_vec();
                i = (end + 1).min(doc.len());
            } else {
                let v = i;
                while i < doc.len() && !doc[i].is_ascii_whitespace() && doc[i] != b'>' {
                    i += 1;
                }
                value = doc[v..i].to_vec();
            }
        }
        attrs.push((key, value));
    }
}

/// The document's inline blocks, or why it is refused. A tag is read as a
/// browser's tokenizer reads it, near enough to find every attribute name:
/// comments are skipped, and a `<script>` or `<style>` element's text runs to
/// its end tag, whatever it holds.
pub fn blocks(doc: &[u8]) -> Result<Blocks, Refused> {
    let mut out = Blocks::default();
    let ldoc = lower(doc);
    let mut i = 0;
    while let Some(lt) = find(doc, b"<", i) {
        if doc[lt..].starts_with(b"<!--") {
            i = find(doc, b"-->", lt + 4).map_or(doc.len(), |e| e + 3);
            continue;
        }
        let next = doc.get(lt + 1).copied().unwrap_or(b' ');
        if !next.is_ascii_alphabetic() {
            i = lt + 1;
            continue;
        }
        let (name, attrs, end) = tag(doc, lt + 1);
        if let Some((k, _)) = attrs.iter().find(|(k, _)| k.starts_with(b"on")) {
            return Err(Refused::Handler(String::from_utf8_lossy(k).into_owned()));
        }
        let attr = |n: &[u8]| attrs.iter().find(|(k, _)| k == n).map(|(_, v)| lower(v));
        if name == b"meta" && attr(b"http-equiv").is_some_and(|v| v.trim_ascii() == b"refresh") {
            return Err(Refused::Refresh);
        }
        i = end;
        if name == b"script" || name == b"style" {
            let close = [b"</".as_slice(), &name].concat();
            let stop = find(&ldoc, &close, end).unwrap_or(doc.len());
            let src = name == b"script" && attr(b"src").is_some();
            if !src {
                let h = hash(&doc[end..stop]);
                let list = if name == b"script" {
                    &mut out.scripts
                } else {
                    &mut out.styles
                };
                if !list.contains(&h) {
                    list.push(h);
                }
            }
            i = stop;
        }
    }
    Ok(out)
}

/// The policy of a document whose blocks are `b`, framed by the view itself
/// and the dashboard at `dashboard` (`https://<TEHAS_DOMAIN>`).
pub fn policy(b: &Blocks, dashboard: &str) -> String {
    let with = |list: &[String]| {
        let mut s = "'self'".to_string();
        for h in list {
            s.push(' ');
            s.push_str(h);
        }
        s
    };
    format!(
        "default-src 'none'; script-src {}; style-src {}; style-src-attr 'unsafe-inline'; \
         img-src 'self' data:; font-src 'self' data:; connect-src 'self'; frame-src 'self'; \
         frame-ancestors 'self' {dashboard}; form-action 'self'; base-uri 'none'; \
         object-src 'none'; worker-src 'none'; manifest-src 'none'",
        with(&b.scripts),
        with(&b.styles)
    )
}

/// The policy of what is not a page: nothing runs, nothing loads.
pub fn file_policy(dashboard: &str) -> String {
    format!("default-src 'none'; frame-ancestors 'self' {dashboard}; sandbox")
}

/// The policy of `doc`, or why it has none.
pub fn of(doc: &[u8], dashboard: &str) -> Result<String, Refused> {
    blocks(doc).map(|b| policy(&b, dashboard))
}

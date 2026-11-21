//! One HTTP/1.1 request per connection, read with limits, as
//! console/mcp/src/http.rs reads one: the head at most 8 KB, the whole request
//! within 10 seconds, a body only with a Content-Length of at most 16 KiB, and
//! every answer closes the connection. The gate's answers are made in
//! [`super::Gate::answer`]; this file only reads and writes them.

use std::fs::File;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

pub const MAX_HEAD: usize = 8192;
/// A press and a sign-out carry no body the gate reads; this bounds what it
/// drains.
pub const MAX_BODY: usize = 16 * 1024;
pub const IO_TIMEOUT: Duration = Duration::from_secs(10);
/// The slowest a client may read an answer: past [`IO_TIMEOUT`], one more
/// second for each this many bytes.
pub const WRITE_RATE: u64 = 128 * 1024;

/// A header the request sent more than once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Twice;

/// A request as read: the method, the target as sent, the headers in order.
#[derive(Debug, Clone, Default)]
pub struct Request {
    pub method: String,
    pub target: String,
    pub headers: Vec<(String, String)>,
}

impl Request {
    pub fn new(method: &str, target: &str, headers: &[(&str, &str)]) -> Request {
        Request {
            method: method.to_string(),
            target: target.to_string(),
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    /// The value of a header sent once. Err: it was sent more than once,
    /// which the gate refuses for the headers it decides by.
    pub fn one(&self, name: &str) -> Result<Option<&str>, Twice> {
        let mut found = self
            .headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str());
        let first = found.next();
        match found.next() {
            Some(_) => Err(Twice),
            None => Ok(first),
        }
    }

    pub fn has(&self, name: &str) -> bool {
        self.headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case(name))
    }
}

pub enum Body {
    Bytes(Vec<u8>),
    /// A file of the view or the kit, sent as it is read.
    File(File, u64),
}

impl Body {
    pub fn len(&self) -> u64 {
        match self {
            Body::Bytes(b) => b.len() as u64,
            Body::File(_, n) => *n,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Body,
}

impl Response {
    pub fn empty(status: u16) -> Response {
        Response {
            status,
            headers: Vec::new(),
            body: Body::Bytes(Vec::new()),
        }
    }

    pub fn with(status: u16, kind: &str, body: Vec<u8>) -> Response {
        let mut r = Response::empty(status);
        r.set("Content-Type", kind);
        r.body = Body::Bytes(body);
        r
    }

    /// Sets a header, replacing one of the same name.
    pub fn set(&mut self, name: &str, value: &str) {
        self.headers.retain(|(k, _)| !k.eq_ignore_ascii_case(name));
        self.headers.push((name.to_string(), value.to_string()));
    }

    /// Adds a header beside any of the same name, as Set-Cookie is sent.
    pub fn add(&mut self, name: &str, value: &str) {
        self.headers.push((name.to_string(), value.to_string()));
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The body's bytes, for a test. A file is read whole.
    pub fn bytes(&mut self) -> Vec<u8> {
        match &mut self.body {
            Body::Bytes(b) => b.clone(),
            Body::File(f, _) => {
                let mut out = Vec::new();
                let _ = f.read_to_end(&mut out);
                out
            }
        }
    }
}

fn read_by(stream: &mut TcpStream, deadline: Instant, buf: &mut [u8]) -> Option<usize> {
    let left = deadline.checked_duration_since(Instant::now())?;
    stream
        .set_read_timeout(Some(left.max(Duration::from_millis(1))))
        .ok()?;
    match stream.read(buf) {
        Ok(0) | Err(_) => None,
        Ok(n) => Some(n),
    }
}

/// Reads one request's head, and drains a body of at most [`MAX_BODY`].
/// Err is the status to answer.
pub fn read(stream: &mut TcpStream, deadline: Instant) -> Result<Request, u16> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 2048];
    let mut body = loop {
        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let body = buf.split_off(end + 4);
            buf.truncate(end);
            break body;
        }
        if buf.len() > MAX_HEAD {
            return Err(431);
        }
        match read_by(stream, deadline, &mut chunk) {
            Some(n) => buf.extend_from_slice(&chunk[..n]),
            None => return Err(408),
        }
    };
    let req = parse_head(&buf).ok_or(400u16)?;
    if req.has("Transfer-Encoding") {
        return Err(411);
    }
    let len = match req.one("Content-Length").map_err(|Twice| 400u16)? {
        None => 0,
        Some(v) => v.parse::<usize>().map_err(|_| 400u16)?,
    };
    if len > MAX_BODY {
        return Err(413);
    }
    while body.len() < len {
        match read_by(stream, deadline, &mut chunk) {
            Some(n) => body.extend_from_slice(&chunk[..n]),
            None => return Err(408),
        }
    }
    Ok(req)
}

pub fn parse_head(head: &[u8]) -> Option<Request> {
    let text = std::str::from_utf8(head).ok()?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let (method, target, proto) = (first.next()?, first.next()?, first.next()?);
    let word = (1..=16).contains(&method.len()) && method.bytes().all(|b| b.is_ascii_uppercase());
    if first.next().is_some() || !proto.starts_with("HTTP/1.") || !word {
        return None;
    }
    let mut headers = vec![];
    for line in lines {
        let (k, v) = line.split_once(':')?;
        if k.is_empty() || k.bytes().any(|b| b <= b' ' || b >= 0x7f) {
            return None;
        }
        headers.push((k.to_string(), v.trim().to_string()));
    }
    Some(Request {
        method: method.to_string(),
        target: target.to_string(),
        headers,
    })
}

pub fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        202 => "Accepted",
        303 => "See Other",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        411 => "Length Required",
        413 => "Content Too Large",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Unknown",
    }
}

fn send(stream: &mut TcpStream, mut bytes: &[u8], deadline: Instant) -> bool {
    while !bytes.is_empty() {
        let Some(left) = deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
        else {
            return false;
        };
        if stream.set_write_timeout(Some(left)).is_err() {
            return false;
        }
        match stream.write(bytes) {
            Ok(0) => return false,
            Ok(n) => bytes = &bytes[n..],
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return false,
        }
    }
    true
}

/// Writes the answer, its body unless `head` (a HEAD request), within
/// [`IO_TIMEOUT`] and a second more for each [`WRITE_RATE`] bytes.
pub fn write(stream: &mut TcpStream, r: Response, head: bool) {
    let len = r.body.len();
    let deadline = Instant::now() + IO_TIMEOUT + Duration::from_secs(len / WRITE_RATE);
    let mut text = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: {len}\r\nConnection: close\r\n",
        r.status,
        reason(r.status)
    );
    for (k, v) in &r.headers {
        text.push_str(&format!("{k}: {v}\r\n"));
    }
    text.push_str("\r\n");
    if !send(stream, text.as_bytes(), deadline) || head {
        return;
    }
    match r.body {
        Body::Bytes(b) => {
            send(stream, &b, deadline);
        }
        Body::File(mut f, n) => {
            let mut left = n;
            let mut chunk = vec![0u8; 64 * 1024];
            while left > 0 {
                let want = chunk.len().min(left as usize);
                match f.read(&mut chunk[..want]) {
                    Ok(0) | Err(_) => return,
                    Ok(got) => {
                        if !send(stream, &chunk[..got], deadline) {
                            return;
                        }
                        left -= got as u64;
                    }
                }
            }
        }
    }
}

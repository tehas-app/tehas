//! One HTTP/1.1 request per connection, read with limits.
//!
//! The server is on the internet behind a proxy, so nothing here waits on a
//! client for long or reads more than it must: the head is at most 8 KB, every
//! read and write times out, the key is checked before any byte of the body is
//! read, the body needs a Content-Length of at most 64 KB, and every answer
//! closes the connection.

use crate::keys::Keys;
use crate::rpc::{self, Request, Response, MAX_BODY};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::{Duration, Instant};

pub const MAX_HEAD: usize = 8192;
pub const IO_TIMEOUT: Duration = Duration::from_secs(10);
const ENDPOINT: &str = "/mcp";

/// Serves the one request on `stream` and returns its access-log line.
pub fn serve(mut stream: TcpStream, keys: &Keys, live: &Path, version: &str) -> String {
    let started = Instant::now();
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let (method, r) = exchange(&mut stream, keys, live, version);
    write(&mut stream, &r);
    format!(
        "{method} {} {}ms key={}",
        r.status,
        started.elapsed().as_millis(),
        r.key.as_deref().unwrap_or("-")
    )
}

/// Sends a short answer on a connection that is not served, such as one over
/// the limit of connections.
pub fn refuse(mut stream: TcpStream, status: u16) {
    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
    write(&mut stream, &Response::empty(status));
}

fn exchange(stream: &mut TcpStream, keys: &Keys, live: &Path, version: &str) -> (String, Response) {
    let (head, mut body) = match read_head(stream) {
        Ok(v) => v,
        Err(status) => return ("-".into(), Response::empty(status)),
    };
    let Some((method, path, headers)) = parse_head(&head) else {
        return ("-".into(), Response::empty(400));
    };
    let mut req = Request {
        method: &method,
        headers,
        body: &[],
    };
    let key = match rpc::gate(&req, keys) {
        Ok(k) => k,
        Err(r) => return (method.clone(), r),
    };
    let refuse = |status| {
        let mut r = Response::empty(status);
        r.key = Some(key.clone());
        (method.clone(), r)
    };
    if path != ENDPOINT {
        return refuse(404);
    }
    let value = |name: &str| {
        req.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.trim().to_string())
    };
    if value("Transfer-Encoding").is_some() {
        return refuse(411);
    }
    let Some(len) = value("Content-Length").and_then(|v| v.parse::<u64>().ok()) else {
        return refuse(411);
    };
    if len > MAX_BODY as u64 {
        return refuse(413);
    }
    let len = len as usize;
    if body.len() < len {
        let have = body.len();
        body.resize(len, 0);
        if stream.read_exact(&mut body[have..]).is_err() {
            return refuse(408);
        }
    }
    body.truncate(len);
    req.body = &body;
    let mut r = rpc::answer(&req, live, version);
    r.key = Some(key);
    (method.clone(), r)
}

/// The head up to its blank line, and whatever of the body came with it.
fn read_head(stream: &mut TcpStream) -> Result<(Vec<u8>, Vec<u8>), u16> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 2048];
    loop {
        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let body = buf.split_off(end + 4);
            buf.truncate(end);
            return Ok((buf, body));
        }
        if buf.len() > MAX_HEAD {
            return Err(431);
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return Err(408),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
}

type Head = (String, String, Vec<(String, String)>);

fn parse_head(head: &[u8]) -> Option<Head> {
    let text = std::str::from_utf8(head).ok()?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let (method, path, proto) = (first.next()?, first.next()?, first.next()?);
    if first.next().is_some() || !proto.starts_with("HTTP/1.") {
        return None;
    }
    let mut headers = vec![];
    for line in lines {
        let (k, v) = line.split_once(':')?;
        headers.push((k.trim().to_string(), v.trim().to_string()));
    }
    Some((method.to_string(), path.to_string(), headers))
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        411 => "Length Required",
        413 => "Content Too Large",
        431 => "Request Header Fields Too Large",
        503 => "Service Unavailable",
        _ => "Unknown",
    }
}

fn write(stream: &mut TcpStream, r: &Response) {
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        r.status,
        reason(r.status),
        r.body.len()
    );
    for (k, v) in &r.headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&r.body);
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Write);
}

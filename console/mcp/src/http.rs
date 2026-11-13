//! One HTTP/1.1 request per connection, read with limits.
//!
//! The server is on the internet behind a proxy, so nothing here waits on a
//! client for long or reads more than it must: the head is at most 8 KB, the
//! whole request must arrive within 10 seconds however it trickles in, the key
//! is checked before any byte of the body is read, the body needs a
//! Content-Length of at most 64 KB, the answer must be read within 10 seconds
//! and one more for each 128 KiB of it, and every answer closes the
//! connection.

use crate::keys::Keys;
use crate::rpc::{self, Request, Response, MAX_BODY};
use crate::tools::Dirs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

pub const MAX_HEAD: usize = 8192;
pub const IO_TIMEOUT: Duration = Duration::from_secs(10);
/// The slowest a client may read an answer: past `--io-timeout`, one more
/// second for each this many bytes.
pub const WRITE_RATE: usize = 128 * 1024;
const ENDPOINT: &str = "/mcp";

/// How long an answer of `len` bytes may take to be read.
pub fn write_time(io_timeout: Duration, len: usize) -> Duration {
    io_timeout + Duration::from_secs((len / WRITE_RATE) as u64)
}

/// Serves the one request on `stream` within `io_timeout` and returns its
/// access-log line. The binary passes the deadline of its `--io-timeout`,
/// which is `IO_TIMEOUT` unless a test asks for a shorter one.
pub fn serve(
    mut stream: TcpStream,
    keys: &Keys,
    dirs: &Dirs,
    version: &str,
    io_timeout: Duration,
) -> String {
    let started = Instant::now();
    let deadline = started + io_timeout;
    let (method, r) = exchange(&mut stream, deadline, keys, dirs, version);
    // The answer has a deadline of its own, however slowly it is read: a log
    // being sent holds one of the few places for logs until it is written.
    write(
        &mut stream,
        &r,
        Instant::now() + write_time(io_timeout, r.body.len()),
    );
    // The line's fields are named, in the order the manual shows them. Only
    // bytes of the server's own go into it: `method` comes from `parse_head`,
    // which keeps capitals; `scope` is the word of the server's own `Scope`,
    // rendered here and nowhere else; and `tool` from the allowlist of
    // `tools::known`.
    let mut line = format!(
        "{method} {} {}ms key={}",
        r.status,
        started.elapsed().as_millis(),
        r.key.as_deref().unwrap_or("-")
    );
    // The scope of the key, the server's own word, and only after the key it
    // belongs to: a refused request names neither.
    if let Some(s) = &r.scope {
        line.push_str(&format!(" scope={}", s.as_str()));
    }
    if let Some(t) = &r.tool {
        // A field of the line, not the client's text: `known` admits only the
        // words of `CALLED`, so no name, space or control byte of a request
        // can add or move a field here.
        line.push_str(&format!(" tool={t}"));
    }
    line
}

/// Sends a short answer on a connection that is not served, such as one over
/// the limit of connections.
pub fn refuse(mut stream: TcpStream, status: u16) {
    let deadline = Instant::now() + Duration::from_secs(1);
    write(&mut stream, &Response::empty(status), deadline);
}

fn exchange(
    stream: &mut TcpStream,
    deadline: Instant,
    keys: &Keys,
    dirs: &Dirs,
    version: &str,
) -> (String, Response) {
    let (head, mut body) = match read_head(stream, deadline) {
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
        Err(r) => return (method.clone(), r.into_response()),
    };
    let refuse = |status| {
        let mut r = Response::empty(status);
        r.key = Some(key.fingerprint().to_string());
        r.scope = Some(key.scope());
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
    while body.len() < len {
        let mut chunk = vec![0u8; len - body.len()];
        match read_by(stream, deadline, &mut chunk) {
            Some(n) => body.extend_from_slice(&chunk[..n]),
            None => return refuse(408),
        }
    }
    body.truncate(len);
    req.body = &body;
    let mut r = rpc::answer(&req, &key, dirs, version);
    r.key = Some(key.fingerprint().to_string());
    r.scope = Some(key.scope());
    (method.clone(), r)
}

/// The head up to its blank line, and whatever of the body came with it.
/// One read that ends by the deadline: the bytes read, or None at the end of
/// the stream, an error or the deadline.
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

fn read_head(stream: &mut TcpStream, deadline: Instant) -> Result<(Vec<u8>, Vec<u8>), u16> {
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
        match read_by(stream, deadline, &mut chunk) {
            Some(n) => buf.extend_from_slice(&chunk[..n]),
            None => return Err(408),
        }
    }
}

type Head = (String, String, Vec<(String, String)>);

fn parse_head(head: &[u8]) -> Option<Head> {
    let text = std::str::from_utf8(head).ok()?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let (method, path, proto) = (first.next()?, first.next()?, first.next()?);
    // A method is a short word of capitals: it goes into the access log.
    let word = (1..=16).contains(&method.len()) && method.bytes().all(|b| b.is_ascii_uppercase());
    if first.next().is_some() || !proto.starts_with("HTTP/1.") || !word {
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

/// Writes `bytes` by `deadline`; false when the deadline or an error came
/// first.
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

fn write(stream: &mut TcpStream, r: &Response, deadline: Instant) {
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
    if send(stream, head.as_bytes(), deadline) && send(stream, &r.body, deadline) {
        let _ = stream.flush();
        let _ = stream.shutdown(std::net::Shutdown::Write);
    } else {
        let _ = stream.shutdown(std::net::Shutdown::Both);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    // A client that never reads does not hold the connection past the
    // deadline, however many writes each move a little.
    #[test]
    fn an_answer_is_written_by_its_deadline_or_dropped() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        let mut r = Response::empty(200);
        r.body = vec![b'x'; 64 << 20];
        let started = Instant::now();
        write(&mut server, &r, started + Duration::from_millis(300));
        let took = started.elapsed();
        assert!(took >= Duration::from_millis(250), "{took:?}");
        assert!(took < Duration::from_secs(3), "{took:?}");
        drop(client);
    }

    #[test]
    fn a_long_answer_gets_a_second_more_per_128_kib() {
        let t = Duration::from_secs(10);
        assert_eq!(write_time(t, 0), t);
        assert_eq!(write_time(t, WRITE_RATE - 1), t);
        assert_eq!(write_time(t, 16 << 20), Duration::from_secs(10 + 128));
    }

    #[test]
    fn a_short_answer_is_written_whole() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        let mut r = Response::empty(200);
        r.body = b"ok".to_vec();
        write(&mut server, &r, Instant::now() + Duration::from_secs(5));
        let mut got = String::new();
        client.read_to_string(&mut got).unwrap();
        assert!(got.starts_with("HTTP/1.1 200 OK\r\n"), "{got}");
        assert!(got.ends_with("\r\n\r\nok"), "{got}");
    }
}

//! The bytes between the proxy and the endpoint the factory configured.
//!
//! The upstream is the `baseUrl` of the provider in the instance's
//! `models.json`, and its key is the one that file names: both stay on the
//! factory, and neither reaches a contractor. The proxy is the only process
//! that sees them, and it writes them into one request and forgets them.
//!
//! An http upstream is one TCP connection and two writes. A body is never
//! read: what the contractor sent goes from its socket to the endpoint's as it
//! arrives, and the answer comes back the same way, so a streamed completion
//! reaches the contractor in the chunks the endpoint sent them (rule 1) and no
//! byte of either is kept to be logged (rule 6).

use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

/// How long the endpoint has to accept the connection.
pub const CONNECT_WAIT: Duration = Duration::from_secs(15);
/// How long the endpoint has to start its answer.
pub const HEAD_WAIT: Duration = Duration::from_secs(30);
/// How long one write to the endpoint may take, including the copy of a
/// contractor's prompt on to it. The prompt is the body of a cycle's ask, and a
/// model that reads it before it answers is no slower than this.
pub const BODY_WAIT: Duration = Duration::from_secs(120);
/// The longest response head the proxy reads. A completion's body is not part
/// of a head, and a head of more than this is no answer a server means.
pub const HEAD_MAX: usize = 64 * 1024;

/// Where one request goes: the endpoint of the provider the instance named,
/// as its `baseUrl` writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upstream {
    pub host: String,
    pub port: u16,
    /// The base path of the address: the `/v1` of `http://host:3000/v1`.
    pub base: String,
}

/// What one request of a contractor asks the endpoint for: the line the proxy
/// writes, the bytes it sends with it, and whether the answer is a stream. The
/// ask is one value, since the things of a request belong to one request and
/// are read together.
#[derive(Debug, Clone)]
pub struct Ask {
    pub method: String,
    /// The path as the contractor sent it, without its query.
    pub path: String,
    /// The contractor's own fields, less its request line and the ones the
    /// proxy owns: see [`client_head`]. What the contractor asked for is asked
    /// of the endpoint unchanged, so an endpoint that decides a stream from the
    /// fields it is given decides it as the contractor meant.
    pub head: Vec<u8>,
    /// What came after the blank line: the start of the ask's JSON, the part of
    /// the body that had already left the contractor's socket when the proxy
    /// read its head. The rest of the body is copied from that socket straight
    /// to the endpoint's by [`ask`], so the prompt of a cycle is never a buffer
    /// of the proxy's.
    pub body: Vec<u8>,
    /// The whole length of the body, as the contractor's own content-length
    /// named it: what the endpoint is told to expect, and what the copy of the
    /// body ends at.
    pub length: usize,
    /// Whether the contractor asked for a stream, which says how the answer
    /// must be carried back.
    pub streaming: bool,
}

impl Ask {
    pub fn new(
        method: &str,
        path: &str,
        head: &[u8],
        body: &[u8],
        length: usize,
        streaming: bool,
    ) -> Ask {
        Ask {
            method: method.to_string(),
            path: path.to_string(),
            head: head.to_vec(),
            body: body.to_vec(),
            length,
            streaming,
        }
    }
}

/// Whether an address is one this proxy can speak to. Only plain http: TLS,
/// certificates and the join handshake are #2274's, and version 1 of the
/// feature is for hosts the owner trusts. An https endpoint is named at the
/// proxy's start, and the proxy refuses to listen rather than answer 502 to
/// every cycle of every contractor.
pub fn speaks(address: &str) -> bool {
    Upstream::of(address).is_some()
}

impl Upstream {
    /// The host, port and base path of an `http://` address. The address comes
    /// from the instance's `models.json`, which `tehas setup` and install.sh
    /// write. Anything else is None: a proxy that cannot tell where a request
    /// goes says so before it listens.
    pub fn of(address: &str) -> Option<Upstream> {
        let rest = address.strip_prefix("http://")?;
        let (authority, base) = match rest.find('/') {
            Some(i) => (&rest[..i], rest[i..].to_string()),
            None => (rest, String::new()),
        };
        if authority.is_empty() || authority.contains('@') {
            return None;
        }
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) => (h, p.parse::<u16>().ok().filter(|p| *p != 0)?),
            None => (authority, 80),
        };
        if host.is_empty()
            || !host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-[_:]".contains(&b))
        {
            return None;
        }
        let base = base.trim_end_matches('/').to_string();
        if base.contains([' ', '\r', '\n']) {
            return None;
        }
        Some(Upstream {
            host: host.to_string(),
            port,
            base,
        })
    }

    /// The endpoint as a person writes it, for a message about it.
    pub fn address(&self) -> String {
        format!("http://{}:{}{}", self.host, self.port, self.base)
    }

    fn reach(&self) -> Result<TcpStream, String> {
        let addrs = (self.host.as_str(), self.port)
            .to_socket_addrs()
            .map_err(|e| format!("{}: could not be resolved: {e}", self.host))?
            .collect::<Vec<_>>();
        if addrs.is_empty() {
            return Err(format!("{}: resolved to no address", self.host));
        }
        let mut last = format!("{}: resolved to no address", self.host);
        for addr in addrs {
            match TcpStream::connect_timeout(&addr, CONNECT_WAIT) {
                Ok(s) => return Ok(s),
                Err(e) => last = format!("{}: could not be reached: {e}", self.host),
            }
        }
        Err(last)
    }
}

/// What the upstream answered: its status, the bytes of its head and of the
/// body that came with it, and the stream the rest of the answer is on.
#[derive(Debug)]
pub struct Answer {
    pub status: u16,
    pub stream: TcpStream,
    /// The answer's head, the blank line included, and the start of its body.
    pub got: Vec<u8>,
    /// Just past the blank line of the head: where the body begins in `got`.
    pub body: usize,
}

/// The request line and the fields the proxy owns: the path under the
/// endpoint's own base, the endpoint's Host, the endpoint's key as the bearer
/// word Pi sends a provider's `apiKey` in, and the length of the body the proxy
/// sends. `length` is the ask the proxy read of the contractor's body, or None
/// to send none: a refusal is answered before the endpoint is asked, and a body
/// is never forwarded in pieces the endpoint cannot count. The contractor's own
/// token is dropped with [`client_head`], so the endpoint answers the factory
/// and never learns that a contractor asked.
pub fn head(
    method: &str,
    path: &str,
    upstream: &Upstream,
    key: &str,
    length: Option<usize>,
) -> String {
    format!(
        "{method} {}{} HTTP/1.1\r\nHost: {}:{}\r\nAuthorization: Bearer {key}\r\nAccept: */*\r\nConnection: close\r\ncontent-length: {}\r\n",
        upstream.base,
        path,
        upstream.host,
        upstream.port,
        length.unwrap_or(0)
    )
}

/// The fields the contractor sent, less its request line and the fields the
/// proxy writes itself: its `Host` and its job token go, since the endpoint is
/// to answer the factory and never learn that a contractor asked. Its own
/// `accept` is what says whether it wants an event stream, and it goes.
pub fn client_head(head: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(head);
    let mut out = String::new();
    for line in text.lines().skip(1) {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            break;
        }
        let name = line
            .split(':')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if !matches!(
            name.as_str(),
            "host"
                | "authorization"
                | "connection"
                | "proxy-connection"
                | "keep-alive"
                | "content-length"
        ) {
            out.push_str(line);
            out.push_str("\r\n");
        }
    }
    out.push_str("\r\n");
    out.into_bytes()
}

/// The end of the head in `buf`, just past the blank line.
pub fn head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

/// The status of a response head: 429 of `HTTP/1.1 429 Too Many Requests`.
/// None for a head that says no status at all, which is answered 502 and is
/// never passed on as if it were the endpoint's own answer.
pub fn status_of(head: &[u8]) -> Option<u16> {
    let text = String::from_utf8_lossy(head);
    let first = text.split("\r\n").next()?;
    let mut words = first.split(' ');
    words.next()?.strip_prefix("HTTP/")?;
    let code: u16 = words.next()?.parse().ok()?;
    (100..1000).contains(&code).then_some(code)
}

/// One connection to the endpoint: the request written on it and the head of
/// the answer read, with the body that came with it. `ask.body` is what had
/// already come off the contractor's socket when the proxy read its head; the
/// rest of the body goes on from that socket to this one by [`relay`], before
/// the endpoint is asked for its answer, so the proxy holds no bound on how
/// long a contractor takes to send a prompt and keeps none of the prompt.
pub fn ask(
    endpoint: &Upstream,
    key: &str,
    idle: Duration,
    ask: &Ask,
    client: &mut TcpStream,
) -> Result<Answer, String> {
    let began = Instant::now();
    let mut upstream = endpoint.reach()?;
    // The contractor's own request line goes, which asks the endpoint for a
    // stream, and the proxy's fields come first so they are the ones an
    // endpoint reads. What the contractor sent for `Host` and `Authorization`
    // is dropped, and its `content-length` is left to the proxy's own, which
    // is the length the contractor named rather than the part of it that had
    // arrived.
    let sent = [
        self::head(&ask.method, &ask.path, endpoint, key, Some(ask.length)).into_bytes(),
        client_head(&ask.head),
        ask.body.to_vec(),
    ]
    .concat();
    upstream
        .set_write_timeout(Some(HEAD_WAIT))
        .map_err(|e| format!("the request could not be set going: {e}"))?;
    upstream
        .write_all(&sent)
        .map_err(|e| format!("the request could not be sent on: {e}"))?;
    // The rest of the prompt, from the contractor's socket to this one, as it
    // arrives. An endpoint that reads the body it was promised before it
    // answers is answered here: a proxy that asked and then waited on a socket
    // it had not drained would hold every hand it has.
    let left = ask.length.saturating_sub(ask.body.len());
    relay(client, &mut upstream, left)
        .map_err(|e| format!("the prompt could not be sent on: {e}"))?;
    // The reads that follow are of an answer that is coming, not of a client
    // that must be cut off: the instance's idle bound, so a model that thinks
    // a while before its first token is no stalled connection.
    let _ = upstream.set_read_timeout(Some(idle));
    let mut got = Vec::new();
    let mut one = [0u8; 1];
    loop {
        if let Some(body) = head_end(&got) {
            let status = status_of(&got[..body])
                .ok_or_else(|| format!("{}: the endpoint answered no status", endpoint.host))?;
            // What comes in the same breath as the head arrives too: a body
            // of a length the head names is read here, so the whole of a
            // completion is in `got`, while a stream of no length waits on its
            // own for the first chunk. `upstream` is left where the answer is.
            let mut answer = Answer {
                status,
                stream: upstream,
                got,
                body,
            };
            // A stream is ended, so nothing of it was read past the head:
            // the proxy will write the head and copy the rest. Otherwise the
            // answer is a completion of a length, read here to its end so it
            // is whole before it is forwarded, which is also what lets its
            // token figures be counted from it.
            // A stream is ended by the endpoint, so nothing of it is read past
            // its head: the proxy writes the head and copies the rest. An
            // answer of a length is read here to its end, so it is whole before
            // it is forwarded, which is what lets its figures be counted.
            if !ask.streaming && content_length(&answer.got[..body]).is_some() {
                read_until_len(&mut answer).map_err(|why| format!("{}: {why}", endpoint.host))?;
            }
            return Ok(answer);
        }
        if got.len() > HEAD_MAX {
            return Err(format!(
                "{}: the endpoint's head is over {} KiB after {} ms",
                endpoint.host,
                HEAD_MAX / 1024,
                began.elapsed().as_millis()
            ));
        }
        // One byte a read, which is slow and short: a head is a line and a
        // few fields. It is what leaves the answer's own bytes on the socket
        // for the caller to copy, rather than behind a boundary the proxy
        // would have to remember.
        match upstream.read(&mut one) {
            Ok(0) if got.is_empty() => {
                return Err(format!("{}: closed without an answer", endpoint.host))
            }
            Ok(0) => return Err(format!("{}: its head has no blank line", endpoint.host)),
            Err(e) => return Err(format!("{}: answered nothing in time: {e}", endpoint.host)),
            Ok(n) => got.extend_from_slice(&one[..n]),
        }
    }
}

/// The rest of a contractor's prompt, from its socket to the endpoint's, as it
/// arrives: `want` bytes of it, which is what the contractor's own
/// content-length promised and the head's copy did not cover. Nothing of it is
/// kept, and no wait is longer than [`BODY_WAIT`] between two reads, so a slow
/// contractor makes a slow request and never a proxy that holds one back or
/// fills its memory with a prompt.
fn relay(client: &mut TcpStream, upstream: &mut TcpStream, want: usize) -> io::Result<()> {
    let _ = client.set_read_timeout(Some(BODY_WAIT));
    let mut left = want;
    let mut chunk = [0u8; 8192];
    while left > 0 {
        let take = left.min(chunk.len());
        match client.read(&mut chunk[..take]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!("{left} bytes of it never came"),
                ))
            }
            Err(e) => return Err(e),
            Ok(n) => {
                upstream.write_all(&chunk[..n])?;
                left -= n;
            }
        }
    }
    upstream.flush()?;
    Ok(())
}

/// The bytes of a body whose length the head names, which is the whole of a
/// completion's answer: it must be read before the answer is forwarded, since
/// the contractor's copy of the answer ends where its own body does.
fn read_until_len(answer: &mut Answer) -> Result<(), String> {
    let want = content_length(&answer.got[..answer.body]).unwrap_or_default();
    let mut chunk = [0u8; 8192];
    while answer.got.len() < want {
        match answer.stream.read(&mut chunk) {
            Ok(0) => {
                return Err(format!(
                    "its body ended at {} of {} bytes",
                    answer.got.len() - answer.body,
                    want - answer.body
                ))
            }
            Err(e) => {
                return Err(format!(
                    "its body stopped after {} bytes: {e}",
                    answer.got.len() - answer.body
                ))
            }
            Ok(n) => answer.got.extend_from_slice(&chunk[..n]),
        }
    }
    Ok(())
}

/// The length the head names, None for an answer that gives none and so keeps
/// its own end: a stream, whose chunks the proxy must pass on as they come.
pub fn content_length(head: &[u8]) -> Option<usize> {
    let text = String::from_utf8_lossy(head).into_owned();
    let mut len = None;
    for line in text.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("content-length") {
            let value = value.trim();
            // A chunked answer names no length: it is a stream, and the proxy
            // must pass its chunks on rather than read them to a total.
            len = if value.eq_ignore_ascii_case("chunked") {
                None
            } else {
                value.parse().ok()
            };
        }
    }
    len
}

/// How much of the end of an answer's body [`pipe`] keeps: the endpoint's
/// figures ride in the last chunk of a stream, or are the whole of a short
/// completion. Nothing before this is held, however long the answer runs.
pub const TAIL: usize = 64 * 1024;

/// The answer's bytes from where the caller left them to the contractor, one
/// chunk at a time: write what was read, then read the next. This is the whole
/// of the proxy's part in a stream, and it is why a chunk reaches the
/// contractor before the endpoint has sent the one after it. `from` is what
/// was already read, its body starting at `body`. The last [`TAIL`] bytes of
/// the body are returned, for the figures the endpoint gave and nothing else.
pub fn pipe(
    upstream: &mut TcpStream,
    client: &mut TcpStream,
    from: &[u8],
    body: usize,
) -> io::Result<Vec<u8>> {
    let mut buf = [0u8; 16 * 1024];
    let mut tail = Vec::new();
    client.write_all(from)?;
    keep_tail(&mut tail, from.get(body..).unwrap_or_default());
    loop {
        let n = upstream.read(&mut buf)?;
        if n == 0 {
            break;
        }
        client.write_all(&buf[..n])?;
        keep_tail(&mut tail, &buf[..n]);
    }
    client.flush()?;
    Ok(tail)
}

fn keep_tail(tail: &mut Vec<u8>, more: &[u8]) {
    tail.extend_from_slice(more);
    if tail.len() > TAIL {
        tail.drain(..tail.len() - TAIL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex, PoisonError};
    use std::thread::{self, Builder};

    /// A connected pair of sockets, the one end a contractor's: what the proxy
    /// of a test copies a prompt's rest from.
    fn pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let at = listener.local_addr().unwrap();
        let client = TcpStream::connect(at).unwrap();
        let (other, _) = listener.accept().unwrap();
        drop(listener);
        (other, client)
    }

    #[test]
    fn an_http_address_of_a_host_and_a_port_is_an_endpoint() {
        for (address, host, port, base) in [
            ("http://127.0.0.1:3000", "127.0.0.1", 3000, ""),
            ("http://llm.example/v1", "llm.example", 80, "/v1"),
            ("http://llm.example:8000/v1/", "llm.example", 8000, "/v1"),
            ("http://localhost", "localhost", 80, ""),
            ("http://[::1]:3000/v1", "[::1]", 3000, "/v1"),
        ] {
            assert_eq!(
                Upstream::of(address),
                Some(Upstream {
                    host: host.to_string(),
                    port,
                    base: base.to_string()
                }),
                "{address}"
            );
        }
    }

    #[test]
    fn an_address_this_proxy_cannot_speak_to_is_none() {
        for address in [
            "https://llm.example/v1",
            "llm.example/v1",
            "http://",
            "http:///v1",
            "http://user@llm.example/v1",
            "http://llm.example:0",
            "http://llm.example:notaport",
            "http://llm.example:65536",
            "http://llm.example/v1 two",
            "ftp://llm.example",
            "",
        ] {
            assert_eq!(Upstream::of(address), None, "{address}");
            assert!(!speaks(address), "{address} is no http address");
        }
        assert!(speaks("http://127.0.0.1:3000/v1"));
    }

    #[test]
    fn the_request_line_and_the_five_fields_are_the_proxy_s_own() {
        let up = Upstream::of("http://llm.example:3000/v1").unwrap();
        assert_eq!(
            head("POST", "/chat/completions", &up, "sk-secret", Some(5)),
            "POST /v1/chat/completions HTTP/1.1\r\nHost: llm.example:3000\r\n\
             Authorization: Bearer sk-secret\r\nAccept: */*\r\nConnection: close\r\ncontent-length: 5\r\n"
        );
        // A request that sends no body says so, rather than name a length the
        // endpoint would wait for.
        assert!(head("GET", "/models", &up, "k", None).ends_with("content-length: 0\r\n"));
    }

    #[test]
    fn the_contractor_s_head_goes_on_except_the_fields_the_proxy_owns() {
        let head = b"POST /v1/chat/completions HTTP/1.1\r\n\
                     Host: proxy:8090\r\n\
                     Authorization: Bearer job-token\r\n\
                     Content-Type: application/json\r\n\
                     Content-Length: 12\r\n\
                     Accept: text/event-stream\r\n\
                     Connection: keep-alive\r\n\r\n{}";
        assert_eq!(
            String::from_utf8(client_head(head)).unwrap(),
            "Content-Type: application/json\r\nAccept: text/event-stream\r\n\r\n"
        );
    }

    #[test]
    fn the_status_of_a_head_is_the_endpoint_s_own_number() {
        for (head, status) in [
            ("HTTP/1.1 429 Too Many Requests\r\n\r\n", 429),
            ("HTTP/1.0 500 Oops\r\n\r\n", 500),
            ("HTTP/1.1 200 OK\r\n\r\n", 200),
        ] {
            assert_eq!(status_of(head.as_bytes()), Some(status), "{head}");
        }
        for head in [
            "",
            "\r\n",
            "HTTP/1.1\r\n",
            "GET / HTTP/1.1\r\n",
            "HTTP/1.1 x\r\n",
        ] {
            assert_eq!(status_of(head.as_bytes()), None, "{head:?}");
        }
    }

    // --- the endpoint of a test ---------------------------------------------

    /// The hands of one stub: the number of connections it answers at once,
    /// counted so a stub that holds a chunk holds nothing else.
    #[derive(Clone)]
    struct Pool(Arc<Mutex<usize>>);

    struct Ticket(Arc<Mutex<usize>>);

    impl Pool {
        fn new(hands: usize) -> Pool {
            Pool(Arc::new(Mutex::new(hands.max(1))))
        }
        /// A place among the hands, or None when they are all busy. The
        /// connection is then left open unanswered, which is what a caller
        /// over the proxy's own cap is answered with on a host.
        fn take(&self) -> Option<Ticket> {
            let mut held = self.0.lock().unwrap_or_else(PoisonError::into_inner);
            (*held > 0).then(|| {
                *held -= 1;
                Ticket(Arc::clone(&self.0))
            })
        }
    }

    impl Drop for Ticket {
        fn drop(&mut self) {
            *self.0.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        }
    }

    /// What the stub of an endpoint was asked, and how many times. The count
    /// is how a refusal that made no upstream call is proved: a test that only
    /// read the status would pass on a proxy that called and threw the answer
    /// away.
    #[derive(Default, Clone)]
    pub struct Seen {
        pub calls: usize,
        pub bytes: Vec<u8>,
    }

    /// The answer of the stub: one head and the chunks of its body, each
    /// written and flushed before the next is written, with a wait before any
    /// but the first.
    struct Stub {
        pub at: std::net::SocketAddr,
        seen: Arc<Mutex<Seen>>,
    }

    struct Plan {
        head: String,
        chunks: Vec<String>,
        before: Vec<Duration>,
        /// Whether the head is a stream's, of no length the reader can
        /// predict: what follows it is written chunk by chunk.
        /// Answer nothing at all, and hold the connection: the idle bound is
        /// what ends the request.
        quiet: bool,
    }

    impl Stub {
        /// A busy endpoint: 429 and its own body, which is what the agents'
        /// retry of a busy model (#215) must go on reading.
        pub fn busy() -> Stub {
            Stub::of(
                "HTTP/1.1 429 Too Many Requests\r\ncontent-type: application/json\r\n\r\n",
                &[r#"{"error":{"type":"rate_limit","code":429}}"#],
            )
        }

        /// An answer of one head and a body of a length, written at once.
        pub fn of(head: &str, body: &[&str]) -> Stub {
            Stub::planned(
                Plan {
                    head: head.to_string(),
                    chunks: body.iter().map(|c| c.to_string()).collect(),
                    before: Vec::new(),
                    quiet: false,
                },
                2,
            )
        }

        /// A stream of two chunks, the second after `between`, with no length
        /// the reader can predict and no end until the endpoint chooses it.
        /// A proxy that buffered the answer would show the reader nothing until
        /// the second chunk had come.
        pub fn streaming(first: &str, second: &str, between: Duration) -> Stub {
            Stub::planned(
                Plan {
                    head: "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\r\n".to_string(),
                    chunks: vec![first.to_string(), second.to_string()],
                    before: vec![Duration::ZERO, between],
                    quiet: false,
                },
                2,
            )
        }

        /// An endpoint that accepts the request and answers nothing: the idle
        /// bound of the instance is what ends it.
        pub fn silent() -> Stub {
            Stub::planned(
                Plan {
                    head: String::new(),
                    chunks: Vec::new(),
                    before: Vec::new(),
                    quiet: true,
                },
                4,
            )
        }

        /// `hands` connections may be open at once, each answered by a thread
        /// of its own: a stub that holds a chunk holds nothing else.
        fn planned(plan: Plan, hands: usize) -> Stub {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let at = listener.local_addr().unwrap();
            let plan = Arc::new(Mutex::new(plan));
            let seen = Arc::new(Mutex::new(Seen::default()));
            let (worker_plan, worker_seen) = (Arc::clone(&plan), Arc::clone(&seen));
            let worker = Builder::new()
                .spawn(move || {
                    // One ticket a connection: a stub that holds a chunk for
                    // a second holds no more of the pool than that one stream.
                    let tickets = Pool::new(hands.max(1));
                    for stream in listener.incoming() {
                        let Ok(stream) = stream else { continue };
                        let (plan, seen) = (Arc::clone(&worker_plan), Arc::clone(&worker_seen));
                        let Some(ticket) = tickets.take() else {
                            continue;
                        };
                        Builder::new()
                            .stack_size(128 * 1024)
                            .spawn(move || {
                                answer(stream, &plan, &seen);
                                drop(ticket);
                            })
                            .ok();
                    }
                })
                .expect("a thread for the stub endpoint");
            // The listener lives as long as the process does, so a test's stub
            // is gone when the process is: nothing joins it, and no port is
            // held past the run.
            std::mem::forget(worker);
            Stub { at, seen }
        }

        /// Where a request of this stub goes: the same host, the stub's own
        /// port, and no base path of the endpoint.
        pub fn upstream(&self) -> Upstream {
            Upstream {
                host: "127.0.0.1".to_string(),
                port: self.at.port(),
                base: String::new(),
            }
        }

        /// How many times the endpoint was asked, and what it was sent.
        pub fn seen(&self) -> Seen {
            self.seen
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }
    }

    /// One connection of the stub: read what it was sent, then answer.
    fn answer(mut stream: TcpStream, plan: &Mutex<Plan>, seen: &Mutex<Seen>) {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut got = Vec::new();
        let mut chunk = [0u8; 4096];
        // The head, then whatever of the body arrives: the stub keeps it all,
        // so a test can read back what the endpoint was sent.
        while head_end(&got).is_none() {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => got.extend_from_slice(&chunk[..n]),
                Err(_) => return,
            }
        }
        // The body, as far as it comes: the stub does not wait for a client
        // that has more to send, and reads only until the stream goes quiet.
        // What the proxy writes first is a head and the ask of a whole prompt,
        // and a stub that waited for the end of the prompt would be a stub of
        // a client that never sends one.
        let _ = stream.set_read_timeout(Some(Duration::from_millis(150)));
        loop {
            match stream.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => got.extend_from_slice(&chunk[..n]),
            }
        }
        {
            let mut held = seen.lock().unwrap_or_else(PoisonError::into_inner);
            held.calls += 1;
            held.bytes = got;
        }
        let plan = plan.lock().unwrap_or_else(PoisonError::into_inner);
        if plan.quiet {
            drop(plan);
            thread::sleep(Duration::from_secs(30));
            return;
        }
        let _ = stream.write_all(plan.head.as_bytes());
        for (n, part) in plan.chunks.iter().enumerate() {
            if let Some(wait) = plan.before.get(n).copied().filter(|w| !w.is_zero()) {
                thread::sleep(wait);
            }
            let _ = stream.write_all(part.as_bytes());
            let _ = stream.flush();
        }
        let _ = stream.flush();
    }

    fn ask(stub: &Stub, idle: Duration) -> Result<Answer, String> {
        ask_of(stub, idle, false)
    }

    fn ask_of(stub: &Stub, idle: Duration, streaming: bool) -> Result<Answer, String> {
        // The ask's body is whole and its length is the body's own, so the copy
        // of the rest has nothing to read and the proxy asks at once. The
        // contractor's socket is a pair's end that sends nothing more.
        let (_kept, client) = pair();
        super::ask(
            &stub.upstream(),
            "sk-upstream",
            idle,
            &Ask::new(
                "POST",
                "/chat/completions",
                b"POST /v1/chat/completions HTTP/1.1\r\n",
                b"content-type: application/json\r\ncontent-length: 5\r\n\r\nhello",
                5,
                streaming,
            ),
            &mut client.try_clone().unwrap(),
        )
    }

    #[test]
    fn a_busy_answer_keeps_its_status_and_its_body() {
        let stub = Stub::busy();
        let answer = ask(&stub, Duration::from_secs(5)).unwrap();
        assert_eq!(answer.status, 429, "a busy endpoint was rewritten");
        // The head is what the proxy read of the answer; the body of an
        // answer of a length is read too, so the whole of the endpoint's
        // reply is what the contractor is given and not a 502 of the proxy's.
        let said = format!(
            "{}{}",
            String::from_utf8_lossy(&answer.got),
            String::from_utf8_lossy(&copied(answer.stream))
        );
        // The body of a busy answer arrives with the head, since the answer
        // that was written at once cannot be told from one that comes by
        // chunks: the proxy passes the whole of it on.
        assert!(
            said.starts_with("HTTP/1.1 429 Too Many Requests\r\n"),
            "{said}"
        );
        assert!(
            said.ends_with(r#"{"error":{"type":"rate_limit","code":429}}"#),
            "{said}"
        );
        assert_eq!(stub.seen().calls, 1, "one call for one ask");
    }

    #[test]
    fn the_endpoint_sees_its_own_key_and_never_the_job_s_token() {
        let stub = Stub::of("HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n", &["ok"]);
        let answer = ask(&stub, Duration::from_secs(5)).unwrap();
        assert_eq!(answer.status, 200);
        let seen = String::from_utf8_lossy(&stub.seen().bytes).into_owned();
        assert!(seen.contains("Authorization: Bearer sk-upstream"), "{seen}");
        assert!(!seen.contains("job-token"), "{seen}");
        assert!(seen.contains("Host: 127.0.0.1:"), "{seen}");
        assert!(seen.contains("content-length: 5"), "{seen}");
        assert!(seen.ends_with("hello"), "{seen}");
    }

    #[test]
    fn only_the_tail_of_a_long_answer_is_kept() {
        let mut tail = Vec::new();
        keep_tail(&mut tail, &vec![b'a'; TAIL]);
        keep_tail(&mut tail, b"last");
        assert_eq!(tail.len(), TAIL);
        assert!(tail.ends_with(b"last"));
        assert!(tail.starts_with(b"a"));
    }

    #[test]
    fn a_stream_reaches_the_reader_chunk_by_chunk_and_not_at_the_end() {
        // The endpoint sends the first chunk at once and the second half a
        // second later. A proxy that buffered the answer would give the reader
        // nothing until the second had come.
        let stub = Stub::streaming(
            "data: {\"choices\":[{\"delta\":{\"content\":\"one\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"two\"}}]}\n\n",
            Duration::from_millis(700),
        );
        let mut answer = ask_of(&stub, Duration::from_secs(5), true).unwrap();
        assert_eq!(answer.status, 200);
        assert_eq!(
            String::from_utf8_lossy(&answer.got[answer.body..]),
            "",
            "the head is all of a stream that the proxy reads before it writes"
        );
        let began = Instant::now();
        let first = read_some(&mut answer.stream, Duration::from_millis(300));
        let waited = began.elapsed();
        assert!(
            first.contains("one"),
            "the first chunk did not arrive: {first:?}"
        );
        assert!(
            !first.contains("two"),
            "both chunks came at once: {first:?}"
        );
        assert!(
            waited < Duration::from_millis(600),
            "the first chunk took {waited:?}, which is the second's wait"
        );
        // And the rest of it comes to the one who waits for the end: the
        // first chunk was read above, so what follows is the second's.
        let mut said = first;
        said.push_str(&read_some(&mut answer.stream, Duration::from_millis(900)));
        assert!(said.contains("one") && said.contains("two"), "{said}");
    }

    /// What the endpoint said, with the proxy's own copy running over a
    /// connected pair of sockets: the writer end is what the contractor holds
    /// and the reader what `pipe` writes to, so the test times the bytes at
    /// the end a contractor would.
    /// What the contractor is left holding: pipe's own copy of the answer, run
    /// over a connected pair of sockets so the test reads the contractor's end.
    fn copied(stream: TcpStream) -> Vec<u8> {
        let (mut reader, mut client) = os_pipe_pair().expect("a pair for the answer");
        thread::spawn(move || {
            let mut up = stream;
            let _ = super::pipe(&mut up, &mut client, &[], 0);
        });
        let mut got = Vec::new();
        let _ = reader.set_read_timeout(Some(Duration::from_secs(5)));
        let mut chunk = [0u8; 4096];
        while let Ok(n) = reader.read(&mut chunk) {
            if n == 0 {
                break;
            }
            got.extend_from_slice(&chunk[..n]);
        }
        got
    }

    /// A connected pair of sockets: the first end is read by the test, the
    /// second is what pipe is given as the client.
    fn os_pipe_pair() -> io::Result<(TcpStream, TcpStream)> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let at = listener.local_addr()?;
        let client = TcpStream::connect(at)?;
        let (reader, _) = listener.accept()?;
        drop(listener);
        reader.set_nodelay(true)?;
        client.set_nodelay(true)?;
        Ok((reader, client))
    }

    /// What a stream says within the time given, stopping at the first chunk
    /// that carries content.
    fn read_some(stream: &mut TcpStream, within: Duration) -> String {
        let mut got = Vec::new();
        let _ = stream.set_read_timeout(Some(within));
        let mut chunk = [0u8; 4096];
        while let Ok(n) = stream.read(&mut chunk) {
            if n == 0 {
                break;
            }
            got.extend_from_slice(&chunk[..n]);
            if got.windows(5).any(|w| w == b"\"one\"") {
                break;
            }
        }
        String::from_utf8_lossy(&got).into_owned()
    }

    #[test]
    fn an_endpoint_that_answers_nothing_is_told_in_the_words_of_the_bound() {
        let stub = Stub::silent();
        let why = ask(&stub, Duration::from_millis(300)).unwrap_err();
        assert!(why.contains("answered nothing in time"), "{why}");
        assert_eq!(stub.seen().calls, 1);
    }

    #[test]
    fn a_refused_connection_is_said_as_the_endpoint_being_unreachable() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let up = Upstream {
            host: "127.0.0.1".to_string(),
            port: listener.local_addr().unwrap().port(),
            base: String::new(),
        };
        drop(listener);
        let (_, mut client) = pair();
        let why = super::ask(
            &up,
            "zebra",
            Duration::from_secs(1),
            &Ask::new(
                "GET",
                "/models",
                b"GET /v1/models HTTP/1.1\r\n",
                b"content-length: 0\r\n\r\n",
                0,
                false,
            ),
            &mut client,
        )
        .unwrap_err();
        assert!(why.contains("could not be reached"), "{why}");
        // The line names the host and the failure, and never the key it was
        // going to send.
        assert!(!why.contains("zebra"), "{why}");
    }
}

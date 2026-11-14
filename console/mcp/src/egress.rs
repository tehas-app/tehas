//! The egress proxy of the sandbox containers (`tehas-mcp egress`): a forward
//! proxy that connects only to the host and port pairs it was given with
//! `--allow`. It shares this binary because the binary is static and already
//! built, installed and run in a pinned image; the MCP server's code and this
//! code share nothing else.
//!
//! A sandbox container sits on an internal network whose one way out is this
//! proxy, and is given `HTTPS_PROXY` and `HTTP_PROXY`. A tool that ignores
//! them reaches nothing. Two forms are served:
//!
//!   CONNECT host:port   a tunnel, for https: git, gh, npm, cargo, the models
//!   GET http://host/... a plain request, for a model endpoint on http
//!
//! Anything else, and any pair not allowed, is answered 403 and closed, with
//! one line on stderr naming the pair. A name is matched in any case and
//! without a trailing dot. An address is never looked up to decide: the list
//! names hosts, and the name is what is compared.

use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// The longest request head read before the request is refused.
const HEAD_MAX: usize = 16 * 1024;
/// How long a client has to send its head, and the proxy to reach the host.
const HEAD_WAIT: Duration = Duration::from_secs(30);
const CONNECT_WAIT: Duration = Duration::from_secs(15);
/// Connections at once. Each holds two threads, and the container allows 512
/// processes.
pub const MAX_CONNECTIONS: usize = 200;

/// The pairs a proxy lets through: host in lower case, and port.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Allowed(Vec<(String, u16)>);

fn norm(host: &str) -> String {
    host.trim_end_matches('.').to_ascii_lowercase()
}

/// `host:port` as `--allow` and CONNECT write it. None for anything else: no
/// port, a port that is no number or 0, an empty host.
pub fn pair(word: &str) -> Option<(String, u16)> {
    let (host, port) = word.rsplit_once(':')?;
    let port: u16 = port.parse().ok().filter(|p| *p != 0)?;
    let host = norm(host);
    let ok = !host.is_empty()
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'));
    ok.then_some((host, port))
}

impl Allowed {
    /// The list from `--allow` words; Err names the first that is no pair.
    pub fn parse(words: &[String]) -> Result<Allowed, String> {
        words
            .iter()
            .map(|w| pair(w).ok_or_else(|| format!("--allow {w} is not host:port")))
            .collect::<Result<Vec<_>, _>>()
            .map(Allowed)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn allows(&self, host: &str, port: u16) -> bool {
        let host = norm(host);
        self.0.iter().any(|(h, p)| *h == host && *p == port)
    }
}

/// What a request head asks for.
#[derive(Debug, PartialEq)]
pub enum Ask {
    /// CONNECT host:port.
    Tunnel(String, u16),
    /// A plain request: host, port, and the head to send on, in origin form.
    Plain(String, u16, Vec<u8>),
    /// Anything this proxy does not serve, with the reason for the log.
    Other(String),
}

/// The end of the head in <buf>, just past the blank line.
fn head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

/// The ask of a whole head, the blank line included.
pub fn ask(head: &[u8]) -> Ask {
    let text = String::from_utf8_lossy(head);
    let mut lines = text.split("\r\n");
    let first = lines.next().unwrap_or("");
    let mut words = first.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (words.next(), words.next(), words.next(), words.next())
    else {
        return Ask::Other("a request line that is not three words".to_string());
    };
    if method == "CONNECT" {
        return match pair(target) {
            Some((host, port)) => Ask::Tunnel(host, port),
            None => Ask::Other(format!("CONNECT {target}")),
        };
    }
    let Some(rest) = target.strip_prefix("http://") else {
        return Ask::Other(format!("{method} {target}"));
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let with_port = if authority.contains(':') {
        authority.to_string()
    } else {
        format!("{authority}:80")
    };
    let Some((host, port)) = (!authority.contains('@'))
        .then(|| pair(&with_port))
        .flatten()
    else {
        return Ask::Other(format!("{method} {target}"));
    };
    // One request per connection: the server closes after its answer, so a
    // client does not send a request for another host down this tunnel. The
    // Host is the allowed one, never the client's, so a server with several
    // names serves only the one the list names.
    let mut out = format!("{method} {path} {version}\r\nHost: {authority}\r\n");
    for line in lines.filter(|l| !l.is_empty()) {
        let name = line
            .split(':')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if !matches!(
            name.as_str(),
            "connection" | "proxy-connection" | "proxy-authorization" | "keep-alive" | "host"
        ) {
            out.push_str(line);
            out.push_str("\r\n");
        }
    }
    out.push_str("Connection: close\r\n\r\n");
    Ask::Plain(host, port, out.into_bytes())
}

fn refuse(mut client: TcpStream, what: &str) {
    eprintln!("tehas-mcp egress: refused {what}");
    let body = format!("{what} is not on the egress allow-list of this tehas host\n");
    let _ = write!(
        client,
        "HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = client.shutdown(Shutdown::Both);
}

fn reach(host: &str, port: u16) -> io::Result<TcpStream> {
    let addrs = (host, port)
        .to_socket_addrs()
        .map_err(|e| io::Error::new(e.kind(), format!("the name was not resolved: {e}")))?;
    first_reached(addrs)
}

/// The first address that answers. A name that resolved to no address says
/// so, apart from a name that did not resolve: the remedies differ.
fn first_reached(addrs: impl Iterator<Item = std::net::SocketAddr>) -> io::Result<TcpStream> {
    let mut last = io::Error::new(io::ErrorKind::NotFound, "the name resolved to no address");
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, CONNECT_WAIT) {
            Ok(s) => return Ok(s),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// Why a client's head never came, for the log; None for a client that
/// connected and closed without a byte, which is a probe, not a request.
fn no_head(read: &io::Result<usize>, had: usize) -> Option<String> {
    match read {
        Ok(_) if had == 0 => None,
        Ok(_) => Some(format!("the client closed after {had} bytes of its head")),
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ) =>
        {
            Some(format!("no request head within {} s", HEAD_WAIT.as_secs()))
        }
        Err(e) => Some(format!("the head could not be read: {e}")),
    }
}

/// Bytes both ways until either side ends.
fn pipe(a: TcpStream, b: TcpStream, what: &str) {
    let (mut a2, mut b2) = match (a.try_clone(), b.try_clone()) {
        (Ok(a2), Ok(b2)) => (a2, b2),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("tehas-mcp egress: {what}: the connection was dropped: {e}");
            return;
        }
    };
    let up = thread::spawn(move || {
        let (mut a, mut b) = (a, b);
        let _ = io::copy(&mut a, &mut b);
        let _ = b.shutdown(Shutdown::Write);
    });
    let _ = io::copy(&mut b2, &mut a2);
    let _ = a2.shutdown(Shutdown::Write);
    let _ = up.join();
}

/// One client: its head, the decision, and then the bytes.
pub fn serve_one(mut client: TcpStream, allowed: &Allowed) {
    let _ = client.set_read_timeout(Some(HEAD_WAIT));
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let end = loop {
        if let Some(end) = head_end(&buf) {
            break end;
        }
        if buf.len() > HEAD_MAX {
            return refuse(client, "a request head over 16 KiB");
        }
        match client.read(&mut chunk) {
            Ok(n) if n > 0 => buf.extend_from_slice(&chunk[..n]),
            read => {
                if let Some(why) = no_head(&read, buf.len()) {
                    eprintln!("tehas-mcp egress: {why}");
                }
                return;
            }
        }
    };
    let (host, port, first) = match ask(&buf[..end]) {
        Ask::Tunnel(h, p) => (h, p, None),
        Ask::Plain(h, p, head) => (h, p, Some(head)),
        Ask::Other(what) => return refuse(client, &what),
    };
    if !allowed.allows(&host, port) {
        return refuse(client, &format!("{host}:{port}"));
    }
    let mut upstream = match reach(&host, port) {
        Ok(s) => s,
        Err(e) => {
            let told = client.write_all(
                b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
            let told = match told {
                Ok(()) => "the client was told 502".to_string(),
                Err(t) => format!("the client was gone before its 502: {t}"),
            };
            eprintln!("tehas-mcp egress: {host}:{port} could not be reached: {e}; {told}");
            return;
        }
    };
    let _ = client.set_read_timeout(None);
    let sent = match &first {
        None => client.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n"),
        Some(head) => upstream.write_all(head),
    };
    // What the client sent after its head is the start of the tunnel's bytes
    // or of the request's body.
    let what = format!("{host}:{port}");
    match sent.and_then(|_| upstream.write_all(&buf[end..])) {
        Ok(()) => pipe(client, upstream, &what),
        Err(e) => eprintln!("tehas-mcp egress: {what}: the request was dropped: {e}"),
    }
}

struct Slot(Arc<AtomicUsize>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Serve <listener> for good. A connection over the cap is answered 503.
pub fn serve(listener: TcpListener, allowed: Allowed) {
    let allowed = Arc::new(allowed);
    let active = Arc::new(AtomicUsize::new(0));
    // One line per run of failed accepts, so a storm of them (out of file
    // descriptors) is said once rather than once a connection.
    let mut failing = false;
    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => {
                failing = false;
                s
            }
            Err(e) => {
                if !failing {
                    eprintln!("tehas-mcp egress: a connection could not be accepted: {e}");
                }
                failing = true;
                continue;
            }
        };
        if active.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            active.fetch_sub(1, Ordering::SeqCst);
            let _ = stream.write_all(
                b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
            continue;
        }
        let slot = Slot(Arc::clone(&active));
        let allowed = Arc::clone(&allowed);
        thread::spawn(move || {
            let _slot = slot;
            serve_one(stream, &allowed);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed(words: &[&str]) -> Allowed {
        Allowed::parse(&words.iter().map(|w| w.to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn a_pair_is_a_host_and_a_port_that_is_a_number() {
        assert_eq!(pair("GitHub.com.:443"), Some(("github.com".into(), 443)));
        for bad in [
            "github.com",
            "github.com:",
            ":443",
            "h:0",
            "h:x",
            "h:70000",
            "a b:1",
        ] {
            assert_eq!(pair(bad), None, "{bad}");
        }
        assert!(Allowed::parse(&["h:1".into(), "nope".into()])
            .unwrap_err()
            .contains("nope"));
    }

    #[test]
    fn only_the_listed_host_on_the_listed_port_is_allowed() {
        let a = allowed(&["github.com:443", "models.example:8080"]);
        assert!(a.allows("github.com", 443));
        assert!(a.allows("GITHUB.COM.", 443));
        assert!(a.allows("models.example", 8080));
        assert!(!a.allows("github.com", 80));
        assert!(!a.allows("evil.github.com", 443));
        assert!(!a.allows("github.com.evil", 443));
        assert!(!a.allows("models.example", 443));
    }

    #[test]
    fn a_connect_is_a_tunnel_and_anything_odd_is_refused() {
        assert_eq!(
            ask(b"CONNECT github.com:443 HTTP/1.1\r\nHost: github.com:443\r\n\r\n"),
            Ask::Tunnel("github.com".into(), 443)
        );
        for odd in [
            &b"CONNECT github.com HTTP/1.1\r\n\r\n"[..],
            b"GET / HTTP/1.1\r\nHost: github.com\r\n\r\n",
            b"GET https://github.com/ HTTP/1.1\r\n\r\n",
            b"GET http://u@github.com/ HTTP/1.1\r\n\r\n",
            b"GET  http://github.com/ HTTP/1.1\r\n\r\n",
            b"\r\n\r\n",
        ] {
            assert!(matches!(ask(odd), Ask::Other(_)), "{odd:?}");
        }
    }

    #[test]
    fn a_plain_request_goes_on_in_origin_form_and_closes() {
        let head = b"POST http://models.example:8080/v1/chat HTTP/1.1\r\nHost: models.example:8080\r\nProxy-Connection: keep-alive\r\nConnection: keep-alive\r\nContent-Length: 2\r\n\r\n";
        let Ask::Plain(host, port, out) = ask(head) else {
            panic!("not plain");
        };
        assert_eq!((host.as_str(), port), ("models.example", 8080));
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "POST /v1/chat HTTP/1.1\r\nHost: models.example:8080\r\nContent-Length: 2\r\nConnection: close\r\n\r\n"
        );
        let Ask::Plain(_, port, out) = ask(b"GET http://m.example HTTP/1.1\r\n\r\n") else {
            panic!("not plain");
        };
        assert_eq!(port, 80);
        assert!(out.starts_with(b"GET / HTTP/1.1\r\nHost: m.example\r\n"));
    }

    #[test]
    fn a_plain_request_names_the_host_it_was_allowed_for() {
        let head = b"GET http://models.example:8080/v1 HTTP/1.1\r\nhost: other.example\r\nHost : again.example\r\nAccept: */*\r\n\r\n";
        let Ask::Plain(_, _, out) = ask(head) else {
            panic!("not plain");
        };
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "GET /v1 HTTP/1.1\r\nHost: models.example:8080\r\nAccept: */*\r\nConnection: close\r\n\r\n"
        );
    }

    #[test]
    fn a_head_that_never_came_is_said_and_a_bare_probe_is_not() {
        assert_eq!(no_head(&Ok(0), 0), None);
        assert_eq!(
            no_head(&Ok(0), 12).as_deref(),
            Some("the client closed after 12 bytes of its head")
        );
        let timed_out = Err(io::Error::from(io::ErrorKind::WouldBlock));
        assert_eq!(
            no_head(&timed_out, 3).as_deref(),
            Some("no request head within 30 s")
        );
        let reset = Err(io::Error::from(io::ErrorKind::ConnectionReset));
        assert!(no_head(&reset, 0)
            .unwrap()
            .starts_with("the head could not be read: "));
    }

    #[test]
    fn a_name_of_no_address_is_not_a_name_that_did_not_resolve() {
        let e = first_reached(std::iter::empty()).unwrap_err();
        assert_eq!(e.to_string(), "the name resolved to no address");
    }

    /// A proxy on a free port, allowed to reach <allowed>, and its address.
    fn proxy(allowed: Allowed) -> std::net::SocketAddr {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let at = l.local_addr().unwrap();
        thread::spawn(move || serve(l, allowed));
        at
    }

    fn said(at: std::net::SocketAddr, send: &[u8]) -> String {
        let mut c = TcpStream::connect(at).unwrap();
        c.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        c.write_all(send).unwrap();
        let mut got = Vec::new();
        let _ = c.read_to_end(&mut got);
        String::from_utf8_lossy(&got).into_owned()
    }

    #[test]
    fn a_tunnel_to_an_allowed_pair_carries_bytes_both_ways() {
        let echo = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = echo.local_addr().unwrap().port();
        thread::spawn(move || {
            let (mut s, _) = echo.accept().unwrap();
            let mut b = [0u8; 5];
            s.read_exact(&mut b).unwrap();
            s.write_all(&b).unwrap();
        });
        let at = proxy(allowed(&[&format!("localhost:{port}")]));
        let got = said(
            at,
            format!("CONNECT LOCALHOST:{port} HTTP/1.1\r\n\r\nhello").as_bytes(),
        );
        assert_eq!(got, "HTTP/1.1 200 Connection established\r\n\r\nhello");
    }

    #[test]
    fn a_pair_not_allowed_is_403_and_never_reached() {
        let other = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = other.local_addr().unwrap().port();
        other.set_nonblocking(true).unwrap();
        let at = proxy(allowed(&["localhost:1"]));
        let got = said(
            at,
            format!("CONNECT localhost:{port} HTTP/1.1\r\n\r\n").as_bytes(),
        );
        assert!(got.starts_with("HTTP/1.1 403 Forbidden\r\n"), "{got}");
        assert!(
            got.contains(&format!("localhost:{port} is not on")),
            "{got}"
        );
        let got = said(
            at,
            format!("GET http://127.0.0.1:{port}/ HTTP/1.1\r\n\r\n").as_bytes(),
        );
        assert!(got.starts_with("HTTP/1.1 403 Forbidden\r\n"), "{got}");
        assert!(
            other.accept().is_err(),
            "the proxy reached a pair it was not given"
        );
    }

    #[test]
    fn a_plain_request_reaches_its_host_and_its_answer_comes_back() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let seen = thread::spawn(move || {
            let (mut s, _) = server.accept().unwrap();
            let mut got = Vec::new();
            let mut b = [0u8; 1024];
            while head_end(&got).is_none() {
                let n = s.read(&mut b).unwrap();
                got.extend_from_slice(&b[..n]);
            }
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                .unwrap();
            String::from_utf8(got).unwrap()
        });
        let at = proxy(allowed(&[&format!("localhost:{port}")]));
        let got = said(
            at,
            format!("GET http://localhost:{port}/v1/models HTTP/1.1\r\nHost: localhost\r\n\r\n")
                .as_bytes(),
        );
        assert!(got.ends_with("\r\n\r\nok"), "{got}");
        assert_eq!(
            seen.join().unwrap(),
            format!(
                "GET /v1/models HTTP/1.1\r\nHost: localhost:{port}\r\nConnection: close\r\n\r\n"
            )
        );
    }
}

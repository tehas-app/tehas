//! The contractor's own way out for a job: a CONNECT proxy on 127.0.0.1 whose
//! every tunnel goes over the factory's egress route
//! (`/outsource/v1/egress`, proxy/src/egress.rs), with the job's model token.
//! The product's clone and the salvage push are made through it, so this
//! program's one connection for a job is the factory's listener.
//!
//! The factory holds each tunnel to its list. This end checks only that a
//! request is a CONNECT to a `host:port`, and passes the factory's refusal on
//! with its status and word.

use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::http;

/// The longest head of a CONNECT, and of the factory's answer.
const HEAD_MAX: usize = 16 * 1024;
const HEAD_WAIT: Duration = Duration::from_secs(30);
/// How long a tunnel may carry nothing either way: the factory's default
/// idle bound, which the factory holds it to as well.
pub const IDLE: Duration = Duration::from_secs(900);
/// Tunnels at once. git opens one at a time.
const MAX: usize = 16;

/// A running local end. Dropping it stops taking tunnels; one already open
/// runs to its end.
pub struct Local {
    pub addr: SocketAddr,
    stop: Arc<AtomicBool>,
}

impl Local {
    /// A local end on a free port of 127.0.0.1, for the factory at `factory`
    /// and the job's model token.
    pub fn start(factory: &str, token: &str) -> io::Result<Local> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let (factory, token) = (factory.to_string(), token.to_string());
        let stopped = Arc::clone(&stop);
        let open = Arc::new(AtomicUsize::new(0));
        thread::Builder::new()
            .stack_size(128 * 1024)
            .spawn(move || {
                for stream in listener.incoming() {
                    if stopped.load(Ordering::SeqCst) {
                        break;
                    }
                    let Ok(mut stream) = stream else { continue };
                    if open.fetch_add(1, Ordering::SeqCst) >= MAX {
                        open.fetch_sub(1, Ordering::SeqCst);
                        let _ = stream.write_all(
                            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        );
                        continue;
                    }
                    let (factory, token, open) =
                        (factory.clone(), token.clone(), Arc::clone(&open));
                    let spawned = thread::Builder::new()
                        .stack_size(128 * 1024)
                        .spawn(move || {
                            if let Err(why) = serve_one(stream, &factory, &token) {
                                eprintln!("tehas outsource contractor: egress: {why}");
                            }
                            open.fetch_sub(1, Ordering::SeqCst);
                        });
                    if spawned.is_err() {
                        eprintln!("tehas outsource contractor: egress: no thread for a tunnel");
                    }
                }
            })?;
        Ok(Local { addr, stop })
    }

    /// The address git and curl take as their proxy.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

impl Drop for Local {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wakes the accept, which then sees the stop.
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_secs(1));
    }
}

/// Read a head up to its blank line: the head, and what came after it.
fn read_head(s: &mut TcpStream) -> io::Result<(Vec<u8>, Vec<u8>)> {
    s.set_read_timeout(Some(HEAD_WAIT))?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let rest = buf.split_off(i + 4);
            return Ok((buf, rest));
        }
        if buf.len() > HEAD_MAX {
            return Err(io::Error::other("a head over 16 KiB"));
        }
        match s.read(&mut chunk)? {
            0 => return Err(io::Error::other("closed before its head ended")),
            n => buf.extend_from_slice(&chunk[..n]),
        }
    }
}

fn answer(client: &mut TcpStream, status: u16, word: &str) {
    let body = format!("{word}\n");
    let _ = write!(
        client,
        "HTTP/1.1 {status} Refused\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = client.shutdown(Shutdown::Both);
}

/// The host and port of a CONNECT head, or None.
pub fn connect_target(head: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(head).ok()?;
    let mut words = text.lines().next()?.split(' ');
    let (Some("CONNECT"), Some(target), Some(_), None) =
        (words.next(), words.next(), words.next(), words.next())
    else {
        return None;
    };
    let (host, port) = tehas_core::egress::pair(target)?;
    Some(format!("{host}:{port}"))
}

/// The Upgrade request of the factory's egress route.
pub fn upgrade_request(factory: &str, to: &str, token: &str) -> String {
    format!(
        "GET {}egress?to={to} HTTP/1.1\r\nhost: {}\r\nconnection: Upgrade\r\nupgrade: tehas-egress\r\nauthorization: Bearer {token}\r\n\r\n",
        http::PREFIX,
        http::host_port(factory)
    )
}

/// One CONNECT: the tunnel asked of the factory, then the bytes. Err is what
/// to log; it names the destination and never the token.
fn serve_one(mut client: TcpStream, factory: &str, token: &str) -> Result<(), String> {
    let (head, early) = match read_head(&mut client) {
        Ok(h) => h,
        Err(e) => {
            answer(&mut client, 400, "no request head");
            return Err(format!("a request with no head: {e}"));
        }
    };
    let Some(to) = connect_target(&head) else {
        answer(&mut client, 405, "only CONNECT host:port");
        return Err("a request that is not CONNECT host:port".to_string());
    };
    let at = http::host_port(factory);
    let Some(sock) = at.to_socket_addrs().ok().and_then(|mut a| a.next()) else {
        answer(&mut client, 502, "the factory is no address");
        return Err(format!("{to}: the factory {at} is no address"));
    };
    let mut up = match TcpStream::connect_timeout(&sock, Duration::from_secs(15)) {
        Ok(s) => s,
        Err(e) => {
            answer(&mut client, 502, "the factory did not answer");
            return Err(format!("{to}: the factory {at}: {e}"));
        }
    };
    let asked = up
        .write_all(upgrade_request(factory, &to, token).as_bytes())
        .and_then(|()| up.flush());
    if let Err(e) = asked {
        answer(&mut client, 502, "the factory did not answer");
        return Err(format!("{to}: the factory {at}: {e}"));
    }
    let (said, from_factory) = match read_head(&mut up) {
        Ok(h) => h,
        Err(e) => {
            answer(&mut client, 502, "the factory did not answer");
            return Err(format!("{to}: the factory {at} said nothing: {e}"));
        }
    };
    let status = String::from_utf8_lossy(&said)
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(502);
    if status != 101 {
        let mut body = from_factory;
        let _ = up.set_read_timeout(Some(Duration::from_secs(5)));
        let _ = (&mut up).take(HEAD_MAX as u64).read_to_end(&mut body);
        let word = serde_json::from_slice::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v["refused"].as_str().map(str::to_string))
            .unwrap_or_else(|| format!("status {status}"));
        answer(
            &mut client,
            status,
            &format!("the factory refused {to}: {word}"),
        );
        return Err(format!("{to}: the factory refused it: {word}"));
    }
    client
        .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
        .and_then(|()| client.write_all(&from_factory))
        .and_then(|()| up.write_all(&early))
        .map_err(|e| format!("{to}: the tunnel could not start: {e}"))?;
    tehas_core::tunnel::pipe(client, up, IDLE);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_connect_to_a_host_and_port_is_taken() {
        assert_eq!(
            connect_target(b"CONNECT GitHub.com:443 HTTP/1.1\r\nHost: github.com:443\r\n\r\n"),
            Some("github.com:443".to_string())
        );
        for odd in [
            &b"GET http://github.com/ HTTP/1.1\r\n\r\n"[..],
            b"CONNECT github.com HTTP/1.1\r\n\r\n",
            b"CONNECT a b:1 HTTP/1.1\r\n\r\n",
            b"\r\n\r\n",
        ] {
            assert_eq!(connect_target(odd), None, "{odd:?}");
        }
    }

    #[test]
    fn the_upgrade_names_the_route_the_destination_and_the_token() {
        let r = upgrade_request("http://192.0.2.7:8090/", "github.com:443", "tok");
        assert_eq!(
            r,
            "GET /outsource/v1/egress?to=github.com:443 HTTP/1.1\r\nhost: 192.0.2.7:8090\r\nconnection: Upgrade\r\nupgrade: tehas-egress\r\nauthorization: Bearer tok\r\n\r\n"
        );
    }

    #[test]
    fn a_factory_that_refuses_is_passed_on_with_its_status_and_word() {
        let factory = TcpListener::bind("127.0.0.1:0").unwrap();
        let at = factory.local_addr().unwrap().to_string();
        thread::spawn(move || {
            let (mut s, _) = factory.accept().unwrap();
            let _ = read_head(&mut s);
            let body = "{\"refused\":\"not on the egress list\"}\n";
            let _ = write!(
                s,
                "HTTP/1.1 403 Forbidden\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
        });
        let local = Local::start(&at, "tok").unwrap();
        let mut c = TcpStream::connect(local.addr).unwrap();
        c.write_all(b"CONNECT example.com:443 HTTP/1.1\r\n\r\n")
            .unwrap();
        let mut got = String::new();
        c.read_to_string(&mut got).unwrap();
        assert!(got.starts_with("HTTP/1.1 403 "), "{got}");
        assert!(
            got.contains("the factory refused example.com:443: not on the egress list"),
            "{got}"
        );
        assert!(!got.contains("tok\n"), "{got}");
    }
}

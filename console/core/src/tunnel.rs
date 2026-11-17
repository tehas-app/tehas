//! A tunnel of outsource: the addresses one may reach and the bytes it
//! carries. The factory's egress route and the contractor's local end share
//! it (#2378).

use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Shutdown, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// Whether an address is on the public internet: not loopback, private,
/// shared (100.64/10), link-local, unspecified, multicast, broadcast or a
/// documentation range, and the same for IPv6, an IPv4 address inside one
/// included.
pub fn public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_multicast()
                || v4.is_broadcast()
                || v4.is_documentation()
                || a == 0
                || (a == 100 && (64..128).contains(&b))
                || (a == 198 && (b == 18 || b == 19))
                || a >= 240)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return public(IpAddr::V4(v4));
            }
            let first = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
                || first == 0x2001 && v6.segments()[1] == 0x0db8)
        }
    }
}

/// Whether an address is one of this host's own: it can be bound here. The
/// factory's dashboard, MCP server and docs listen on such an address.
pub fn own(ip: IpAddr) -> bool {
    UdpSocket::bind((ip, 0)).is_ok()
}

/// The rule of a destination after its name resolved: public, and not this
/// host.
pub fn open(ip: IpAddr) -> bool {
    public(ip) && !own(ip)
}

/// The bytes a tunnel carried each way.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Moved {
    /// From `a` to `b`.
    pub up: u64,
    /// From `b` to `a`.
    pub down: u64,
}

/// How often a quiet side looks at the clock.
const TICK: Duration = Duration::from_millis(500);

/// Bytes both ways until both sides end, or until neither has sent anything
/// for `idle`. Nothing is buffered beyond one read, and no write has a cap of
/// its own: a long push or install passes as long as bytes keep moving.
pub fn pipe(a: TcpStream, b: TcpStream, idle: Duration) -> Moved {
    let tick = TICK.min(idle);
    for s in [&a, &b] {
        let _ = s.set_read_timeout(Some(tick));
        let _ = s.set_write_timeout(Some(idle));
    }
    let began = Instant::now();
    let last = Arc::new(AtomicU64::new(0));
    let cut = Arc::new(AtomicBool::new(false));
    let (Ok(a2), Ok(b2)) = (a.try_clone(), b.try_clone()) else {
        let _ = a.shutdown(Shutdown::Both);
        let _ = b.shutdown(Shutdown::Both);
        return Moved::default();
    };
    let side = {
        let (last, cut) = (Arc::clone(&last), Arc::clone(&cut));
        move |from: TcpStream, to: TcpStream| one_way(from, to, idle, began, &last, &cut)
    };
    let up_side = side.clone();
    let up = thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(move || up_side(a, b));
    let down = side(b2, a2);
    let up = match up {
        Ok(h) => h.join().unwrap_or(0),
        Err(_) => 0,
    };
    Moved { up, down }
}

fn one_way(
    mut from: TcpStream,
    mut to: TcpStream,
    idle: Duration,
    began: Instant,
    last: &AtomicU64,
    cut: &AtomicBool,
) -> u64 {
    let now = || u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX);
    let idle_ms = u64::try_from(idle.as_millis()).unwrap_or(u64::MAX);
    let mut buf = [0u8; 16 * 1024];
    let mut moved = 0u64;
    let end_both = |from: &TcpStream, to: &TcpStream| {
        cut.store(true, Ordering::SeqCst);
        let _ = from.shutdown(Shutdown::Both);
        let _ = to.shutdown(Shutdown::Both);
    };
    loop {
        if cut.load(Ordering::SeqCst) {
            return moved;
        }
        match from.read(&mut buf) {
            Ok(0) => {
                let _ = to.shutdown(Shutdown::Write);
                return moved;
            }
            Ok(n) => {
                last.store(now(), Ordering::SeqCst);
                if to.write_all(&buf[..n]).is_err() {
                    end_both(&from, &to);
                    return moved;
                }
                moved += n as u64;
                last.store(now(), Ordering::SeqCst);
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                if now().saturating_sub(last.load(Ordering::SeqCst)) >= idle_ms {
                    end_both(&from, &to);
                    return moved;
                }
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(_) => {
                end_both(&from, &to);
                return moved;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr, TcpListener};

    #[test]
    fn only_a_public_address_is_open() {
        // Built from their parts: the lint keeps private addresses out of
        // the source as text.
        let v4 = |a, b, c, d| IpAddr::V4(Ipv4Addr::new(a, b, c, d));
        let v6 = |first, last| IpAddr::V6(Ipv6Addr::new(first, 0, 0, 0, 0, 0, 0, last));
        for closed in [
            v4(127, 0, 0, 1),
            v4(10, 1, 2, 3),
            v4(172, 16, 0, 1),
            v4(192, 168, 1, 1),
            v4(169, 254, 169, 254),
            v4(100, 100, 1, 1),
            v4(0, 0, 0, 0),
            v4(224, 0, 0, 1),
            v4(255, 255, 255, 255),
            v4(192, 0, 2, 7),
            v6(0, 1),
            v6(0, 0),
            v6(0xfe80, 1),
            v6(0xfd00, 1),
            IpAddr::V6(Ipv4Addr::new(127, 0, 0, 1).to_ipv6_mapped()),
            IpAddr::V6(Ipv4Addr::new(10, 0, 0, 1).to_ipv6_mapped()),
            IpAddr::V6(Ipv6Addr::new(0x2001, 0x0db8, 0, 0, 0, 0, 0, 1)),
        ] {
            assert!(!public(closed), "{closed}");
            assert!(!open(closed), "{closed}");
        }
        for far in ["140.82.112.3", "104.16.0.1", "2606:4700::1111"] {
            assert!(public(far.parse().unwrap()), "{far}");
        }
        assert!(own("127.0.0.1".parse().unwrap()));
        assert!(!own("140.82.112.3".parse().unwrap()));
    }

    /// A connected pair of sockets: the test's end and the pipe's end.
    fn pair() -> (TcpStream, TcpStream) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let c = TcpStream::connect(l.local_addr().unwrap()).unwrap();
        let (s, _) = l.accept().unwrap();
        (c, s)
    }

    #[test]
    fn a_slow_but_live_transfer_is_not_cut_and_a_silent_one_is() {
        let (mut client, a) = pair();
        let (mut server, b) = pair();
        let run = thread::spawn(move || pipe(a, b, Duration::from_millis(600)));
        // Bytes every 300 ms for over twice the idle bound, one way only.
        for _ in 0..5 {
            server.write_all(b"chunk").unwrap();
            thread::sleep(Duration::from_millis(300));
        }
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut got = [0u8; 25];
        client.read_exact(&mut got).unwrap();
        assert_eq!(&got, b"chunkchunkchunkchunkchunk");
        client.write_all(b"up").unwrap();
        let mut two = [0u8; 2];
        server
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        server.read_exact(&mut two).unwrap();
        // Then silence: the pipe ends on its own.
        let moved = run.join().unwrap();
        assert_eq!(moved, Moved { up: 2, down: 25 });
        let mut rest = Vec::new();
        assert_eq!(client.read_to_end(&mut rest).unwrap_or(0), 0);
    }

    #[test]
    fn one_side_ending_lets_the_other_finish() {
        let (mut client, a) = pair();
        let (mut server, b) = pair();
        let run = thread::spawn(move || pipe(a, b, Duration::from_secs(5)));
        client.write_all(b"request").unwrap();
        client.shutdown(Shutdown::Write).unwrap();
        let mut req = Vec::new();
        server.read_to_end(&mut req).unwrap();
        assert_eq!(req, b"request");
        server.write_all(b"answer").unwrap();
        drop(server);
        let mut ans = Vec::new();
        client.read_to_end(&mut ans).unwrap();
        assert_eq!(ans, b"answer");
        assert_eq!(run.join().unwrap(), Moved { up: 7, down: 6 });
    }
}

//! The egress tunnel route (#2378): `/outsource/v1/egress?to=<host>:<port>`
//! on the proxy's listener, the one way a contractor's cycle reaches GitHub,
//! the registries and the hosts the factory lists.
//!
//! It is an HTTP/1.1 Upgrade, not a CONNECT, so it passes a TLS terminator in
//! front of the listener as well as the listener's own port: a GET with
//! `Connection: Upgrade` and `Upgrade: tehas-egress`, answered
//! `101 Switching Protocols`, after which the bytes are the tunnel's.
//!
//! The token is the job's model token, so a tunnel is tied to a live job.
//! The destination is held to the factory's list, the one the in-house egress
//! proxy takes (`tehas_core::egress::listed`: the base and
//! `TEHAS_EGRESS_HOSTS`), less every model endpoint of models.json: model
//! traffic stays on the counted model route. After the name resolves, an
//! address that is not public, or that is this host's own (the dashboard, the
//! MCP server, the docs), is refused. No host alias is read.

use std::io;
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tehas_core::egress::{self as list, Allowed};
use tehas_core::models::ModelFile;

/// The word of the Upgrade field.
pub const UPGRADE: &str = "tehas-egress";

/// How long the factory waits for a listed host to accept.
const CONNECT_WAIT: Duration = Duration::from_secs(15);

/// Tunnels open at once, over every job. A tunnel holds two threads and no
/// hand of the listener, so a cycle's parallel downloads never crowd out a
/// check-in.
pub const TUNNELS: usize = 128;

/// Why a tunnel was not opened: its status and the word the contractor reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub status: u16,
    pub word: String,
}

impl Refused {
    fn of(status: u16, word: &str) -> Refused {
        Refused {
            status,
            word: word.to_string(),
        }
    }
}

/// The route's settings and its count of open tunnels.
pub struct Egress {
    pub allowed: Allowed,
    /// The model endpoints, never tunnelled to.
    pub models: Vec<(String, u16)>,
    /// How a name is looked up. The host's is the system resolver.
    pub resolve: fn(&str, u16) -> io::Result<Vec<SocketAddr>>,
    /// Whether an address the name resolved to may be reached. The host's is
    /// [`tehas_core::tunnel::open`].
    pub open: fn(IpAddr) -> bool,
    /// How long a tunnel may carry nothing either way.
    pub idle: Duration,
    tunnels: Arc<AtomicUsize>,
}

fn system(host: &str, port: u16) -> io::Result<Vec<SocketAddr>> {
    Ok((host, port).to_socket_addrs()?.collect())
}

fn pairs(words: &[String]) -> Vec<(String, u16)> {
    words.iter().filter_map(|w| list::pair(w)).collect()
}

impl Egress {
    /// The route of a host: the list of `TEHAS_EGRESS_HOSTS` in `extra`, the
    /// endpoints of `models`, the system resolver and the public rule.
    pub fn of_host(extra: &str, models: &ModelFile, idle: Duration) -> Egress {
        let allowed = Allowed::parse(&list::listed(extra)).unwrap_or_default();
        Egress::with(
            allowed,
            pairs(&list::model_pairs(models)),
            system,
            tehas_core::tunnel::open,
            idle,
        )
    }

    pub fn with(
        allowed: Allowed,
        models: Vec<(String, u16)>,
        resolve: fn(&str, u16) -> io::Result<Vec<SocketAddr>>,
        open: fn(IpAddr) -> bool,
        idle: Duration,
    ) -> Egress {
        Egress {
            allowed,
            models,
            resolve,
            open,
            idle,
            tunnels: Arc::new(AtomicUsize::new(TUNNELS)),
        }
    }

    /// The host and port a path asks for, held to the list.
    pub fn judge(&self, path: &str) -> Result<(String, u16), Refused> {
        let to = path
            .split_once('?')
            .map(|(_, q)| q)
            .and_then(|q| q.strip_prefix("to="))
            .map(|t| t.replace("%3A", ":").replace("%3a", ":"))
            .unwrap_or_default();
        let Some((host, port)) = list::pair(&to) else {
            return Err(Refused::of(400, "no host:port to tunnel to"));
        };
        if self.models.iter().any(|(h, p)| *h == host && *p == port) {
            return Err(Refused::of(403, "a model endpoint: use the model route"));
        }
        if !self.allowed.allows(&host, port) {
            return Err(Refused::of(403, "not on the egress list"));
        }
        Ok((host, port))
    }

    /// A stream to a listed host, by an address the rule lets through.
    pub fn reach(&self, host: &str, port: u16) -> Result<TcpStream, Refused> {
        let addrs = (self.resolve)(host, port)
            .map_err(|_| Refused::of(502, "the host could not be resolved"))?;
        if addrs.is_empty() {
            return Err(Refused::of(502, "the host resolved to no address"));
        }
        // One address that is not open refuses the name: a name that mixes a
        // public address with a private one is not tried for the public one.
        if addrs.iter().any(|a| !(self.open)(a.ip())) {
            return Err(Refused::of(403, "an address that is not public"));
        }
        let mut last = Refused::of(502, "the host could not be reached");
        for addr in addrs {
            match TcpStream::connect_timeout(&addr, CONNECT_WAIT) {
                Ok(s) => return Ok(s),
                Err(_) => last = Refused::of(502, "the host could not be reached"),
            }
        }
        Err(last)
    }

    /// A place for one more tunnel, given back when it is dropped.
    pub fn place(&self) -> Option<Place> {
        self.tunnels
            .try_update(Ordering::Acquire, Ordering::Relaxed, |n| n.checked_sub(1))
            .ok()
            .map(|_| Place(Arc::clone(&self.tunnels)))
    }
}

pub struct Place(Arc<AtomicUsize>);

impl Drop for Place {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Release);
    }
}

/// Whether a head asks for this route's Upgrade.
pub fn upgrade_asked(fields: &str) -> bool {
    let field = |name: &str| {
        fields.lines().skip(1).find_map(|line| {
            let (n, v) = line.split_once(':')?;
            n.trim()
                .eq_ignore_ascii_case(name)
                .then(|| v.trim().to_ascii_lowercase())
        })
    };
    field("upgrade").is_some_and(|v| v == UPGRADE)
        && field("connection").is_some_and(|v| v.split(',').any(|w| w.trim() == "upgrade"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route() -> Egress {
        let allowed = Allowed::parse(&list::listed("pypi.org models.example:8080")).unwrap();
        Egress::with(
            allowed,
            vec![("models.example".to_string(), 8080)],
            system,
            tehas_core::tunnel::open,
            Duration::from_secs(5),
        )
    }

    #[test]
    fn a_path_names_a_listed_host_or_is_refused() {
        let r = route();
        assert_eq!(
            r.judge("/outsource/v1/egress?to=GitHub.com:443"),
            Ok(("github.com".to_string(), 443))
        );
        assert_eq!(
            r.judge("/outsource/v1/egress?to=pypi.org%3A443"),
            Ok(("pypi.org".to_string(), 443))
        );
        for (path, status) in [
            ("/outsource/v1/egress", 400),
            ("/outsource/v1/egress?to=github.com", 400),
            ("/outsource/v1/egress?x=1&to=github.com:443", 400),
            ("/outsource/v1/egress?to=example.com:443", 403),
            ("/outsource/v1/egress?to=github.com:22", 403),
            ("/outsource/v1/egress?to=models.example:8080", 403),
        ] {
            assert_eq!(r.judge(path).map_err(|e| e.status), Err(status), "{path}");
        }
        assert!(r
            .judge("/outsource/v1/egress?to=models.example:8080")
            .unwrap_err()
            .word
            .contains("model route"));
    }

    #[test]
    fn the_upgrade_is_asked_by_both_fields() {
        let head = |rest: &str| {
            format!("GET /outsource/v1/egress?to=github.com:443 HTTP/1.1\r\n{rest}\r\n")
        };
        assert!(upgrade_asked(&head(
            "Connection: keep-alive, Upgrade\r\nUpgrade: tehas-egress\r\n"
        )));
        assert!(!upgrade_asked(&head("Upgrade: tehas-egress\r\n")));
        assert!(!upgrade_asked(&head(
            "Connection: Upgrade\r\nUpgrade: websocket\r\n"
        )));
    }
}

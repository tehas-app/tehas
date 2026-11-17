//! The egress tunnel route against a real state store and a real listener,
//! with fake destinations on ports of 127.0.0.1 and no network: what tunnels,
//! what is refused, and that a refusal never connects.

use std::io::{self, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};
use tehas_core::testdir::TestDir;
use tehas_cycle::relay::Scope;
use tehas_proxy::channel::{key_hash, Asked, Call, Channel, Factory};
use tehas_proxy::egress::Egress;
use tehas_proxy::jobs::Issuer;
use tehas_proxy::records::NoneAtAll;
use tehas_proxy::serve::{self, Serve};

const KEY: &str = "the-contractor-key-of-box1";

fn call(home: &Path, args: &[&str]) -> Value {
    let text =
        store::call(home, args, &mut io::empty()).unwrap_or_else(|e| panic!("{args:?}: {e:?}"));
    serde_json::from_str(&text).unwrap()
}

struct Fake(PathBuf);

impl Factory for Fake {
    fn store(&self, args: &[&str]) -> Option<Value> {
        let text = store::call(&self.0, args, &mut io::empty()).ok()?;
        serde_json::from_str(&text).ok()
    }
    fn cycle_token(&self) -> Option<(String, i64)> {
        None
    }
    fn relay(
        &self,
        _: &[String],
        _: Option<&str>,
        _: &mut Scope,
    ) -> Result<(String, String, i32), String> {
        Err("no relay here".into())
    }
    fn keep(&self, _: i64, _: &[u8], _: i64) -> Result<(), String> {
        Ok(())
    }
    fn carried(&self) -> Value {
        json!({ "settings": {}, "models": {} })
    }
    fn timeout(&self) -> i64 {
        600
    }
    fn random(&self) -> Option<String> {
        tehas_proxy::jobs::random()
    }
    fn allowed(&self, name: &str) -> bool {
        name == "box1"
    }
}

/// Where the test's names resolve: each case names hosts of its own.
static NAMES: Mutex<Vec<(String, SocketAddr)>> = Mutex::new(Vec::new());

fn named(host: &str, at: SocketAddr) {
    NAMES.lock().unwrap().push((host.to_string(), at));
}

fn resolve(host: &str, _port: u16) -> io::Result<Vec<SocketAddr>> {
    let found: Vec<SocketAddr> = NAMES
        .lock()
        .unwrap()
        .iter()
        .filter(|(h, _)| h == host)
        .map(|(_, a)| *a)
        .collect();
    if found.is_empty() {
        return Err(io::Error::other("no such name"));
    }
    Ok(found)
}

fn any(_: IpAddr) -> bool {
    true
}

/// A factory with box1 holding a running job of issue 7: its listener, its
/// home, the job's two tokens and the lines the proxy wrote.
struct Case {
    at: SocketAddr,
    home: TestDir,
    job: i64,
    model: String,
    job_token: String,
    lines: Arc<Mutex<Vec<String>>>,
}

fn case(name: &str, extra: &str, models: &[(&str, u16)], open: fn(IpAddr) -> bool) -> Case {
    case_idle(name, extra, models, open, Duration::from_secs(5))
}

fn case_idle(
    name: &str,
    extra: &str,
    models: &[(&str, u16)],
    open: fn(IpAddr) -> bool,
    idle: Duration,
) -> Case {
    let home = TestDir::new(name);
    let h = home.path();
    std::fs::create_dir_all(h.join("state")).unwrap();
    call(
        h,
        &[
            "init",
            "--version",
            "v0.30.0",
            "--commit",
            "cb6912a",
            "--instance",
            "example",
            "--repo",
            "example-owner/example-product",
            "--outsource",
        ],
    );
    let c = rusqlite::Connection::open(h.join("state/tehas.db")).unwrap();
    c.execute(
        "INSERT INTO issues (issue, kind, kind_at, checked_at) VALUES (7, 'issue', 1, 1)",
        [],
    )
    .unwrap();
    drop(c);
    call(
        h,
        &["contractor-add", "--name", "box1", "--key", &key_hash(KEY)],
    );
    call(h, &["job-offer", "--issue", "7", "--contractor", "box1"]);
    let issuer: &'static Issuer = Box::leak(Box::new(Issuer::new()));
    let fake: &'static Fake = Box::leak(Box::new(Fake(h.to_path_buf())));
    let allowed = tehas_core::egress::Allowed::parse(&tehas_core::egress::listed(extra)).unwrap();
    let models = models.iter().map(|(h, p)| (h.to_string(), *p)).collect();
    let egress: &'static Egress =
        Box::leak(Box::new(Egress::with(allowed, models, resolve, open, idle)));
    let chan: &'static Channel<'static> =
        Box::leak(Box::new(Channel::new(issuer, fake).with_egress(egress)));
    let job = chan.answer(&Asked {
        call: Call::Job,
        bearer: Some(KEY),
        name: Some("box1"),
        body: b"",
    });
    assert_eq!(job.status, 200, "{job:?}");
    let lines = Arc::new(Mutex::new(Vec::new()));
    let sink: &'static (dyn tehas_proxy::log::Sink + Sync) =
        Box::leak(Box::new(serve::kept(&lines)));
    let serve: &'static Serve<'static> = Box::leak(Box::new(Serve {
        endpoint: tehas_proxy::forward::Upstream::of("http://127.0.0.1:1/v1").unwrap(),
        key: "sk-never-sent".to_string(),
        records: &NoneAtAll,
        sink,
        idle,
        channel: Some(chan),
    }));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let at = listener.local_addr().unwrap();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            thread::spawn(move || {
                let mut stream = stream;
                serve::one(&mut stream, serve);
            });
        }
    });
    Case {
        at,
        home,
        job: job.body["job"].as_i64().unwrap(),
        model: job.body["model_token"].as_str().unwrap().to_string(),
        job_token: job.body["token"].as_str().unwrap().to_string(),
        lines,
    }
}

/// A destination that is never to be reached: a listener nobody may accept
/// on. `reached` says whether anything connected.
struct Untouched(TcpListener);

impl Untouched {
    fn new() -> Untouched {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.set_nonblocking(true).unwrap();
        Untouched(l)
    }
    fn at(&self) -> SocketAddr {
        self.0.local_addr().unwrap()
    }
    fn reached(&self) -> bool {
        self.0.accept().is_ok()
    }
}

/// The Upgrade asked, and the head of the answer with the stream to go on.
fn tunnel(at: SocketAddr, token: &str, to: &str) -> (String, TcpStream) {
    let mut s = TcpStream::connect(at).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    write!(
        s,
        "GET /outsource/v1/egress?to={to} HTTP/1.1\r\nHost: factory\r\nConnection: Upgrade\r\nUpgrade: tehas-egress\r\nAuthorization: Bearer {token}\r\n\r\n"
    )
    .unwrap();
    let mut head = Vec::new();
    let mut one = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if s.read(&mut one).unwrap_or(0) == 0 {
            break;
        }
        head.push(one[0]);
    }
    (String::from_utf8_lossy(&head).into_owned(), s)
}

fn status(head: &str) -> u16 {
    head.split_whitespace().nth(1).unwrap().parse().unwrap()
}

/// The JSON a refusal carried.
fn refusal(head: &str, s: &mut TcpStream) -> String {
    assert!(head.contains("content-type: application/json"), "{head}");
    let mut body = String::new();
    s.read_to_string(&mut body).unwrap();
    let v: Value = serde_json::from_str(&body).unwrap();
    v["refused"].as_str().unwrap().to_string()
}

#[test]
fn a_listed_host_tunnels_both_ways_and_the_log_names_no_token() {
    let echo = TcpListener::bind("127.0.0.1:0").unwrap();
    named("listed.example", echo.local_addr().unwrap());
    thread::spawn(move || {
        let (mut s, _) = echo.accept().unwrap();
        let mut b = [0u8; 5];
        s.read_exact(&mut b).unwrap();
        s.write_all(&b).unwrap();
    });
    let c = case("egress-listed", "listed.example", &[], any);
    let (head, mut s) = tunnel(c.at, &c.model, "listed.example:443");
    assert_eq!(status(&head), 101, "{head}");
    assert!(head.to_ascii_lowercase().contains("upgrade: tehas-egress"));
    s.write_all(b"hello").unwrap();
    let mut got = [0u8; 5];
    s.read_exact(&mut got).unwrap();
    assert_eq!(&got, b"hello");
    drop(s);
    thread::sleep(Duration::from_millis(300));
    let lines = c.lines.lock().unwrap().join("\n");
    assert!(lines.contains("egress 101"), "{lines}");
    assert!(
        lines.contains("contractor box1 to listed.example:443"),
        "{lines}"
    );
    assert!(!lines.contains(&c.model), "{lines}");
}

#[test]
fn an_unlisted_host_a_model_endpoint_and_a_bad_target_are_refused_unreached() {
    let far = Untouched::new();
    named("unlisted.example", far.at());
    named("models.example", far.at());
    let c = case(
        "egress-unlisted",
        "models.example:8080",
        &[("models.example", 8080)],
        any,
    );
    for (to, want, word) in [
        ("unlisted.example:443", 403, "not on the egress list"),
        (
            "models.example:8080",
            403,
            "a model endpoint: use the model route",
        ),
        ("github.com", 400, "no host:port to tunnel to"),
    ] {
        let (head, mut s) = tunnel(c.at, &c.model, to);
        assert_eq!(status(&head), want, "{to}: {head}");
        assert_eq!(refusal(&head, &mut s), word, "{to}");
    }
    assert!(!far.reached());
}

#[test]
fn a_private_address_and_the_factory_s_own_port_are_refused_unreached() {
    // The dashboard stands on a port of this host.
    let dashboard = Untouched::new();
    named("dashboard.example", dashboard.at());
    named(
        "private.example",
        SocketAddr::from((std::net::Ipv4Addr::new(10, 0, 0, 1), 443)),
    );
    let c = case(
        "egress-private",
        "dashboard.example private.example",
        &[],
        tehas_core::tunnel::open,
    );
    for to in ["dashboard.example:443", "private.example:443"] {
        let (head, mut s) = tunnel(c.at, &c.model, to);
        assert_eq!(status(&head), 403, "{to}: {head}");
        assert_eq!(refusal(&head, &mut s), "an address that is not public");
    }
    assert!(!dashboard.reached());
}

#[test]
fn a_wrong_or_unknown_token_and_a_plain_get_are_refused() {
    let far = Untouched::new();
    named("token.example", far.at());
    let c = case("egress-tokens", "token.example", &[], any);
    let (head, mut s) = tunnel(c.at, &c.job_token, "token.example:443");
    assert_eq!(status(&head), 403, "{head}");
    assert_eq!(refusal(&head, &mut s), "scope");
    let (head, mut s) = tunnel(c.at, &"x".repeat(32), "token.example:443");
    assert_eq!(status(&head), 401, "{head}");
    assert_eq!(refusal(&head, &mut s), "no job");
    let mut s = TcpStream::connect(c.at).unwrap();
    write!(
        s,
        "GET /outsource/v1/egress?to=token.example:443 HTTP/1.1\r\nAuthorization: Bearer {}\r\n\r\n",
        c.model
    )
    .unwrap();
    let mut got = String::new();
    s.read_to_string(&mut got).unwrap();
    assert!(got.starts_with("HTTP/1.1 400 "), "{got}");
    assert!(!far.reached());
}

#[test]
fn an_ended_job_and_a_revoked_contractor_are_refused_with_one_line_and_no_connect() {
    let far = Untouched::new();
    named("ended.example", far.at());
    let c = case("egress-ended", "ended.example", &[], any);
    let h = c.home.path();
    call(h, &["job-release", "--job", &c.job.to_string()]);
    let (head, mut s) = tunnel(c.at, &c.model, "ended.example:443");
    assert_eq!(status(&head), 403, "{head}");
    assert_ne!(refusal(&head, &mut s), "revoked");

    let c = case("egress-revoked", "ended.example", &[], any);
    call(c.home.path(), &["contractor-revoke", "--name", "box1"]);
    let (head, mut s) = tunnel(c.at, &c.model, "ended.example:443");
    assert_eq!(status(&head), 403, "{head}");
    assert_eq!(refusal(&head, &mut s), "revoked");
    let lines = c.lines.lock().unwrap().clone();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].ends_with(": refused revoked"), "{lines:?}");
    assert!(!far.reached());
}

#[test]
fn a_slow_but_live_transfer_is_not_cut() {
    let slow = TcpListener::bind("127.0.0.1:0").unwrap();
    named("slow.example", slow.local_addr().unwrap());
    thread::spawn(move || {
        let (mut s, _) = slow.accept().unwrap();
        for _ in 0..6 {
            s.write_all(b"part").unwrap();
            thread::sleep(Duration::from_millis(400));
        }
    });
    let c = case_idle(
        "egress-slow",
        "slow.example",
        &[],
        any,
        Duration::from_secs(1),
    );
    let (head, mut s) = tunnel(c.at, &c.model, "slow.example:443");
    assert_eq!(status(&head), 101, "{head}");
    let mut got = Vec::new();
    s.read_to_end(&mut got).unwrap();
    assert_eq!(got, b"part".repeat(6));
}

#[test]
fn the_route_is_a_call_of_the_model_scope() {
    assert_eq!(
        Call::of("/outsource/v1/egress?to=github.com:443"),
        Some(Call::Egress)
    );
    assert_eq!(Call::Egress.scope(), Some(tehas_proxy::jobs::Scope::Model));
}

#[test]
fn a_job_lost_to_a_silence_still_tunnels_for_its_salvage_push() {
    let echo = TcpListener::bind("127.0.0.1:0").unwrap();
    named("salvage.example", echo.local_addr().unwrap());
    thread::spawn(move || {
        let _ = echo.accept();
    });
    let c = case("egress-salvage", "salvage.example", &[], any);
    call(
        c.home.path(),
        &[
            "job-lose",
            "--job",
            &c.job.to_string(),
            "--reason",
            "timeout",
        ],
    );
    let (head, _s) = tunnel(c.at, &c.model, "salvage.example:443");
    assert_eq!(status(&head), 101, "{head}");
}

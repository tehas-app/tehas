//! The read-only MCP server's command: `tehasbot mcp serve` runs the program
//! in its container, `tehas mcp key` prints a new key. What each does for a
//! person is in src/usage/mcp.txt.
//!
//!   console tehas|tehasbot mcp serve
//!   console tehas|tehasbot mcp key
//!
//! serve runs under `console run`; key needs no settings. The
//! container is a security boundary: its arguments are pinned by
//! test/mcp_test.sh, and keys reach it by name, never in an argument.
//!
//! Exit: see src/usage/mcp.txt.

use std::env;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream, ToSocketAddrs};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tehas_core::mask;
use tehas_core::models;
use tehas_core::settings;

const NAME: &str = "tehas-mcp";
/// The port the container publishes on TEHAS_HOST_ADDR.
pub const PORT: u16 = 8082;
const MASQUERADE: &str = "com.docker.network.bridge.enable_ip_masquerade";

fn say(line: &str) {
    eprintln!("tehas: {line}");
}

fn setting(name: &str) -> String {
    env::var_os(name).map_or(String::new(), |v| v.to_string_lossy().into_owned())
}

/// A command's standard output as `$(...)` keeps it, with its standard error
/// left to the caller. None when it could not start or did not succeed.
fn stdout_of(command: &mut Command) -> Option<String> {
    let out = command.output().ok()?;
    out.status.success().then(|| {
        String::from_utf8_lossy(&out.stdout)
            .trim_end_matches('\n')
            .to_string()
    })
}

/// 32 random bytes, base64url without padding.
fn key() -> u8 {
    let mut bytes = [0u8; 32];
    match File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes)) {
        Ok(()) => {
            tehas_core::outln!("{}", base64url(&bytes));
            0
        }
        Err(e) => {
            say(&format!("could not read /dev/urandom: {e}"));
            1
        }
    }
}

fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let group =
            chunk.iter().fold(0u32, |n, b| n << 8 | u32::from(*b)) << (8 * (3 - chunk.len()));
        for i in 0..=chunk.len() {
            out.push(char::from(ALPHABET[(group >> (18 - 6 * i) & 63) as usize]));
        }
    }
    out
}

/// Whether the setting holds a key. It stops at a bad one, naming the setting
/// and the key's place, never the key. The shape is the server's rule, the
/// byte check of `listed` in console/mcp/src/keys.rs, read here before docker
/// is asked for a container.
fn holds_key(name: &str, value: &str) -> Result<bool, String> {
    let line = value.split('\n').next().unwrap_or_default();
    let keys = line.split([' ', '\t']).filter(|k| !k.is_empty());
    let mut any = false;
    for (i, key) in keys.enumerate() {
        let good = key.len() >= 32
            && key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !good {
            return Err(format!(
                "{name}: key {} is not at least 32 characters of A-Za-z0-9_-",
                i + 1
            ));
        }
        any = true;
    }
    Ok(any)
}

/// Each list is checked where it is written, so a bad key is named by its own
/// setting and place.
fn check_keys(full: &str, dashboard: &str) -> Result<(), String> {
    let full = holds_key("TEHAS_MCP_KEYS", full)?;
    let dashboard = holds_key("TEHAS_MCP_DASHBOARD_KEYS", dashboard)?;
    if full || dashboard {
        Ok(())
    } else {
        Err("TEHAS_MCP_KEYS and TEHAS_MCP_DASHBOARD_KEYS are both empty: give at least one key in either (tehas mcp key makes one)".to_string())
    }
}

/// The version the server states: the first line of the file, in the
/// characters a version has.
fn version(file: &[u8]) -> String {
    let line = file.split(|b| *b == b'\n').next().unwrap_or_default();
    let kept: String = line
        .iter()
        .filter(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-'))
        .map(|b| char::from(*b))
        .collect();
    if kept.is_empty() {
        "unknown".to_string()
    } else {
        kept
    }
}

/// A network made by hand under this name is used only if it lets nothing out.
fn network() -> Result<(), String> {
    let format = format!("{{{{index .Options \"{MASQUERADE}\"}}}}");
    let seen = stdout_of(
        Command::new("docker")
            .args(["network", "inspect", "--format", &format, NAME])
            .stderr(Stdio::null()),
    );
    match seen.as_deref() {
        Some("false") => Ok(()),
        Some(_) => Err(format!(
            "the network {NAME} lets the container out (masquerade is not false): remove it and start again"
        )),
        None => Command::new("docker")
            .args(["network", "create", "--driver", "bridge", "-o"])
            .arg(format!("{MASQUERADE}=false"))
            .arg(NAME)
            .stdout(Stdio::null())
            .status()
            .ok()
            .filter(|status| status.success())
            .map(|_| ())
            .ok_or(format!("could not make the network {NAME}")),
    }
}

/// Made here, so Docker does not make it as root. The snapshot in it is
/// private: it holds what the agents saw.
fn snapshot_dir(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir)
        .and_then(|()| fs::set_permissions(dir, fs::Permissions::from_mode(0o700)))
        .map_err(|e| format!("could not make {}: {e}", dir.display()))
}

fn remove_container() {
    let _ = Command::new("docker")
        .args(["rm", "-f", NAME])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

struct Run {
    home: String,
    addr: String,
    user: String,
    digest: String,
    version: String,
    private: Vec<String>,
    /// The sums of the secret values the host masks, given to the container by
    /// name as the keys are: the server masks them and never holds them.
    sums: String,
}

/// Every argument the container gets. A change here widens or narrows what a
/// caller on the internet can reach.
fn run_args(run: &Run) -> Vec<String> {
    let home = &run.home;
    let mut args: Vec<String> = [
        "run",
        "--rm",
        "--init",
        "--name",
        NAME,
        "--user",
        &run.user,
        "--network",
        NAME,
        "--read-only",
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges:true",
        "--memory",
        "256m",
        "--pids-limit",
        "64",
        "-p",
        &format!("{}:{PORT}:{PORT}", run.addr),
        "-v",
        &format!("{home}/live:/live:ro"),
        "-v",
        &format!("{home}/mcp:/sessions:ro"),
        "-v",
        &format!("{home}/logs:/logs:ro"),
        "-v",
        &format!("{home}/libexec/tehas-mcp:/tehas-mcp:ro"),
        "-e",
        "TEHAS_MCP_KEYS",
        "-e",
        "TEHAS_MCP_DASHBOARD_KEYS",
        "-e",
        "TEHAS_MCP_MASK",
        "--entrypoint",
        "/tehas-mcp",
        &format!("caddy@{}", run.digest),
        "--live",
        "/live",
        "--sessions",
        "/sessions",
        "--logs",
        "/logs",
        "--listen",
        &format!("0.0.0.0:{PORT}"),
        "--server-version",
        &run.version,
    ]
    .map(str::to_string)
    .to_vec();
    for name in &run.private {
        args.push("--private".to_string());
        args.push(name.clone());
    }
    args
}

fn is_program(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

fn prepare(home: &str) -> Result<Run, (u8, String)> {
    let fail = |line: String| (1, line);
    check_keys(
        &setting("TEHAS_MCP_KEYS"),
        &setting("TEHAS_MCP_DASHBOARD_KEYS"),
    )
    .map_err(|line| (78, line))?;
    let bin = format!("{home}/libexec/tehas-mcp");
    if !is_program(Path::new(&bin)) {
        return Err(fail(format!("no {bin}: run install.sh")));
    }
    let versions = fs::read(format!("{home}/versions.env")).unwrap_or_default();
    let versions = String::from_utf8_lossy(&versions);
    let digest = tehas_core::egress::caddy_digest(&versions)
        .ok_or(fail(format!(
            "no CADDY_DIGEST in {home}/versions.env: run install.sh"
        )))?
        .to_string();
    let version = version(&fs::read(format!("{home}/docs/version.txt")).unwrap_or_default());
    let mut salt = [0u8; 16];
    File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut salt))
        .map_err(|e| fail(format!("could not read /dev/urandom: {e}")))?;
    network().map_err(fail)?;
    snapshot_dir(&Path::new(home).join("mcp")).map_err(fail)?;
    // Standard error stays the command's, as git's complaint about a broken
    // configuration says more than an empty name.
    let email = stdout_of(
        Command::new("git")
            .args(["config", "--global", "user.email"])
            .stderr(Stdio::inherit()),
    )
    .unwrap_or_default();
    let id = |flag: &str| stdout_of(Command::new("id").arg(flag)).unwrap_or_default();
    let addr = setting("TEHAS_HOST_ADDR");
    Ok(Run {
        home: home.to_string(),
        addr: if addr.is_empty() {
            "127.0.0.1".to_string()
        } else {
            addr
        },
        user: format!("{}:{}", id("-u"), id("-g")),
        digest,
        version,
        private: mask::host_names(&models::host_path(), &email),
        sums: mask::sums_line(&salt),
    })
}

/// What the health page says of the server: off, or on and whether it is
/// installed and answers.
#[derive(Debug, PartialEq)]
pub enum Liveness {
    Off,
    NotInstalled,
    Answers,
    Silent(String),
}

/// The health page's look at the server, with no key: `TEHAS_MCP` as the
/// settings gave it, the binary install.sh puts in libexec/, and a `POST /mcp`
/// to <addr>:<port>. A server that is up refuses a request with no key with
/// 401 before it reads a byte of the body, so a 401 is the one answer that
/// says the server is there; a proxy's 502 or a stranger's 200 is not it.
/// Connecting and the answer are each bounded at <bound>.
pub fn liveness(home: &Path, on: &str, addr: &str, port: u16, bound: Duration) -> Liveness {
    match on {
        "" | "0" => return Liveness::Off,
        "1" => {}
        _ => return Liveness::Silent("TEHAS_MCP is not 0 or 1".to_string()),
    }
    if !is_program(&home.join("libexec/tehas-mcp")) {
        return Liveness::NotInstalled;
    }
    let target = probe_addr(addr, port);
    let Some(target) = target else {
        return Liveness::Silent(format!("TEHAS_HOST_ADDR {addr} names no address"));
    };
    match status_of(target, bound) {
        Ok(401) => Liveness::Answers,
        Ok(status) => Liveness::Silent(format!("POST /mcp on {target} answered {status}")),
        Err(e) => Liveness::Silent(format!("POST /mcp on {target}: {e}")),
    }
}

/// Where the published port is reached from this host: TEHAS_HOST_ADDR, the
/// loopback when it is empty, and the loopback too for an address of every
/// interface, which is no address to connect to.
fn probe_addr(addr: &str, port: u16) -> Option<SocketAddr> {
    let host = addr.trim_start_matches('[').trim_end_matches(']');
    match host.parse::<IpAddr>() {
        Ok(ip) if ip == IpAddr::V4(Ipv4Addr::UNSPECIFIED) => {
            Some(SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port))
        }
        Ok(ip) if ip == IpAddr::V6(Ipv6Addr::UNSPECIFIED) => {
            Some(SocketAddr::new(Ipv6Addr::LOCALHOST.into(), port))
        }
        Ok(ip) => Some(SocketAddr::new(ip, port)),
        Err(_) if host.is_empty() => Some(SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port)),
        Err(_) => (host, port).to_socket_addrs().ok()?.next(),
    }
}

/// The status of the answer to one `POST /mcp` with no key and no body.
fn status_of(target: SocketAddr, bound: Duration) -> Result<u16, String> {
    let mut stream = TcpStream::connect_timeout(&target, bound).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + bound;
    let _ = stream.set_write_timeout(Some(bound));
    stream
        .write_all(
            b"POST /mcp HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )
        .map_err(|e| e.to_string())?;
    let mut head = Vec::new();
    let mut chunk = [0u8; 256];
    while !head.windows(2).any(|w| w == b"\r\n") && head.len() < 1024 {
        let left = deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| format!("no answer within {} s", bound.as_secs()))?;
        let _ = stream.set_read_timeout(Some(left));
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => head.extend_from_slice(&chunk[..n]),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Err(format!("no answer within {} s", bound.as_secs()));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    let line = head.split(|b| *b == b'\r').next().unwrap_or_default();
    let line = String::from_utf8_lossy(line);
    let mut words = line.split(' ');
    match (
        words.next(),
        words.next().and_then(|s| s.parse::<u16>().ok()),
    ) {
        (Some(proto), Some(status)) if proto.starts_with("HTTP/1.") => Ok(status),
        _ if head.is_empty() => Err("the connection closed with no answer".to_string()),
        _ => Err("the answer is not HTTP".to_string()),
    }
}

fn serve() -> u8 {
    // A run that skipped the load must not read as a server turned off.
    if let Err(rc) = settings::require(&["TEHAS_HOME", "TEHAS_INSTANCE_DIR"]) {
        return rc;
    }
    match setting("TEHAS_MCP").as_str() {
        "" | "0" => {
            remove_container();
            return 0;
        }
        "1" => {}
        _ => {
            say("TEHAS_MCP is 0 or 1 (in ~/.config/tehas/env)");
            return 78;
        }
    }
    let run = match prepare(&setting("TEHAS_HOME")) {
        Ok(run) => run,
        Err((rc, line)) => {
            say(&line);
            return rc;
        }
    };
    remove_container();
    let error = Command::new("docker")
        .args(run_args(&run))
        .env("TEHAS_MCP_MASK", &run.sums)
        .exec();
    say(&format!("could not run docker: {error}"));
    127
}

pub fn main(given: &[String]) -> u8 {
    match given.first().map(String::as_str) {
        Some("serve") => serve(),
        Some("key") => key(),
        _ => {
            eprintln!("usage: tehasbot mcp serve | tehas mcp key");
            64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "abcdefghijklmnopqrstuvwxyz012345";

    #[test]
    fn a_key_is_the_url_safe_alphabet_with_no_padding() {
        assert_eq!(base64url(b""), "");
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(&[0xfb, 0xff, 0xfe]), "-__-");
        assert_eq!(base64url(&[0; 32]).len(), 43);
    }

    #[test]
    fn keys_are_read_in_words_of_the_first_line() {
        assert_eq!(holds_key("X", ""), Ok(false));
        assert_eq!(holds_key("X", " \t "), Ok(false));
        assert_eq!(holds_key("X", KEY), Ok(true));
        assert_eq!(holds_key("X", &format!("{KEY}\t {KEY}")), Ok(true));
        assert_eq!(holds_key("X", &format!("\n{KEY}")), Ok(false));
    }

    #[test]
    fn a_bad_key_is_named_by_setting_and_place_and_not_by_value() {
        let bad = format!("{KEY} {}+", &KEY[..31]);
        let said = holds_key("TEHAS_MCP_KEYS", &bad).unwrap_err();
        assert_eq!(
            said,
            "TEHAS_MCP_KEYS: key 2 is not at least 32 characters of A-Za-z0-9_-"
        );
        assert!(check_keys("", "").unwrap_err().contains("both empty"));
        assert!(check_keys(KEY, "x")
            .unwrap_err()
            .starts_with("TEHAS_MCP_DASHBOARD_KEYS: key 1"));
        assert_eq!(check_keys("", KEY), Ok(()));
    }

    #[test]
    fn the_version_keeps_its_characters_of_the_first_line() {
        assert_eq!(version(b"v0.7.30+abc1234\nnext\n"), "v0.7.30+abc1234");
        assert_eq!(version(b"v1 (x)/2\n"), "v1x2");
        assert_eq!(version(b""), "unknown");
        assert_eq!(version(b"//\n"), "unknown");
    }

    #[test]
    fn no_argument_of_the_container_holds_a_key_or_a_mount_beyond_the_four() {
        let args = run_args(&Run {
            home: "/h".to_string(),
            addr: "127.0.0.1".to_string(),
            user: "1:2".to_string(),
            digest: "sha256:x".to_string(),
            version: "v1".to_string(),
            private: vec!["gpu1".to_string()],
            sums: "salt 9:abcd:sum".to_string(),
        });
        let mounts: Vec<&String> = args
            .iter()
            .zip(args.iter().skip(1))
            .filter(|(flag, _)| *flag == "-v")
            .map(|(_, mount)| mount)
            .collect();
        assert_eq!(
            mounts,
            [
                "/h/live:/live:ro",
                "/h/mcp:/sessions:ro",
                "/h/logs:/logs:ro",
                "/h/libexec/tehas-mcp:/tehas-mcp:ro"
            ]
        );
        assert_eq!(args[args.len() - 2..], ["--private", "gpu1"]);
        // The sums go by name, never in an argument.
        assert!(args.iter().any(|a| a == "TEHAS_MCP_MASK"));
        assert!(!args.iter().any(|a| a.contains("abcd")));
    }

    /// A home with the binary install.sh puts in libexec/.
    fn installed(name: &str) -> tehas_core::testdir::TestDir {
        let home = tehas_core::testdir::TestDir::new(&format!("mcp-live-{name}"));
        fs::create_dir_all(home.join("libexec")).unwrap();
        let bin = home.join("libexec/tehas-mcp");
        fs::write(&bin, "").unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        home
    }

    /// A server on the loopback that reads one request and answers <said>,
    /// or holds the connection open and says nothing when it is None.
    fn server(said: Option<&'static str>) -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut c, _) = listener.accept().unwrap();
            let mut buf = [0u8; 512];
            let _ = c.read(&mut buf);
            match said {
                Some(text) => {
                    let _ = c.write_all(text.as_bytes());
                }
                None => std::thread::sleep(Duration::from_secs(2)),
            }
        });
        port
    }

    fn look(home: &Path, port: u16) -> Liveness {
        liveness(home, "1", "127.0.0.1", port, Duration::from_millis(500))
    }

    #[test]
    fn a_server_that_is_off_is_not_probed() {
        let home = tehas_core::testdir::TestDir::new("mcp-live-off");
        assert_eq!(liveness(&home, "", "", 1, Duration::ZERO), Liveness::Off);
        assert_eq!(liveness(&home, "0", "", 1, Duration::ZERO), Liveness::Off);
        assert_eq!(
            liveness(&home, "yes", "", 1, Duration::ZERO),
            Liveness::Silent("TEHAS_MCP is not 0 or 1".to_string())
        );
    }

    #[test]
    fn a_server_on_with_no_binary_is_not_installed() {
        let home = tehas_core::testdir::TestDir::new("mcp-live-none");
        assert_eq!(look(&home, 1), Liveness::NotInstalled);
    }

    #[test]
    fn a_401_to_no_key_is_a_server_that_answers() {
        let home = installed("401");
        let port = server(Some(
            "HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Bearer\r\nContent-Length: 0\r\n\r\n",
        ));
        assert_eq!(look(&home, port), Liveness::Answers);
    }

    #[test]
    fn any_other_answer_is_not_the_server() {
        let home = installed("502");
        let port = server(Some("HTTP/1.1 502 Bad Gateway\r\n\r\n"));
        assert_eq!(
            look(&home, port),
            Liveness::Silent(format!("POST /mcp on 127.0.0.1:{port} answered 502"))
        );
        let port = server(Some("SSH-2.0-OpenSSH\r\n"));
        assert_eq!(
            look(&home, port),
            Liveness::Silent(format!(
                "POST /mcp on 127.0.0.1:{port}: the answer is not HTTP"
            ))
        );
        let port = server(Some(""));
        assert_eq!(
            look(&home, port),
            Liveness::Silent(format!(
                "POST /mcp on 127.0.0.1:{port}: the connection closed with no answer"
            ))
        );
    }

    #[test]
    fn a_stopped_or_silent_server_does_not_answer() {
        let home = installed("stopped");
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        match look(&home, port) {
            Liveness::Silent(why) => {
                assert!(
                    why.starts_with(&format!("POST /mcp on 127.0.0.1:{port}: ")),
                    "{why}"
                );
            }
            other => panic!("{other:?}"),
        }
        let port = server(None);
        let started = Instant::now();
        assert_eq!(
            liveness(&home, "1", "", port, Duration::from_secs(1)),
            Liveness::Silent(format!(
                "POST /mcp on 127.0.0.1:{port}: no answer within 1 s"
            ))
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn the_probe_reaches_the_address_the_port_is_published_on() {
        let at = |a: &str| probe_addr(a, 8082).map(|s| s.to_string());
        assert_eq!(at("").as_deref(), Some("127.0.0.1:8082"));
        assert_eq!(at("0.0.0.0").as_deref(), Some("127.0.0.1:8082"));
        assert_eq!(at("::").as_deref(), Some("[::1]:8082"));
        assert_eq!(at("192.0.2.9").as_deref(), Some("192.0.2.9:8082"));
        assert_eq!(at("fe80::1").as_deref(), Some("[fe80::1]:8082"));
    }
}

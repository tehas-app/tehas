//! The program itself: off says why and binds nothing; on, it answers on its
//! port while its control socket answers too, and it reads the factory's
//! records from the state store of the home it is given.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tehas_core::testdir::TestDir;

const BIN: &str = env!("CARGO_BIN_EXE_tehas-proxy");

/// A home with a models.json of one provider on `endpoint`, its key named by a
/// variable, as an instance writes it.
fn home(name: &str, endpoint: &str) -> TestDir {
    let dir = TestDir::new(name);
    let agent = dir.path().join(".pi/agent");
    std::fs::create_dir_all(&agent).unwrap();
    std::fs::write(
        agent.join("models.json"),
        format!(
            r#"{{"providers":{{"main":{{"baseUrl":"{endpoint}","apiKey":"$TEHAS_MODEL_KEY_MAIN","api":"openai-completions","models":[{{"id":"m"}}]}}}}}}"#
        ),
    )
    .unwrap();
    dir
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn proxy(home: &Path, on: &str, addr: &str, args: &[&str]) -> Command {
    let mut cmd = Command::new(BIN);
    cmd.args(args)
        .env_clear()
        .env("HOME", home)
        .env("TEHAS_HOME", home.join("tehas"))
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("TEHAS_OUTSOURCE", on)
        // Outsource is a building feature: only a test turns it on.
        .env("TEHAS_TEST_FEATURES", "outsource")
        .env("TEHAS_OUTSOURCE_PROXY_ADDR", addr)
        .env("TEHAS_OUTSOURCE_TIMEOUT", "600")
        .env("TEHAS_OUTSOURCE_PROXY_IDLE", "900")
        .env("PI_ENSEMBLE_SUBAGENT_PROVIDER", "main")
        .env("TEHAS_MODEL_KEY_MAIN", "the-factory-key")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

struct Running(Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// How long a test waits for the proxy to start or to end. A loaded host
/// running several gates at once has taken more than 20 s to start it.
const WAIT: Duration = Duration::from_secs(90);

/// The status of a request with no token, once the port answers.
fn status_of_no_token(addr: &str) -> Option<u16> {
    let until = Instant::now() + WAIT;
    while Instant::now() < until {
        if let Ok(mut s) = TcpStream::connect(addr) {
            s.set_read_timeout(Some(Duration::from_secs(10))).ok();
            s.write_all(b"GET /v1/models HTTP/1.1\r\nHost: x\r\n\r\n")
                .ok()?;
            let mut got = String::new();
            let _ = s.read_to_string(&mut got);
            return got.split_whitespace().nth(1)?.parse().ok();
        }
        thread::sleep(Duration::from_millis(50));
    }
    None
}

#[test]
fn off_says_why_and_binds_nothing() {
    let dir = home("proxy-off", "http://127.0.0.1:9/v1");
    let addr = format!("127.0.0.1:{}", free_port());
    let out = proxy(dir.path(), "0", &addr, &[]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stderr).contains("outsource is off"));
    assert!(
        TcpStream::connect(&addr).is_err(),
        "an off proxy bound {addr}"
    );
    assert!(!dir.path().join("tehas/state/outsource").exists());
}

#[test]
fn a_building_feature_stays_off_without_the_test_variable() {
    let dir = home("proxy-building", "http://127.0.0.1:9/v1");
    let addr = format!("127.0.0.1:{}", free_port());
    let out = proxy(dir.path(), "on", &addr, &[])
        .env_remove("TEHAS_TEST_FEATURES")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stderr).contains("outsource is off"));
    assert!(
        TcpStream::connect(&addr).is_err(),
        "a building proxy bound {addr}"
    );
}

#[test]
fn on_it_answers_on_its_port_and_its_control_socket_at_once() {
    let dir = home("proxy-on", "http://127.0.0.1:9/v1");
    let addr = format!("127.0.0.1:{}", free_port());
    let _running = Running(proxy(dir.path(), "1", &addr, &[]).spawn().unwrap());
    // A request with no token is refused by the listener, which is what says
    // the port is served while the control line is open.
    assert_eq!(status_of_no_token(&addr), Some(401));
    let sock = dir.path().join("tehas/state/outsource/proxy.sock");
    assert!(sock.exists(), "no control socket at {sock:?}");
    // The control line answers too: a token is issued and written to the job's
    // directory, and the caller is told the path only.
    let out = proxy(dir.path(), "1", &addr, &["--issue", "7", "1", "gpu1"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let said = String::from_utf8_lossy(&out.stdout);
    let token_file = dir.path().join("tehas/state/outsource/jobs/7/job-token");
    assert_eq!(said.trim(), token_file.to_string_lossy());
    let token = std::fs::read_to_string(&token_file).unwrap();
    assert!(!said.contains(token.trim()));
}

#[test]
fn a_port_it_cannot_bind_ends_it_with_1() {
    let dir = home("proxy-taken", "http://127.0.0.1:9/v1");
    let taken = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = taken.local_addr().unwrap().to_string();
    let mut child = proxy(dir.path(), "1", &addr, &[]).spawn().unwrap();
    let until = Instant::now() + WAIT;
    let code = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status.code();
        }
        if Instant::now() > until {
            let _ = child.kill();
            break None;
        }
        thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(code, Some(1));
}

#[test]
fn on_with_no_home_stops_with_78() {
    let dir = home("proxy-nohome", "http://127.0.0.1:9/v1");
    let addr = format!("127.0.0.1:{}", free_port());
    let out = proxy(dir.path(), "1", &addr, &[])
        .env_remove("TEHAS_HOME")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(78), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("TEHAS_HOME"));
}

//! `tehas mcp` and `tehasbot mcp`: the settings it refuses, the container it
//! runs the MCP server in, and the keys it makes. bin/tehasbot and bin/tehas
//! of this checkout run with this build's console, against a stub docker that
//! records its calls and what the container would be given, and the real
//! tehas-cycle in the home. Each test is a case of the shell file it
//! replaced, by the same name.

use regex::Regex;
use std::cell::RefCell;
use std::fs::{self, File, FileTimes};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Once;
use std::time::{Duration, SystemTime};
use tehas_core::home::{Home, State, TokenDir};
use tehas_core::testdir::TestDir;
use tehas_core::testing::{link_exe, write_exe};

const MINIMAL: &[&str] = &[
    "TEHAS_GITHUB_APP_ID=1",
    "TEHAS_REPO=acme/widget",
    "TEHAS_CONTROL_ISSUE=1",
    "TEHAS_ALLOWED_LABELERS=alice",
    "PI_ENSEMBLE_SUBAGENT_MODEL=test-model",
    "PI_ENSEMBLE_SUBAGENT_PROVIDER=test-provider",
];

const KEY1: &str = "abcdefghijklmnopqrstuvwxyz012345";
const KEY2: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ-_6789";
const USAGE: &str = "usage: tehasbot mcp serve | fetch | tehas mcp key";

/// The stub docker: the network the server runs on exists and lets nothing
/// out, and a run writes the keys and the sums it was given, by name, to
/// `$T/keys.seen` and `$T/mask.seen`.
const DOCKER: &str = r#"case "$*" in "network inspect"*) echo false ;; esac
[ "$1" != run ] || printf "%s\n%s\n" "${TEHAS_MCP_KEYS:-none}" "${TEHAS_MCP_DASHBOARD_KEYS:-none}" >"$T/keys.seen"
[ "$1" != run ] || printf "%s\n" "${TEHAS_MCP_MASK:-none}" >"$T/mask.seen""#;

/// The programs no stub may leave to the host's own.
const DENIED: &str = "docker gh systemctl curl ssh";

fn digest() -> String {
    format!("sha256:{}", "a".repeat(64))
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// tehas-cycle of this build: above the harness when the workspace built it,
/// built once here otherwise.
fn cycle_binary() -> PathBuf {
    fn above() -> Option<PathBuf> {
        let exe = std::env::current_exe().unwrap();
        exe.ancestors()
            .skip(1)
            .map(|dir| dir.join("tehas-cycle"))
            .find(|p| p.is_file())
    }
    if let Some(found) = above() {
        return found;
    }
    static BUILT: Once = Once::new();
    BUILT.call_once(|| {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let target = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target"));
        let out = Command::new("cargo")
            .args([
                "build",
                "--locked",
                "--offline",
                "--quiet",
                "--manifest-path",
            ])
            .arg(&manifest)
            .arg("--target-dir")
            .arg(&target)
            .args(["-p", "tehas-cycle"])
            .args((!cfg!(debug_assertions)).then_some("--release"))
            .output()
            .expect("cargo");
        assert!(
            out.status.success(),
            "tehas-cycle did not build: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    });
    above().expect("tehas-cycle of this build is above the test binary")
}

fn which(name: &str) -> String {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .map(|d| Path::new(d).join(name))
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("/usr/bin/{name}"))
}

fn id(flag: &str) -> String {
    let out = Command::new("id").arg(flag).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[derive(Debug)]
struct Run {
    rc: i32,
    out: String,
}

/// One host, as reset() leaves it.
struct Host {
    t: TestDir,
    th: PathBuf,
    bash: String,
    cycle: PathBuf,
    instance_on: RefCell<bool>,
    env: RefCell<Vec<(String, String)>>,
}

impl Host {
    fn new(name: &str) -> Host {
        let t = TestDir::new(&format!("mcp-{name}"));
        for d in ["bin", "deny", "tmp", "home"] {
            fs::create_dir_all(t.join(d)).unwrap();
        }
        let cycle = t.join("cycle-bin");
        link_exe(cycle_binary(), &cycle).unwrap();
        let h = Host {
            th: t.join("home/tehas"),
            bash: which("bash"),
            cycle,
            t,
            instance_on: RefCell::new(false),
            env: RefCell::new(vec![]),
        };
        for name in DENIED.split(' ') {
            write_exe(
                h.t.join("deny").join(name),
                format!(
                    "#!{}\necho \"{name} $*\" >>\"$T/denied\"\necho \"no stub stands in for {name}\" >&2\nexit 97\n",
                    h.bash
                ),
            )
            .unwrap();
        }
        h.reset();
        h
    }

    /// The home again, with the server's binary, tehas-cycle, the pins and
    /// the version installed, the default stub docker, the minimal instance
    /// and no variable of a run but the console.
    fn reset(&self) {
        let _ = fs::remove_dir_all(&self.th);
        let _ = fs::remove_dir_all(self.t.join("calls"));
        let _ = fs::remove_file(self.t.join("keys.seen"));
        let _ = fs::remove_file(self.t.join("mask.seen"));
        for d in ["libexec", "docs"] {
            fs::create_dir_all(self.th.join(d)).unwrap();
        }
        fs::create_dir_all(Home::new(&self.th).live_dir()).unwrap();
        fs::create_dir_all(self.t.join("calls")).unwrap();
        write_exe(self.th.join("libexec/tehas-mcp"), "binary\n").unwrap();
        link_exe(&self.cycle, self.th.join("libexec/tehas-cycle")).unwrap();
        self.versions(&format!("CADDY_VERSION=2.8.4\nCADDY_DIGEST={}\n", digest()));
        fs::write(self.th.join("docs/version.txt"), "v0.7.30+abc1234\n").unwrap();
        self.stub("docker", DOCKER);
        self.minimal(&[]);
        self.set_env(&[]);
    }

    fn versions(&self, text: &str) {
        fs::write(self.th.join("versions.env"), text).unwrap();
    }

    fn stub(&self, name: &str, body: &str) {
        let text = format!(
            "#!{}\nprintf '%s\\n' \"$*\" >>\"$T/calls/{name}\"\n{body}\n",
            self.bash
        );
        write_exe(self.t.join("bin").join(name), text).unwrap();
    }

    fn instance(&self, lines: &[&str]) {
        let dir = self.t.join("instance");
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        let mut text = lines.join("\n");
        if !text.is_empty() {
            text.push('\n');
        }
        fs::write(dir.join("instance.env"), text).unwrap();
        *self.instance_on.borrow_mut() = true;
        let home = Home::new(&self.th);
        if !home.state(State::BotLogin).exists() {
            let until = tehas_core::time::now() + 86400;
            for (dir, token) in [
                (TokenDir::App, "app"),
                (TokenDir::Cycle, "cycle"),
                (TokenDir::Session, "session"),
            ] {
                let d = home.token_dir(dir);
                write(d.join("token"), &format!("ghs_{token}\n"));
                write(d.join("token.expires"), &format!("{until}\n"));
            }
            write(home.state(State::BotLogin), "tehas-bot[bot]\n");
        }
    }

    fn minimal(&self, more: &[&str]) {
        let mut lines = MINIMAL.to_vec();
        lines.extend_from_slice(more);
        self.instance(&lines);
    }

    fn no_instance(&self) {
        *self.instance_on.borrow_mut() = false;
        fs::remove_dir_all(self.t.join("instance")).unwrap();
    }

    /// The variables of every later run beside the console, in place of those
    /// before (`t_env`).
    fn set_env(&self, pairs: &[&str]) {
        *self.env.borrow_mut() = pairs
            .iter()
            .map(|p| {
                let (k, v) = p.split_once('=').expect("K=V");
                (k.to_string(), v.to_string())
            })
            .collect();
    }

    fn command(&self, program: impl AsRef<Path>) -> Command {
        let mut c = Command::new(program.as_ref());
        let path = format!(
            "{}:{}:{}",
            self.t.join("bin").display(),
            self.t.join("deny").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        c.env_clear()
            .env("PATH", path)
            .env("HOME", self.t.join("home"))
            .env("TEHAS_HOME", &self.th)
            .env("T", self.t.path())
            .env("T_ROOT", root())
            .env("CARGO_TARGET_DIR", self.t.join("target"))
            .env("TEHAS_SETTINGS_CMD", env!("CARGO_BIN_EXE_console"))
            .env("TEHAS_TEST_STORE_ANSWERS", self.t.join("store-answers"))
            .env("TMPDIR", self.t.join("tmp"));
        if *self.instance_on.borrow() {
            c.env("TEHAS_INSTANCE_DIR", self.t.join("instance"));
        }
        c.env("TEHAS_CONSOLE", env!("CARGO_BIN_EXE_console"));
        c.envs(self.env.borrow().iter().map(|(k, v)| (k, v)));
        c
    }

    /// bin/tehasbot mcp with these words.
    fn mcp(&self, words: &[&str]) -> Run {
        self.bin("tehasbot", &[&["mcp"], words].concat())
    }

    /// bin/<name> of this checkout with these words.
    fn bin(&self, name: &str, words: &[&str]) -> Run {
        let mut c = self.command(root().join("bin").join(name));
        c.args(words);
        run(c)
    }

    fn serve(&self) -> Run {
        self.mcp(&["serve"])
    }

    /// What the stub docker was asked, one call a line, the last newline
    /// gone.
    fn docker(&self) -> String {
        fs::read_to_string(self.t.join("calls/docker"))
            .unwrap_or_default()
            .trim_end_matches('\n')
            .to_string()
    }

    /// The run line's words from `--<from>` to its end.
    fn run_tail(&self, from: &str) -> String {
        self.docker()
            .lines()
            .filter(|l| l.starts_with("run "))
            .filter_map(|l| l.find(from).map(|i| l[i..].to_string()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// A file the stub wrote under `$T`, its last newline gone.
    fn seen(&self, name: &str) -> String {
        fs::read_to_string(self.t.join(name))
            .unwrap_or_default()
            .trim_end_matches('\n')
            .to_string()
    }

    /// A file under `$T/home`.
    fn home_file(&self, rel: &str, text: &str) {
        write(self.t.join("home").join(rel), text);
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        let denied = fs::read_to_string(self.t.join("denied")).unwrap_or_default();
        assert!(denied.is_empty(), "no stub stands in for:\n{denied}");
    }
}

fn run(mut c: Command) -> Run {
    let (mut r, w) = std::io::pipe().unwrap();
    c.stdin(Stdio::null())
        .stdout(w.try_clone().unwrap())
        .stderr(w);
    let mut child = c.spawn().unwrap();
    drop(c);
    let mut out = String::new();
    std::io::Read::read_to_string(&mut r, &mut out).unwrap();
    let status = child.wait().unwrap();
    Run {
        rc: status.code().unwrap_or(-1),
        out: out.trim_end_matches('\n').to_string(),
    }
}

fn write(p: impl AsRef<Path>, text: &str) {
    let p = p.as_ref();
    if let Some(d) = p.parent() {
        fs::create_dir_all(d).unwrap();
    }
    fs::write(p, text).unwrap();
}

#[track_caller]
fn has(got: &str, want: &str) {
    assert!(got.contains(want), "{want:?} is not in:\n{got}");
}

#[track_caller]
fn lacks(got: &str, want: &str) {
    assert!(!got.contains(want), "{want:?} should not be in:\n{got}");
}

#[track_caller]
fn rc(r: &Run, want: i32) {
    assert_eq!(r.rc, want, "exit {} and not {want}:\n{}", r.rc, r.out);
}

/// The output is one key and nothing else.
#[track_caller]
fn a_key(out: &str) {
    assert!(
        Regex::new("^[A-Za-z0-9_-]{43}$").unwrap().is_match(out),
        "not a key: {out}"
    );
}

/// The mode of a path, as `stat -c %a` prints it.
fn mode(p: impl AsRef<Path>) -> String {
    format!(
        "{:o}",
        fs::metadata(p).unwrap().permissions().mode() & 0o7777
    )
}

#[test]
fn off_or_unset_it_runs_nothing_and_removes_a_container_left_running() {
    let h = Host::new("off");
    for setting in ["", "TEHAS_MCP=0"] {
        h.reset();
        if !setting.is_empty() {
            h.set_env(&[setting, &format!("TEHAS_MCP_KEYS={KEY1}")]);
        }
        let r = h.serve();
        rc(&r, 0);
        assert_eq!(h.docker(), "rm -f tehas-mcp", "{setting:?}");
    }
}

// Both settings go through the one check of their shape, since a key of either
// reaches the same program: at least one key stands in one list or the other,
// and every key of the right shape. The name of a setting is said and never the
// value of a key. That the server reads the two lists apart, refuses a host with
// no key at all, and refuses a key of both lists whose scope would be a guess,
// are the server's own rules and are tested of the binary.
//
// One run per row of: full list, dashboard list, the status, a word the line
// must hold, and the two values the container was given, which is also how the
// run is said to have started at all: a run of no keys never got there. The
// keys reach the container by name, from the environment, so the file the stub
// writes is read as the values it was given.
#[test]
fn the_keys_of_both_lists_are_checked_and_both_reach_the_container_by_name() {
    let h = Host::new("keys");
    let check = |full: &str, dash: &str, want: i32, word: &str, values: &str| {
        h.reset();
        h.set_env(&[
            "TEHAS_MCP=1",
            &format!("TEHAS_MCP_KEYS={full}"),
            &format!("TEHAS_MCP_DASHBOARD_KEYS={dash}"),
        ]);
        let r = h.serve();
        rc(&r, want);
        if !word.is_empty() {
            has(&r.out, word);
        }
        if !values.is_empty() {
            assert_eq!(h.seen("keys.seen"), values, "{full:?} {dash:?}");
        }
        if !full.is_empty() {
            lacks(&r.out, full);
        }
        if !dash.is_empty() {
            lacks(&r.out, dash);
        }
    };
    let short1 = &KEY1[..31];
    let short2 = format!("{}+", &KEY2[..31]);
    check(KEY1, "", 0, "", &format!("{KEY1}\nnone"));
    check("", KEY1, 0, "", &format!("none\n{KEY1}"));
    check(KEY1, KEY2, 0, "", &format!("{KEY1}\n{KEY2}"));
    check("", "", 78, "TEHAS_MCP_DASHBOARD_KEYS", "");
    check("   ", "   ", 78, "TEHAS_MCP_KEYS", "");
    check(short1, "", 78, "TEHAS_MCP_KEYS", "");
    // The load refuses a key of the wrong shape first, as the host setting it is.
    check(
        "........",
        KEY2,
        78,
        "TEHAS_MCP_KEYS is not valid: key 1",
        "",
    );
    check(KEY1, &short2, 78, "TEHAS_MCP_DASHBOARD_KEYS", "");
    check(KEY1, KEY1, 0, "", &format!("{KEY1}\n{KEY1}"));
    check(
        &format!("{KEY1} {KEY2}"),
        KEY2,
        0,
        "",
        &format!("{KEY1} {KEY2}\n{KEY2}"),
    );
    check(
        &format!("{KEY1} {KEY2}"),
        KEY1,
        0,
        "",
        &format!("{KEY1} {KEY2}\n{KEY1}"),
    );
    h.reset();
    h.set_env(&[
        "TEHAS_MCP=1",
        &format!("TEHAS_MCP_KEYS={KEY1} {KEY2}"),
        &format!("TEHAS_MCP_DASHBOARD_KEYS={KEY2}"),
    ]);
    let r = h.serve();
    rc(&r, 0);
    lacks(&h.docker(), KEY2);
    // The run line of a server that started names both settings and holds no
    // key of either, which the rows above say of the checks that refused to
    // start.
    h.reset();
    h.set_env(&[
        "TEHAS_MCP=1",
        &format!("TEHAS_MCP_KEYS={KEY1}"),
        &format!("TEHAS_MCP_DASHBOARD_KEYS={KEY2}"),
    ]);
    h.serve();
    has(
        &h.docker(),
        "-e TEHAS_MCP_KEYS -e TEHAS_MCP_DASHBOARD_KEYS -e TEHAS_MCP_MASK --entrypoint",
    );
    lacks(&h.docker(), KEY1);
    lacks(&h.docker(), KEY2);
}

#[test]
fn a_value_of_tehas_mcp_other_than_0_or_1_is_78() {
    let h = Host::new("value");
    h.set_env(&["TEHAS_MCP=yes", &format!("TEHAS_MCP_KEYS={KEY1}")]);
    let r = h.serve();
    rc(&r, 78);
    has(&r.out, "TEHAS_MCP");
    assert_eq!(h.docker(), "");
}

#[test]
fn keys_in_the_instance_directory_are_refused_it_is_in_a_repository() {
    let h = Host::new("instance-keys");
    h.minimal(&[&format!("TEHAS_MCP_KEYS={KEY1}")]);
    h.set_env(&["TEHAS_MCP=1"]);
    let r = h.serve();
    rc(&r, 78);
    has(&r.out, ".config/tehas/env");
    assert_eq!(h.docker(), "");
}

// The container is a security boundary: these are all the arguments it gets.
// A change here widens or narrows what a caller on the internet can reach.
#[test]
fn on_it_runs_the_binary_in_a_locked_down_container_with_live_mcp_and_logs_read_only_and_nothing_else(
) {
    let h = Host::new("locked");
    h.minimal(&[
        "TEHAS_HOST_ADDR=192.0.2.9",
        "PI_ENSEMBLE_HOST_ALIASES=gpu1:192.0.2.10",
    ]);
    h.home_file(
        ".pi/agent/models.json",
        "{\"providers\":{\"test-provider\":{\"baseUrl\":\"http://models.lan:8086/v1\",\"apiKey\":\"unused\"}}}\n",
    );
    h.home_file(
        ".gitconfig",
        "[user]\n\tname = Test Bot\n\temail = bot@example.com\n",
    );
    h.set_env(&["TEHAS_MCP=1", &format!("TEHAS_MCP_KEYS={KEY1}\t{KEY2}")]);
    let r = h.serve();
    rc(&r, 0);
    let th = h.th.display();
    let live = Home::new(&h.th).live_dir();
    let want = format!(
        "network inspect --format {{{{index .Options \"com.docker.network.bridge.enable_ip_masquerade\"}}}} tehas-mcp
rm -f tehas-mcp
run --rm --init --name tehas-mcp --user {}:{} --network tehas-mcp --read-only --cap-drop ALL --security-opt no-new-privileges:true --memory 256m --pids-limit 64 -p 192.0.2.9:8082:8082 -v {}:/live:ro -v {th}/mcp:/sessions:ro -v {th}/logs:/logs:ro -v {th}/libexec/tehas-mcp:/tehas-mcp:ro -e TEHAS_MCP_KEYS -e TEHAS_MCP_DASHBOARD_KEYS -e TEHAS_MCP_MASK --entrypoint /tehas-mcp caddy@{} --live /live --sessions /sessions --logs /logs --listen 0.0.0.0:8082 --server-version v0.7.30+abc1234 --private gpu1 --private 192.0.2.10 --private models.lan --private bot@example.com",
        id("-u"),
        id("-g"),
        live.display(),
        digest()
    );
    assert_eq!(h.docker(), want);
    lacks(&h.docker(), KEY1);
    lacks(&h.docker(), KEY2);
}

#[test]
fn with_outsource_on_in_a_test_build_the_server_is_told_to_serve_factory_contractors_and_only_then()
{
    let h = Host::new("outsource");
    h.minimal(&["TEHAS_OUTSOURCE=on"]);
    h.set_env(&[
        "TEHAS_MCP=1",
        &format!("TEHAS_MCP_KEYS={KEY1}"),
        "TEHAS_TEST_FEATURES=outsource",
    ]);
    let r = h.serve();
    rc(&r, 0);
    assert_eq!(
        h.run_tail("--server-version "),
        "--server-version v0.7.30+abc1234 --outsource"
    );
    h.reset();
    h.minimal(&["TEHAS_OUTSOURCE=off"]);
    h.set_env(&[
        "TEHAS_MCP=1",
        &format!("TEHAS_MCP_KEYS={KEY1}"),
        "TEHAS_TEST_FEATURES=outsource",
    ]);
    let r = h.serve();
    rc(&r, 0);
    lacks(&h.docker(), "--outsource");
}

#[test]
fn the_secret_values_the_host_masks_reach_the_container_as_sums_by_name_never_as_values() {
    let h = Host::new("mask");
    let model_key = "Qw8rTy3uIo1pAs6dFg";
    let token = "Zx5cVb7nMm2Lk4Jh";
    h.home_file(
        ".pi/agent/models.json",
        &format!("{{\"providers\":{{\"p\":{{\"baseUrl\":\"http://models.lan:8086/v1\",\"apiKey\":\"{model_key}\"}}}}}}\n"),
    );
    let vars = [
        "TEHAS_MCP=1".to_string(),
        format!("TEHAS_MCP_KEYS={KEY1}"),
        format!("SOME_MODEL_TOKEN={token}"),
    ];
    let vars: Vec<&str> = vars.iter().map(String::as_str).collect();
    h.set_env(&vars);
    let r = h.serve();
    rc(&r, 0);
    has(&h.docker(), "-e TEHAS_MCP_MASK --entrypoint");
    let seen = h.seen("mask.seen");
    // The salt, then the two values and the key of TEHAS_MCP_KEYS.
    assert_eq!(seen.split_whitespace().count(), 4, "{seen}");
    for value in [model_key, token, KEY1] {
        lacks(&seen, value);
        lacks(&h.docker(), value);
    }
    h.reset();
    h.set_env(&vars);
    let r = h.serve();
    rc(&r, 0);
    assert_ne!(
        h.seen("mask.seen"),
        seen,
        "the salt is the same on two starts"
    );
}

#[test]
fn the_private_names_are_those_the_console_is_given_none_under_4_characters_none_without_settings()
{
    let h = Host::new("private");
    h.minimal(&["PI_ENSEMBLE_HOST_ALIASES=gb:192.0.2.10,,v6host:2001:db8::1"]);
    h.set_env(&["TEHAS_MCP=1", &format!("TEHAS_MCP_KEYS={KEY1}")]);
    let r = h.serve();
    rc(&r, 0);
    assert_eq!(
        h.run_tail("--server-version "),
        "--server-version v0.7.30+abc1234 --private 192.0.2.10 --private v6host --private 2001:db8::1"
    );
    h.reset();
    h.set_env(&["TEHAS_MCP=1", &format!("TEHAS_MCP_KEYS={KEY1}")]);
    h.serve();
    lacks(&h.docker(), "--private");
}

#[test]
fn the_names_are_those_of_tehas_cycle_private_role_providers_and_repeats_included() {
    let h = Host::new("private-cycle");
    h.minimal(&[
        "PI_ENSEMBLE_HOST_ALIASES=gb:192.0.2.10,,v6host:2001:db8::1",
        "PI_ENSEMBLE_MODEL_DEVELOPER=big",
        "PI_ENSEMBLE_PROVIDER_DEVELOPER=test-provider",
        "PI_ENSEMBLE_MODEL_EXPLORE=m",
        "PI_ENSEMBLE_PROVIDER_EXPLORE=role-provider",
    ]);
    h.home_file(
        ".pi/agent/models.json",
        r#"{"providers":{"test-provider":{"baseUrl":"http://user@models.lan:8086/v1","backupBaseUrl":"https://[2001:db8::2]:1/v1"},
"role-provider":{"baseUrl":"https://gb/v1","backupBaseUrl":"role.lan"}}}
"#,
    );
    h.home_file(".gitconfig", "[user]\n\temail = bot@example.com\n");
    h.set_env(&["TEHAS_MCP=1", &format!("TEHAS_MCP_KEYS={KEY1}")]);
    let r = h.serve();
    rc(&r, 0);
    let given = h.run_tail("--private ").replace("--private ", "");
    let mut c = h.command(&h.bash);
    c.arg("-c").arg(format!(
        ". '{}/lib/config.sh'; tehas_load\n'{}' private \"$HOME/.pi/agent/models.json\" bot@example.com | grep -vx -e --private | tr '\\n' ' '",
        root().display(),
        h.cycle.display()
    ));
    let r = run(c);
    assert_eq!(format!("{given} "), r.out);
    has(&given, "role.lan");
}

#[test]
fn it_makes_the_snapshots_directory_private_before_the_container_runs_so_docker_never_makes_it() {
    let h = Host::new("snapshot");
    h.stub(
        "docker",
        r#"case "$*" in "network inspect"*) echo false ;; esac
[ "$1" != run ] || stat -c %a "$TEHAS_HOME/mcp" >"$T/mode.seen""#,
    );
    h.set_env(&["TEHAS_MCP=1", &format!("TEHAS_MCP_KEYS={KEY1}")]);
    let r = h.serve();
    rc(&r, 0);
    assert_eq!(h.seen("mode.seen"), "700");
    assert_eq!(mode(h.th.join("mcp")), "700");
    h.reset();
    fs::create_dir(h.th.join("mcp")).unwrap();
    fs::set_permissions(h.th.join("mcp"), fs::Permissions::from_mode(0o755)).unwrap();
    h.set_env(&["TEHAS_MCP=1", &format!("TEHAS_MCP_KEYS={KEY1}")]);
    h.serve();
    assert_eq!(mode(h.th.join("mcp")), "700");
}

#[test]
fn without_its_network_it_makes_one_that_has_no_way_out() {
    let h = Host::new("network");
    h.stub(
        "docker",
        r#"case "$*" in "network inspect"*) exit 1 ;; esac"#,
    );
    h.set_env(&["TEHAS_MCP=1", &format!("TEHAS_MCP_KEYS={KEY1}")]);
    let r = h.serve();
    rc(&r, 0);
    has(
        &h.docker(),
        "network create --driver bridge -o com.docker.network.bridge.enable_ip_masquerade=false tehas-mcp",
    );
    has(&h.docker(), "--network tehas-mcp");
}

#[test]
fn a_network_of_that_name_that_lets_the_container_out_is_refused() {
    let h = Host::new("network-out");
    h.stub(
        "docker",
        r#"case "$*" in "network inspect"*) echo "" ;; esac"#,
    );
    h.set_env(&["TEHAS_MCP=1", &format!("TEHAS_MCP_KEYS={KEY1}")]);
    let r = h.serve();
    rc(&r, 1);
    has(&r.out, "tehas-mcp");
    lacks(&h.docker(), "run ");
}

#[test]
fn without_a_host_address_it_binds_the_loopback_address() {
    let h = Host::new("loopback");
    h.set_env(&["TEHAS_MCP=1", &format!("TEHAS_MCP_KEYS={KEY1}")]);
    h.serve();
    has(&h.docker(), " -p 127.0.0.1:8082:8082 ");
}

#[test]
fn a_missing_binary_or_digest_is_1_and_runs_nothing() {
    let h = Host::new("missing");
    fs::remove_file(h.th.join("libexec/tehas-mcp")).unwrap();
    h.set_env(&["TEHAS_MCP=1", &format!("TEHAS_MCP_KEYS={KEY1}")]);
    let r = h.serve();
    rc(&r, 1);
    has(&r.out, "install.sh");
    lacks(&h.docker(), "run ");
    h.reset();
    h.versions("CADDY_DIGEST=latest\n");
    h.set_env(&["TEHAS_MCP=1", &format!("TEHAS_MCP_KEYS={KEY1}")]);
    let r = h.serve();
    rc(&r, 1);
    lacks(&h.docker(), "run ");
}

/// The files under `dir` changed after `since`, as `find -newer -type f`.
fn newer(dir: &Path, since: SystemTime) -> Vec<PathBuf> {
    let mut found = vec![];
    for e in fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        let m = fs::symlink_metadata(&p).unwrap();
        if m.is_dir() {
            found.extend(newer(&p, since));
        } else if m.is_file() && m.modified().unwrap() > since {
            found.push(p);
        }
    }
    found
}

#[test]
fn a_key_is_43_characters_of_the_url_safe_alphabet_new_each_time_and_nothing_is_written() {
    let h = Host::new("key");
    std::thread::sleep(Duration::from_millis(10));
    let before = h.t.join("before-key");
    let f = File::create(&before).unwrap();
    let now = SystemTime::now();
    f.set_times(FileTimes::new().set_accessed(now).set_modified(now))
        .unwrap();
    let since = fs::metadata(&before).unwrap().modified().unwrap();
    let r = h.mcp(&["key"]);
    rc(&r, 0);
    a_key(&r.out);
    let first = r.out;
    let r = h.mcp(&["key"]);
    assert_ne!(r.out, first, "the same key twice");
    assert_eq!(newer(&h.th, since), Vec::<PathBuf>::new());
}

#[test]
fn an_unknown_argument_is_64_whatever_the_settings_hold() {
    let h = Host::new("unknown");
    h.no_instance();
    let r = h.mcp(&["nope"]);
    rc(&r, 64);
    has(&r.out, USAGE);
    assert_eq!(h.docker(), "");
}

#[test]
fn a_word_after_key_or_serve_is_64_whatever_the_settings_hold_and_prints_no_key() {
    let h = Host::new("word-after");
    h.no_instance();
    let any_key = Regex::new("[A-Za-z0-9_-]{43}").unwrap();
    for (bin, words) in [
        ("tehasbot", &["mcp", "key", "--bogus-flag"][..]),
        ("tehasbot", &["mcp", "key", "x"]),
        ("tehasbot", &["mcp", "serve", "--bogus-flag"]),
        ("tehasbot", &["mcp", "serve", "x"]),
        ("tehas", &["mcp", "key", "--bogus-flag"]),
        ("tehas", &["mcp", "serve", "x"]),
    ] {
        let r = h.bin(bin, words);
        rc(&r, 64);
        has(&r.out, USAGE);
        assert!(!any_key.is_match(&r.out), "a key was printed: {}", r.out);
    }
    assert_eq!(h.docker(), "");
}

#[test]
fn both_commands_reach_it_a_key_needs_no_settings_the_server_does() {
    let h = Host::new("both");
    h.no_instance();
    let r = h.bin("tehas", &["mcp", "key"]);
    rc(&r, 0);
    a_key(&r.out);
    let r = h.bin("tehas", &["mcp", "serve"]);
    rc(&r, 78);
    assert_eq!(h.docker(), "");
    let r = h.bin("tehas", &["help", "mcp"]);
    rc(&r, 0);
    has(&r.out, "tehas mcp key");
}

#[test]
fn a_console_run_without_the_load_refuses_and_does_not_remove_the_container() {
    let h = Host::new("no-load");
    let mut c = h.command(env!("CARGO_BIN_EXE_console"));
    c.env_remove("TEHAS_INSTANCE_DIR")
        .env_remove("TEHAS_HOME")
        .env("TEHAS_MCP", "")
        .args(["tehasbot", "mcp", "serve"]);
    let r = run(c);
    rc(&r, 78);
    assert_eq!(h.docker(), "");
}

/// The manual's pages, every file under host/docs.
fn docs_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = vec![];
    for e in fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            found.extend(docs_files(&p));
        } else {
            found.push(p);
        }
    }
    found
}

#[test]
fn the_manual_leaves_mcp_to_the_server_no_page_there_and_no_link_to_it() {
    let docs = root().join("host/docs");
    assert!(
        !docs.join("mcp.md").exists(),
        "host/docs/mcp.md would be served at /mcp"
    );
    assert!(
        docs.join("mcp-server.md").is_file(),
        "host/docs/mcp-server.md is missing"
    );
    let linking: Vec<PathBuf> = docs_files(&docs)
        .into_iter()
        .filter(|p| {
            let text = String::from_utf8_lossy(&fs::read(p).unwrap()).into_owned();
            ["](/mcp)", "href=\"/mcp\"", "/mcp.md)"]
                .iter()
                .any(|w| text.contains(w))
        })
        .collect();
    assert_eq!(linking, Vec::<PathBuf>::new());
}

// tehasbot mcp serve gives the binary no --io-timeout, so the only deadline a
// host ever waits is the default the manual states.
#[test]
fn the_manual_keeps_the_servers_deadline_at_10_seconds_and_names_the_flag_it_is_given_by() {
    let page = fs::read_to_string(root().join("host/docs/mcp-server.md")).unwrap();
    assert!(
        page.contains("must arrive whole within 10 seconds"),
        "the manual no longer gives the 10 second deadline"
    );
    assert!(
        page.contains("--io-timeout"),
        "the manual does not name --io-timeout"
    );
}

#[test]
fn the_flag_is_the_binarys_not_the_containers_the_run_line_does_not_hold_it() {
    let h = Host::new("io-timeout");
    h.set_env(&["TEHAS_MCP=1", &format!("TEHAS_MCP_KEYS={KEY1}")]);
    let r = h.serve();
    rc(&r, 0);
    lacks(&h.docker(), "--io-timeout");
}

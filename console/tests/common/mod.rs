//! What the tests of the factory's state share:
//! `tehasbot status`, `tehasbot health` and `tehas status` through the console,
//! against a stub gh, the real store and a scratch home.
//!
//! The world starts as reset() made it: issues #3 and #4 ready, #5 awaiting a
//! merge, #6 needing a person, the factory not paused, nothing shipped, and a
//! fresh store with today's backup. The fixtures are files directly under `$T`,
//! named as the stubs read them (the comments of GH say which); every stub
//! records its calls in `calls/<name>`.

use serde_json::{json, Value};
use std::cell::RefCell;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Once;
use tehas_core::testdir::TestDir;
use tehas_core::testing::{copy_exe, write_exe};

/// The answers of the stub gh. One answer per search: the count is the only
/// call that asks for a total and no nodes, so that shape tells the two apart.
/// A file `<x>-down` makes its read fail; GH_EDIT_FAILS fails an edit and
/// GH_AUTH is what `gh auth status` exits with.
const GH: &str = r####"gh_graphql() {
  case "$1" in
    *subIssues*) [ ! -e "$T/epics-down" ] || exit 1; cat "$T/epics-page.json" ;;
    *"first: 0) { issueCount }"*) [ ! -e "$T/count-down" ] || exit 1
      cat "$T/count.json" ;;
    *) cat "$T/shipped.json" ;;
  esac
}
[ ! -e "$T/gh-down" ] || exit 1
case "$*" in
  "issue list"*--search*) cat "$T/issues.json" ;;
  "issue list"*) [ ! -e "$T/all-down" ] || exit 1; cat "$T/all.json" ;;
  # The names of the labels of the control issue, as --jq joins them.
  "issue view"*) [ ! -e "$T/control-down" ] || exit 1
    { [ "$(cat "$T/paused")" != true ] || echo tehas:pause; [ ! -e "$T/allhands" ] || echo tehas:all-hands; } | paste -sd " " ;;
  "issue edit"*) [ -z "${GH_EDIT_FAILS:-}" ] || exit 1 ;;
  "auth status") exit "${GH_AUTH:-0}" ;;
  "api repos/acme/widget/issues/"*"/events"*) cat "$T/events.json" 2>/dev/null || exit 1 ;;
  "api graphql"*) [ ! -e "$T/shipped-down" ] || exit 1; gh_graphql "$*" ;;
  # The published releases of the platform; nothing until rel.json is there.
  "api repos/"*"/releases"*) [ ! -e "$T/rel.json" ] || cat "$T/rel.json" ;;
esac"####;

/// The other stubs, by name: what each does after it records its call.
const STUBS: &[(&str, &str)] = &[
    // A release host reads the releases with curl when gh is not signed in;
    // CURL_RC is what curl answers with, and 22 is no answer at all.
    (
        "curl",
        r#"[ ! -e "${CURL_RC:-}" ] && exit "${CURL_RC:-0}"; exit 0"#,
    ),
    // No sandbox image: the label of the one cycles run is not found.
    ("docker", "exit 1"),
    // The failed tehas units: $T/failed-units, and SYSTEMCTL_RC.
    (
        "systemctl",
        r#"cat "$T/failed-units" 2>/dev/null; exit "${SYSTEMCTL_RC:-0}""#,
    ),
];

/// The values of a minimal valid instance (T_MINIMAL).
pub const MINIMAL: &[&str] = &[
    "TEHAS_GITHUB_APP_ID=1",
    "TEHAS_REPO=acme/widget",
    "TEHAS_CONTROL_ISSUE=1",
    "TEHAS_ALLOWED_LABELERS=alice",
    "PI_ENSEMBLE_SUBAGENT_MODEL=test-model",
    "PI_ENSEMBLE_SUBAGENT_PROVIDER=test-provider",
];

/// The commit a fresh store is made with (T_COMMIT).
pub const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

/// The logins whose user ids install.sh records, as the shell tests list them.
const LOGINS: &[&str] = &[
    "tehas-bot",
    "alice",
    "bob",
    "bob-2",
    "carol",
    "boss",
    "tehas-fixer",
    "tehas-mill",
    "millbot",
    "mill-bot",
    "acme-millwright",
    "acme-app-millwright",
];

/// What reset() takes away, under `$T`.
const RESET: &[&str] = &[
    "gh-down",
    "all-down",
    "shipped-down",
    "count-down",
    "control-down",
    "allhands",
    "epics-down",
    "rel.json",
    "home/tehas/live",
    "home/tehas/state",
    "home/tehas/current",
    "home/tehas/versions",
    "home/tehas/versions.env",
];

/// The checkout of this source: lib/ and bin/ come from it.
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// The store of this build: above the harness when the workspace built it,
/// built once here otherwise.
fn store_binary() -> PathBuf {
    fn above() -> Option<PathBuf> {
        let exe = std::env::current_exe().unwrap();
        exe.ancestors()
            .skip(1)
            .map(|dir| dir.join("store"))
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
            .args(["-p", "store"])
            .args((!cfg!(debug_assertions)).then_some("--release"))
            .output()
            .expect("cargo");
        assert!(
            out.status.success(),
            "the store did not build: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    });
    above().expect("the store of this build is above the test binary")
}

/// A program on the PATH of the tests, by its path.
fn which(name: &str) -> String {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .map(|d| Path::new(d).join(name))
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("/usr/bin/{name}"))
}

/// A hard link of a built program, or a copy where the two cannot link.
fn place(from: &Path, to: &Path) {
    let _ = fs::remove_file(to);
    if fs::hard_link(from, to).is_err() {
        copy_exe(from, to).unwrap();
    }
}

/// The end of one run: its exit status and what it printed, both streams.
#[derive(Debug)]
pub struct Run {
    pub rc: i32,
    pub out: String,
}

/// One host, as reset() leaves it.
pub struct World {
    t: TestDir,
    /// `~/tehas` of the host.
    pub home: PathBuf,
    /// `live/` of the home, where status writes.
    pub live: PathBuf,
    bash: String,
    console: PathBuf,
    instance: RefCell<bool>,
    env: RefCell<Vec<(String, String)>>,
    always: RefCell<Vec<(String, String)>>,
}

#[allow(dead_code)] // each test file uses a part of this module
impl World {
    pub fn new(name: &str) -> World {
        let t = TestDir::new(&format!("status-{name}"));
        let home = t.join("home/tehas");
        for d in ["bin", "calls", "deny", "tmp", "tree/bin", "tree/libexec"] {
            fs::create_dir_all(t.join(d)).unwrap();
        }
        fs::create_dir_all(home.join("libexec")).unwrap();
        copy_exe(store_binary(), t.join("store-bin")).unwrap();
        let console = t.join("console-bin");
        place(Path::new(env!("CARGO_BIN_EXE_console")), &console);
        // The command as a host has it: bin/tehasbot above the console, with
        // lib/ beside them.
        std::os::unix::fs::symlink(root().join("lib"), t.join("tree/lib")).unwrap();
        place(&console, &t.join("tree/libexec/console"));
        std::os::unix::fs::symlink(root().join("bin/tehasbot"), t.join("tree/bin/tehasbot"))
            .unwrap();
        let w = World {
            live: home.join("live"),
            home,
            bash: which("bash"),
            console,
            t,
            instance: RefCell::new(false),
            env: RefCell::new(vec![]),
            always: RefCell::new(vec![]),
        };
        for name in ["docker", "gh", "systemctl", "curl", "ssh"] {
            write_exe(
                w.t.join("deny").join(name),
                format!(
                    "#!{}\necho \"{name} $*\" >>\"$T/denied\"\necho \"no stub stands in for {name}\" >&2\nexit 97\n",
                    w.bash
                ),
            )
            .unwrap();
        }
        w.gh_stub(GH);
        for (name, body) in STUBS {
            w.stub(name, body);
        }
        w.fx("gh-n.lock", "");
        fs::set_permissions(w.t.join("gh-n.lock"), fs::Permissions::from_mode(0o600)).unwrap();
        w.env_always("TEHAS_CONSOLE", &w.console.to_string_lossy());
        w.reset();
        w
    }

    /// The world of the shell's reset(): the fixtures and the home of a case
    /// before it says anything, the minimal instance, and a fresh store with
    /// today's backup.
    pub fn reset(&self) {
        self.clear_calls();
        for rel in RESET {
            let p = self.t.join(rel);
            if p.is_dir() && !p.is_symlink() {
                let _ = fs::remove_dir_all(&p);
            } else {
                let _ = fs::remove_file(&p);
            }
        }
        self.fx("all.json", "[]\n");
        self.epics_page(&[]);
        self.shipped(&[]);
        self.shipped_count(0);
        self.env.borrow_mut().clear();
        self.minimal(&[]);
        self.store_home();
        self.store_ok(&["backup"]);
        let issues = [
            issue(3, "tehas:ready", "OPEN"),
            issue(4, "tehas:ready", "OPEN"),
            issue(5, "tehas:awaiting-merge", "OPEN"),
            issue(6, "needs-human-attention", "OPEN"),
        ];
        self.fx("issues.json", &format!("[{}]\n", issues.join(",")));
        self.fx("paused", "false\n");
        self.fx("events.json", "[]\n");
    }

    /// The real store in the home, a fresh database and the account ids
    /// (t_store_home).
    fn store_home(&self) {
        for f in ["store-answers", "store-answers.n", "calls/store"] {
            let _ = fs::remove_file(self.t.join(f));
        }
        place(&self.t.join("store-bin"), &self.home.join("libexec/store"));
        self.store_db();
        let ids: String = LOGINS.iter().map(|l| format!("{l} {}\n", uid(l))).collect();
        fs::write(self.home.join("state/account-ids"), ids).unwrap();
    }

    /// The database is gone, with its journal files.
    pub fn no_db(&self) {
        for f in ["tehas.db", "tehas.db-wal", "tehas.db-shm"] {
            let _ = fs::remove_file(self.home.join("state").join(f));
        }
    }

    /// A fresh database, made by the store's own init (t_store_db).
    pub fn store_db(&self) {
        self.no_db();
        fs::create_dir_all(self.home.join("state")).unwrap();
        self.store_ok(&[
            "init",
            "--version",
            "v0.0.0",
            "--commit",
            COMMIT,
            "--instance",
            "instance",
            "--repo",
            "acme/widget",
        ]);
    }

    /// The scratch root, `$T` of the stubs.
    pub fn t(&self) -> &Path {
        &self.t
    }

    /// The console of this build, which every run is given as TEHAS_CONSOLE.
    pub fn console(&self) -> &Path {
        &self.console
    }

    /// A file of `live/`.
    pub fn live_file(&self, name: &str) -> PathBuf {
        self.live.join(name)
    }

    /// A JSON file of `live/`, Null when there is none or it is no JSON.
    pub fn live_json(&self, name: &str) -> Value {
        fs::read_to_string(self.live_file(name))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(Value::Null)
    }

    /// `tehasbot status` as a shell snippet: bin/tehasbot of the tree.
    pub fn status_cmd(&self) -> String {
        self.t
            .join("tree/bin/tehasbot status")
            .display()
            .to_string()
    }

    /// `tehasbot health` as a shell snippet: bin/tehasbot of the checkout.
    pub fn health_cmd(&self) -> String {
        root().join("bin/tehasbot health").display().to_string()
    }

    /// `tehas status` as a shell snippet: bin/tehas of the checkout.
    pub fn summary_cmd(&self) -> String {
        root().join("bin/tehas status").display().to_string()
    }

    /// tehasbot status, which writes the queue snapshot and the health file.
    pub fn status(&self) -> Run {
        let mut c = self.command(&self.t.join("tree/bin/tehasbot").to_string_lossy());
        c.arg("status");
        run(c)
    }

    /// tehasbot health.
    pub fn health(&self) -> Run {
        let mut c = self.command(&root().join("bin/tehasbot").to_string_lossy());
        c.arg("health");
        run(c)
    }

    /// tehas status with these words.
    pub fn summary(&self, words: &[&str]) -> Run {
        let mut c = self.command(&root().join("bin/tehas").to_string_lossy());
        c.arg("status").args(words);
        run(c)
    }

    /// A bash snippet in the environment of a run (t_run).
    pub fn sh(&self, snippet: &str) -> Run {
        let mut c = self.command(&self.bash);
        c.args(["-c", snippet]);
        run(c)
    }

    fn command(&self, program: &str) -> Command {
        let mut c = Command::new(program);
        let path = format!(
            "{}:{}:{}",
            self.t.join("bin").display(),
            self.t.join("deny").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        c.env_clear()
            .env("PATH", path)
            .env("HOME", self.t.join("home"))
            .env("TEHAS_HOME", &self.home)
            .env("T", self.t.path())
            .env("T_ROOT", root())
            .env("CARGO_TARGET_DIR", self.t.join("target"))
            .env("TEHAS_SETTINGS_CMD", &self.console)
            .env("TEHAS_TEST_STORE_ANSWERS", self.t.join("store-answers"))
            .env("TMPDIR", self.t.join("tmp"));
        if *self.instance.borrow() {
            c.env("TEHAS_INSTANCE_DIR", self.t.join("instance"));
        }
        c.envs(self.env.borrow().iter().map(|(k, v)| (k, v)))
            .envs(self.always.borrow().iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null());
        c
    }

    /// A stub program on the PATH of a run that records its arguments, then
    /// runs `body`. A body that starts with `#!` is the whole program.
    pub fn stub(&self, name: &str, body: &str) {
        let text = if body.starts_with("#!") {
            body.to_string()
        } else {
            format!(
                "#!{}\nprintf '%s\\n' \"$*\" >>\"$T/calls/{name}\"\n{body}\n",
                self.bash
            )
        };
        write_exe(self.t.join("bin").join(name), text).unwrap();
    }

    /// The stub gh with these answers in place of GH, as `t_stub gh` writes
    /// one: each call is numbered, recorded with a body read from standard
    /// input written out, answered by gh_fail() when it names it, and every
    /// login in its answer is given the test's user id.
    pub fn gh_stub(&self, answers: &str) {
        let text = format!(
            "#!{}\n{GH_HEAD}\nt_answer() {{\n:\n{answers}\n}}\n{GH_TAIL}",
            self.bash
        );
        write_exe(self.t.join("bin/gh"), text).unwrap();
    }

    /// The `n`-th gh call prints `message` on stderr and ends with `rc`.
    /// Several calls may be named.
    pub fn gh_fail(&self, n: u32, rc: u8, message: &str) {
        assert!(n > 0 && rc > 0 && !message.contains(['\t', '\n']));
        let path = self.t.join("gh-fails");
        let mut text = fs::read_to_string(&path).unwrap_or_default();
        assert!(
            !text
                .lines()
                .any(|l| l.split('\t').next() == Some(&n.to_string())),
            "call {n} is already named"
        );
        text.push_str(&format!("{n}\t{rc}\t{message}\n"));
        fs::write(path, text).unwrap();
    }

    /// A stub of the analytics program where the commands look for it
    /// (t_stub_analytics); its calls are `calls("analytics")`.
    pub fn stub_analytics(&self, body: &str) {
        let text = format!(
            "#!{}\nprintf '%s\\n' \"$*\" >>\"$T/calls/analytics\"\n{body}\n",
            self.bash
        );
        write_exe(self.home.join("libexec/analytics"), text).unwrap();
    }

    /// instance.env, as given, and the app's tokens unless they are there
    /// (t_instance).
    pub fn instance(&self, lines: &[&str]) {
        let dir = self.t.join("instance");
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        let mut text = lines.join("\n");
        if !text.is_empty() {
            text.push('\n');
        }
        fs::write(dir.join("instance.env"), text).unwrap();
        *self.instance.borrow_mut() = true;
        if !self.home.join("state/github/login").exists() {
            self.app_world("tehas-bot");
        }
    }

    /// The minimal instance, with more settings after it.
    pub fn minimal(&self, more: &[&str]) {
        let mut lines = MINIMAL.to_vec();
        lines.extend_from_slice(more);
        self.instance(&lines);
    }

    /// No instance directory, and none named to a run (t_no_instance).
    pub fn no_instance(&self) {
        assert!(*self.instance.borrow(), "no instance to remove");
        *self.instance.borrow_mut() = false;
        fs::remove_dir_all(self.t.join("instance")).unwrap();
    }

    /// The factory as the one GitHub App `slug` is: its login and tokens with
    /// hours left (t_app_world).
    pub fn app_world(&self, slug: &str) {
        let until = tehas_core::time::now() + 86400;
        for (dir, token) in [
            ("github", "app"),
            ("github-cycle", "cycle"),
            ("github-session", "session"),
        ] {
            let d = self.home.join("state").join(dir);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("token"), format!("ghs_{token}\n")).unwrap();
            fs::write(d.join("token.expires"), format!("{until}\n")).unwrap();
        }
        fs::write(
            self.home.join("state/github/login"),
            format!("{slug}[bot]\n"),
        )
        .unwrap();
    }

    /// The variables of every later run until reset(), in place of those
    /// before: `T_ENV=(K=V ...)`.
    pub fn set_env(&self, pairs: &[&str]) {
        *self.env.borrow_mut() = pairs
            .iter()
            .map(|p| {
                let (k, v) = p.split_once('=').expect("K=V");
                (k.to_string(), v.to_string())
            })
            .collect();
    }

    /// One more variable of every later run, until reset() (T_ENV+=).
    pub fn env(&self, k: &str, v: &str) {
        self.env.borrow_mut().push((k.into(), v.into()));
    }

    /// A variable of every later run, reset() or not (T_ENV_ALWAYS+=).
    pub fn env_always(&self, k: &str, v: &str) {
        self.always.borrow_mut().push((k.into(), v.into()));
    }

    /// A fixture: a file under `$T`, with these bytes.
    pub fn fx(&self, rel: &str, text: &str) {
        let p = self.t.join(rel);
        if let Some(d) = p.parent() {
            fs::create_dir_all(d).unwrap();
        }
        fs::write(p, text).unwrap();
    }

    /// A fixture file that is there, empty: `gh-down` and the like.
    pub fn touch(&self, rel: &str) {
        self.fx(rel, "");
    }

    /// A fixture file is gone.
    pub fn rm_fx(&self, rel: &str) {
        let _ = fs::remove_file(self.t.join(rel));
    }

    /// A file under the home, with these bytes.
    pub fn home_file(&self, rel: &str, text: &str) {
        let p = self.home.join(rel);
        if let Some(d) = p.parent() {
            fs::create_dir_all(d).unwrap();
        }
        fs::write(p, text).unwrap();
    }

    /// What a file under the home holds, "" when there is none.
    pub fn read_home(&self, rel: &str) -> String {
        fs::read_to_string(self.home.join(rel)).unwrap_or_default()
    }

    /// The closed issues the search for shipped work answers.
    pub fn shipped(&self, nodes: &[String]) {
        self.fx(
            "shipped.json",
            &format!(
                r#"{{"data":{{"search":{{"nodes":[{}]}}}}}}"#,
                nodes.join(",")
            ),
        );
    }

    /// The one page of open issues the read of the epics answers.
    pub fn epics_page(&self, nodes: &[String]) {
        self.fx(
            "epics-page.json",
            &format!(
                r#"{{"data":{{"repository":{{"issues":{{"pageInfo":{{"hasNextPage":false,"endCursor":null}},"nodes":[{}]}}}}}}}}"#,
                nodes.join(",")
            ),
        );
    }

    /// The total of the closed issues labelled tehas:shipped.
    pub fn shipped_count(&self, n: u64) {
        self.fx(
            "count.json",
            &format!(r#"{{"data":{{"search":{{"issueCount":{n}}}}}}}"#),
        );
    }

    /// The state the count cases share: one closed issue a merged pull
    /// request closed, and the count still saying 0.
    pub fn one_shipped(&self) {
        self.reset();
        self.shipped(&[closed(20, "2026-01-02T00:00:00Z", r#"{"merged":true}"#, "")]);
    }

    /// One cycle of issue `n` on the store, as the live view shows it (t_live):
    /// started by hand `started` seconds ago [600], its heartbeat `updated`
    /// seconds ago [5 running, else 180] with `step`, and when not running,
    /// its end then with `status`.
    pub fn one(
        &self,
        n: u64,
        running: bool,
        status: &str,
        step: &str,
        started: Option<i64>,
        updated: Option<i64>,
    ) {
        let now = tehas_core::time::now();
        let ago = updated.unwrap_or(if running { 5 } else { 180 });
        let n_s = n.to_string();
        self.store_must(
            &["cycle-start", &n_s, "--by-hand"],
            Some(now - started.unwrap_or(600)),
            Some(n),
        );
        let attempt = self.attempt(n).to_string();
        let state = if running { "--running" } else { "--ended" };
        self.store_must(
            &[
                "cycle-heartbeat",
                &n_s,
                state,
                "--status",
                status,
                "--step",
                step,
            ],
            Some(now - ago),
            None,
        );
        if !running {
            self.store_must(
                &[
                    "cycle-end",
                    &n_s,
                    "--attempt",
                    &attempt,
                    "--status",
                    status,
                    "--step",
                    step,
                ],
                Some(now - ago),
                Some(n),
            );
        }
    }

    /// `one` of issue 12 with the default times.
    pub fn cycle(&self, running: bool, status: &str, step: &str) {
        self.one(12, running, status, step, None, None);
    }

    /// The hold of the last poll in live/dispatch.json, written 30 seconds
    /// ago: why, its text, the free bytes and the room to await.
    pub fn held(&self, why: &str, hold: &str, free: Value, await_room: Value) {
        fs::create_dir_all(&self.live).unwrap();
        let v = json!({
            "time": tehas_core::time::iso(tehas_core::time::now() - 30),
            "why": why, "hold": hold, "count": 1, "room": 3,
            "committed": 2147483648u64, "max": 21474836480u64, "reserve": "2g",
            "free": free, "awaitRoom": await_room,
        });
        fs::write(self.live.join("dispatch.json"), v.to_string()).unwrap();
    }

    /// What a stub was called with, one call per line.
    pub fn calls(&self, name: &str) -> String {
        fs::read_to_string(self.t.join("calls").join(name)).unwrap_or_default()
    }

    /// The gh calls.
    pub fn gh(&self) -> String {
        self.calls("gh")
    }

    /// The calls are forgotten, and so are the gh answers of gh_fail.
    pub fn clear_calls(&self) {
        let d = self.t.join("calls");
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d).unwrap();
        for f in ["gh-fails", "gh-n"] {
            let _ = fs::remove_file(self.t.join(f));
        }
    }

    fn store_cmd(&self, args: &[&str], now: Option<i64>, lock: Option<u64>) -> Run {
        let mut c = match lock {
            Some(n) => {
                let cycles = self.home.join("state/cycles");
                fs::create_dir_all(&cycles).unwrap();
                let mut c = Command::new("flock");
                c.args(["-w", "5"])
                    .arg(cycles.join(format!("{n}.lock")))
                    .arg(self.t.join("store-bin"));
                c
            }
            None => Command::new(self.t.join("store-bin")),
        };
        c.env("TEHAS_HOME", &self.home).args(args);
        if let Some(t) = now {
            c.env("TEHAS_STORE_TEST_NOW", t.to_string());
        }
        let out = c.stdin(Stdio::null()).output().unwrap();
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        if !out.status.success() {
            text.push_str(&String::from_utf8_lossy(&out.stderr));
        }
        Run {
            rc: out.status.code().unwrap_or(-1),
            out: text.trim_end().to_string(),
        }
    }

    fn store_must(&self, args: &[&str], now: Option<i64>, lock: Option<u64>) {
        let r = self.store_cmd(args, now, lock);
        assert_eq!(r.rc, 0, "the store refused {args:?}: {}", r.out);
    }

    /// A verb of the real store against this home. A refusal carries its
    /// stderr.
    pub fn store(&self, args: &[&str]) -> Run {
        self.store_cmd(args, None, None)
    }

    /// A verb of the store with its clock at `epoch` (t_at).
    pub fn store_at(&self, epoch: i64, args: &[&str]) -> Run {
        self.store_cmd(args, Some(epoch), None)
    }

    /// A verb of the store under the lock of issue `n`, as a cycle runs it.
    pub fn store_locked(&self, n: u64, args: &[&str]) -> Run {
        self.store_cmd(args, None, Some(n))
    }

    /// A verb of the real store; its answer, or a panic.
    pub fn store_ok(&self, args: &[&str]) -> Value {
        let out = self.store(args);
        assert_eq!(out.rc, 0, "store {args:?}: {}", out.out);
        serde_json::from_str(&out.out).unwrap_or(Value::Null)
    }

    /// The attempt on record for issue `n`, 0 for none.
    pub fn attempt(&self, n: u64) -> u64 {
        self.store_ok(&["cycle-show", &n.to_string()])["cycle"]["attempt"]
            .as_u64()
            .unwrap_or(0)
    }

    /// The store of a run answers a call whose arguments are `args`, or begin
    /// with them and a space, with `rc` and `line`, for its first `times`
    /// calls (all for 0), and is the real store otherwise (t_store_answers).
    pub fn store_answers(&self, args: &str, rc: i32, line: &str, times: u32) {
        let times = if times == 0 {
            String::new()
        } else {
            format!("|{times}")
        };
        self.fx("store-answers", &format!("{args}|{rc}|{line}{times}\n"));
        self.rm_fx("store-answers.n");
    }

    /// The store answers as itself again.
    pub fn store_answers_off(&self) {
        self.rm_fx("store-answers");
    }

    /// How many calls the answer matched.
    pub fn store_answered(&self) -> u64 {
        fs::read_to_string(self.t.join("store-answers.n"))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    }
}

impl Drop for World {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        let denied = fs::read_to_string(self.t.join("denied")).unwrap_or_default();
        assert!(denied.is_empty(), "no stub stands in for:\n{denied}");
    }
}

/// The start of the stub gh: the call numbered and recorded, and the answer
/// gh_fail() named for it.
const GH_HEAD: &str = r####"exec 6>>"$T/gh-n.lock" || exit 64
command -p flock 6
raw=$(command -p cat "$T/gh-n" 2>/dev/null)
case $raw in '' | *[!0-9]*) raw=0 ;; esac
n=$((raw + 1))
echo "$n" >"$T/gh-n"
exec 6<&-
call="$*"
case " $* " in
  *" --body-file - "*) body=$(cat); call=${call/--body-file -/--body $body} ;;
  *" --input - "*) body=$(cat); body=$(jq -r '.body // empty' <<<"$body" 2>/dev/null); call=${call/--input -/-f body=$body} ;;
esac
printf '%s\n' "$call" >>"$T/calls/gh"
f_rc=
f_answer=
while IFS=$'\t' read -r f_n f_w f_msg || [ -n "$f_n" ]; do
  [ "$f_n" = "$n" ] || continue
  f_rc=$f_w
  f_answer=$f_msg
done < <(command -p cat "$T/gh-fails" 2>/dev/null)
if [ -n "$f_rc" ]; then
  printf '%s\n' "$f_answer" >&2
  exit "$f_rc"
fi"####;

/// The end of the stub gh: the answer run in the background, so a signal
/// ends it at once, and every login in it given its user id.
const GH_TAIL: &str = r####"t_got=$T/gh-out.$$
exec 3<&0
( t_answer "$@" ) <&3 3<&- >"$t_got" 2>"$t_got.e" &
t_pid=$!
exec 3<&-
trap 'kill -TERM "$t_pid" 2>/dev/null; command -p cat "$t_got.e" >&2; command -p rm -f "$t_got" "$t_got.e"; exit 143' TERM INT
wait "$t_pid"
t_rc=$?
command -p cat "$t_got.e" >&2
if [ "$t_rc" = 0 ] && [ ! -s "$t_got" ]; then
  case "$1 $2 $3 $4" in
    "api users/"*" --jq .id") jq -rn --arg l "${2#users/}" '$l | sub("\\[bot\\]$"; "") | explode | add' >"$t_got" ;;
    "api repos/"*/labels/*" --jq .node_id") t_l=${2##*/}; echo "LA_${t_l#tehas:}" >"$t_got" ;;
  esac
fi
if command -p grep -q '"login"' "$t_got" &&
  jq -c 'walk(if type == "object" and has("login") and (has("id") or has("databaseId") | not)
    then . + {id: (.login | tostring | sub("\\[bot\\]$"; "") | explode | add)} else . end)' "$t_got" >"$t_got.j" 2>/dev/null; then
  command -p cat "$t_got.j"
else
  command -p cat "$t_got"
fi
command -p rm -f "$t_got" "$t_got.j" "$t_got.e"
exit "$t_rc"
"####;

#[allow(dead_code)] // each test file uses a part of this module
/// A closed issue as the search for shipped work returns it; `closer` is
/// JSON, and an empty `label` gives it none.
pub fn closed(n: u64, closed_at: &str, closer: &str, label: &str) -> String {
    let labels = if label.is_empty() {
        String::new()
    } else {
        json!({"name": label}).to_string()
    };
    format!(
        r#"{{"number":{n},"title":"t","url":"u","createdAt":"2025-12-01T00:00:00Z","updatedAt":"{closed_at}","closedAt":"{closed_at}","labels":{{"nodes":[{labels}]}},"timelineItems":{{"nodes":[{{"closer":{closer}}}]}}}}"#
    )
}

#[allow(dead_code)] // each test file uses a part of this module
/// As `closed`, with no labels field at all: a read that said nothing of them.
pub fn no_labels(n: u64, closed_at: &str, closer: &str) -> String {
    format!(
        r#"{{"number":{n},"title":"t","url":"u","updatedAt":"{closed_at}","closedAt":"{closed_at}","timelineItems":{{"nodes":[{{"closer":{closer}}}]}}}}"#
    )
}

#[allow(dead_code)] // each test file uses a part of this module
/// An issue as `gh issue list` returns it, with one label.
pub fn issue(n: u64, label: &str, state: &str) -> String {
    format!(
        r#"{{"number":{n},"title":"t","url":"u","state":"{state}","labels":[{{"name":"{label}"}}],"createdAt":"2025-12-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z","closedAt":null}}"#
    )
}

#[allow(dead_code)] // each test file uses a part of this module
/// The GitHub user id of a login in a test: the sum of its characters.
pub fn uid(login: &str) -> u64 {
    login
        .trim_end_matches("[bot]")
        .chars()
        .map(|c| c as u64)
        .sum()
}

#[allow(dead_code)] // each test file uses a part of this module
/// The ways `value` breaks api/<schema>.schema.json, as test/schema.jq
/// prints them: "" when it fits.
pub fn schema_errors(schema: &str, value: &str) -> String {
    let file = root().join(format!("api/{schema}.schema.json"));
    let filter = root().join("test/schema.jq");
    jq_with(
        &[
            "-r",
            "--slurpfile",
            "schema",
            &file.to_string_lossy(),
            "-f",
            &filter.to_string_lossy(),
        ],
        value,
    )
}

#[allow(dead_code)] // each test file uses a part of this module
/// jq -c `filter` of `input`, its output without the last newline.
pub fn jq(filter: &str, input: &str) -> String {
    jq_with(&["-c", filter], input)
}

fn jq_with(args: &[&str], input: &str) -> String {
    use std::io::Write;
    let mut child = Command::new("jq")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    s.trim_end_matches('\n').to_string()
}

#[allow(dead_code)] // each test file uses a part of this module
/// The epoch of a time `date -d` reads.
pub fn epoch(when: &str) -> i64 {
    let out = Command::new("date")
        .args(["-u", "-d", when, "+%s"])
        .output()
        .unwrap();
    assert!(out.status.success(), "date -d {when}");
    String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
}

#[allow(dead_code)] // each test file uses a part of this module
/// A program run to its end, its two streams as one.
pub fn run(mut c: Command) -> Run {
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

#[allow(dead_code)] // each test file uses a part of this module
/// `want` is in `got`.
#[track_caller]
pub fn has(got: &str, want: &str) {
    assert!(got.contains(want), "{want:?} is not in:\n{got}");
}

#[allow(dead_code)] // each test file uses a part of this module
/// `want` is not in `got`.
#[track_caller]
pub fn lacks(got: &str, want: &str) {
    assert!(!got.contains(want), "{want:?} should not be in:\n{got}");
}

#[allow(dead_code)] // each test file uses a part of this module
/// The lines of `text` that start with `prefix`.
pub fn count_lines(text: &str, prefix: &str) -> usize {
    text.lines().filter(|l| l.starts_with(prefix)).count()
}

#[allow(dead_code)] // each test file uses a part of this module
/// The JSON of a run's output; a panic with the output when it is none.
#[track_caller]
pub fn json_of(r: &Run) -> Value {
    serde_json::from_str(&r.out).unwrap_or_else(|e| panic!("no JSON ({e}):\n{}", r.out))
}

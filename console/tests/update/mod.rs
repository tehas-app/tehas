//! What the tests of `tehas update` share, as test/update_lib.sh had it: a
//! scratch host, the bare repository its platform checkout clones, the stubs
//! and the helpers. git runs for real against the scratch repositories (no
//! network); docker, systemctl, timeout, sudo and the sandbox build are stubs
//! that record their arguments in `calls/<name>`. A run is the console's
//! `update run`, as libexec/update execs it.

#![allow(dead_code)] // each test file uses a part of this module

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tehas_core::testdir::TestDir;
use tehas_core::testing::{copy_exe, write_exe};

/// The instance a minimal valid host has (T_MINIMAL).
pub const MINIMAL: &[&str] = &[
    "TEHAS_GITHUB_APP_ID=1",
    "TEHAS_REPO=acme/widget",
    "TEHAS_CONTROL_ISSUE=1",
    "TEHAS_ALLOWED_LABELERS=alice",
    "PI_ENSEMBLE_SUBAGENT_MODEL=test-model",
    "PI_ENSEMBLE_SUBAGENT_PROVIDER=test-provider",
];

/// The logins whose user ids an install records (T_LOGINS).
pub const LOGINS: &[&str] = &[
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

/// The commit a scratch tree names in COMMIT (T_COMMIT).
pub const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

/// The programs no case may reach without a stub of its own.
const DENIED: &[&str] = &["docker", "gh", "systemctl", "curl", "ssh"];

/// The gh stub's head: the call's number under a lock, and the call recorded
/// with a body from standard input written as an argument.
const GH_HEAD: &str = r#"exec 6>>"$T/gh-n.lock" || exit 64
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
"#;

/// The answers of `Host::gh_fail`: `<n>\t<status>\t<message>` per line.
const GH_FAIL: &str = r#"fails=$(command -p cat "$T/gh-fails" 2>/dev/null)
if [ -n "$fails" ]; then
  f_rc=
  f_answer=
  while IFS=$'\t' read -r f_n f_w f_msg || [ -n "$f_n" ]; do
    [ "$f_n" = "$n" ] || continue
    f_rc=$f_w
    f_answer=$f_msg
  done <<<"$fails"
  if [ -n "$f_rc" ]; then
    printf '%s\n' "$f_answer" >&2
    exit "$f_rc"
  fi
fi
"#;

/// The gh stub's tail: the body's answer, or a user id, or a label's node id,
/// and an id for every login an answer names without one.
const GH_TAIL: &str = r#"}
t_got=$T/gh-out.$$
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
"#;

/// docker of a host whose containers run. A sandbox build's capabilities
/// question is answered with the image of pi-rukas main, its smoke run (told
/// apart by --entrypoint) with a step that started, its vipune check (the run
/// with --cpuset-cpus) quiet.
const DOCKER_UP: &str = r#"if [ "$1" = ps ]; then printf "c1\n"; fi
if [ "$1" = run ]; then
  for a in "$@"; do [ "$a" != --cpuset-cpus ] || { echo quiet; exit 0; }; done
  for a in "$@"; do [ "$a" != --entrypoint ] || {
    cat "$T_ROOT/test/fixtures/smoke-started.json"; exit 0; }; done
  cat "$T_ROOT/test/fixtures/headless-c5ddc0a/capabilities.json"; fi"#;

/// cargo as install.sh runs it, from test/lib.sh's t_stub_cargo: a build
/// writes a line of text where cargo leaves the program, a store build the
/// stand-in, and CONSOLE_BIN, STORE_BIN and CYCLE_BIN hand over a real one.
/// CARGO_OFFLINE, CARGO_RC, CARGO_FAIL and NO_CONSOLE make it fail or build
/// nothing.
const CARGO: &str = r##"pkg=console triple="" prev="" offline=""
for a in "$@"; do
  [ "$prev" != -p ] || pkg=$a
  [ "$prev" != --target ] || triple=$a/
  prev=$a
  [ "$a" != --offline ] || offline=1
done
if [ -n "$offline" ] && [ -n "${CARGO_OFFLINE:-}" ]; then
  case $CARGO_OFFLINE in
    miss) echo "error: no matching package named \"$pkg\" found" >&2
      echo "note: offline mode (via --offline) can sometimes cause surprising resolution failures" >&2 ;;
    fetch_miss) echo "error: failed to get \"$pkg\": attempting to make an HTTP request, but --offline was specified" >&2 ;;
    noise) echo "error: failed to run custom build command for $pkg" >&2
      echo "  process did not exit successfully: build-script-build --offline" >&2 ;;
    fail) echo "error: no matching package named \"$pkg\" found" >&2 ;;
    *)
      echo "test stub: unknown CARGO_OFFLINE=$CARGO_OFFLINE" >&2
      exit 99
      ;;
  esac
  exit 101
fi
if [ "${CARGO_RC:-0}" != 0 ]; then echo "error: could not compile $pkg" >&2; exit "$CARGO_RC"; fi
if [ "${CARGO_FAIL:-}" = "$pkg" ]; then echo "error: could not compile $pkg" >&2; exit 101; fi
if [ "${CARGO_OFFLINE:-}" = fail ]; then echo "error: could not compile $pkg after fetching" >&2; exit 101; fi
[ -z "${NO_CONSOLE:-}" ] || exit 0
mkdir -p "${CARGO_TARGET_DIR:?}/${triple}release"
if [ "$pkg" = store ]; then
  out=$CARGO_TARGET_DIR/${triple}release/store
  if [ -n "${STORE_BIN:-}" ]; then cp "$STORE_BIN" "$out"; exit 0; fi
  { cat "$T/store-stand-in"; echo "# a store built from ${CONSOLE_BUILD:-source}"; } >"$out"
  chmod 755 "$out"
  exit 0
fi
if [ "$pkg" = console ] && [ -n "${CONSOLE_BIN:-}" ]; then
  cp --remove-destination "$CONSOLE_BIN" "$CARGO_TARGET_DIR/${triple}release/console"
  exit 0
fi
if [ "$pkg" = tehas-cycle ] && [ -n "${CYCLE_BIN:-}" ]; then
  cp "$CYCLE_BIN" "$CARGO_TARGET_DIR/${triple}release/tehas-cycle"
  exit 0
fi
case $pkg in analytics) art=an ;; *) art=a ;; esac
with=""
[ "${CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS:-}" != true ] || with=" with debug assertions"
[ "${TEHAS_LAB_BUILD:-}" != on ] || with="$with as a lab build"
flags=$(env | grep -E "^([A-Z0-9_]*RUSTFLAGS|CARGO_PROFILE_RELEASE_[A-Z0-9_]*)=." | grep -vx CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=false | LC_ALL=C sort | tr "\n" " ")
[ -z "$flags" ] || with="$with with ${flags% }"
[ -z "${CARGO_CONFIG_FLAGS:-}" ] || [ -n "${CARGO_ENCODED_RUSTFLAGS+set}${RUSTFLAGS+set}" ] ||
  with="$with with configured $CARGO_CONFIG_FLAGS"
echo "$art $pkg built from ${CONSOLE_BUILD:-source}$with" >"$CARGO_TARGET_DIR/${triple}release/$pkg"
"##;

/// The store install.sh runs after the cargo stub built it: it records each
/// call and says what the store says, or fails when STORE_RC is set.
const STORE_STAND_IN: &str = r##"#!/usr/bin/env bash
printf '%s\n' "$*" >>"$T/calls/store"
if [ "${STORE_RC:-0}" != 0 ]; then echo "store: corrupt the database: a stand-in" >&2; exit "$STORE_RC"; fi
case $1 in
  init) echo "store: made $TEHAS_HOME/state/tehas.db" ;;
  migrate) echo "store: $TEHAS_HOME/state/tehas.db at schema 1, backup $3/tehas-1-0.db" ;;
esac
"##;

/// The checkout this test runs from (T_ROOT).
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// The path of a program on the PATH of the test process.
pub fn which(name: &str) -> String {
    let path = std::env::var("PATH").unwrap_or_default();
    path.split(':')
        .map(|d| Path::new(d).join(name))
        .find(|p| p.is_file())
        .unwrap_or_else(|| panic!("no {name} on the PATH"))
        .display()
        .to_string()
}

/// The end of one run: its exit status and both streams as one, trailing
/// newlines cut as `$(...)` cuts them.
pub struct Run {
    pub rc: i32,
    pub out: String,
}

impl Run {
    /// The last line of what the run printed.
    pub fn last(&self) -> &str {
        self.out.lines().last().unwrap_or("")
    }
}

/// One scratch host: `$T` of the stubs, the tehas home in `home/tehas` and
/// the platform checkout in it.
pub struct Host {
    t: TestDir,
    /// `$th`, the tehas home.
    pub home: PathBuf,
    /// The platform checkout.
    pub checkout: PathBuf,
    /// The record of steps owed.
    pub owed: PathBuf,
    /// Where bootstrap.sh dev writes outside any home, pointed at scratch.
    pub dev_etc: PathBuf,
    pub dev_bin: PathBuf,
    console: PathBuf,
    instance: Option<PathBuf>,
    env: Vec<(String, String)>,
}

impl Host {
    /// A host with a gh that answers user ids, and nothing else stubbed.
    pub fn new(name: &str) -> Host {
        let t = TestDir::new(&format!("update-{name}"));
        for d in ["bin", "calls", "home", "deny", "tmp"] {
            fs::create_dir_all(t.join(d)).unwrap();
        }
        let denied = t.join("denied");
        for d in DENIED {
            write_exe(
                t.join("deny").join(d),
                format!(
                    "#!/usr/bin/env bash\necho \"{d} $*\" >>'{}'\necho \"no stub stands in for {d}\" >&2\nexit 97\n",
                    denied.display()
                ),
            )
            .unwrap();
        }
        let console = t.join("console-bin");
        copy_exe(env!("CARGO_BIN_EXE_console"), &console).unwrap();
        let home = t.join("home/tehas");
        let h = Host {
            checkout: home.join("platform"),
            owed: home.join("state/update-pending"),
            dev_etc: t.join("etc"),
            dev_bin: t.join("usr-local-bin"),
            home,
            t,
            console,
            instance: None,
            env: vec![],
        };
        h.gh("");
        h
    }

    /// The scratch root, `$T`.
    pub fn t(&self) -> &Path {
        &self.t
    }

    /// A path under `$T`.
    pub fn join(&self, p: impl AsRef<Path>) -> PathBuf {
        self.t.join(p)
    }

    /// The console a run is given.
    pub fn console(&self) -> &Path {
        &self.console
    }

    /// A variable of every later run, as T_ENV+= is.
    pub fn env(&mut self, k: &str, v: &str) {
        self.env.push((k.into(), v.into()));
    }

    /// A stub on the PATH of a run that records its arguments, then runs
    /// `body`.
    pub fn stub(&self, name: &str, body: &str) {
        write_exe(
            self.t.join("bin").join(name),
            format!(
                "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >>\"$T/calls/{name}\"\n{body}\n:\n"
            ),
        )
        .unwrap();
    }

    /// The gh stub, with `body` answering first. A call it leaves unanswered
    /// gets a user id or a label's node id, and a login without an id is given
    /// `uid`'s.
    pub fn gh(&self, body: &str) {
        let body = if body.is_empty() { ":" } else { body };
        write_exe(
            self.t.join("bin/gh"),
            format!("#!/usr/bin/env bash\n{GH_HEAD}{GH_FAIL}t_answer() {{\n{body}\n{GH_TAIL}"),
        )
        .unwrap();
        let _ = fs::remove_file(self.t.join("gh-fails"));
        let _ = fs::remove_file(self.t.join("gh-n"));
    }

    /// The `n`th gh call of the case fails with `rc`, saying `msg`.
    pub fn gh_fail(&self, n: u32, rc: u8, msg: &str) {
        use std::io::Write;
        assert!(rc > 0, "gh_fail: no status of 1 to 255");
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.t.join("gh-fails"))
            .unwrap()
            .write_all(format!("{n}\t{rc}\t{msg}\n").as_bytes())
            .unwrap();
    }

    /// What a stub was called with, one call per line.
    pub fn calls(&self, name: &str) -> String {
        fs::read_to_string(self.t.join("calls").join(name))
            .unwrap_or_default()
            .trim_end_matches('\n')
            .to_string()
    }

    /// The calls are forgotten.
    pub fn clear_calls(&self) {
        let d = self.t.join("calls");
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d).unwrap();
    }

    /// instance.env, as given (t_instance).
    pub fn instance_with(&mut self, lines: &[&str]) {
        let dir = self.t.join("instance");
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        let mut text = lines.join("\n");
        if !text.is_empty() {
            text.push('\n');
        }
        fs::write(dir.join("instance.env"), text).unwrap();
        self.instance = Some(dir);
    }

    /// The minimal instance (instance).
    pub fn instance(&mut self) {
        self.instance_with(MINIMAL);
    }

    /// The minimal instance with the model list install.sh wants beside it.
    pub fn instance_models(&mut self) {
        self.instance();
        fs::write(
            self.instance_dir().join("models.json"),
            "{\"providers\": {\"test-provider\": {\"models\": [{\"id\": \"test-model\"}]}}}\n",
        )
        .unwrap();
    }

    /// The instance directory of the runs; a panic without one.
    pub fn instance_dir(&self) -> &Path {
        self.instance
            .as_deref()
            .expect("no instance: call instance()")
    }

    /// The runs have no instance directory, and it is gone.
    pub fn no_instance(&mut self) {
        let dir = self.instance.take().expect("no instance to remove");
        let _ = fs::remove_dir_all(dir);
    }

    /// The user ids an install records, of every login of `LOGINS` (t_ids).
    pub fn ids(&self, home: &Path) {
        fs::create_dir_all(home.join("state")).unwrap();
        let ids: String = LOGINS.iter().map(|l| format!("{l} {}\n", uid(l))).collect();
        fs::write(home.join("state/account-ids"), ids).unwrap();
    }

    /// docker of a host whose containers run.
    pub fn docker_up(&self) {
        self.stub("docker", DOCKER_UP);
    }

    /// docker that answers ps for the docs project only: `docker ps --quiet
    /// --filter label=com.docker.compose.project=<name>` has the label fourth.
    pub fn docker_up_docs(&self) {
        self.stub(
            "docker",
            r#"if [ "$1" = ps ]; then case "$4" in *tehas-docs) printf "c1\n" ;; esac; fi"#,
        );
    }

    /// docker that cannot answer: no daemon, or no compose plugin.
    pub fn docker_no_answer(&self) {
        self.stub(
            "docker",
            r#"if [ "$1" = ps ]; then echo "could not connect to the Docker daemon" >&2; exit 125; fi"#,
        );
    }

    /// systemctl with every unit active.
    pub fn systemctl_active(&self) {
        self.stub(
            "systemctl",
            r#"if [ "$2" = is-active ]; then echo active; fi"#,
        );
    }

    /// systemctl that prints an error and exits 0, as the wrapper some
    /// containers ship in place of systemd does.
    pub fn systemctl_no_answer(&self) {
        self.stub(
            "systemctl",
            r#"if [ "$2" = is-active ]; then echo "systemd is not running in this container"; exit 0; fi"#,
        );
    }

    /// timeout that records what it bounds, then runs the real one.
    pub fn bound_calls(&self) {
        self.stub("timeout", &format!("exec {} \"$@\"", which("timeout")));
    }

    /// The host's own `tehas` in `bin`, as the labels step runs it: by its
    /// path, not by name. `body` is what it does after it records the call.
    pub fn labels_put(&self, bin: &Path, body: &str) {
        fs::create_dir_all(bin).unwrap();
        write_exe(
            bin.join("tehas"),
            format!(
                "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >>'{}'\n{body}\n",
                self.t.join("calls/tehas").display()
            ),
        )
        .unwrap();
    }

    /// The host's tehas makes the labels.
    pub fn labels_ok(&self) {
        self.labels_put(
            &self.home.join("bin"),
            r#"case "$*" in labels) echo "label tehas:ready" ;; esac"#,
        );
    }

    /// The host's tehas fails to make the labels.
    pub fn labels_fail(&self) {
        self.labels_put(
            &self.home.join("bin"),
            r#"case "$*" in labels) echo "tehas labels: could not make tehas:ready" >&2; exit 1 ;; esac"#,
        );
    }

    /// install.sh in `dir` (the checkout for None): it records `install
    /// <args>` in calls/install, then runs `body`. It is not committed here.
    pub fn write_install(&self, body: &str, dir: Option<&Path>) {
        let dir = dir.unwrap_or(&self.checkout);
        let text = format!(
            "#!/usr/bin/env bash\nprintf \"install %s\\n\" \"${{*:-}}\" >>\"{}\"\n{body}\n",
            self.t.join("calls/install").display()
        );
        fs::write(dir.join("install.sh"), text).unwrap();
        fs::set_permissions(dir.join("install.sh"), fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// The follow-ups install.sh asks for when its units, its sandbox files
    /// and both compose servers changed, from the console, in its order.
    pub fn next_lines(&self) -> Vec<String> {
        let out = Command::new(&self.console)
            .args([
                "update",
                "steps",
                "for",
                "systemd/tehas-dispatch.timer",
                "sandbox/Dockerfile",
                "versions.env",
                "edge/docs.compose.yaml",
                "edge/router.compose.yaml",
            ])
            .output()
            .unwrap();
        assert!(out.status.success(), "update steps: {out:?}");
        String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .map(String::from)
            .collect()
    }

    /// install.sh asking for every follow-up it knows, as an install body (ASK).
    pub fn ask_all(&self) -> String {
        let mut s = String::from("echo \"installed: libexec/update\"\n");
        for step in self.next_lines() {
            s.push_str(&format!("echo \"next: {step}\"\n"));
        }
        s
    }

    /// A platform checkout with a remote that holds `behind` commits it does
    /// not, the host's normal answers, and the parts of a host an update
    /// touches. install.sh runs `body`, `echo "installed: nothing"` for "".
    pub fn platform(&self, behind: u32, body: &str) {
        let body = if body.is_empty() {
            "echo \"installed: nothing\""
        } else {
            body
        };
        let _ = fs::remove_dir_all(&self.home);
        let _ = fs::remove_dir_all(self.t.join("origin"));
        let _ = fs::remove_dir_all(self.t.join("upstream"));
        self.clear_calls();
        self.systemctl_active();
        self.docker_up();
        self.bound_calls();
        for d in ["libexec", "lib"] {
            fs::create_dir_all(self.checkout.join(d)).unwrap();
        }
        for d in ["sandbox", "edge"] {
            fs::create_dir_all(self.home.join(d)).unwrap();
        }
        self.ids(&self.home);
        self.write_install(body, None);
        self.labels_ok();
        fs::copy(
            root().join("lib/config.sh"),
            self.checkout.join("lib/config.sh"),
        )
        .unwrap();
        fs::write(self.checkout.join("libexec/work"), "").unwrap();
        self.first_commit();
        write_exe(
            self.home.join("sandbox/build.sh"),
            format!(
                "#!/usr/bin/env bash\nprintf \"%s\\n\" \"build\" >>\"{}/build\"\necho \"built tehas/pi-rukas\"\n",
                self.t.join("calls").display()
            ),
        )
        .unwrap();
        fs::write(self.home.join("edge/docs.compose.yaml"), "").unwrap();
        fs::write(self.home.join("edge/router.compose.yaml"), "").unwrap();
        for i in 1..=behind {
            self.push_one(&format!("change {i}"));
        }
    }

    /// The checkout's first commit, its bare remote and origin/HEAD.
    fn first_commit(&self) {
        let c = &self.checkout;
        git(None, &["init", "-q", "--initial-branch=main", &s(c)]);
        git(Some(c), &["config", "user.name", "test"]);
        git(Some(c), &["config", "user.email", "test@example.com"]);
        git(Some(c), &["add", "-A"]);
        git(Some(c), &["commit", "-qm", "first"]);
        let origin = s(&self.t.join("origin"));
        git(None, &["clone", "--bare", "-q", &s(c), &origin]);
        git(Some(c), &["remote", "add", "origin", &origin]);
        git(
            Some(c),
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        );
    }

    /// A fresh clone of the remote in `$T/upstream`, as another person's.
    fn upstream(&self) -> PathBuf {
        let up = self.t.join("upstream");
        let _ = fs::remove_dir_all(&up);
        git(None, &["clone", "-q", &s(&self.t.join("origin")), &s(&up)]);
        git(Some(&up), &["config", "user.name", "other"]);
        git(Some(&up), &["config", "user.email", "other@example.com"]);
        up
    }

    /// Everything in `up` committed with `msg` and pushed to `branch`.
    fn push_up(&self, up: &Path, msg: &str, branch: &str) {
        git(Some(up), &["add", "-A"]);
        git(Some(up), &["commit", "-qm", msg]);
        git(Some(up), &["push", "-q", "origin", branch]);
    }

    /// One commit from somebody else: `line` added to notes.
    pub fn push_one(&self, line: &str) {
        let up = self.upstream();
        append(&up.join("notes"), &format!("{line}\n"));
        self.push_up(&up, "pushed", "main");
    }

    /// New install.sh on the remote, running `body`.
    pub fn push_install(&self, body: &str) {
        let up = self.upstream();
        self.write_install(body, Some(&up));
        self.push_up(&up, "new install.sh", "main");
    }

    /// A remote whose checkout no longer has `path`.
    pub fn push_delete(&self, path: &str) {
        let up = self.upstream();
        git(Some(&up), &["rm", "-q", path]);
        git(Some(&up), &["commit", "-qm", &format!("removed {path}")]);
        git(Some(&up), &["push", "-q", "origin", "main"]);
    }

    /// A branch of its own on the remote.
    pub fn push_branch(&self, branch: &str) {
        let up = self.upstream();
        git(Some(&up), &["checkout", "-q", "-b", branch]);
        append(&up.join("notes"), &format!("on {branch}\n"));
        self.push_up(&up, &format!("on {branch}"), branch);
    }

    /// A remote whose lib/config.sh requires a setting nothing gives.
    pub fn push_settings_change(&self) {
        let up = self.upstream();
        append(
            &up.join("lib/config.sh"),
            r#"# added by this case: a required setting nothing can give a default for.
tehas_require_instance_settings() {
  [ -n "${TEHAS_NEW_SETTING:-}" ] || { tehas_err "TEHAS_NEW_SETTING is not set (see $TEHAS_INSTANCE_DIR/instance.env)"; exit 78; }
}
"#,
        );
        self.push_up(&up, "a change that adds a required setting", "main");
    }

    /// A remote whose lib/config.sh predates tehas_load.
    pub fn push_old_config(&self) {
        let up = self.upstream();
        fs::write(
            up.join("lib/config.sh"),
            "tehas_require_instance_settings() { return 0; }\n",
        )
        .unwrap();
        self.push_up(
            &up,
            "a lib/config.sh that does not define tehas_load",
            "main",
        );
    }

    /// `<branch> <head> <origin/main>` of the checkout, twelve hex each,
    /// `none` for no origin/main.
    pub fn state(&self) -> String {
        let c = Some(self.checkout.as_path());
        let origin = git_try(c, &["rev-parse", "--short=12", "refs/remotes/origin/main"])
            .unwrap_or_else(|| "none".into());
        format!(
            "{} {} {origin}",
            git(c, &["symbolic-ref", "--quiet", "--short", "HEAD"]),
            self.head_sha()
        )
    }

    /// The checkout's commit, twelve hex.
    pub fn head_sha(&self) -> String {
        git(Some(&self.checkout), &["rev-parse", "--short=12", "HEAD"])
    }

    /// The record of steps owed, "" without one.
    pub fn owed_steps(&self) -> String {
        fs::read_to_string(&self.owed)
            .unwrap_or_default()
            .trim_end_matches('\n')
            .to_string()
    }

    /// The record of steps owed, these lines, as install.sh writes it.
    pub fn record(&self, lines: &[&str]) {
        fs::create_dir_all(self.owed.parent().unwrap()).unwrap();
        let mut text = lines.join("\n");
        text.push('\n');
        fs::write(&self.owed, text).unwrap();
    }

    /// The tehas.rukas label of the sandbox image, `none` for no label or no
    /// image; the rest of docker answers as `docker_up`.
    pub fn rukas_image(&self, label: &str) {
        let fx = self.t.join("fx");
        fs::create_dir_all(&fx).unwrap();
        if label == "none" {
            let _ = fs::remove_file(fx.join("label"));
        } else {
            fs::write(fx.join("label"), format!("{label}\n")).unwrap();
        }
        self.stub(
            "docker",
            &format!(
                r#"{DOCKER_UP}
if [ "$1" = image ]; then case "$*" in
  "image inspect tehas/pi-rukas:release-"*) exit 1 ;;
  *"image inspect"*) [ -e "$T/fx/label" ] || exit 1; cat "$T/fx/label" ;;
esac; fi"#
            ),
        );
    }

    /// The host's clone of pi-rukas at one commit made at one instant, or
    /// none for `none`. `label` is the 40 hex the case names; the sha git
    /// wrote comes back, "" for none.
    pub fn rukas_clone(&self, label: &str) -> String {
        let dir = self.home.join("repos/pi-rukas");
        let _ = fs::remove_dir_all(&dir);
        if label == "none" {
            return String::new();
        }
        assert!(
            label.len() == 40 && label.bytes().all(|b| b.is_ascii_hexdigit()),
            "rukas_clone: {label} is not 40 hex digits"
        );
        fs::create_dir_all(&dir).unwrap();
        let d = Some(dir.as_path());
        git(None, &["init", "-q", "--initial-branch=main", &s(&dir)]);
        git(d, &["config", "user.name", "test"]);
        git(d, &["config", "user.email", "test@example.com"]);
        fs::write(dir.join("README"), "pi-rukas\n").unwrap();
        git(d, &["add", "-A"]);
        let out = Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(["commit", "-q", "--no-gpg-sign", "-m"])
            .arg(format!("pi-rukas at {label}"))
            .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00")
            .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00")
            .output()
            .unwrap();
        assert!(out.status.success(), "rukas_clone: {out:?}");
        git(d, &["rev-parse", "HEAD"])
    }

    /// git as the host has it, but the pi-rukas clone holds every commit and
    /// archives an empty tree.
    pub fn rukas_tree(&self) {
        self.stub(
            "git",
            &format!(
                r#"case "$1 $3" in
  "-C cat-file" | "-C archive") case $2 in */repos/pi-rukas) exit 0 ;; esac ;;
esac
exec '{}' "$@""#,
                which("git")
            ),
        );
    }

    /// cargo as install.sh runs it, and the store stand-in it builds.
    pub fn stub_cargo(&self) {
        self.stub("cargo", CARGO);
        fs::write(self.t.join("store-stand-in"), STORE_STAND_IN).unwrap();
    }

    /// The workspace's source in `dir/console`, without its build, and the
    /// tree's COMMIT.
    pub fn console_source(&self, dir: &Path) {
        let from = root().join("console");
        let to = dir.join("console");
        fs::create_dir_all(&to).unwrap();
        cp(
            &[
                &from.join("Cargo.toml"),
                &from.join("Cargo.lock"),
                &from.join("src"),
            ],
            &to,
        );
        // The workspace's own list, so a new member is copied with no
        // change here.
        let manifest = fs::read_to_string(from.join("Cargo.toml")).unwrap();
        let members = manifest
            .lines()
            .find_map(|l| l.strip_prefix("members = ["))
            .expect("the workspace names its members")
            .trim_end_matches(']')
            .split(',')
            .map(|m| m.trim().trim_matches('"'))
            .filter(|m| !m.is_empty());
        for m in members {
            let d = to.join(m);
            fs::create_dir_all(&d).unwrap();
            cp(
                &[&from.join(m).join("Cargo.toml"), &from.join(m).join("src")],
                &d,
            );
        }
        fs::write(dir.join("COMMIT"), format!("{COMMIT}\n")).unwrap();
    }

    /// The platform checkout with this repository's real install.sh, its
    /// libraries and host/ files, one commit behind its remote.
    pub fn real_platform(&self) {
        let _ = fs::remove_dir_all(self.t.join("home"));
        let _ = fs::remove_dir_all(self.t.join("origin"));
        let _ = fs::remove_dir_all(self.t.join("upstream"));
        self.clear_calls();
        fs::create_dir_all(self.t.join("home")).unwrap();
        self.systemctl_active();
        self.docker_up();
        self.bound_calls();
        self.rukas_tree();
        fs::create_dir_all(&self.checkout).unwrap();
        let r = root();
        cp(
            &[
                &r.join("install.sh"),
                &r.join("bootstrap.sh"),
                &r.join("bin"),
                &r.join("libexec"),
                &r.join("lib"),
                &r.join("host"),
            ],
            &self.checkout,
        );
        self.console_source(&self.checkout);
        self.stub_cargo();
        self.first_commit();
        self.push_one("change 1");
    }

    /// What `sudo ./bootstrap.sh dev` leaves: the marker naming the tehas
    /// user, and the copy of the command other accounts type. sudo is a stub.
    pub fn login_setup(&self) {
        fs::create_dir_all(self.dev_etc.join("tehas")).unwrap();
        fs::create_dir_all(&self.dev_bin).unwrap();
        fs::write(self.dev_etc.join("tehas/manager"), "boss\n").unwrap();
        copy_exe(
            self.checkout.join("host/login/tehas"),
            self.dev_bin.join("tehas"),
        )
        .unwrap();
        self.stub("sudo", "");
    }

    /// The copy of the login command is no longer the checkout's.
    pub fn login_age(&self) {
        append(
            &self.dev_bin.join("tehas"),
            "# the copy an earlier bootstrap.sh dev installed\n",
        );
    }

    /// The copy of the login command, as bytes.
    pub fn login_state(&self) -> Vec<u8> {
        fs::read(self.dev_bin.join("tehas")).unwrap_or_default()
    }

    fn command(&self, program: &Path) -> Command {
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
            .env("TMPDIR", self.t.join("tmp"))
            .env("BOOTSTRAP_ETC", &self.dev_etc)
            .env("BOOTSTRAP_BIN", &self.dev_bin)
            .env("T_IN_TEST", "1");
        if let Some(dir) = &self.instance {
            c.env("TEHAS_INSTANCE_DIR", dir);
        }
        c.envs(self.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .env("TEHAS_CONSOLE", &self.console)
            .env("GH_TOKEN", "ghs_test");
        c
    }

    /// `tehas update <args>`: the console's `update run`, as libexec/update
    /// execs it.
    pub fn update(&self, args: &[&str]) -> Run {
        let mut c = self.command(&self.console);
        c.args(["update", "run"]).arg(&self.console).args(args);
        run(c)
    }

    /// A bash snippet in a run's environment, as t_run runs one.
    pub fn sh(&self, snippet: &str) -> Run {
        let mut c = self.command(Path::new("bash"));
        c.args(["-c", snippet]);
        run(c)
    }

    /// libexec/update of this checkout, for a snippet.
    pub fn update_script(&self) -> String {
        s(&root().join("libexec/update"))
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

/// The order an update ran the steps it was asked for, from its own lines.
pub fn step_order(out: &str) -> String {
    const STEPS: &[&str] = &[
        "systemd: restarted",
        "sandbox: building",
        "docs: updating",
        "edge: updating",
        "labels: making",
    ];
    out.lines()
        .filter_map(|l| STEPS.iter().find(|s| l.starts_with(**s)))
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
}

/// The GitHub user id of a login in a test: the sum of its characters.
pub fn uid(login: &str) -> u64 {
    login
        .trim_end_matches("[bot]")
        .chars()
        .map(|c| c as u64)
        .sum()
}

/// git in `dir` with these words; its output, or a panic.
pub fn git(dir: Option<&Path>, args: &[&str]) -> String {
    git_try(dir, args).unwrap_or_else(|| panic!("git {args:?} in {dir:?} failed"))
}

/// git in `dir` with these words; its output, None when it fails.
pub fn git_try(dir: Option<&Path>, args: &[&str]) -> Option<String> {
    let mut c = Command::new("git");
    if let Some(d) = dir {
        c.arg("-C").arg(d);
    }
    let out = c.args(args).stdin(Stdio::null()).output().unwrap();
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

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

/// `want` is in `got`.
#[track_caller]
pub fn has(got: &str, want: &str) {
    assert!(got.contains(want), "{want:?} is not in:\n{got}");
}

/// `want` is not in `got`.
#[track_caller]
pub fn lacks(got: &str, want: &str) {
    assert!(!got.contains(want), "{want:?} should not be in:\n{got}");
}

/// `text` added to the end of the file at `path`.
pub fn append(path: &Path, text: &str) {
    use std::io::Write;
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
}

/// A path as a string.
pub fn s(p: &Path) -> String {
    p.display().to_string()
}

fn cp(from: &[&Path], to: &Path) {
    let out = Command::new("cp")
        .arg("-R")
        .args(from)
        .arg(to)
        .output()
        .unwrap();
    assert!(out.status.success(), "cp: {out:?}");
}

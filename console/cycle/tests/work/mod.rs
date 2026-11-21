//! What the tests of `tehas work` share, as test/work_lib.sh does for the
//! shell tests: `tehas work` through the console, which execs the tehas-cycle
//! beside it, against stub docker, gh and curl, the real store and a scratch
//! acme/widget whose default branch is main. A stub docker records the words
//! of each run, one per line, in `calls/docker.args.<pid>` and ends at once,
//! so no cycle runs; it answers --capabilities from `$T/caps.json`.
//!
//! The world starts as work_lib.sh leaves it: the minimal instance with a host
//! alias, a slice of 8 GiB with nothing in it, a runner of result schema 5 and
//! nothing on record. Fixtures are files under `$T` and `$T/fx`, named as the
//! stubs read them (the comments of the stubs say which); each stub records
//! its calls in `calls/<name>`. Each test is one world, so what a shell case
//! inherited from the case before it, a test makes itself.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::cell::{Cell, RefCell};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Once;
use tehas_core::testdir::TestDir;
use tehas_core::testing::{copy_exe, write_exe};

/// The settings of a minimal valid instance (T_MINIMAL).
#[allow(dead_code)] // each test file uses a part of this module
pub const MINIMAL: &[&str] = &[
    "TEHAS_GITHUB_APP_ID=1",
    "TEHAS_REPO=acme/widget",
    "TEHAS_CONTROL_ISSUE=1",
    "TEHAS_ALLOWED_LABELERS=alice",
    "PI_ENSEMBLE_SUBAGENT_MODEL=test-model",
    "PI_ENSEMBLE_SUBAGENT_PROVIDER=test-provider",
];

/// The commit a scratch store is initialised with (T_COMMIT).
#[allow(dead_code)] // each test file uses a part of this module
pub const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

/// The runner inside the sandbox image.
#[allow(dead_code)] // each test file uses a part of this module
pub const RUNNER: &str = "/opt/pi-rukas/bin/pi-rukas-headless";

/// Where GitHub keeps an issue's images and its files.
#[allow(dead_code)] // each test file uses a part of this module
pub const A: &str = "https://github.com/user-attachments/assets";
#[allow(dead_code)] // each test file uses a part of this module
pub const F: &str = "https://github.com/user-attachments/files";

/// The first bytes of an image of each kind, as GitHub would hand one out.
#[allow(dead_code)] // each test file uses a part of this module
pub const PNG: &[u8] = b"\x89PNG\r\n\x1a\nPNGDATA";
#[allow(dead_code)] // each test file uses a part of this module
pub const JPG: &[u8] = b"\xff\xd8\xff\xe0JPGDATA";
#[allow(dead_code)] // each test file uses a part of this module
pub const GIF: &[u8] = b"GIF89aGIFDATA";
#[allow(dead_code)] // each test file uses a part of this module
pub const WEBP: &[u8] = b"RIFF\x10\x00\x00\x00WEBPVP8 ";

/// What the docker stub says for a push check the sandbox refused.
#[allow(dead_code)] // each test file uses a part of this module
pub const PUSH_SAYS: &str = "fatal: could not read from remote repository";

/// The options of the slice limits, by name.
#[allow(dead_code)] // each test file uses a part of this module
pub const LIMITS: &str =
    "--cgroup-parent --memory --memory-swap --memory-reservation --cpus --pids-limit --cpu-shares";

/// The options of the slice limits and of the security list, by name: the
/// allow-list of unknown_options for every suite of a cycle's container.
#[allow(dead_code)] // each test file uses a part of this module
pub const DOCKER_OPTS: &str = "--cgroup-parent --memory --memory-swap --memory-reservation --cpus --pids-limit --cpu-shares --cap-drop --cap-add --security-opt --tmpfs";

/// The documented security options of a container, for the ceiling the
/// stubbed slice gives it (--memory 2048m): /tmp is half of it. A literal, not
/// read from console/core/src/sandbox.rs, which is the code under test.
#[allow(dead_code)] // each test file uses a part of this module
pub const SECURITY_OPTS: &str = "--cap-drop ALL --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true --tmpfs /tmp:rw,exec,nosuid,nodev,size=1024m,mode=1777";

/// What the cycle's container is given (AGENTS.md rule 4): its variables, by
/// name, in the order the shell tests list them.
#[allow(dead_code)] // each test file uses a part of this module
pub const WANT: &str = "GH_TOKEN
GIT_CONFIG_COUNT
GIT_CONFIG_KEY_0
GIT_CONFIG_KEY_1
GIT_CONFIG_KEY_2
GIT_CONFIG_KEY_3
GIT_CONFIG_VALUE_0
GIT_CONFIG_VALUE_1
GIT_CONFIG_VALUE_2
GIT_CONFIG_VALUE_3
GIT_TERMINAL_PROMPT
CBM_CACHE_DIR
PI_ENSEMBLE_CYCLE_DEADLINE
PI_ENSEMBLE_HANDOFF_CHANNEL
PI_ENSEMBLE_HANDOFF_TIMEOUT_MS
PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS
PI_ENSEMBLE_MAX_WORKSTREAMS
PI_ENSEMBLE_SANDBOX_MODE
PI_ENSEMBLE_SPAWN_TIMEOUT_MS
PI_ENSEMBLE_SUBAGENT_MODEL
PI_ENSEMBLE_SUBAGENT_PROVIDER
PI_ENSEMBLE_VERIFY_CONCURRENCY
PI_ENSEMBLE_VERIFY_SUITE_TIMEOUT_MS
PI_ENSEMBLE_VERIFY_TIMEOUT_MS
PI_ENSEMBLE_WORKSPACE_ROOT";

/// Alive until docker stop is called, then gone with 137, as a container is.
#[allow(dead_code)] // each test file uses a part of this module
pub const HOLD: &str = r#""$nap" 60 >/dev/null 2>&1 & echo $! >"$T/sleeper.pid""#;
#[allow(dead_code)] // each test file uses a part of this module
pub const GONE: &str = "wait; exit 137";

/// The logins whose user ids install.sh records (T_LOGINS).
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

/// The old record files the store replaced, as <dir>, <prefix>, <suffix>
/// under the home (t_old_state). No case leaves one.
const OLD_STATE: &[(&str, &str, &str)] = &[
    ("state/cycles", "", ".taken"),
    ("state/cycles", "", ".by"),
    ("state/cycles", "", ".requeued"),
    ("state/cycles", "", ".rukas"),
    ("state/cycles", "", ".image"),
    ("state/cycles", "", ".settings"),
    ("state/cycles", "", ".branch"),
    ("state/cycles", "", ".approved.json"),
    ("state/cycles", "", ".approved.json.new"),
    ("state/cycles", "", ".park"),
    ("state/cycles", "", ".park.new"),
    ("state/cycles", "", ".quiet"),
    ("state/cycles", "plan-", ".history.jsonl"),
    ("state", "park-", ".id"),
    ("state", "park-", ".json"),
    ("state", "plan-", ".json"),
    ("state", "plan-", ".json.new"),
    ("state", "plan-", ".json.brief"),
    ("state", "plan-", ".json.answer"),
    ("state", "plan-", ".json.answer.new"),
    ("state", "plan-", ".json.keep"),
    ("state", "plan-", ".turn"),
    ("state", "plan-", ".history.jsonl"),
];

/// The console of the cycle, in place of the program a host has beside the
/// script. It records its words and keeps the two lines beside its file as the
/// console does (console/src/now.rs), read from the log given with --progress.
/// `$T/now.step` and `$T/now.text` are two lines of a case's own, copied
/// instead of the log; `$T/now.off` means no now file at all; `$T/now.wait`
/// waits up to ten seconds for a step-started line in the log. A size verb
/// goes to the real console.
const CONSOLE: &str = r#"[ "${1:-}" != size ] || exec "$T/console-bin" "$@"
prev=
for a in "$@"; do
  case $prev in
    --progress) log=$a ;;
    --now) now=$a ;;
  esac
  prev=$a
done
[ -n "${now:-}" ] || exit 0
[ -e "$T/now.off" ] && exit 0
i=0
while [ -e "$T/now.wait" ] && [ "$i" -lt 200 ] && ! grep -qs step-started "${log:-}"; do
  command -p sleep 0.05
  i=$((i + 1))
done
if [ -e "$T/now.step" ] || [ -e "$T/now.text" ]; then
  step=$(cat "$T/now.step" 2>/dev/null)
  text=$(cat "$T/now.text" 2>/dev/null)
elif [ -f "$log" ]; then
  step=$(jq -r -s "$(cat "$T/now.step.jq")" "$log" 2>/dev/null)
  text=$(jq -r -s "$(cat "$T/now.text.jq")" "$log" 2>/dev/null)
else
  exit 0
fi
printf '%s\n%s\n' "$step" "$text" >"$now.new"
mv "$now.new" "$now""#;

/// The two reads of the log above: one per line, its stable lines only.
const NOW_STEP_JQ: &str = r#"[.[] | objects | select(.v == 1 and (.unstable // false) == false)
  | select(.kind == "step-started") | .event.step | strings] | last // ""
"#;
const NOW_TEXT_JQ: &str = r#"[.[] | objects | select(.v == 1 and (.unstable // false) == false)
  | select(.kind == "log") | .message | strings
  | [.[] | split("\u000a") | .[0]]
  | map(select(. != "" and (startswith("pi-rukas:driver-event") | not))) | last // ""
"#;

/// The cycle clock, which the watch asks through TEHAS_WATCH_CLOCK. By
/// default it counts on with the wall clock from the mark of tehas work's own
/// start. `$T/clock.now` is one reading per call of `clock now`, in order, and
/// `$T/clock.check` the word `clock check` answers with. Any other verb goes to
/// the real program beside it.
const CLOCK: &str = r#"case "$1 $2" in
  "clock mark") date +%s >"$3"; date +%s >"$T/clock.start" ;;
  "clock now")
    if [ -e "$T/clock.now" ]; then
      n=$(cat "$T/clock.n" 2>/dev/null || echo 0); n=$((n + 1))
      echo "$n" >"$T/clock.n"; sed -n "${n}p" "$T/clock.now"
    else
      date +%s
    fi ;;
  "clock check"*)
    echo "$*" >>"$T/clock.calls"
    if [ -e "$T/clock.check" ]; then cat "$T/clock.check"; exit 0; fi
    limit=0 stall=0 since=0 prev=""
    for a in "$@"; do
      case $prev in --limit) limit=$a ;; --stall) stall=$a ;; --since) since=$a ;; esac
      prev=$a
    done
    now=$(date +%s); start=$(cat "$T/clock.start" 2>/dev/null || echo "$now")
    [ $((now - start)) -le "$limit" ] || { echo over; exit 2; }
    [ $((now - since)) -lt "$stall" ] || { echo stalled; exit 1; }
    echo ok ;;
  *) exec "$0.real" "$@" ;;
esac
exit 0"#;

/// The stub docker. `$T/docker.info` answers info, `$T/image.inspect`,
/// `$T/image.labels` and `$T/image.label` the image's reads, and `$T/cid` is
/// written to a cid file, refused when one is there already. What the
/// container of a cycle does is `$T/container.sh`. The variables CAPS_RC,
/// PUSH_RC, INSTALL_RC and INSTALL_SAYS, and the files push-hangs, push-slow,
/// rm-slow and stop-hangs, change how a verb answers.
const DOCKER: &str = r#"if [ "$1" = info ]; then cat "$T/docker.info"; exit; fi
case " $* " in *" --capabilities "*)
  printf "%s\n" "$*" >>"$T/calls/docker.caps"
  [ ! -e "$T/caps.json" ] || cat "$T/caps.json"
  exit "${CAPS_RC:-0}" ;;
esac
if [ "$1" = image ]; then
  case " $* " in *"{{.Id}}"*) cat "$T/image.inspect" ;; *"{{json .Config.Labels}}"*) cat "$T/image.labels" ;; *) cat "$T/image.label" ;; esac 2>/dev/null
  exit
fi
printf "%s\n" "$@" >"$T/calls/docker.args.$$"
printf "%s\n" "${GH_TOKEN:-}" >"$T/calls/docker.token"
prev=""
for a in "$@"; do
  if [ "$prev" = --cidfile ]; then
    [ ! -e "$a" ] || { echo "docker: container ID file found" >&2; exit 125; }
    cat "$T/cid" >"$a"
  fi
  prev=$a
done
case " $* " in
  *"pi-rukas-headless work "*)
    [ -e "$T/container.sh" ] || cp "$T/container-default" "$T/container.sh"
    . "$T/container.sh" ;;
  *" push --dry-run "*)
    [ ! -e "$T/push-hangs" ] || exec "$(command -pv sleep)" 30
    [ ! -e "$T/push-slow" ] || { : >"$T/slow-began"; "$(command -pv sleep)" 1; }
    [ "${PUSH_RC:-0}" != 125 ] || { echo "docker: Error response from daemon: failed to create task" >&2; exit 125; }
    [ "${PUSH_RC:-0}" = 0 ] || { echo "fatal: could not read from remote repository" >&2; exit "$PUSH_RC"; } ;;
  *" --entrypoint bash "*)
    [ -z "${INSTALL_SAYS:-}" ] || echo "$INSTALL_SAYS" >&2
    exit "${INSTALL_RC:-0}" ;;
  " rm -f "*)
    [ ! -e "$T/rm-slow" ] || { : >"$T/slow-began"; "$(command -pv sleep)" 1; } ;;
  " stop "*)
    [ ! -e "$T/stop-hangs" ] || exec "$(command -pv sleep)" 30
    kill "$(cat "$T/sleeper.pid" 2>/dev/null)" 2>/dev/null ;;
  " kill "*) kill "$(cat "$T/sleeper.pid" 2>/dev/null)" 2>/dev/null ;;
  " pause "* | " unpause "*) printf "%s\n" "$1" >>"$T/calls/docker.freeze" ;;
esac"#;

/// What docker_hangs puts before DOCKER: the run of the container is a client
/// that ignores TERM and never ends while `$T/client-hangs` names a sleep.
const DOCKER_HANGS: &str = r#"nap=$(cat "$T/client-hangs" 2>/dev/null)
case "$*" in
  *"pi-rukas-headless work "*)
    [ -z "$nap" ] || { echo "$$" >"$T/client-hangs.pid"; trap '' TERM; exec "$nap" 30; } ;;
esac
"#;

/// The container of a cycle that named none: one line of progress, so the
/// stream is not the empty one of a container that never came up.
const CONTAINER_DEFAULT: &str = r#"nap=$(command -pv sleep)
progress() { printf "%s\n" "$1" >&2; }
progress cycle
exit 0
"#;

/// The head of a container a case names: `says <status> [cap]` writes the
/// result to standard output, `says2 <status> <cap> <failure>` with a failure
/// too, `progress <line>` one line of the NDJSON. The error file of its log
/// goes first, so a case reads only what its own run wrote there.
const CONTAINER_HEAD: &str = r#"nap=$(command -pv sleep)
c=$T/home/tehas/logs; b=7; rm -f "$c"/work-$b-*.jsonl.err
says2() { echo "{\"schemaVersion\":5,\"buildCommit\":null,\"exitCode\":10,\"issues\":[{\"issue\":7,\"status\":\"$1\",\"cap\":\"${2:-}\",\"failedStep\":\"develop\",\"branch\":\"feature/issue-7-x\",\"steps\":[{\"step\":\"explore\"},{\"step\":\"develop\"}],\"usage\":{\"input\":5,\"output\":2,\"cacheRead\":1,\"turns\":3,\"dispatches\":2,\"byRole\":{}},\"failure\":${3:-null},\"exitCode\":10}]}"; }
says() { says2 "$1" "${2:-}"; }
progress() { printf "%s\n" "$1" >&2; }
"#;

/// The answers of the stub gh (stub_github). `fx/issue.json` is issue #7,
/// `fx/events.json` its events (none is an issue nobody labelled),
/// `fx/events-fail` a failed read of them, `fx/comment-fails` a comment GitHub
/// refuses.
const GH: &str = r#"case "$1 $2" in
  "api repos/acme/widget/issues/7") cat "$T/fx/issue.json" 2>/dev/null || exit 1 ;;
  "api repos/acme/widget/issues/7/events?per_page=100")
    [ ! -e "$T/fx/events-fail" ] || exit 1
    cat "$T/fx/events.json" 2>/dev/null || echo "[]" ;;
  "issue comment") [ ! -e "$T/fx/comment-fails" ] || exit 1 ;;
esac"#;

/// The stub curl (stub_github): it hands out `fx/net/<address, with _ for
/// every other character>`; `<name>.rc` is an exit status to fail with. The
/// header given on standard input goes to `calls/curl-headers`, a config file
/// of a model request to `calls/curl-config`.
const CURL: &str = r#"out="" url="" max=0 header=0 conf=""
while [ $# -gt 0 ]; do
  case "$1" in
    -o) out=$2; shift ;;
    --max-filesize) max=$2; shift ;;
    -H) header=1; shift ;;
    -K) conf=$2; shift ;;
    https://*) url=$1 ;;
  esac
  shift
done
case "$conf" in /proc/*/fd/*) cat "$conf" >>"$T/calls/curl-config" ;; esac
key=$(printf %s "$url" | tr -c "A-Za-z0-9" _)
[ "$header" = 1 ] && { printf "%s " "$url"; cat; } >>"$T/calls/curl-headers"
[ -f "$T/fx/net/$key.rc" ] && exit "$(cat "$T/fx/net/$key.rc")"
[ -f "$T/fx/net/$key" ] || exit 22
[ -n "$out" ] || { cat "$T/fx/net/$key"; exit 0; }
[ "$(stat -c %s "$T/fx/net/$key")" -le "$max" ] || exit 63
cp "$T/fx/net/$key" "$out"
printf application/octet-stream
exit 0"#;

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

/// The checkout of this source: lib/ comes from it.
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// The programs of the workspace tehas work runs besides tehas-cycle.
struct Programs {
    store: PathBuf,
    console: PathBuf,
    attach: PathBuf,
}

/// The store, the console and the attachment checker of this build: above the
/// harness when the workspace built them, built once here otherwise.
fn programs() -> Programs {
    fn above(name: &str) -> Option<PathBuf> {
        let exe = std::env::current_exe().unwrap();
        exe.ancestors()
            .skip(1)
            .map(|dir| dir.join(name))
            .find(|p| p.is_file())
    }
    fn found() -> Option<Programs> {
        Some(Programs {
            store: above("store")?,
            console: above("console")?,
            attach: above("attach")?,
        })
    }
    if let Some(p) = found() {
        return p;
    }
    static BUILT: Once = Once::new();
    BUILT.call_once(|| {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.toml");
        let target = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../target"));
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
            .args(["-p", "store", "-p", "console", "-p", "attach"])
            .args((!cfg!(debug_assertions)).then_some("--release"))
            .output()
            .expect("cargo");
        assert!(
            out.status.success(),
            "the programs did not build: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    });
    found().expect("the programs of this build are above the test binary")
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

/// The user id of this process, as `id -u` says it.
fn euid() -> String {
    let out = Command::new("id").arg("-u").output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[allow(dead_code)] // each test file uses a part of this module
/// The end of one run: its exit status and what it printed, both streams.
#[derive(Debug)]
pub struct Run {
    pub rc: i32,
    pub out: String,
}

#[allow(dead_code)] // each test file uses a part of this module
/// One host with issue #7 of acme/widget, as work_lib.sh leaves it.
pub struct World {
    t: TestDir,
    /// `~/tehas` of the host.
    pub home: PathBuf,
    /// The fixtures of gh and curl.
    pub fx: PathBuf,
    /// The tree of the script, with the console stub and tehas-cycle in
    /// `libexec/` and lib/ beside it.
    pub tree: PathBuf,
    /// The product repository.
    pub upstream: PathBuf,
    /// The host's checkout of it.
    pub checkout: PathBuf,
    /// The workspace of #7 on the host.
    pub ws: PathBuf,
    /// The path every cycle has its workspace at inside its container.
    pub inside: PathBuf,
    /// The runner's runs of #7.
    pub runs: PathBuf,
    /// The cycle's own state on the host, `<ws>.state`.
    pub state: PathBuf,
    /// The host's directory of the code index.
    pub hostindex: PathBuf,
    /// The attachments of the cycle, `<ws>.images`.
    pub imgs: PathBuf,
    /// The manifest of a start, as the host keeps it.
    pub kept: PathBuf,
    pub live: PathBuf,
    pub cid_file: PathBuf,
    pub oom_file: PathBuf,
    pub forks_file: PathBuf,
    pub result_file: PathBuf,
    pub progress_file: PathBuf,
    /// The id of the container of the slice: `a` 64 times.
    pub cid: String,
    /// The sandbox image of this account.
    pub image: String,
    bash: String,
    console: PathBuf,
    env: RefCell<Vec<(String, String)>>,
    always: RefCell<Vec<(String, String)>>,
    instance_on: Cell<bool>,
    record: RefCell<Value>,
}

#[allow(dead_code)] // each test file uses a part of this module
impl World {
    pub fn new(name: &str) -> World {
        let t = TestDir::new(&format!("work-{name}"));
        let home = t.join("home/tehas");
        let tree = t.join("tree");
        for d in ["bin", "calls", "deny", "tmp", "fx/net", "tree/libexec"] {
            fs::create_dir_all(t.join(d)).unwrap();
        }
        fs::create_dir_all(&home).unwrap();
        std::os::unix::fs::symlink(root().join("lib"), tree.join("lib")).unwrap();
        let p = programs();
        copy_exe(&p.store, t.join("store-bin")).unwrap();
        copy_exe(&p.attach, t.join("attach-bin")).unwrap();
        place(&p.console, &t.join("console-bin"));
        let ws = home.join("work/widget-7");
        let mut state = ws.clone().into_os_string();
        state.push(".state");
        let mut imgs = ws.clone().into_os_string();
        imgs.push(".images");
        let cycles = home.join("state/cycles");
        let w = World {
            fx: t.join("fx"),
            upstream: t.join("git/acme/widget"),
            checkout: home.join("repos/widget"),
            inside: home.join("work/widget"),
            runs: t.join("home/.pi/agent/ensemble-runs/issue-7"),
            state: PathBuf::from(state),
            imgs: PathBuf::from(imgs),
            hostindex: home.join("state/index"),
            kept: cycles.join("7.manifest.json"),
            live: home.join("live"),
            cid_file: cycles.join("7.cid"),
            oom_file: cycles.join("7.oom"),
            forks_file: cycles.join("7.forks"),
            result_file: cycles.join("7.result.json"),
            progress_file: cycles.join("7.progress.json"),
            cid: slice_id('a'),
            image: format!("tehas/pi-rukas:uid{}", euid()),
            ws,
            home,
            tree,
            bash: which("bash"),
            console: t.join("console-bin"),
            t,
            env: RefCell::new(vec![]),
            always: RefCell::new(vec![]),
            instance_on: Cell::new(false),
            record: RefCell::new(Value::Null),
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
        w.fs("gh-n.lock", "");
        fs::set_permissions(w.t.join("gh-n.lock"), fs::Permissions::from_mode(0o600)).unwrap();
        w.stub("nproc", "echo 16");
        w.env_always("TEHAS_WATCH_TICK", "0.1");
        w.env_always("TEHAS_CONSOLE_CMD", &w.console.to_string_lossy());
        let libexec = w.tree.join("libexec");
        w.fs("now.step.jq", NOW_STEP_JQ);
        w.fs("now.text.jq", NOW_TEXT_JQ);
        w.stub_at(&libexec.join("console"), "console", CONSOLE);
        w.stub_at(&libexec.join("tehas-cycle"), "tehas-cycle", CLOCK);
        w.env_always(
            "TEHAS_WATCH_CLOCK",
            &libexec.join("tehas-cycle").to_string_lossy(),
        );
        place(
            Path::new(env!("CARGO_BIN_EXE_tehas-cycle")),
            &libexec.join("tehas-cycle.real"),
        );
        // The renewer of the cycle's token, which tehas work starts from its home.
        fs::create_dir_all(w.home.join("libexec")).unwrap();
        write_exe(
            w.home.join("libexec/tehas-cycle"),
            format!("#!{}\necho \"$*\" >>\"$T/renew.calls\"\n", w.bash),
        )
        .unwrap();
        // tehas work as bin/tehas runs it: through `console run`, with the
        // directory of the program as libexec.
        write_exe(
            libexec.join("work"),
            format!(
                "#!{}\nexec \"{}\" run \"$(cd \"${{0%/*}}\" && pwd)\" \"$EUID\" \"$BASH\" tehas work \"$@\"\n",
                w.bash,
                w.console.display()
            ),
        )
        .unwrap();
        // The product names its gate in .pi/verify-cmd, a tracked file of its
        // one commit; the checkout's worktree holds an untracked file of its
        // own, which decides nothing.
        fs::create_dir_all(w.upstream.join(".pi")).unwrap();
        git(&["init", "-q", "-b", "main"], &w.upstream);
        fs::write(w.upstream.join(".pi/verify-cmd"), "scripts/verify.sh\n").unwrap();
        git(&["add", "-f", "--", ".pi/verify-cmd"], &w.upstream);
        w.commit_upstream("first", None);
        let out = Command::new("git")
            .args(["clone", "-q"])
            .arg(&w.upstream)
            .arg(&w.checkout)
            .output()
            .unwrap();
        assert!(out.status.success(), "git clone: {out:?}");
        fs::write(w.checkout.join("one.txt"), "placeholder\n").unwrap();
        w.docker_stub("");
        w.fs("container-default", CONTAINER_DEFAULT);
        w.fs(
            "home/.gitconfig",
            "[user]\n\tname = Test Bot\n\temail = bot@example.com\n",
        );
        w.caps_good();
        w.fs(
            "home/.pi/agent/settings.json",
            "{\"retry\":{\"maxRetries\":10,\"baseDelayMs\":5000,\"maxAgentDelayMs\":120000,\"provider\":{\"timeoutMs\":600000,\"maxRetries\":8,\"maxRetryDelayMs\":60000}}}\n",
        );
        w.fs("home/.pi/agent/models.json", "{\"providers\":{}}\n");
        w.stub("systemctl", r#"cat "$T/slice.show""#);
        w.stub("journalctl", r#"cat "$T/kernel.log""#);
        w.slice_good();
        let mut first: Vec<&str> = MINIMAL.to_vec();
        first.push("PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9");
        w.instance(&first);
        w.env_ok();
        w.store_home();
        w
    }

    /// The scratch root, `$T` of the stubs.
    pub fn t(&self) -> &Path {
        &self.t
    }

    /// A path under the scratch root.
    pub fn path(&self, rel: &str) -> PathBuf {
        self.t.join(rel)
    }

    /// The program the cases run as `$work`: tehas work as bin/tehas runs it.
    pub fn work_cmd(&self) -> PathBuf {
        self.tree.join("libexec/work")
    }

    /// The mounts of a cycle given nothing of the issue, as the shell tests
    /// list them: one `<host>:<container>[:ro]` per line.
    pub fn mounts_off(&self) -> String {
        let (ws, state, home) = (self.ws.display(), self.state.display(), self.home.display());
        let pi = self.t.join("home/.pi/agent");
        let pi = pi.display();
        format!(
            "{ws}:{inside}
{runs}:/home/vscode/.pi/agent/ensemble-runs
{state}/cache:/home/vscode/.tehas-cache
{state}/memory:/home/vscode/.vipune
{state}/sessions:/home/vscode/.pi/agent/sessions
{pi}/mcp.json:/home/vscode/.pi/agent/mcp.json:ro
{state}/models.json:/home/vscode/.pi/agent/models.json:ro
{pi}/settings.json:/home/vscode/.pi/agent/settings.json:ro
{home}/state/github-cycle:/run/tehas/github:ro
{state}/relay:/run/tehas/relay",
            inside = self.inside.display(),
            runs = self.runs.display(),
        )
    }

    // --- Files and stubs --------------------------------------------------

    /// A file under the scratch root, with these bytes.
    pub fn fs(&self, rel: &str, text: impl AsRef<[u8]>) {
        let p = self.t.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    /// A file under the scratch root that is there, empty.
    pub fn touch(&self, rel: &str) {
        self.fs(rel, "");
    }

    /// A file under the scratch root, taken away.
    pub fn rm(&self, rel: &str) {
        let p = self.t.join(rel);
        if p.is_dir() && !p.is_symlink() {
            let _ = fs::remove_dir_all(p);
        } else {
            let _ = fs::remove_file(p);
        }
    }

    /// What a file under the scratch root holds, "" for none.
    pub fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.t.join(rel)).unwrap_or_default()
    }

    /// A file under the home, with these bytes.
    pub fn home_file(&self, rel: &str, text: impl AsRef<[u8]>) {
        let p = self.home.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    /// A fixture under `fx/`.
    pub fn fx(&self, rel: &str, text: impl AsRef<[u8]>) {
        self.fs(&format!("fx/{rel}"), text);
    }

    /// A stub program on the PATH of a run, recording its arguments in
    /// `calls/<name>` before its body runs. A body that starts with `#!` is
    /// the whole program.
    pub fn stub(&self, name: &str, body: &str) {
        self.stub_at(&self.t.join("bin").join(name), name, body);
    }

    fn stub_at(&self, path: &Path, name: &str, body: &str) {
        let text = if body.starts_with("#!") {
            body.to_string()
        } else {
            format!(
                "#!{}\nprintf '%s\\n' \"$*\" >>\"$T/calls/{name}\"\n{body}\n",
                self.bash
            )
        };
        write_exe(path, text).unwrap();
    }

    /// A stub taken off the PATH: a run finds the deny stub or the host's own.
    pub fn unstub(&self, name: &str) {
        let _ = fs::remove_file(self.t.join("bin").join(name));
    }

    /// The stub gh with these answers, as `t_stub gh` writes one: each call
    /// is numbered, recorded with a body read from standard input written
    /// out, answered by gh_fail() when it names it, and every login in its
    /// answer is given the test's user id.
    pub fn gh_stub(&self, answers: &str) {
        let text = format!(
            "#!{}\n{GH_HEAD}\nt_answer() {{\n:\n{answers}\n}}\n{GH_TAIL}",
            self.bash
        );
        write_exe(self.t.join("bin/gh"), text).unwrap();
    }

    /// The `n`-th gh call of the test prints `message` on stderr and ends
    /// with `rc`. Several calls may be named.
    pub fn gh_fail(&self, n: u32, rc: u8, message: &str) {
        assert!(n > 0 && rc > 0 && !message.contains(['\t', '\n']));
        let path = self.t.join("gh-fails");
        let mut text = fs::read_to_string(&path).unwrap_or_default();
        text.push_str(&format!("{n}\t{rc}\t{message}\n"));
        fs::write(path, text).unwrap();
    }

    /// gh answers for the issue and its events from fixtures, and curl hands
    /// out what `net` wrote (stub_github).
    pub fn stub_github(&self) {
        self.gh_stub(GH);
        self.stub("curl", CURL);
    }

    /// Something GitHub would hand out at `url`.
    pub fn net(&self, url: &str, bytes: impl AsRef<[u8]>) {
        self.fx(&format!("net/{}", net_key(url)), bytes);
    }

    /// The fetch of `url` ends with `rc`.
    pub fn net_rc(&self, url: &str, rc: i32) {
        self.fx(&format!("net/{}.rc", net_key(url)), format!("{rc}\n"));
    }

    /// The stub docker, with `before` ahead of its ordinary body.
    fn docker_stub(&self, before: &str) {
        self.stub("docker", &format!("{before}{DOCKER}"));
    }

    /// The same docker, but the run of the container is a client that never
    /// ends and ignores TERM, as the attached `docker run` of a wedged dockerd
    /// is. Every other verb answers as before.
    pub fn docker_hangs(&self) {
        self.fs("client-hangs", format!("{}\n", which("sleep")));
        self.docker_stub(DOCKER_HANGS);
    }

    /// What the container of the cycle does: a bash body after the helpers
    /// `says`, `says2` and `progress`. "" is one line of progress and an end.
    pub fn container(&self, body: &str) {
        let body = if body.is_empty() {
            "progress cycle; exit 0"
        } else {
            body
        };
        self.fs("container.sh", format!("{CONTAINER_HEAD}{body}\n"));
    }

    /// What a runner on result schema 5 answers to --capabilities, cut to
    /// what the tests use.
    pub fn caps_good(&self) {
        let mut inputs: Vec<Value> = ["SANDBOX_MODE", "WORKSPACE_ROOT"]
            .iter()
            .map(|n| json!({"name": format!("PI_ENSEMBLE_{n}"), "scope": "sandbox"}))
            .collect();
        inputs.extend(
            [
                "MAX_WORKSTREAMS",
                "VERIFY_TIMEOUT_MS",
                "INTEGRATION_VERIFY_TIMEOUT_MS",
                "VERIFY_SUITE_TIMEOUT_MS",
                "SUBAGENT_MODEL",
                "SUBAGENT_PROVIDER",
                "MODEL_<ROLE>",
                "PROVIDER_<ROLE>",
                "EXTRA_IMAGE_DIRS",
                "COMMENT_RETENTION",
                "HANDOFF_CHANNEL",
                "HANDOFF_TIMEOUT_MS",
                "CYCLE_DEADLINE",
                "VERIFY_CONCURRENCY",
                "SPAWN_TIMEOUT_MS",
            ]
            .iter()
            .map(|n| json!({"name": format!("PI_ENSEMBLE_{n}"), "scope": "cycle"})),
        );
        inputs.push(json!({"name": "PI_ENSEMBLE_SUBAGENT_MODE", "scope": "internal"}));
        self.set_caps(&json!({
            "schemaVersion": 2, "version": "0.12.80", "commit": null,
            "contracts": {"exitCodes": {"version": 1}, "resultSchema": {"version": 5},
                "ndjson": {"version": 1, "stable": {"version": 1}}},
            "verbs": {"work": {"flags": ["--restart", "--json", "--timeout"]}},
            "inputs": inputs,
            "features": {"resultFromMemory": true, "cycleDeadline": true,
                "handoffChannel": true, "restartClearsAttention": true}
        }));
    }

    /// The runner's answer to --capabilities.
    pub fn caps(&self) -> Value {
        serde_json::from_str(&self.read("caps.json")).unwrap_or(Value::Null)
    }

    /// The runner answers --capabilities with this.
    pub fn set_caps(&self, v: &Value) {
        self.fs("caps.json", format!("{v}\n"));
    }

    /// The runner's answer, changed.
    pub fn caps_edit(&self, change: impl FnOnce(&mut Value)) {
        let mut v = self.caps();
        change(&mut v);
        self.set_caps(&v);
    }

    /// The flag a runner that can continue a pull request adds to work.
    pub fn caps_continue(&self) {
        self.caps_edit(|v| {
            v["verbs"]["work"]["flags"]
                .as_array_mut()
                .unwrap()
                .push(json!("--continue-pr"))
        });
    }

    /// What the sandbox image says about its own pi-rukas: `<commit>|<reads>`.
    /// "none" is an image that carries no label.
    pub fn reads_image(&self, reads: &str) {
        self.rm("image.label");
        if reads != "none" {
            self.fs("image.label", format!("{}|{reads}\n", img_commit()));
        }
    }

    // --- The slice ---------------------------------------------------------

    /// A host with the root step done: the slice loaded from its file with
    /// 8 GiB, so a container's ceiling is 2048m, Docker on the systemd driver
    /// and cgroup v2, and nothing in the slice.
    pub fn slice_good(&self) {
        self.fs(
            "slice.show",
            "FragmentPath=/etc/systemd/system/tehas.slice\nActiveState=active\nMemoryMax=8589934592\n",
        );
        self.fs("docker.info", "systemd 2\n");
        self.fs("cid", format!("{}\n", self.cid));
        self.slice_state("0.00", "");
    }

    /// The scratch cgroup tree, again: the controllers and the slice's
    /// pressure, full and some averages over 60 seconds ("" is 0.00).
    pub fn slice_state(&self, full: &str, some: &str) {
        let some = if some.is_empty() { "0.00" } else { some };
        self.rm("cgroup");
        self.fs(
            "cgroup/cgroup.controllers",
            "cpuset cpu io memory hugetlb pids rdma misc\n",
        );
        self.fs(
            "cgroup/tehas.slice/memory.pressure",
            format!(
                "some avg10=0.00 avg60={some} avg300=0.00 total=0\nfull avg10=0.00 avg60={full} avg300=0.00 total=0\n"
            ),
        );
    }

    /// A container of the slice: its anonymous and slab bytes, its
    /// memory.low and its count of OOM kills.
    pub fn slice_child(&self, id: &str, anon: u64, slab: u64, low: &str, oom_kill: u64) {
        let dir = format!("cgroup/tehas.slice/docker-{id}.scope");
        self.fs(
            &format!("{dir}/memory.stat"),
            format!(
                "anon {anon}\nfile 0\nkernel {slab}\nkernel_stack 0\nshmem 0\nslab_reclaimable {slab}\nslab_unreclaimable 0\n"
            ),
        );
        self.fs(&format!("{dir}/memory.low"), format!("{low}\n"));
        self.fs(
            &format!("{dir}/memory.events"),
            format!("low 0\nhigh 0\nmax 0\noom 0\noom_kill {oom_kill}\noom_group_kill 0\n"),
        );
    }

    // --- The instance and the environment ----------------------------------

    /// instance.env, as given, and the factory as its GitHub App unless the
    /// login is kept already (t_instance).
    pub fn instance(&self, lines: &[&str]) {
        let dir = self.t.join("instance");
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        let mut text = lines.join("\n");
        if !lines.is_empty() {
            text.push('\n');
        }
        fs::write(dir.join("instance.env"), text).unwrap();
        self.instance_on.set(true);
        if !self.home.join("state/github/login").exists() {
            self.app_world("tehas-bot");
        }
    }

    /// The minimal instance and these settings after it.
    pub fn minimal(&self, more: &[&str]) {
        let mut all: Vec<&str> = MINIMAL.to_vec();
        all.extend_from_slice(more);
        self.instance(&all);
    }

    /// No instance directory: a run has none named (t_no_instance).
    pub fn no_instance(&self) {
        assert!(self.instance_on.get(), "no instance to remove");
        self.instance_on.set(false);
        let _ = fs::remove_dir_all(self.t.join("instance"));
    }

    /// The factory as the GitHub App `<slug>[bot]`, with its tokens.
    pub fn app_world(&self, slug: &str) {
        let until = tehas_core::time::now() + 86400;
        for (dir, token) in [
            ("github", "app"),
            ("github-cycle", "cycle"),
            ("github-session", "session"),
        ] {
            self.home_file(&format!("state/{dir}/token"), format!("ghs_{token}\n"));
            self.home_file(&format!("state/{dir}/token.expires"), format!("{until}\n"));
        }
        self.home_file("state/github/login", format!("{slug}[bot]\n"));
    }

    /// A variable of every later run, as T_ENV+= is. A name given again
    /// replaces its value.
    pub fn env(&self, k: &str, v: &str) {
        let mut e = self.env.borrow_mut();
        e.retain(|(n, _)| n != k);
        e.push((k.into(), v.into()));
    }

    /// A variable of the runs taken away.
    pub fn env_unset(&self, k: &str) {
        self.env.borrow_mut().retain(|(n, _)| n != k);
    }

    /// The variables of the runs are those of env_ok alone: the cgroup root.
    pub fn env_ok(&self) {
        let root = self.t.join("cgroup").to_string_lossy().into_owned();
        *self.env.borrow_mut() = vec![("TEHAS_CGROUP_ROOT".into(), root)];
    }

    /// A variable of every run, whatever env says (T_ENV_ALWAYS).
    pub fn env_always(&self, k: &str, v: &str) {
        let mut e = self.always.borrow_mut();
        e.retain(|(n, _)| n != k);
        e.push((k.into(), v.into()));
    }

    // --- Runs ----------------------------------------------------------------

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
            .env("TMPDIR", self.t.join("tmp"));
        if self.instance_on.get() {
            c.env("TEHAS_INSTANCE_DIR", self.t.join("instance"));
        }
        c.envs(self.env.borrow().iter().map(|(k, v)| (k, v)))
            .envs(self.always.borrow().iter().map(|(k, v)| (k, v)));
        c
    }

    /// `tehas work <words>`, as bin/tehas runs it.
    pub fn work(&self, words: &[&str]) -> Run {
        let mut c = self.command(&self.work_cmd());
        c.args(words);
        run(c)
    }

    /// `tehas work 7`.
    pub fn work7(&self) -> Run {
        self.work(&["7"])
    }

    /// A bash snippet in the environment of a run (t_run).
    pub fn sh(&self, snippet: &str) -> Run {
        let mut c = self.command(Path::new(&self.bash));
        c.args(["-c", snippet]);
        run(c)
    }

    /// What a stub was called with, one call per line.
    pub fn calls(&self, name: &str) -> String {
        self.read(&format!("calls/{name}"))
    }

    /// The gh calls.
    pub fn gh(&self) -> String {
        self.calls("gh")
    }

    /// The calls are forgotten, the count of gh calls with them.
    pub fn clear_calls(&self) {
        self.rm("calls");
        fs::create_dir_all(self.t.join("calls")).unwrap();
        self.rm("gh-n");
        self.rm("gh-fails");
    }

    /// The words of the docker calls whose words hold one line equal to
    /// `line`: of the first such call by the name of its file.
    fn docker_call(&self, lines: &[&str]) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.t.join("calls"))
            .map(|d| {
                d.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| n.starts_with("docker.args."))
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        for n in names {
            let text = self.calls(&n);
            let words: Vec<String> = text.lines().map(String::from).collect();
            if words.iter().any(|w| lines.contains(&w.as_str())) {
                return words;
            }
        }
        vec![]
    }

    /// The words of the docker call that starts the cycle.
    pub fn cycle_args(&self) -> Vec<String> {
        self.docker_call(&["pi-rukas-headless", RUNNER])
    }

    /// The words of the docker call that installs the product before the
    /// cycle: the one whose script starts with set -e.
    pub fn install_args(&self) -> Vec<String> {
        self.docker_call(&["set -e"])
    }

    /// The words of the docker call that checks the push before the cycle.
    pub fn preflight_args(&self) -> Vec<String> {
        self.docker_call(&["--entrypoint"])
    }

    /// The values that follow `flag` in the cycle's words, e.g. every -v.
    pub fn after(&self, flag: &str) -> Vec<String> {
        security_of(&self.cycle_args(), flag)
    }

    /// The fault of a sandbox that cannot push, compact, "" for none.
    pub fn push_fault_file(&self) -> String {
        fs::read_to_string(self.home.join("state/push-fault.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .map(|v| v.to_string())
            .unwrap_or_default()
    }

    /// The three fields of the fault as three lines: whether `at` is the form
    /// a host writes, the account and git's own line; "" for no fault.
    pub fn push_fault_parts(&self) -> String {
        let Ok(text) = fs::read_to_string(self.home.join("state/push-fault.json")) else {
            return String::new();
        };
        let Ok(v) = serde_json::from_str::<Value>(&text) else {
            return String::new();
        };
        let at =
            regex::Regex::new(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$").unwrap();
        let ok = v["at"].as_str().is_some_and(|a| at.is_match(a));
        format!("{ok}\n{}\n{}", text_of(&v["login"]), text_of(&v["line"]))
    }

    /// The account the factory acts as.
    pub fn cycle_account(&self) -> String {
        fs::read_to_string(self.home.join("state/github/login"))
            .unwrap_or_default()
            .trim_end()
            .to_string()
    }

    /// The store of the runs records each call in `calls/store` and is the
    /// real store, except that it refuses the end of a push fault with `rc`
    /// and `line` (the store's busy line for ""), or takes it for 0.
    pub fn push_fault_store(&self, rc: i32, line: &str) {
        let line = if line.is_empty() {
            "store: busy #7: the database is locked"
        } else {
            line
        };
        let bin = self.home.join("libexec/store");
        let _ = fs::remove_file(&bin);
        self.rm("calls/store");
        let calls = self.t.join("calls/store");
        write_exe(
            &bin,
            format!(
                "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >>{}\ncase \" $* \" in *\" --status push-fault \"*)\n  [ {rc} = 0 ] || {{ printf '%s\\n' {} >&2; exit {rc}; }}\nesac\nexec {} \"$@\"\n",
                sh_quote(&calls.to_string_lossy()),
                sh_quote(line),
                sh_quote(&self.t.join("store-bin").to_string_lossy()),
            ),
        )
        .unwrap();
    }

    /// The word of the end of issue `n` on record, read from the database
    /// read-only; "skipped" without sqlite3.
    pub fn push_fault_ended(&self, n: u64) -> String {
        let out = Command::new("sqlite3")
            .args(["-cmd", ".mode list"])
            .arg(self.home.join("state/tehas.db"))
            .arg(format!("select status from endings where issue={n};"))
            .stderr(Stdio::null())
            .output();
        match out {
            Err(_) => "skipped".into(),
            Ok(o) => String::from_utf8_lossy(&o.stdout).trim_end().to_string(),
        }
    }

    /// What the store of the runs was asked, one call per line.
    pub fn store_lines(&self) -> String {
        self.calls("store")
    }

    // --- Resets ------------------------------------------------------------

    /// What a case of the shell file left goes before the next: the calls,
    /// the live cycles, the container's body and its sleeper, the logs of #7
    /// and the host's push fault, and a fresh store with no attempt.
    pub fn fresh(&self) {
        self.clear_calls();
        let _ = fs::remove_dir_all(self.live.join("cycles"));
        for f in ["sleeper.pid", "container.sh", "now.wait", "client-hangs"] {
            self.rm(f);
        }
        remove_like(&self.home.join("logs"), "work-7-*");
        let _ = fs::remove_file(self.home.join("state/push-fault.json"));
        self.store_home();
        self.env_unset("TEHAS_CYCLE_ATTEMPT");
    }

    fn images_reset(&self, logs: bool) {
        self.rm("fx");
        self.clear_calls();
        let _ = fs::remove_dir_all(self.home.join("work"));
        if logs {
            let _ = fs::remove_dir_all(self.home.join("logs"));
        }
        let _ = fs::remove_file(&self.kept);
        let _ = fs::remove_file(self.home.join("state/push-fault.json"));
        self.rm("container.sh");
        self.store_home();
        fs::create_dir_all(self.fx.join("net")).unwrap();
        self.slice_good();
        self.reads_image("1");
        self.env_ok();
    }

    /// The instance that gives the cycle the issue's images, these settings
    /// after it, on a host with nothing of a start before.
    pub fn images_on(&self, more: &[&str]) {
        self.images_reset(false);
        let mut all = vec!["TEHAS_WORK_IMAGES=1"];
        all.extend_from_slice(more);
        self.minimal(&all);
    }

    /// The same instance with nothing given to the cycle.
    pub fn images_off(&self) {
        self.images_reset(true);
        self.minimal(&["TEHAS_WORK_IMAGES=0", "TEHAS_WORK_FILES=0"]);
    }

    // --- GitHub ------------------------------------------------------------

    /// The answer for issue 7 as REST gives it: open, with `body`, the
    /// author's association (OWNER for "") and a title when one is given.
    pub fn issue_is(&self, body: &str, association: &str, title: &str) {
        let who = if association.is_empty() {
            "OWNER"
        } else {
            association
        };
        let mut v = json!({"number": 7, "state": "open", "body": body, "author_association": who});
        if !title.is_empty() {
            v["title"] = json!(title);
        }
        self.fx("issue.json", format!("{v}\n"));
    }

    /// The account that applied tehas:ready last.
    pub fn labelled_by(&self, who: &str) {
        let v = json!([
            {"event": "labeled", "label": {"name": "tehas:plan"}, "actor": {"login": "alice"}, "created_at": "2026-10-12T00:00:00Z"},
            {"event": "labeled", "label": {"name": "tehas:ready"}, "actor": {"login": who}, "created_at": "2026-10-11T00:00:00Z"}
        ]);
        self.fx("events.json", format!("{v}\n"));
    }

    /// How many times a start asked GitHub for the issue itself.
    pub fn issue_reads(&self) -> usize {
        self.gh()
            .lines()
            .filter(|l| *l == "api repos/acme/widget/issues/7")
            .count()
    }

    /// jq -c `filter` of the cycle's manifest, or what jq says.
    pub fn manifest(&self, filter: &str) -> String {
        let mut c = Command::new("jq");
        c.args(["-c", filter]).arg(self.imgs.join("manifest.json"));
        let out = c.output().unwrap();
        let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
        s.push_str(&String::from_utf8_lossy(&out.stderr));
        s.trim_end_matches('\n').to_string()
    }

    // --- The product -------------------------------------------------------

    /// A commit of the product, writing `file` with `line` when one is given.
    pub fn commit_upstream(&self, subject: &str, file: Option<(&str, &str)>) {
        commit(&self.upstream, subject, file);
    }

    /// The checkout fetches every branch of the product.
    pub fn fetched(&self) {
        git(
            &[
                "fetch",
                "-q",
                &self.upstream.to_string_lossy(),
                "+refs/heads/*:refs/remotes/origin/*",
            ],
            &self.checkout,
        );
    }

    // --- The store ---------------------------------------------------------

    fn store_cmd(&self, args: &[&str], now: Option<i64>, lock: Option<u64>) -> Run {
        let mut c = match lock {
            Some(n) => {
                fs::create_dir_all(self.home.join("state/cycles")).unwrap();
                let mut c = Command::new("flock");
                c.args(["-w", "5"])
                    .arg(self.home.join(format!("state/cycles/{n}.lock")))
                    .arg(self.t.join("store-bin"));
                c
            }
            None => Command::new(self.t.join("store-bin")),
        };
        c.env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("HOME", self.t.join("home"))
            .env("TEHAS_HOME", &self.home)
            .args(args);
        if let Some(at) = now {
            c.env("TEHAS_STORE_TEST_NOW", at.to_string());
        }
        let out = c.output().unwrap();
        Run {
            rc: out.status.code().unwrap_or(-1),
            out: format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
            .trim_end()
            .to_string(),
        }
    }

    fn store_must(&self, args: &[&str], now: Option<i64>, lock: Option<u64>) -> Value {
        let out = self.store_cmd(args, now, lock);
        assert_eq!(out.rc, 0, "the store refused {args:?}: {}", out.out);
        serde_json::from_str(&out.out).unwrap_or(Value::Null)
    }

    /// A verb of the real store against this home.
    pub fn store(&self, args: &[&str]) -> Run {
        self.store_cmd(args, None, None)
    }

    /// The same at this time (TEHAS_STORE_TEST_NOW).
    pub fn store_at(&self, epoch: i64, args: &[&str]) -> Run {
        self.store_cmd(args, Some(epoch), None)
    }

    /// A verb of the real store; its answer, or a panic.
    pub fn store_ok(&self, args: &[&str]) -> Value {
        self.store_must(args, None, None)
    }

    /// What the store has on record, through a read verb.
    pub fn row(&self, args: &[&str]) -> Value {
        self.store_ok(args)
    }

    /// A fresh database, made by the store's own init (t_store_db).
    pub fn store_db(&self) {
        for f in ["tehas.db", "tehas.db-wal", "tehas.db-shm"] {
            let _ = fs::remove_file(self.home.join("state").join(f));
        }
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

    /// The real store and the attachment checker in the home's libexec, a
    /// fresh database, no answers of store_answers and the account ids
    /// install.sh records (t_store_home).
    pub fn store_home(&self) {
        let libexec = self.home.join("libexec");
        fs::create_dir_all(&libexec).unwrap();
        for f in ["store-answers", "store-answers.n", "calls/store"] {
            self.rm(f);
        }
        place(&self.t.join("store-bin"), &libexec.join("store"));
        self.store_db();
        place(&self.t.join("attach-bin"), &libexec.join("attach"));
        let ids: String = LOGINS.iter().map(|l| format!("{l} {}\n", uid(l))).collect();
        self.home_file("state/account-ids", ids);
    }

    /// The store of a run answers a call whose arguments begin with `args`
    /// with `rc` and `line`, for its first `times` calls (all for 0).
    pub fn store_answers(&self, args: &str, rc: i32, line: &str, times: u32) {
        let times = if times == 0 {
            String::new()
        } else {
            format!("|{times}")
        };
        self.fs("store-answers", format!("{args}|{rc}|{line}{times}\n"));
        self.rm("store-answers.n");
    }

    /// How many calls the answer matched.
    pub fn store_answered(&self) -> u64 {
        self.read("store-answers.n").trim().parse().unwrap_or(0)
    }

    /// An approval of issue `n` on record, as the dispatcher records it: the
    /// body hash of "Words." unless `extra` gives one, tehas:ready for a
    /// label and a fixer (t_approve).
    pub fn approve(&self, n: u64, by: &str, how: &str, at: i64, extra: &[&str]) {
        let (n, at) = (n.to_string(), at.to_string());
        let mut a = vec!["approval-record", &n, "--at", &at, "--by", by, "--how", how];
        if matches!(how, "label" | "fixer") {
            a.extend(["--label", "tehas:ready"]);
        }
        let sum = wsum();
        if !extra.contains(&"--body-sha") {
            a.extend(["--body-sha", &sum]);
        }
        a.extend_from_slice(extra);
        self.store_ok(&a);
    }

    /// A cycle start of issue `n` on record, made under the issue's lock
    /// (t_started).
    pub fn started(&self, n: u64, extra: &[&str]) {
        self.started_at(n, None, extra);
    }

    /// The same at this time.
    pub fn started_at(&self, n: u64, now: Option<i64>, extra: &[&str]) {
        let s = n.to_string();
        let mut a = vec!["cycle-start", &s];
        a.extend_from_slice(extra);
        self.store_must(&a, now, Some(n));
    }

    /// The attempt on record for `n`, 0 for none.
    pub fn attempt(&self, n: u64) -> u64 {
        self.row(&["cycle-show", &n.to_string()])["cycle"]["attempt"]
            .as_u64()
            .unwrap_or(0)
    }

    /// A cycle of `n` on record that has ended (cycle-end `args`, status
    /// failed unless given), on `by`'s approval of `how`, by hand for "-".
    pub fn ended(&self, n: u64, by: &str, how: &str, args: &[&str]) {
        let a = self.attempt(n);
        if by == "-" {
            self.started(n, &["--by-hand"]);
        } else {
            self.approve(n, by, how, 1_700_000_000 + a as i64, &[]);
            self.started(n, &[]);
        }
        let (s, next) = (n.to_string(), (a + 1).to_string());
        let mut e = vec!["cycle-end", &s, "--attempt", &next];
        if !args.contains(&"--status") {
            e.extend(["--status", "failed"]);
        }
        e.extend_from_slice(args);
        self.store_must(&e, None, Some(n));
    }

    /// A cycle of `n` as the live view shows it: started by hand `started`
    /// seconds ago, its heartbeat `updated` seconds ago with `step`, and, when
    /// not running, its end then with `status` (t_live).
    pub fn live_cycle(
        &self,
        n: u64,
        running: bool,
        status: &str,
        step: &str,
        started: i64,
        updated: i64,
    ) {
        let now = tehas_core::time::now() as i64;
        self.started_at(n, Some(now - started), &["--by-hand"]);
        let a = self.attempt(n).to_string();
        let s = n.to_string();
        let state = if running { "--running" } else { "--ended" };
        self.store_must(
            &[
                "cycle-heartbeat",
                &s,
                state,
                "--status",
                status,
                "--step",
                step,
            ],
            Some(now - updated),
            None,
        );
        if !running {
            self.store_must(
                &[
                    "cycle-end",
                    &s,
                    "--attempt",
                    &a,
                    "--status",
                    status,
                    "--step",
                    step,
                ],
                Some(now - updated),
                Some(n),
            );
        }
    }

    /// On a fresh store, the approval `j` describes in the shape approval-view
    /// gives it, used by a start of `n`: with the person's approval it rests
    /// on (`rests_on`), and the fixer's (`fixer_starts`) and the refreshes'
    /// (`refreshes`) starts it counts, as ended starts before it (t_record).
    pub fn record_on(&self, n: u64, j: &Value) {
        self.store_db();
        let how = j["how"].as_str().unwrap_or_default().to_string();
        let mut at = epoch(j["at"].as_str().unwrap_or_default());
        let mut last = 0i64;
        let used = |by: &str, how: &str, at: i64, extra: &[&str]| -> i64 {
            let a = self.attempt(n);
            self.approve(n, by, how, at, extra);
            self.started(n, &[]);
            let (s, next) = (n.to_string(), (a + 1).to_string());
            self.store_must(
                &["cycle-end", &s, "--attempt", &next, "--status", "handoff"],
                None,
                Some(n),
            );
            at
        };
        let base = &j["rests_on"];
        if matches!(how.as_str(), "fixer" | "refresh") && base.is_object() {
            last = used(
                &text_of(&base["by"]),
                &text_of(&base["how"]),
                epoch(&text_of(&base["at"])),
                &["--body-sha", &text_of(&base["body_sha256"])],
            );
            let mut count = j["fixer_starts"].as_i64().unwrap_or(0);
            if how == "fixer" {
                count -= 1;
            }
            for _ in 0..count.max(0) {
                last = used("tehas-fixer", "fixer", last + 10, &[]);
            }
            if how == "refresh" {
                let head = "a".repeat(40);
                for _ in 1..j["refreshes"].as_i64().unwrap_or(1) {
                    last = used(
                        &text_of(&j["by"]),
                        "refresh",
                        last + 10,
                        &["--refresh-pr", "50", "--refresh-head", &head],
                    );
                }
            }
        }
        if at <= last {
            at = last + 10;
        }
        let mut input =
            json!({"links": j.get("links").cloned().filter(|v| !v.is_null()).unwrap_or(json!([]))});
        for k in ["comment_links", "retry_context", "driver", "spec"] {
            if let Some(v) = j.get(k) {
                input[k] = v.clone();
            }
        }
        if how == "refresh" {
            input["refresh_of"] = j.get("refresh_of").cloned().unwrap_or(Value::Null);
        }
        let file = self.t.join("record-input.json");
        fs::write(&file, input.to_string()).unwrap();
        let (s, at, file) = (n.to_string(), at.to_string(), file.to_string_lossy());
        let by = text_of(&j["by"]);
        let sum = text_of(&j["body_sha256"]);
        let mut a = vec![
            "approval-record",
            &s,
            "--at",
            &at,
            "--by",
            &by,
            "--how",
            &how,
            "--body-sha",
            &sum,
            "--input-file",
            &file,
        ];
        if matches!(how.as_str(), "label" | "fixer") {
            a.extend(["--label", "tehas:ready"]);
        }
        let title = text_of(&j["title_sha256"]);
        if !title.is_empty() {
            a.extend(["--title-sha", &title]);
        }
        let pr = j["refresh_of"]["pr"]
            .as_i64()
            .map_or("50".to_string(), |p| p.to_string());
        let head = j["refresh_of"]["head_sha"]
            .as_str()
            .map_or("a".repeat(40), String::from);
        if how == "refresh" {
            a.extend(["--refresh-pr", &pr, "--refresh-head", &head]);
        }
        self.store_ok(&a);
        self.started(n, &[]);
    }

    /// The approval of the start of #7 this world holds (record_json).
    pub fn record(&self) -> Value {
        self.record.borrow().clone()
    }

    /// The approval of #7 put on a fresh store with a start that used it
    /// (record_put).
    pub fn record_put(&self) {
        let j = self.record();
        self.record_on(7, &j);
    }

    /// The approval of a start: by `by`, of `body`, with `links`, by label on
    /// 2026-10-14 (record_is).
    pub fn record_is(&self, by: &str, body: &str, links: &[&str]) {
        *self.record.borrow_mut() = json!({"at": "2026-10-14T00:00:00Z", "by": by, "how": "label",
            "body_sha256": body_sha256(body), "links": links});
        self.record_put();
    }

    /// The approval changed, and put again (record_edit).
    pub fn record_edit(&self, change: impl FnOnce(&mut Value)) {
        change(&mut self.record.borrow_mut());
        self.record_put();
    }

    /// The approval binds the title's hash too (record_binds_title).
    pub fn record_binds_title(&self, title: &str) {
        let sum = title_sha256(title);
        self.record_edit(|v| v["title_sha256"] = json!(sum));
    }

    /// A run as tehasbot cycle makes it by hand: its start on record, and
    /// tehas work given its attempt.
    pub fn under_cycle(&self) {
        self.started(7, &["--by-hand"]);
        let a = self.attempt(7).to_string();
        self.env("TEHAS_CYCLE_ATTEMPT", &a);
    }
}

impl Drop for World {
    fn drop(&mut self) {
        for f in ["sleeper.pid", "client-hangs.pid"] {
            if let Ok(pid) = fs::read_to_string(self.t.join(f)) {
                let _ = Command::new("kill")
                    .args(["-KILL", pid.trim()])
                    .stderr(Stdio::null())
                    .status();
            }
        }
        if std::thread::panicking() {
            return;
        }
        let denied = self.read("denied");
        assert!(denied.is_empty(), "no stub stands in for:\n{denied}");
        let mut left = vec![];
        for (dir, start, end) in OLD_STATE {
            let Ok(entries) = fs::read_dir(self.home.join(dir)) else {
                continue;
            };
            for e in entries.flatten() {
                let f = e.file_name().to_string_lossy().into_owned();
                if f.starts_with(start) && f.ends_with(end) && f.len() >= start.len() + end.len() {
                    left.push(format!("{dir}/{f}"));
                }
            }
        }
        if let Ok(entries) = fs::read_dir(self.home.join("state/cycles")) {
            for e in entries.flatten() {
                let f = e.file_name().to_string_lossy().into_owned();
                if f.ends_with(".history.jsonl")
                    && f.starts_with(|c: char| ('1'..='9').contains(&c))
                {
                    left.push(format!("state/cycles/{f}"));
                }
            }
        }
        assert!(left.is_empty(), "an old record file is there: {left:?}");
    }
}

fn git(args: &[&str], dir: &Path) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

#[allow(dead_code)] // each test file uses a part of this module
/// What git says in `dir`, its last newline gone; a panic when it fails.
pub fn git_out(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout)
        .trim_end_matches('\n')
        .to_string()
}

#[allow(dead_code)] // each test file uses a part of this module
/// The exit status of git in `dir`.
pub fn git_rc(dir: &Path, args: &[&str]) -> i32 {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .code()
        .unwrap_or(-1)
}

#[allow(dead_code)] // each test file uses a part of this module
/// A commit in `dir` as user t, writing `file` with `line` when one is given.
pub fn commit(dir: &Path, subject: &str, file: Option<(&str, &str)>) {
    if let Some((f, line)) = file {
        fs::write(dir.join(f), format!("{line}\n")).unwrap();
        git(&["add", "-f", "--", f], dir);
    }
    git(
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            subject,
        ],
        dir,
    );
}

/// Remove the entries of `dir` named `pattern`, or starting with it when it
/// ends in `*`.
fn remove_like(dir: &Path, pattern: &str) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let hit = match pattern.strip_suffix('*') {
            Some(start) => name.starts_with(start),
            None => name == pattern,
        };
        if hit {
            let p = e.path();
            if p.is_dir() && !p.is_symlink() {
                let _ = fs::remove_dir_all(&p);
            } else {
                let _ = fs::remove_file(&p);
            }
        }
    }
}

/// A string as itself, any other value as its JSON, null as "".
fn text_of(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        v => v.to_string(),
    }
}

/// A word in single quotes, for a stub's body.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// The name curl's stub keeps an address under.
fn net_key(url: &str) -> String {
    url.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

#[allow(dead_code)] // each test file uses a part of this module
/// The commit the sandbox image's label names: `c` 40 times.
pub fn img_commit() -> String {
    "c".repeat(40)
}

#[allow(dead_code)] // each test file uses a part of this module
/// The hash of a body as the dispatcher records it and tehas work compares
/// it: its carriage returns gone. Written here apart from the Rust under test.
pub fn body_sha256(body: &str) -> String {
    format!("{:x}", Sha256::digest(body.replace('\r', "").as_bytes()))
}

#[allow(dead_code)] // each test file uses a part of this module
/// The hash of a title: its carriage returns and line ends gone.
pub fn title_sha256(title: &str) -> String {
    let t = title.replace(['\r', '\n'], "");
    format!("{:x}", Sha256::digest(t.as_bytes()))
}

#[allow(dead_code)] // each test file uses a part of this module
/// The sha256 of the body `Words.`, which an approval is recorded against.
pub fn wsum() -> String {
    format!("{:x}", Sha256::digest(b"Words."))
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
/// A container id: the digit 64 times.
pub fn slice_id(digit: char) -> String {
    std::iter::repeat_n(digit, 64).collect()
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

// --- The readers of a docker call's words (test/docker_args.sh) -------------

#[allow(dead_code)] // each test file uses a part of this module
/// The mount of the tmpfs and its options: the word after --tmpfs.
pub fn security_tmpfs() -> String {
    let w: Vec<&str> = SECURITY_OPTS.split(' ').collect();
    let i = w.iter().position(|x| *x == "--tmpfs").unwrap();
    w[i + 1].to_string()
}

#[allow(dead_code)] // each test file uses a part of this module
/// The words after each `flag`, as docker reads them.
pub fn security_of(args: &[String], flag: &str) -> Vec<String> {
    args.windows(2)
        .filter(|p| p[0] == flag)
        .map(|p| p[1].clone())
        .collect()
}

#[allow(dead_code)] // each test file uses a part of this module
/// The documented options in order, each followed by a space, from their
/// first full run in `args`; "" when they are not there in one run.
pub fn security_line(args: &[String]) -> String {
    let doc: Vec<&str> = SECURITY_OPTS.split(' ').collect();
    args.windows(doc.len())
        .find(|w| w.iter().zip(&doc).all(|(a, d)| a == d))
        .map(|w| w.iter().map(|a| format!("{a} ")).collect())
        .unwrap_or_default()
}

#[allow(dead_code)] // each test file uses a part of this module
/// How many times an option stands in `args`, `--flag=<value>` among them.
pub fn count_of(args: &[String], flag: &str) -> usize {
    args.iter()
        .filter(|a| a.split_once('=').map_or(a.as_str(), |(t, _)| t) == flag)
        .count()
}

#[allow(dead_code)] // each test file uses a part of this module
/// Every option of a docker run that is not one the command is known to
/// pass, up to the image: `valued` names the options that take a value,
/// `plain` those that take none, each list words split by spaces.
pub fn unknown_options(args: &[String], image: &str, valued: &str, plain: &str) -> Vec<String> {
    let valued: Vec<&str> = valued.split_whitespace().collect();
    let plain: Vec<&str> = plain.split_whitespace().collect();
    let host = regex::Regex::new(r"^--add-host=[A-Za-z0-9.-]+:[0-9a-fA-F.:]+$").unwrap();
    let mut out = vec![];
    let mut skip = false;
    for (i, a) in args.iter().enumerate() {
        if i == 0 || (i == 1 && a == "--pull=never") {
            continue;
        }
        if skip {
            skip = false;
            continue;
        }
        if a == image {
            break;
        }
        let t = a.split_once('=').map_or(a.as_str(), |(t, _)| t);
        if valued.contains(&t) {
            skip = t == a;
            continue;
        }
        if plain.contains(&t) || host.is_match(a) {
            continue;
        }
        out.push(a.clone());
    }
    out
}

#[allow(dead_code)] // each test file uses a part of this module
/// The security options as the manual's table gives them, one
/// `<option>|<value>` per row.
pub fn security_doc_rows() -> Vec<String> {
    let tmpfs = security_tmpfs().replace("size=1024m", "size=<half of --memory>m");
    vec![
        "--cap-drop|ALL".into(),
        "--cap-add|CHOWN".into(),
        "--cap-add|FOWNER".into(),
        "--security-opt|no-new-privileges:true".into(),
        format!("--tmpfs|{tmpfs}"),
    ]
}

// --- Runs and assertions ------------------------------------------------------

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

/// Nothing at `p`, not even a dangling link, as `find p -maxdepth 0` finds
/// none.
#[allow(dead_code)] // each test file uses a part of this module
pub fn absent(p: &Path) -> bool {
    fs::symlink_metadata(p).is_err()
}

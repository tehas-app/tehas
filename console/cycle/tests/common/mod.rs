//! What the tests of `tehas-cycle cycle` share: one cycle for issue #7 of
//! acme/widget against a stub gh, the real store, and a stub tehas work that
//! does what the case's fixture files say. The fixtures are files in `fx/`,
//! named as the stubs read them; the stubs record each call in `calls/<name>`.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Once;
use tehas_core::home::{Home, State};
use tehas_core::testdir::TestDir;
use tehas_core::testing::{link_exe, write_exe};

/// The stub gh. A body given on standard input is written down as an argument,
/// as the cycle's own calls give it. Its fixtures are named beside each arm.
const GH: &str = r##"#!/usr/bin/env bash
call="$*"
body=""
case " $* " in
  *" --body-file - "*) body=$(cat); call=${call/--body-file -/--body $body} ;;
  *" --input - "*) body=$(cat); body=$(jq -r '.body // empty' <<<"$body" 2>/dev/null); call=${call/--input -/-f body=$body} ;;
esac
printf '%s\n' "$call" >>"$T/calls/gh"
nap=$(command -pv sleep)
# gh-hangs: a GitHub that does not answer.
[ ! -e "$T/fx/gh-hangs" ] || exec "$nap" 30
rest_verb=$3
rest_path=$4
# fails <fixture> [<countdown>]: exit 1 when the first is there, or while the
# second holds a count above 0, which it lowers.
fails() {
  local left
  if [ -s "$T/fx/${2:-none}" ]; then
    left=$(cat "$T/fx/$2")
    [ "$left" -gt 0 ] || return 0
    echo $((left - 1)) >"$T/fx/$2"
    exit 1
  fi
  [ ! -e "$T/fx/$1" ] || exit 1
}
# countdown <fixture>: exit 1 while it holds a count above 0, which it lowers.
countdown() {
  local left
  [ -s "$T/fx/$1" ] || return 0
  left=$(cat "$T/fx/$1")
  [ "$left" -gt 0 ] || return 0
  echo $((left - 1)) >"$T/fx/$1"
  exit 1
}
# taken <fixture>: when it is there, the body of this call is a comment of the
# issue now, as GitHub keeps one it answered 5xx for.
taken() {
  [ -e "$T/fx/$1" ] || return 0
  jq -n --arg b "$body" --arg t "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    "[{id: 4002, body: \$b, created_at: \$t}]" >"$T/fx/comments.json"
}
not_found() { echo "message: Not Found" >&2; exit 1; }
# api_rest <verb> <path-glob>: whether this is gh api -X <verb> on that path
# of the issues endpoint.
api_rest() {
  [ "$rest_verb" = "$1" ] || return 1
  case "$rest_path" in
    repos/acme/widget/issues/$2) return 0 ;;
    *) return 1 ;;
  esac
}
case "$1 $2" in
  # A label of an issue or a pull request through the REST endpoint, and the
  # body of the issue. label-missing answers 404; mark-fails, unmark-fails and
  # patch-fails fail at once, the -times ones count down. With edit-slow the
  # call takes a second and says when it starts.
  "api -X")
    [ ! -e "$T/fx/edit-slow" ] || { : >"$T/calls/edit.started"; sleep 1; }
    if api_rest POST "*/labels"; then
      [ ! -e "$T/fx/label-missing" ] || not_found
      fails mark-fails mark-fails-times
    elif api_rest DELETE "*/labels/*"; then
      [ ! -e "$T/fx/label-missing" ] || not_found
      fails unmark-fails unmark-fails-times
    elif api_rest PATCH 7; then
      fails patch-fails
    else
      exit 1
    fi ;;
  "pr list")
    countdown pr-fails-times
    # pr.json is the whole answer. pr.txt is the address of one pull request
    # of the bot on the branch asked for, in the state pr-state says [OPEN],
    # whose body is pr-body [Fixes #7], a draft when pr-draft is there, created
    # now.
    if [ -e "$T/fx/pr.json" ]; then cat "$T/fx/pr.json"
    elif [ -e "$T/fx/pr.txt" ]; then
      head=$(printf "%s\n" "$@" | sed -n "/^--head$/{n;p;}")
      jq -n --arg url "$(cat "$T/fx/pr.txt")" --arg head "$head" \
        --arg state "$(cat "$T/fx/pr-state" 2>/dev/null || echo OPEN)" \
        --arg body "$(cat "$T/fx/pr-body" 2>/dev/null || echo "Fixes #7")" \
        --arg created "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
        --argjson draft "$([ -e "$T/fx/pr-draft" ] && echo true || echo false)" \
        "[{url: \$url, state: \$state, body: \$body, headRefName: \$head, isDraft: \$draft, createdAt: \$created}]"
    else echo "[]"; fi ;;
  "issue edit")
    countdown edit-fails-times
    # An edit whose arguments hold the text of edit-fails-on fails.
    if [ -s "$T/fx/edit-fails-on" ]; then
      case "$*" in *"$(cat "$T/fx/edit-fails-on")"*) exit 1 ;; esac
    fi ;;
  # The park question, whose id comes back. post-fails holds what gh says when
  # it fails; post-taken makes the failed post a comment all the same.
  "api repos/acme/widget/issues/"*"/comments") [ ! -e "$T/fx/post-fails" ] || { taken post-taken; cat "$T/fx/post-fails" >&2; exit 1; }; echo 4002 ;;
  # The comments of the issue. No comments.json fails the read.
  "api repos/acme/widget/issues/"*"/comments?per_page=100") cat "$T/fx/comments.json" 2>/dev/null || exit 1 ;;
  # A comment on the issue. comment-fails-times counts down, saying
  # comment-says; comment-fails holds what gh says, and comment-taken makes the
  # failed comment one all the same.
  "issue comment")
    if [ -s "$T/fx/comment-fails-times" ]; then
      left=$(cat "$T/fx/comment-fails-times")
      if [ "$left" -gt 0 ]; then
        echo $((left - 1)) >"$T/fx/comment-fails-times"
        cat "$T/fx/comment-says" >&2 2>/dev/null
        exit 1
      fi
    fi
    [ ! -e "$T/fx/comment-fails" ] || { taken comment-taken; cat "$T/fx/comment-fails" >&2; exit 1; } ;;
  # What the number is: an open issue unless issue-<n>.json says otherwise.
  "api repos/acme/widget/issues/"[0-9]*)
    n=${2#repos/acme/widget/issues/}
    case "$n" in *[!0-9]*) exit 1 ;; esac
    countdown issue-fails-times
    [ ! -e "$T/fx/issue-fails" ] || exit 1
    if [ -e "$T/fx/issue-$n.json" ]; then cat "$T/fx/issue-$n.json"; else echo "{\"number\":$n,\"state\":\"open\"}"; fi ;;
esac
"##;

/// The stub tehas work. A result and a progress the case wrote are put where
/// tehas work leaves them, and so is the note of a container that never
/// started. Without a progress of its own a case gets an empty one. work.sh is
/// what the runner does; rc is the exit of tehas work.
const WORK: &str = r#"#!/usr/bin/env bash
printf '%s\n' "$*" >>"$T/calls/fake-work"
for f in result progress; do
  [ ! -e "$T/fx/$f.json" ] || cp "$T/fx/$f.json" "$TEHAS_HOME/state/cycles/7.$f.json"
done
[ -e "$T/fx/progress.json" ] || [ -e "$T/fx/no-progress" ] ||
  echo "{\"roles\":[],\"reason\":\"\",\"reviewHeld\":false}" >"$TEHAS_HOME/state/cycles/7.progress.json"
[ -e "$T/fx/nostart" ] && cat "$T/fx/nostart" >"$TEHAS_HOME/state/cycles/7.nostart"
[ ! -e "$T/fx/work.sh" ] || . "$T/fx/work.sh"
exit "$(cat "$T/fx/rc" 2>/dev/null || echo 0)"
"#;

/// What tehas work leaves for a runner that died: an empty result, a progress,
/// the runner's status and exit, and a run log of the steps it reached.
const DIED: &str = r#"mkdir -p "$TEHAS_HOME/state/cycles" "$TEHAS_HOME/logs"
: >"$TEHAS_HOME/state/cycles/7.result.json"
echo '{"roles":[],"reason":"","reviewHeld":false}' >"$TEHAS_HOME/state/cycles/7.progress.json"
[ -s "$T/fx/status" ] && cat "$T/fx/status" >"$TEHAS_HOME/state/cycles/7.status"
cat "$T/fx/runner_exit" >"$TEHAS_HOME/state/cycles/7.runner_exit"
log=$TEHAS_HOME/logs/work-7-$(date -u +%y%m%d-%H%M%S).jsonl
: >"$log"
while IFS= read -r step; do
  printf '{"v":1,"kind":"step-started","issue":7,"event":{"step":"%s"}}\n' "$step" >>"$log"
done <"$T/fx/steps"
cp "$log" "$TEHAS_HOME/logs/dispatch-work-7.log"
"#;

/// The settings the console gives tehas-cycle for the minimal instance of the
/// shell tests, with the paths under the home left out.
const SETTINGS: &[(&str, &str)] = &[
    ("PI_ENSEMBLE_HOST_ALIASES", ""),
    ("PI_ENSEMBLE_IMAGE", "tehas/pi-rukas:uid1000"),
    ("PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS", "1800000"),
    ("PI_ENSEMBLE_MAX_WORKSTREAMS", "1"),
    ("PI_ENSEMBLE_NO_DOCKER_SOCKET", "1"),
    ("PI_ENSEMBLE_NO_SSH", "1"),
    ("PI_ENSEMBLE_SUBAGENT_MODEL", "test-model"),
    ("PI_ENSEMBLE_SUBAGENT_PROVIDER", "test-provider"),
    ("PI_ENSEMBLE_VERIFY_SUITE_TIMEOUT_MS", "1800000"),
    ("PI_ENSEMBLE_VERIFY_TIMEOUT_MS", "1800000"),
    ("TEHAS_ALLOWED_LABELERS", "alice"),
    ("TEHAS_ANALYTICS", "on"),
    ("TEHAS_BOT_LOGIN", "tehas-bot[bot]"),
    ("TEHAS_CHILD_TIMEOUT", "7200"),
    (
        "TEHAS_CLEAN_DIRS",
        "target node_modules __pycache__ .venv .pytest_cache .mypy_cache .next .turbo .gradle",
    ),
    ("TEHAS_CLONE_DEPTH", "100"),
    ("TEHAS_CONSOLE_CMD", "fake-console"),
    ("TEHAS_CONTROL_ISSUE", "1"),
    ("TEHAS_CYCLE_RETRY_WAIT", "0"),
    ("TEHAS_DRIVER", "classic"),
    ("TEHAS_GITHUB_APP_ID", "1"),
    ("TEHAS_LEAN", "0"),
    ("TEHAS_MAX_CYCLES", "127"),
    ("TEHAS_MILLWRIGHT_LOGIN", "tehas-bot[bot]"),
    ("TEHAS_READ_ONLY", "0"),
    ("TEHAS_REPO", "acme/widget"),
    ("TEHAS_RUKAS_WORKAROUNDS", ""),
    ("TEHAS_STATUS_CMD", "fake-status"),
    ("TEHAS_WORK_CMD", "fake-work"),
    ("TEHAS_WORK_IMAGES", "0"),
    ("TEHAS_WORK_STALL", "3600"),
    ("TEHAS_WORK_TIMEOUT", "21600"),
];

/// The logins whose user ids install.sh records, as the shell tests list them.
const LOGINS: &[&str] = &[
    "tehas-bot",
    "alice",
    "bob",
    "bob-2",
    "carol",
    "boss",
    "tehas-millwright",
    "tehas-mill",
    "millbot",
    "mill-bot",
    "acme-millwright",
    "acme-app-millwright",
];

/// Files of records that live on the store now, by their ends in `state/cycles`:
/// no cycle leaves one.
const OLD_STATE: &[&str] = &[
    ".taken",
    ".by",
    ".requeued",
    ".rukas",
    ".image",
    ".settings",
    ".branch",
    ".approved.json",
    ".approved.json.new",
    ".history.jsonl",
    ".park",
    ".park.new",
    ".quiet",
];

#[allow(dead_code)] // each test file uses a part of this module
/// The sha256 of the body `Words.`, which an approval is recorded against.
pub fn wsum() -> String {
    format!("{:x}", Sha256::digest(b"Words."))
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

/// The end of one run: its exit status and what it printed, both streams.
pub struct Run {
    pub rc: i32,
    pub out: String,
}

/// One host with issue #7, as reset() in the shell tests made it.
pub struct World {
    t: TestDir,
    pub home: PathBuf,
    pub fx: PathBuf,
    env: Vec<(String, String)>,
}

#[allow(dead_code)] // each test file uses a part of this module
impl World {
    pub fn new(name: &str) -> World {
        let t = TestDir::new(&format!("cycle-{name}"));
        let home = t.join("home/tehas");
        let fx = t.join("fx");
        for d in ["fx", "calls", "bin", "tmp", "instance"] {
            fs::create_dir_all(t.join(d)).unwrap();
        }
        for d in ["libexec", "logs", "state"] {
            fs::create_dir_all(home.join(d)).unwrap();
        }
        link_exe(store_binary(), t.join("store-bin")).unwrap();
        link_exe(store_binary(), home.join("libexec/store")).unwrap();
        let w = World {
            t,
            home,
            fx,
            env: vec![],
        };
        w.stub("gh", GH);
        w.stub("fake-work", WORK);
        w.stub(
            "docker",
            "[ -e \"$T/fx/image.label\" ] || exit 1; cat \"$T/fx/image.label\"",
        );
        for s in ["fake-status", "fake-console", "systemctl"] {
            w.stub(s, "");
        }
        w.store_ok(&[
            "init",
            "--version",
            "v0.0.0",
            "--commit",
            "0123456789abcdef0123456789abcdef01234567",
            "--instance",
            "instance",
            "--repo",
            "acme/widget",
        ]);
        let ids: String = LOGINS.iter().map(|l| format!("{l} {}\n", uid(l))).collect();
        fs::write(Home::new(&w.home).state(State::AccountIds), ids).unwrap();
        let until = tehas_core::time::now() + 86400;
        for (dir, token) in [
            ("github", "app"),
            ("github-cycle", "cycle"),
            ("github-session", "session"),
        ] {
            let d = Home::new(&w.home).state_dir().join(dir);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("token"), format!("ghs_{token}\n")).unwrap();
            fs::write(d.join("token.expires"), format!("{until}\n")).unwrap();
        }
        fs::write(
            Home::new(&w.home).state(State::BotLogin),
            "tehas-bot[bot]\n",
        )
        .unwrap();
        w.instance(&[
            "TEHAS_GITHUB_APP_ID=1",
            "TEHAS_REPO=acme/widget",
            "TEHAS_CONTROL_ISSUE=1",
            "TEHAS_ALLOWED_LABELERS=alice",
            "PI_ENSEMBLE_SUBAGENT_MODEL=test-model",
            "PI_ENSEMBLE_SUBAGENT_PROVIDER=test-provider",
        ]);
        w
    }

    /// The scratch root, `$T` of the stubs.
    pub fn t(&self) -> &Path {
        &self.t
    }

    /// The cycle's state directory.
    pub fn state(&self) -> PathBuf {
        Home::new(&self.home).cycles_dir()
    }

    /// The workspace tehas work makes for #7.
    pub fn workdir(&self) -> PathBuf {
        self.home.join("work/widget-7")
    }

    /// How a comment names the workspace: in ~ form.
    pub fn kept_here(&self) -> String {
        "~/tehas/work/widget-7".to_string()
    }

    /// A stub program on the PATH of a run, recording its arguments.
    pub fn stub(&self, name: &str, body: &str) {
        let text = if body.starts_with("#!") {
            body.to_string()
        } else {
            format!("#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >>\"$T/calls/{name}\"\n{body}\n")
        };
        write_exe(self.t.join("bin").join(name), text).unwrap();
    }

    /// A stub of the analytics program where the cycle looks for it.
    pub fn stub_analytics(&self, body: &str) {
        let text = format!(
            "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >>\"$T/calls/analytics\"\n{body}\n"
        );
        write_exe(self.home.join("libexec/analytics"), text).unwrap();
    }

    /// instance.env, as given.
    pub fn instance(&self, lines: &[&str]) {
        let mut text = lines.join("\n");
        text.push('\n');
        fs::write(self.t.join("instance/instance.env"), text).unwrap();
    }

    /// A variable of every later run, as T_ENV+= is.
    pub fn env(&mut self, k: &str, v: &str) {
        self.env.push((k.into(), v.into()));
    }

    /// A fixture file, with these bytes.
    pub fn fx(&self, name: &str, text: &str) {
        fs::write(self.fx.join(name), text).unwrap();
    }

    /// A fixture file that is there, empty.
    pub fn touch(&self, name: &str) {
        self.fx(name, "");
    }

    /// What the runner does, as a bash snippet tehas work sources.
    pub fn work(&self, snippet: &str) {
        self.fx("work.sh", &format!("{snippet}\n"));
    }

    /// More of what the runner does, after what it does already.
    pub fn work_more(&self, snippet: &str) {
        let path = self.fx.join("work.sh");
        let mut text = fs::read_to_string(&path).unwrap_or_default();
        text.push_str(snippet);
        text.push('\n');
        fs::write(path, text).unwrap();
    }

    /// The runner's result for the cycle and the exit of tehas work that goes
    /// with it. A handoff's cap is a waiting merge unless one is named; "-"
    /// is none.
    pub fn ended(&self, status: &str, step: &str, branch: &str, cap: &str) {
        let cap = match (cap, status) {
            ("", "handoff") => "awaiting-human-merge",
            ("-", _) => "",
            (c, _) => c,
        };
        fs::create_dir_all(self.workdir()).unwrap();
        let mut entry = json!({"issue": 7, "status": status, "failedStep": step, "branch": branch});
        if !cap.is_empty() {
            entry["cap"] = json!(cap);
        }
        let r = json!({"schemaVersion": 5, "issues": [entry]});
        self.fx("result.json", &r.to_string());
        let rc = match status {
            "merged" => 0,
            "handoff" => 10,
            "aborted" => 20,
            "interrupted" => 21,
            _ => 30,
        };
        self.fx("rc", &format!("{rc}\n"));
    }

    /// The runner's result, changed.
    pub fn result(&self, change: impl FnOnce(&mut Value)) {
        let path = self.fx.join("result.json");
        let mut v: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        change(&mut v);
        fs::write(path, v.to_string()).unwrap();
    }

    /// The first entry of the runner's result, changed.
    pub fn entry(&self, change: impl FnOnce(&mut Value)) {
        self.result(|r| change(&mut r["issues"][0]));
    }

    /// The progress summary tehas work leaves.
    pub fn progress(&self, v: Value) {
        self.fx("progress.json", &v.to_string());
    }

    /// The bot's pull request on the branch, merged.
    pub fn merged_pr(&self) {
        self.fx("pr.txt", "https://github.com/acme/widget/pull/9\n");
        self.fx("pr-state", "MERGED\n");
    }

    /// The bot's pull request on the branch, open.
    pub fn open_pr(&self, url: &str) {
        let url = if url.is_empty() {
            "https://github.com/acme/widget/pull/347"
        } else {
            url
        };
        self.fx("pr.txt", &format!("{url}\n"));
        self.fx("pr-state", "OPEN\n");
    }

    /// The note tehas work leaves for a container that never started.
    pub fn nostart(&self, bytes: &str) {
        self.fx("nostart", &format!("{bytes}\n"));
    }

    /// The runner of a cycle that died, having reached these steps. The
    /// words `rc=<n>` (the container's exit, 134), `work=<n>` (tehas work's
    /// own, 2 or 137 for a kill) and `status=<word>` (how tehas work saw the
    /// container end) may be among them.
    pub fn died(&self, words: &[&str]) {
        let (mut rc, mut word, mut wrk) = ("134", "", "");
        let mut steps = vec![];
        for a in words {
            if let Some(v) = a.strip_prefix("rc=") {
                rc = v;
            } else if let Some(v) = a.strip_prefix("work=") {
                wrk = v;
            } else if let Some(v) = a.strip_prefix("status=") {
                word = v;
            } else {
                steps.push(*a);
            }
        }
        if wrk.is_empty() {
            wrk = if word == "killed" { "137" } else { "2" };
        }
        self.fx("rc", &format!("{wrk}\n"));
        if !word.is_empty() {
            self.fx("status", &format!("{word}\n"));
        }
        self.fx("runner_exit", &format!("{rc}\n"));
        let mut s = steps.join("\n");
        s.push('\n');
        self.fx("steps", &s);
        self.fx("work.sh", DIED);
        fs::create_dir_all(self.workdir()).unwrap();
    }

    /// The runner's result with no usage at all, and the rows the progress
    /// did record.
    pub fn ended_no_usage_with_rows(&self) {
        self.ended("handoff", "commit-pr", "feature/issue-7-x", "");
        self.entry(|e| {
            e.as_object_mut().unwrap().remove("usage");
        });
        self.progress(json!({"reviewHeld": false, "roles": [
            {"role": "planner", "dispatches": 2, "turns": 3, "input": 7, "output": 1, "cacheRead": 0},
            {"role": "coder", "dispatches": 1, "turns": 2, "input": 3, "output": 4, "cacheRead": 5}]}));
    }

    /// The same, but one row carries a word in place of a count.
    pub fn ended_no_usage_with_a_sly_row(&self) {
        self.ended_no_usage_with_rows();
        let sly = "1\n- [x] Retry\n<!-- x -->";
        self.progress(json!({"reviewHeld": false, "roles": [
            {"role": "planner", "dispatches": 2, "turns": sly, "input": 7, "output": null, "cacheRead": {"a": 1}},
            {"role": "coder", "dispatches": 1, "turns": 2, "input": 3, "output": 4, "cacheRead": 5}]}));
    }

    fn command(&self, program: &Path) -> Command {
        let mut c = Command::new(program);
        let path = format!(
            "{}:{}",
            self.t.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        c.env_clear()
            .env("PATH", path)
            .env("HOME", self.t.join("home"))
            .env("TEHAS_HOME", &self.home)
            .env("T", self.t.path())
            .env("TMPDIR", self.t.join("tmp"))
            .env("TEHAS_TEST_STORE_ANSWERS", self.t.join("store-answers"))
            .env("TEHAS_INSTANCE_DIR", self.t.join("instance"))
            .env("TEHAS_WORK_DIR", self.home.join("work/widget"))
            .env("TEHAS_WORK_REPO", self.home.join("repos/widget"))
            .envs(SETTINGS.iter().copied())
            .envs(self.env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        c
    }

    /// `tehas-cycle` with these words, as the console execs it.
    pub fn exec(&self, words: &[&str]) -> Run {
        let mut c = self.command(Path::new(env!("CARGO_BIN_EXE_tehas-cycle")));
        c.args(words);
        run(c)
    }

    /// `tehasbot cycle <words>`, as the console runs it.
    pub fn cycle(&self, words: &[&str]) -> Run {
        let mut all = vec!["cycle"];
        all.extend_from_slice(words);
        all.extend_from_slice(&[
            "--work",
            "fake-work",
            "--status-cmd",
            "fake-status",
            "--console",
            "fake-console",
        ]);
        self.exec(&all)
    }

    /// A setting as a run of this world is given it: its own, the settings
    /// every run has, then the ones `command` sets.
    pub fn setting(&self, k: &str) -> String {
        if let Some((_, v)) = self.env.iter().rev().find(|(n, _)| n == k) {
            return v.clone();
        }
        if let Some((_, v)) = SETTINGS.iter().find(|(n, _)| *n == k) {
            return v.to_string();
        }
        match k {
            "HOME" => self.t.join("home").display().to_string(),
            "TEHAS_HOME" => self.home.display().to_string(),
            "TEHAS_WORK_DIR" => self.home.join("work/widget").display().to_string(),
            _ => String::new(),
        }
    }

    /// The writes of #7's report, decided and made in process on the ports'
    /// fake `site`: the start on record, the stub tehas work this world sets
    /// up, the facts the report reads from this home, and its writes through
    /// the fake's handles. `run7` proves the binary reaches the adapter; this
    /// is what its report decides to write, in tehas's terms.
    pub fn report_writes(&self, site: &tehas_ports::fake::World) -> Vec<tehas_ports::Write> {
        let fake = tehas_ports::fake::FakeSite::new(site);
        self.report_through(site, &fake).applied
    }

    /// The same, through `through`, a site over `site` such as a `Faulty`
    /// one: the writes `site` took, and each write the report sent.
    pub fn report_through(
        &self,
        site: &tehas_ports::fake::World,
        through: &impl tehas_ports::Site,
    ) -> Tried {
        use tehas_ports::{Actor, Caller, Scope};
        // The cycle's lock, held as tehas-cycle holds it while a cycle runs:
        // the store starts no cycle and takes no park without it.
        let cycles = Home::new(&self.home).cycles_dir();
        fs::create_dir_all(&cycles).unwrap();
        let _held =
            tehas_cycle::lock::take(&cycles.join("7.lock"), std::time::Duration::from_secs(5))
                .expect("the lock")
                .expect("the lock is free");
        // As tehas-cycle starts it: by hand unless the unit runs it.
        let mut start = vec!["cycle-start", "7"];
        if self.setting("TEHAS_CYCLE_FROM") != "unit" {
            start.push("--by-hand");
        }
        let started = Command::new(self.t.join("store-bin"))
            .args(&start)
            .env("TEHAS_HOME", &self.home)
            .output()
            .unwrap();
        assert!(started.status.success(), "cycle-start: {started:?}");
        let answer: Value = serde_json::from_slice(&started.stdout).unwrap_or(Value::Null);
        let text = |k: &str| {
            answer
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        let attempt = answer.get("attempt").and_then(Value::as_u64).unwrap_or(0);
        // As tehas-cycle clears them before tehas work: an earlier cycle's notes.
        for note in tehas_core::cycle_notes::started() {
            let _ = fs::remove_file(cycles.join(format!("7.{note}")));
        }
        let mut work = self.command(&self.t.join("bin/fake-work"));
        work.arg("7");
        let rc = run(work).rc;
        let start = tehas_cycle::host::Start {
            issue: 7,
            attempt,
            rc: i64::from(rc),
            started: tehas_core::time::now(),
            signalled: false,
            by: text("started_by"),
            how: text("how"),
            approval_at: match answer.get("approval_at") {
                Some(Value::Number(x)) => x.to_string(),
                _ => String::new(),
            },
            requeued: answer.get("requeued") == Some(&Value::Bool(true)),
            now: None,
            console: "fake-console".to_string(),
            status_cmd: "fake-status".to_string(),
            status_args: vec![],
        };
        let env = |k: &str| self.setting(k);
        let facts_json = tehas_cycle::host::facts_with(&self.home, &start, &env);
        let facts = tehas_cycle::Facts::from_json(&facts_json).expect("the facts read");
        let mut ask = self.fx_answers();
        let mut r = tehas_cycle::report::report(&facts, &mut ask);
        let fake = through;
        let (by, c) = (Actor::Ours(Scope::Full), Caller::new("report"));
        let sent = std::rc::Rc::new(std::cell::RefCell::new(vec![]));
        let handles = tehas_cycle::apply::Handles {
            writes: Box::new(Counted {
                inner: Box::new(tehas_ports::Writer {
                    tickets: fake.ticket_writes(&by, c).unwrap(),
                    blueprints: fake.blueprint_writes(&by, c).unwrap(),
                    code_host: fake.code_host_writes(&by, c).unwrap(),
                }),
                sent: sent.clone(),
            }),
            tickets: fake.tickets(&by, c).unwrap(),
            blueprints: fake.blueprints(&by, c).unwrap(),
            code_host: fake.code_host(&by, c).unwrap(),
        };
        let how = tehas_cycle::apply::How {
            repo: facts.repo.clone(),
            issue: 7,
            attempt,
            now: tehas_core::time::now(),
            gh_bin: self.as_run(&self.t.join("bin/gh-as-run"), &self.t.join("bin/gh")),
            tries: 3,
            retry_wait: std::time::Duration::ZERO,
            bound: std::time::Duration::from_secs(10),
            grace: std::time::Duration::ZERO,
            workdir: tehas_cycle::host::workdir_with(&start, &env),
        };
        // The store the report calls answers as a run's store does, unless
        // the case took it away.
        let store = self.home.join("libexec/store");
        if store.exists() {
            self.as_run(&store, &self.t.join("store-bin"));
        }
        let _ = tehas_cycle::apply::write(&self.home, &how, &mut r, &facts, &mut ask, handles);
        let sent = sent.borrow().clone();
        Tried {
            applied: site.wrote().into_iter().map(|w| w.what).collect(),
            sent,
        }
    }

    /// `path`, a program that runs `program` with what a run of this world
    /// gives the stubs and the store: `$T` and the store's answers.
    fn as_run(&self, path: &Path, program: &Path) -> String {
        let text = format!(
            "#!/usr/bin/env bash\nexport T='{}' TEHAS_TEST_STORE_ANSWERS='{}'\nexec '{}' \"$@\"\n",
            self.t.display(),
            self.t.join("store-answers").display(),
            program.display()
        );
        write_exe(path, text).unwrap();
        path.display().to_string()
    }

    /// The report's questions of GitHub answered from the fixtures the stub
    /// gh answers them from: the bot's pull request of `pr.txt` (or the whole
    /// of `pr.json`), none at all while `pr-fails-times` outlasts the report's
    /// three asks, and the issue of `issue-7.json`.
    pub fn fx_answers(&self) -> tehas_cycle::report::Given {
        let fx = |n: &str| fs::read_to_string(self.fx.join(n)).ok();
        let or = |n: &str, d: &str| fx(n).map_or(d.to_string(), |t| t.trim().to_string());
        let refused = fx("pr-fails-times")
            .and_then(|t| t.trim().parse::<u32>().ok())
            .is_some_and(|n| n >= 3);
        let prs = if refused {
            Value::Null
        } else if let Some(all) = fx("pr.json") {
            serde_json::from_str(&all).unwrap_or(Value::Null)
        } else if let Some(url) = fx("pr.txt") {
            json!([{"url": url.trim(), "state": or("pr-state", "OPEN"),
                "body": or("pr-body", "Fixes #7"), "isDraft": self.fx.join("pr-draft").exists(),
                "createdAt": tehas_core::time::iso(tehas_core::time::now())}])
        } else {
            json!([])
        };
        let issue = fx("issue-7.json")
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_else(|| json!({"number": 7, "state": "open"}));
        let attention = issue["labels"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|l| l.get("name").and_then(Value::as_str) == Some("needs-human-attention"));
        tehas_cycle::report::Given(json!({"prs": prs, "attention": attention, "issue": issue}))
    }

    /// A cycle of #7.
    pub fn run7(&self) -> Run {
        self.cycle(&["7"])
    }

    /// What a stub was called with, one call per line.
    pub fn calls(&self, name: &str) -> String {
        fs::read_to_string(self.t.join("calls").join(name)).unwrap_or_default()
    }

    /// The gh calls.
    pub fn gh(&self) -> String {
        self.calls("gh")
    }

    /// The calls are forgotten.
    pub fn clear_calls(&self) {
        let d = self.t.join("calls");
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d).unwrap();
    }

    /// A verb of the real store against this home; its answer, or a panic.
    pub fn store_ok(&self, args: &[&str]) -> Value {
        let out = self.store(args);
        assert_eq!(out.rc, 0, "store {args:?}: {}", out.out);
        serde_json::from_str(&out.out).unwrap_or(Value::Null)
    }

    /// A verb of the real store against this home.
    pub fn store(&self, args: &[&str]) -> Run {
        let mut c = Command::new(self.t.join("store-bin"));
        c.env("TEHAS_HOME", &self.home).args(args);
        let out = c.output().unwrap();
        Run {
            rc: out.status.code().unwrap_or(-1),
            out: String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
        }
    }

    /// What the store has on record, through a read verb.
    pub fn row(&self, args: &[&str]) -> Value {
        self.store_ok(args)
    }

    /// The store verb run under the issue's lock, as a cycle holds it.
    pub fn store_locked(&self, args: &[&str]) {
        fs::create_dir_all(self.state()).unwrap();
        let out = Command::new("flock")
            .args(["-w", "5"])
            .arg(self.state().join("7.lock"))
            .arg(self.t.join("store-bin"))
            .args(args)
            .env("TEHAS_HOME", &self.home)
            .output()
            .unwrap();
        assert!(out.status.success(), "store {args:?}: {out:?}");
    }

    /// An approval on record, as the dispatcher records one.
    pub fn approve(&self, by: &str, how: &str, at: u64, extra: &[&str]) {
        let at = at.to_string();
        let mut a = vec![
            "approval-record",
            "7",
            "--at",
            &at,
            "--by",
            by,
            "--how",
            how,
        ];
        if matches!(how, "approved" | "millwright") {
            a.extend(["--label", "tehas:ready"]);
        }
        let sum = wsum();
        if !extra.contains(&"--body-sha") {
            a.extend(["--body-sha", &sum]);
        }
        a.extend_from_slice(extra);
        self.store_ok(&a);
    }

    /// The attempt on record for #7, 0 for none.
    pub fn attempt(&self) -> u64 {
        self.row(&["cycle-show", "7"])["cycle"]["attempt"]
            .as_u64()
            .unwrap_or(0)
    }

    /// An earlier cycle of #7 that ended with these words, started by `by`
    /// with an approval of `how`, or by hand for "-".
    pub fn t_ended(&self, by: &str, how: &str, args: &[&str]) {
        let a = self.attempt();
        if by == "-" {
            self.store_locked(&["cycle-start", "7", "--by-hand"]);
        } else {
            self.approve(by, how, 1_700_000_000 + a, &[]);
            self.store_locked(&["cycle-start", "7"]);
        }
        let next = (a + 1).to_string();
        let mut e = vec!["cycle-end", "7", "--attempt", &next];
        if !args.contains(&"--status") {
            e.extend(["--status", "failed"]);
        }
        e.extend_from_slice(args);
        self.store_locked(&e);
    }

    /// A park of #7 on record, with its question when an id is given.
    pub fn t_park(&self, reason: &str, question: &str, args: &[&str]) {
        let a = self.attempt().to_string();
        let mut p = vec!["park", "7", "--attempt", &a, "--reason", reason];
        p.extend_from_slice(args);
        self.store_ok(&p);
        if question != "-" {
            self.store_ok(&["question-asked", "7", "--id", question, "--kind", "park"]);
        }
    }

    /// The store answers a call whose arguments begin with `args` with `rc`
    /// and `line`, for its first `times` calls (all for 0).
    pub fn store_answers(&self, args: &str, rc: i32, line: &str, times: u32) {
        let times = if times == 0 {
            String::new()
        } else {
            format!("|{times}")
        };
        fs::write(
            self.t.join("store-answers"),
            format!("{args}|{rc}|{line}{times}\n"),
        )
        .unwrap();
        let _ = fs::remove_file(self.t.join("store-answers.n"));
    }

    /// The store answers as itself again.
    pub fn store_answers_off(&self) {
        let _ = fs::remove_file(self.t.join("store-answers"));
    }

    /// How many calls the answer matched.
    pub fn store_answered(&self) -> u64 {
        fs::read_to_string(self.t.join("store-answers.n"))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    }

    /// #7's current park on record.
    pub fn park(&self) -> Value {
        self.row(&["park-show", "7"])["park"].clone()
    }

    /// The question of #7's current park, "none" without one.
    pub fn park_q(&self) -> String {
        match &self.park()["question"] {
            Value::Null => "none".into(),
            Value::String(s) => s.clone(),
            v => v.to_string(),
        }
    }

    /// The first row of #7's history.
    pub fn history0(&self) -> Value {
        self.row(&["history-show", "7"])["rows"][0].clone()
    }

    /// The names of the files in a directory, sorted.
    pub fn files(&self, dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir)
            .map(|d| {
                d.map(|e| e.unwrap().file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }
}

impl Drop for World {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        let mut left = vec![];
        let home = Home::new(&self.home);
        for f in self.files(&home.cycles_dir()) {
            if OLD_STATE.iter().any(|suffix| f.ends_with(suffix)) {
                left.push(home.cycles_dir().join(f));
            }
        }
        for f in self.files(&home.state_dir()) {
            if f.starts_with("park-") || f.starts_with("plan-") {
                left.push(home.state_dir().join(f));
            }
        }
        assert!(left.is_empty(), "an old record file is there: {left:?}");
    }
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

/// The writes of a report: those the site took, and each one sent, a write
/// tried again once for each try.
pub struct Tried {
    pub applied: Vec<tehas_ports::Write>,
    pub sent: Vec<tehas_ports::Write>,
}

/// A writer that keeps each write it is given before it sends it on.
struct Counted {
    inner: Box<dyn tehas_ports::Writes>,
    sent: std::rc::Rc<std::cell::RefCell<Vec<tehas_ports::Write>>>,
}

impl tehas_ports::Writes for Counted {
    fn send(
        &mut self,
        w: &tehas_ports::Write,
        files: &dyn tehas_ports::Blobs,
    ) -> tehas_ports::Got<tehas_ports::Receipt> {
        self.sent.borrow_mut().push(w.clone());
        self.inner.send(w, files)
    }
    fn read_only(&self) -> bool {
        self.inner.read_only()
    }
}

/// The same case run twice: the binary on one world, for what it owns (its
/// exit, the store, its reads of GitHub), and its report decided and written
/// in process on the ports' fake on another, for what the report writes.
#[allow(dead_code)] // each test file uses a part of this module
pub fn twin(name: &str, setup: impl Fn(&mut World)) -> (World, Run, String) {
    let mut w = World::new(name);
    setup(&mut w);
    let r = w.run7();
    let mut twin = World::new(&format!("{name}-writes"));
    setup(&mut twin);
    let said = said(&twin.report_writes(&site()));
    (w, r, said)
}

/// A case's report alone, in process, through the site `wrap` makes of the
/// fake: its world, and the writes the site took and each write sent, as
/// `said` says them.
#[allow(dead_code)] // each test file uses a part of this module
pub fn through<S: tehas_ports::Site>(
    name: &str,
    setup: impl Fn(&mut World),
    wrap: impl FnOnce(tehas_ports::fake::FakeSite) -> S,
) -> (World, String, String) {
    let mut w = World::new(name);
    setup(&mut w);
    let world = site();
    let t = w.report_through(&world, &wrap(tehas_ports::fake::FakeSite::new(&world)));
    (w, said(&t.applied), said(&t.sent))
}

/// The site the report writes to in process: issue 7, running, and the bot's
/// pull requests #9 and #347 of the cases.
#[allow(dead_code)] // each test file uses a part of this module
pub fn site() -> tehas_ports::fake::World {
    use tehas_ports::{Mark, State};
    // The site's clock reads the host's, as a journal row's time is the host's.
    let now = tehas_ports::At::new(tehas_core::time::now());
    tehas_ports::fake::World::new()
        .on_clock(tehas_ports::fake::Clock::new(now))
        .ticket(7, |t| {
            t.title("Install.sh checks the instance env")
                .mark(Mark::State(State::Running))
        })
        .change(9, "feature/issue-7-a", |c| c)
        .change(347, "feature/issue-7-b", |c| c)
}

/// The report's writes as lines a test reads: each mark taken off or put on,
/// and each note or comment with its text, in their order.
#[allow(dead_code)] // each test file uses a part of this module
pub fn said(writes: &[tehas_ports::Write]) -> String {
    use tehas_ports::{Body, Write};
    let text = |b: &Body| match b {
        Body::Text(t) => t.clone(),
        Body::Summed(_) => String::new(),
    };
    writes
        .iter()
        .map(|w| match w {
            Write::Unmark { mark, .. } => format!("unmark {mark:?}"),
            Write::Mark { mark, .. } => format!("mark {mark:?}"),
            Write::TicketNote { body, .. } => format!("note: {}", text(body)),
            Write::CommentChange { no, body, .. } => {
                format!("change {} comment: {}", no.get(), text(body))
            }
            Write::UnmarkChange { no, mark } => format!("change {} unmark {mark:?}", no.get()),
            other => format!("{:?}", other.kind()),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The writes the stub saw the adapter send, by their method alone: the
/// binary's report reaches the code host through the adapter at all. No path
/// or field is read, so no GitHub word is written here.
#[allow(dead_code)] // each test file uses a part of this module
pub fn write_methods(w: &World) -> Vec<String> {
    w.gh()
        .lines()
        .filter_map(|l| l.strip_prefix("api -X "))
        .filter_map(|rest| rest.split_whitespace().next())
        .map(str::to_string)
        .collect()
}

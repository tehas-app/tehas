//! What the tests of `tehas index` share: the command as a host has it, the
//! console's `run` beside tehas-cycle, against a stub docker that records the
//! words of each call, one per line, in `calls/docker.args.<pid>` and answers
//! as the indexer would, the real timeout, and a scratch acme/widget whose
//! default branch is main.
//!
//! A host starts as new() leaves it: the minimal instance, a slice of 8 GiB
//! with nothing in it, Docker on the systemd driver and cgroup v2, nothing
//! indexed. Each test is one host, so what a shell case inherited from the
//! case before it, a test makes itself.

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

/// The id the stub gives the sandbox image, and another one a test puts in
/// its place with IMAGE_ID.
#[allow(dead_code)] // each test file uses a part of this module
pub const IMAGE_ID: &str =
    "sha256:1111111111111111111111111111111111111111111111111111111111111111";
#[allow(dead_code)] // each test file uses a part of this module
pub const OTHER_ID: &str =
    "sha256:2222222222222222222222222222222222222222222222222222222222222222";

/// Where the indexer has the index.
#[allow(dead_code)] // each test file uses a part of this module
pub const CACHE: &str = "/home/vscode/.tehas-cache";

/// The options of the slice limits, by name.
#[allow(dead_code)] // each test file uses a part of this module
pub const LIMITS: &str =
    "--cgroup-parent --memory --memory-swap --memory-reservation --cpus --pids-limit --cpu-shares";

/// The stub docker. It answers `info` from `$T/docker.info`, `image inspect`
/// with $IMAGE_ID and by $IMAGE_RC, and `run` by $RUN_RC. $RUN_SIGNAL is sent
/// to tehas-cycle from inside the run, the parent of timeout. For a run it
/// also notes the commit of the tree mounted read-only, which is gone by the
/// time the command returns, and what the directory it is given read-write
/// held when the run started. Only a run that succeeded writes a database
/// into that directory, under the name of the tree, as the indexer would.
const DOCKER: &str = r#"if [ "$1" = info ]; then cat "$T/docker.info"; exit; fi
printf "%s\n" "$@" >"$T/calls/docker.args.$$"
[ "$1" = image ] && { printf "%s\n" "${IMAGE_ID:-@IMAGE_ID@}"; exit "${IMAGE_RC:-0}"; }
[ "$1" = rm ] && exit 0
written=""
for a in "$@"; do
  case "$a" in
    *:ro) git -C "${a%%:*}" rev-parse HEAD >"$T/calls/indexed.head"; echo "${a%%:*}" >"$T/calls/indexed.dir"
      git -C "${a%%:*}" rev-list --count HEAD >"$T/calls/indexed.count" ;;
    *:/home/vscode/.tehas-cache) written=${a%%:*} ;;
  esac
done
[ -z "$written" ] || find "$written" -mindepth 1 -maxdepth 1 -printf "%f\n" | LC_ALL=C sort | tr "\n" " " >"$T/calls/index.at.run"
[ -z "${RUN_SIGNAL:-}" ] || { kill "-$RUN_SIGNAL" "$(ps -o ppid= -p "$PPID")"; exec sleep 1; }
[ -z "${RUN_SLEEP:-}" ] || exec sleep "$RUN_SLEEP"
[ "${RUN_RC:-0}" = 0 ] || exit "$RUN_RC"
echo index >"$written/widget.db"
echo "${RUN_OUT:-{\"project\":\"p\",\"status\":\"indexed\",\"nodes\":5,\"edges\":7\}}""#;

/// The real timeout. Around the indexer, the one called with -k, the bound is
/// $TIMEOUT_BOUND when it is set.
const TIMEOUT: &str = r#"real=$(PATH=${PATH#"$T/bin:"} command -v timeout)
if [ -n "${TIMEOUT_BOUND:-}" ] && [ "$1" = -k ]; then exec "$real" -k 1 "$TIMEOUT_BOUND" "${@:4}"; fi
exec "$real" "$@""#;

/// The programs no stub may leave to the host's own.
const DENIED: &str = "docker gh systemctl curl ssh";

/// The checkout of this source: lib/ comes from it.
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// The console of this build: above the harness when the workspace built it,
/// built once here otherwise.
fn console_binary() -> PathBuf {
    fn above() -> Option<PathBuf> {
        let exe = std::env::current_exe().unwrap();
        exe.ancestors()
            .skip(1)
            .map(|dir| dir.join("console"))
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
            .args(["-p", "console"])
            .args((!cfg!(debug_assertions)).then_some("--release"))
            .output()
            .expect("cargo");
        assert!(
            out.status.success(),
            "the console did not build: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    });
    above().expect("the console of this build is above the test binary")
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

/// `id -u` or `id -g`.
fn id(flag: &str) -> String {
    let out = Command::new("id").arg(flag).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// The end of one run: its exit status and both streams as one.
#[derive(Debug)]
pub struct Run {
    pub rc: i32,
    pub out: String,
}

/// One host with a checkout of acme/widget.
#[allow(dead_code)] // each test file uses a part of this module
pub struct Host {
    t: TestDir,
    /// `~/tehas`.
    pub th: PathBuf,
    bash: String,
    console: PathBuf,
    /// The product repository.
    pub upstream: PathBuf,
    /// The host's checkout of it.
    pub checkout: PathBuf,
    /// The path a cycle works in.
    pub ws: PathBuf,
    /// The sandbox image of this account.
    pub image: String,
    env: RefCell<Vec<(String, String)>>,
    instance_on: RefCell<bool>,
}

#[allow(dead_code)] // each test file uses a part of this module
impl Host {
    pub fn new(name: &str) -> Host {
        let t = TestDir::new(&format!("index-{name}"));
        for d in ["bin", "calls", "deny", "tmp", "tree/libexec"] {
            fs::create_dir_all(t.join(d)).unwrap();
        }
        let th = t.join("home/tehas");
        fs::create_dir_all(&th).unwrap();
        std::os::unix::fs::symlink(root().join("lib"), t.join("tree/lib")).unwrap();
        let console = t.join("console-bin");
        link_exe(console_binary(), &console).unwrap();
        link_exe(
            env!("CARGO_BIN_EXE_tehas-cycle"),
            t.join("tree/libexec/tehas-cycle"),
        )
        .unwrap();
        let h = Host {
            upstream: t.join("git/acme/widget"),
            checkout: th.join("repos/widget"),
            ws: th.join("work/widget"),
            image: format!("tehas/pi-rukas:uid{}", id("-u")),
            th,
            bash: which("bash"),
            console,
            t,
            env: RefCell::new(vec![]),
            instance_on: RefCell::new(false),
        };
        // The command as a host has it: through `console run`, with the
        // directory of the program as libexec, beside tehas-cycle.
        write_exe(
            h.t.join("tree/libexec/index"),
            format!(
                "#!{}\nexec \"{}\" run \"$(cd \"${{0%/*}}\" && pwd)\" \"$EUID\" \"$BASH\" tehas index \"$@\"\n",
                h.bash,
                h.console.display()
            ),
        )
        .unwrap();
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
        // The CPU quota and the ticket ceiling follow the host's CPUs: the
        // same on every test machine.
        h.stub("nproc", "echo 16");
        h.stub("docker", &DOCKER.replace("@IMAGE_ID@", IMAGE_ID));
        h.stub("timeout", TIMEOUT);
        h.stub("systemctl", r#"cat "$T/slice.show""#);
        h.slice_good();
        fs::create_dir_all(&h.upstream).unwrap();
        h.git(&h.upstream, &["init", "-q", "-b", "main"]);
        h.commit("first");
        let out = h
            .git_cmd(&h.t.join("git"), &["clone", "-q"])
            .arg(&h.upstream)
            .arg(&h.checkout)
            .output()
            .unwrap();
        assert!(out.status.success(), "git clone: {out:?}");
        h.minimal(&[]);
        h
    }

    // --- Paths ---------------------------------------------------------------

    /// `$T` and a path under it.
    pub fn t(&self, rel: &str) -> PathBuf {
        self.t.join(rel)
    }

    fn home(&self) -> Home {
        Home::new(&self.th)
    }

    /// `~/tehas` and a path under it.
    pub fn th(&self, rel: &str) -> PathBuf {
        self.th.join(rel)
    }

    /// The record of the last run that succeeded.
    pub fn marker(&self) -> PathBuf {
        self.home().state(State::CodeIndexState)
    }

    /// The tip at which the indexer last failed.
    pub fn failed(&self) -> PathBuf {
        self.home().state(State::CodeIndexFailed)
    }

    /// Where the host keeps the index.
    pub fn indexdir(&self) -> PathBuf {
        self.home().state(State::CodeIndex)
    }

    /// `tmp/` of the home, where a rebuild sets aside what it moves.
    pub fn tmp(&self) -> PathBuf {
        self.th.join("tmp")
    }

    pub fn dispatch_lock(&self) -> PathBuf {
        self.home().state(State::DispatchLock)
    }

    pub fn index_lock(&self) -> PathBuf {
        self.home().state(State::CodeIndexLock)
    }

    pub fn cycle_lock(&self, n: u32) -> PathBuf {
        self.home().cycles_dir().join(format!("{n}.lock"))
    }

    // --- Stubs and settings --------------------------------------------------

    /// A stub on the PATH of a run that records its arguments in
    /// `calls/<name>`, then runs `body`.
    fn stub(&self, name: &str, body: &str) {
        let text = format!(
            "#!{}\nprintf '%s\\n' \"$*\" >>\"$T/calls/{name}\"\n{body}\n",
            self.bash
        );
        write_exe(self.t.join("bin").join(name), text).unwrap();
    }

    /// A host with the root step done: the slice loaded from its file with
    /// 8 GiB, so a container's ceiling is 2048m, Docker on the systemd driver
    /// and cgroup v2, and nothing in the slice.
    pub fn slice_good(&self) {
        self.slice_show("FragmentPath=/etc/systemd/system/tehas.slice\nActiveState=active\nMemoryMax=8589934592\n");
        self.docker_info("systemd 2\n");
        let cg = self.t.join("cgroup");
        let _ = fs::remove_dir_all(&cg);
        write(
            cg.join("cgroup.controllers"),
            "cpuset cpu io memory hugetlb pids rdma misc\n",
        );
        write(
            cg.join("tehas.slice/memory.pressure"),
            "some avg10=0.00 avg60=0.00 avg300=0.00 total=0\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=0\n",
        );
    }

    /// What systemctl says of the slice.
    pub fn slice_show(&self, text: &str) {
        write(self.t.join("slice.show"), text);
    }

    /// What docker info says of its cgroup driver and version.
    pub fn docker_info(&self, text: &str) {
        write(self.t.join("docker.info"), text);
    }

    /// instance.env, as given, and the factory's login and tokens unless the
    /// login is kept already (t_instance).
    pub fn instance(&self, lines: &[&str]) {
        let dir = self.t.join("instance");
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        let mut text = lines.join("\n");
        if !text.is_empty() {
            text.push('\n');
        }
        fs::write(dir.join("instance.env"), text).unwrap();
        *self.instance_on.borrow_mut() = true;
        if !self.home().state(State::BotLogin).exists() {
            let until = tehas_core::time::now() + 86400;
            for (dir, token) in [
                (TokenDir::App, "app"),
                (TokenDir::Cycle, "cycle"),
                (TokenDir::Session, "session"),
            ] {
                let d = self.home().token_dir(dir);
                write(d.join("token"), &format!("ghs_{token}\n"));
                write(d.join("token.expires"), &format!("{until}\n"));
            }
            write(self.home().state(State::BotLogin), "tehas-bot[bot]\n");
        }
    }

    /// The minimal instance and these settings after it.
    pub fn minimal(&self, more: &[&str]) {
        let mut lines = MINIMAL.to_vec();
        lines.extend_from_slice(more);
        self.instance(&lines);
    }

    /// The variables of every later run beside the cgroup root and a wait of
    /// one second for the dispatcher's lock, in place of those before
    /// (`T_ENV=(... "$root" "$wait")`).
    pub fn set_env(&self, pairs: &[&str]) {
        *self.env.borrow_mut() = pairs
            .iter()
            .map(|p| {
                let (k, v) = p.split_once('=').expect("K=V");
                (k.to_string(), v.to_string())
            })
            .collect();
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
            .env("TEHAS_HOME", &self.th)
            .env("T", self.t.path())
            .env("T_ROOT", root())
            .env("CARGO_TARGET_DIR", self.t.join("target"))
            .env("TEHAS_SETTINGS_CMD", &self.console)
            .env("TEHAS_TEST_STORE_ANSWERS", self.t.join("store-answers"))
            .env("TMPDIR", self.t.join("tmp"))
            .env("TEHAS_CGROUP_ROOT", self.t.join("cgroup"))
            .env("TEHAS_INDEX_LOCK_WAIT", "1");
        if *self.instance_on.borrow() {
            c.env("TEHAS_INSTANCE_DIR", self.t.join("instance"));
        }
        c.envs(self.env.borrow().iter().map(|(k, v)| (k, v)));
        // The slice is the console's, which a checkout's libexec does not hold.
        c.env("TEHAS_CONSOLE_CMD", &self.console);
        c
    }

    /// `tehas index` with these words.
    pub fn index(&self, words: &[&str]) -> Run {
        let mut c = self.command(&self.t.join("tree/libexec/index"));
        c.args(words);
        run(c)
    }

    /// A run that succeeds, its calls forgotten: the host has an index at the
    /// tip, recorded with the image the stub answers by default.
    pub fn indexed(&self) {
        let r = self.index(&[]);
        rc(&r, 0);
        self.reset();
    }

    /// The calls are forgotten (`reset` of the shell).
    pub fn reset(&self) {
        let _ = fs::remove_dir_all(self.t.join("calls"));
        fs::create_dir_all(self.t.join("calls")).unwrap();
    }

    /// What a stub was called with, one call a line.
    pub fn calls(&self, name: &str) -> String {
        fs::read_to_string(self.t.join("calls").join(name)).unwrap_or_default()
    }

    pub fn docker(&self) -> String {
        self.calls("docker")
    }

    /// The files of the docker calls that hold the word `index_repository`,
    /// by name.
    fn run_files(&self) -> Vec<PathBuf> {
        let mut found: Vec<PathBuf> = fs::read_dir(self.t.join("calls"))
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("docker.args.")
            })
            .collect();
        found.sort();
        found
    }

    /// How many docker calls were made with their words recorded.
    pub fn docker_args_files(&self) -> usize {
        self.run_files().len()
    }

    /// Every word of every recorded docker call, one a line.
    pub fn all_args(&self) -> String {
        self.run_files()
            .iter()
            .map(|p| fs::read_to_string(p).unwrap())
            .collect()
    }

    fn indexer_files(&self) -> Vec<PathBuf> {
        self.run_files()
            .into_iter()
            .filter(|p| {
                fs::read_to_string(p)
                    .unwrap()
                    .lines()
                    .any(|l| l == "index_repository")
            })
            .collect()
    }

    /// How many times the indexer ran.
    pub fn runs(&self) -> usize {
        self.indexer_files().len()
    }

    /// The words of the first run of the indexer.
    pub fn run_args(&self) -> Vec<String> {
        self.indexer_files()
            .first()
            .map(|p| fs::read_to_string(p).unwrap())
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }

    /// Every word that follows `flag` in the indexer's run, one a line.
    pub fn after(&self, flag: &str) -> Vec<String> {
        self.run_args()
            .windows(2)
            .filter(|p| p[0] == flag)
            .map(|p| p[1].clone())
            .collect()
    }

    /// What the stub noted of a run: `indexed.head`, `indexed.dir`,
    /// `indexed.count`, `index.at.run`; its last newline gone.
    pub fn noted(&self, name: &str) -> String {
        self.calls(name).trim_end_matches('\n').to_string()
    }

    // --- The product ---------------------------------------------------------

    fn git_cmd(&self, dir: &Path, args: &[&str]) -> Command {
        let mut c = Command::new("git");
        c.arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .stdin(Stdio::null());
        c
    }

    /// git in `dir`, which must succeed; what it printed, its last newline
    /// gone.
    pub fn git(&self, dir: &Path, args: &[&str]) -> String {
        let out = self.git_cmd(dir, args).output().unwrap();
        assert!(
            out.status.success(),
            "git {args:?} in {}: {}",
            dir.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout)
            .trim_end_matches('\n')
            .to_string()
    }

    /// An empty commit on the product's main.
    pub fn commit(&self, msg: &str) {
        self.git(
            &self.upstream,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                msg,
            ],
        );
    }

    /// The checkout fetches.
    pub fn fetch(&self) {
        self.git(&self.checkout, &["fetch", "-q"]);
    }

    /// A commit on main that the checkout has fetched.
    pub fn moved(&self, msg: &str) {
        self.commit(msg);
        self.fetch();
    }

    /// The tip of the product's main.
    pub fn tip(&self) -> String {
        self.git(&self.upstream, &["rev-parse", "main"])
    }

    // --- What the host keeps -------------------------------------------------

    /// The record, as it is on disk; "" for none.
    pub fn record(&self) -> String {
        fs::read_to_string(self.marker()).unwrap_or_default()
    }

    /// A field of the record, as jq -r prints it.
    pub fn field(&self, name: &str) -> String {
        let v: serde_json::Value = serde_json::from_str(&self.record()).expect("a record");
        match &v[name] {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        }
    }

    /// The record with its indexer_image set to `image`, or taken out for
    /// None.
    pub fn record_image(&self, image: Option<&str>) {
        let mut v: serde_json::Value = serde_json::from_str(&self.record()).expect("a record");
        let o = v.as_object_mut().unwrap();
        match image {
            Some(i) => {
                o.insert("indexer_image".into(), i.into());
            }
            None => {
                o.remove("indexer_image");
            }
        }
        fs::write(
            self.marker(),
            format!("{}\n", serde_json::to_string_pretty(&v).unwrap()),
        )
        .unwrap();
    }

    /// The tip the indexer last failed at, its newline gone; "" for none.
    pub fn failed_at(&self) -> String {
        fs::read_to_string(self.failed())
            .unwrap_or_default()
            .trim_end_matches('\n')
            .to_string()
    }

    /// The index directory, emptied and made again with these files, each
    /// holding the word given; a name ending in `/` is a directory.
    pub fn index_holds(&self, files: &[(&str, &str)]) {
        let dir = self.indexdir();
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for (name, text) in files {
            if let Some(d) = name.strip_suffix('/') {
                fs::create_dir_all(dir.join(d)).unwrap();
            } else {
                write(dir.join(name), &format!("{text}\n"));
            }
        }
    }

    /// The names in the index directory, sorted, each followed by a space
    /// (`find -printf '%f\n' | sort | tr '\n' ' '`).
    pub fn index_names(&self) -> String {
        names(&self.indexdir())
    }

    /// What a file of the index directory holds, its newline gone.
    pub fn index_file(&self, name: &str) -> String {
        fs::read_to_string(self.indexdir().join(name))
            .unwrap_or_default()
            .trim_end_matches('\n')
            .to_string()
    }

    /// How many paths are below `tmp/` of the home.
    pub fn tmp_count(&self) -> usize {
        below(&self.tmp())
    }

    /// A directory that a killed run left set aside under `tmp/`, holding
    /// one file.
    pub fn orphan(&self, file: &str, text: &str) {
        fs::create_dir_all(self.tmp()).unwrap();
        let out = Command::new("mktemp")
            .arg("-d")
            .arg(self.tmp().join("index.stale.XXXXXX"))
            .output()
            .unwrap();
        assert!(out.status.success());
        let dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
        write(Path::new(&dir).join(file), &format!("{text}\n"));
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

/// A program run to its end, its two streams as one.
#[allow(dead_code)] // each test file uses a part of this module
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

/// The names in `dir`, sorted as C sorts them, each followed by a space.
#[allow(dead_code)] // each test file uses a part of this module
pub fn names(dir: &Path) -> String {
    let mut all: Vec<String> = fs::read_dir(dir)
        .map(|d| {
            d.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    all.sort();
    all.iter().map(|n| format!("{n} ")).collect()
}

/// How many paths are below `dir` (`find <dir> -mindepth 1 | wc -l`), 0 when
/// it is not there.
#[allow(dead_code)] // each test file uses a part of this module
pub fn below(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| {
            let p = e.path();
            1 + if p.is_dir() && !p.is_symlink() {
                below(&p)
            } else {
                0
            }
        })
        .sum()
}

/// A file with these bytes, its directory made.
#[allow(dead_code)] // each test file uses a part of this module
pub fn write(p: impl AsRef<Path>, text: &str) {
    let p = p.as_ref();
    if let Some(d) = p.parent() {
        fs::create_dir_all(d).unwrap();
    }
    fs::write(p, text).unwrap();
}

/// The file dated `secs` ago.
#[allow(dead_code)] // each test file uses a part of this module
pub fn age(p: impl AsRef<Path>, secs: u64) {
    let when = SystemTime::now() - Duration::from_secs(secs);
    File::options()
        .write(true)
        .open(p)
        .unwrap()
        .set_times(FileTimes::new().set_accessed(when).set_modified(when))
        .unwrap();
}

/// A lock held by this process until the value drops (`exec 9>; flock -n 9`).
#[allow(dead_code)] // each test file uses a part of this module
pub fn hold(p: impl AsRef<Path>) -> File {
    let p = p.as_ref();
    if let Some(d) = p.parent() {
        fs::create_dir_all(d).unwrap();
    }
    let f = File::create(p).unwrap();
    f.try_lock().expect("the lock is free");
    f
}

/// The words a strict reader of docker's options does not know: every option
/// up to the image that is neither one that takes a value nor one that takes
/// none, as the shell's unknown_options reads them.
#[allow(dead_code)] // each test file uses a part of this module
pub fn unknown_options(args: &[String], image: &str, valued: &str, plain: &str) -> Vec<String> {
    let valued: Vec<&str> = valued.split_whitespace().collect();
    let plain: Vec<&str> = plain.split_whitespace().collect();
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
        if valued.contains(&a.as_str()) {
            skip = true;
            continue;
        }
        if plain.contains(&a.as_str()) {
            continue;
        }
        out.push(a.clone());
    }
    out
}

/// `want` is in `got`.
#[track_caller]
#[allow(dead_code)] // each test file uses a part of this module
pub fn has(got: &str, want: &str) {
    assert!(got.contains(want), "{want:?} is not in:\n{got}");
}

/// `want` is not in `got`.
#[track_caller]
#[allow(dead_code)] // each test file uses a part of this module
pub fn lacks(got: &str, want: &str) {
    assert!(!got.contains(want), "{want:?} should not be in:\n{got}");
}

/// The exit status of a run, with its output when it is another.
#[track_caller]
#[allow(dead_code)] // each test file uses a part of this module
pub fn rc(r: &Run, want: i32) {
    assert_eq!(r.rc, want, "exit {} and not {want}:\n{}", r.rc, r.out);
}

/// The user and group of this process, as a container is run with them.
#[allow(dead_code)] // each test file uses a part of this module
pub fn user() -> String {
    format!("{}:{}", id("-u"), id("-g"))
}

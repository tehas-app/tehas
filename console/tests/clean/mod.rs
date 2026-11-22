//! What the tests of `tehasbot clean` share: one host, the command as a host
//! has it (bin/tehasbot above this build's console), the real store, and stub
//! docker and df that answer from fixture files under `$T/fx`. Every stub
//! records its calls in `$T/calls/<name>`.
//!
//! A host starts as reset() leaves it: the minimal instance, a fresh store,
//! the pins of a host that follows one pi-rukas commit, 40 GiB free on every
//! file system and docker keeping its images on /srv/docker.

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

/// The stub docker. Each knob is a variable of the run: DOCKER_RC fails the
/// prune of the build cache, IMAGE_LS_RC the listing of the sandbox image's
/// tags, PS_RC the listing of containers, INSPECT_RC what those containers
/// were made from, BASE_LS_RC the listing of the base's tags, HELD_LS_RC the
/// listing of the sandbox image's tags step 5 asks, PRUNE_RC every removal
/// (124 is what timeout(1) leaves), RMI_FAILS the removal of one name. The
/// answers are files of `$T/fx`: `images` and `base-images` ("<repository>
/// <tag> <id>" or "<tag> <id>" per line, as listed), `containers` ("<id>
/// <image>" per line; no file is no container), `size-<id>` and
/// `created-<id>` of one image, `docker-dir` where docker keeps its images
/// and `docker-down` for a docker that names none. `ps` and `inspect` of a
/// host with no container answer nothing and succeed, as docker does.
const DOCKER: &str = r#"case "$1" in
  info) case "$*" in
      *DockerRootDir*) [ -e "$T/fx/docker-down" ] || cat "$T/fx/docker-dir" 2>/dev/null ;;
    esac ;;
  builder) [ "${DOCKER_RC:-0}" = 0 ] || { echo "docker: cannot connect" >&2; exit "${DOCKER_RC:-1}"; }
    echo "Total: 4.2GB" ;;
  image)
    case "$2" in
    ls)
      [ -z "${IMAGE_LS_RC:-}" ] || exit "$IMAGE_LS_RC"
      names=()
      shift 2
      while [ "$#" -gt 0 ]; do
        case "$1" in
        --format) fmt=$2; shift ;;
        -*) ;;
        *) names+=("$1") ;;
        esac
        shift
      done
      if [ "${#names[@]}" -gt 1 ]; then
        echo "docker: docker image ls requires at most 1 argument" >&2
        exit 1
      fi
      if [ "${names[0]:-}" = "${BASE_NAME:-none}" ]; then
        [ -z "${BASE_LS_RC:-}" ] || exit "$BASE_LS_RC"
        cat "$T/fx/base-images" 2>/dev/null
      else
        if [ -n "${HELD_LS_RC:-}" ] && [[ ${fmt:-} == *Repository* ]]; then exit "$HELD_LS_RC"; fi
        cat "$T/fx/images" 2>/dev/null
      fi
      exit 0 ;;
    inspect)
      [ "$3 $4" = "--format {{.Id}} {{.Size}} {{.Created}}" ] || exit 1
      shift 4
      rc=0
      for id in "$@"; do
        if [ -s "$T/fx/size-$id" ] || [ -s "$T/fx/created-$id" ]; then
          echo "$id $(cat "$T/fx/size-$id" 2>/dev/null || echo 0) $(cat "$T/fx/created-$id" 2>/dev/null)"
        else
          echo "Error: No such image: $id" >&2
          rc=1
        fi
      done
      exit "$rc" ;;
    prune) echo "docker: no image prune" >&2; exit 1 ;;
    *) exit 1 ;;
    esac ;;
  rmi) [ "${2:-}" = "${RMI_FAILS:-}" ] && exit 1
    [ "${PRUNE_RC:-0}" = 0 ] || { echo "docker: prune refused" >&2; exit "$PRUNE_RC"; }
    echo "Untagged: $2" ;;
  ps) [ -z "${PS_RC:-}" ] || exit "$PS_RC"
    if [ -s "$T/fx/containers" ]; then cut -d" " -f1 "$T/fx/containers"; fi ;;
  inspect) [ -z "${INSPECT_RC:-}" ] || exit "$INSPECT_RC"
    [ ! -s "$T/fx/containers" ] && exit 0
    cut -d" " -f2 "$T/fx/containers" ;;
  *) exit 1 ;;
esac"#;

/// The stub df: `$T/fx/free` kilobytes on every file system (40 GiB with no
/// file), `$T/fx/free-docker` on the one docker names. `df-fails` fails
/// every reading, `df-fails-of` the one of the path it names; an answer that
/// is no number is no reading.
const DF: &str = r#"[ ! -e "$T/fx/df-fails" ] || exit 1
[ ! -e "$T/fx/df-fails-of" ] || [ "${2%/}" = "$(cat "$T/fx/df-fails-of")" ] || exit 1
docker_dir=$(cat "$T/fx/docker-dir" 2>/dev/null)
if [ -e "$T/fx/docker-down" ]; then docker_dir=""; fi
answer=$(cat "$T/fx/free" 2>/dev/null || echo 41943040)
if [ -n "$docker_dir" ] && [ "${2%/}" = "$docker_dir" ]; then
  answer=$(cat "$T/fx/free-docker" 2>/dev/null)
fi
[[ $answer =~ ^(0|[1-9][0-9]{0,17})$ ]] || exit 1
echo "Filesystem 1024-blocks Used Available Capacity Mounted on"
echo "/dev/x 60000000 1 $answer 1% /""#;

/// The programs no stub may leave to the host's own.
const DENIED: &str = "docker gh systemctl curl ssh";

/// The values of a minimal valid instance (T_MINIMAL).
#[allow(dead_code)] // each test file uses a part of this module
pub const MINIMAL: &[&str] = &[
    "TEHAS_GITHUB_APP_ID=1",
    "TEHAS_REPO=acme/widget",
    "TEHAS_CONTROL_ISSUE=1",
    "TEHAS_ALLOWED_LABELERS=alice",
    "PI_ENSEMBLE_SUBAGENT_MODEL=test-model",
    "PI_ENSEMBLE_SUBAGENT_PROVIDER=test-provider",
];

/// The sandbox image, without its tag.
#[allow(dead_code)] // each test file uses a part of this module
pub const IMAGE: &str = "tehas/pi-rukas";
/// The base's repository, as the pins name it.
#[allow(dead_code)] // each test file uses a part of this module
pub const BASE_REPO: &str = "ghcr.io/up-owner/pi-rukas";
/// The base of the commit the host follows.
#[allow(dead_code)] // each test file uses a part of this module
pub const BASE_IMAGE: &str = "ghcr.io/up-owner/pi-rukas:sha-aaaaaaaaaaaa";
/// The directory the fixture's worktrees are made in; the cleaner names none.
#[allow(dead_code)] // each test file uses a part of this module
pub const WORKTREES: &str = ".worktrees";
/// The hours of the default age.
#[allow(dead_code)] // each test file uses a part of this module
pub const AFTER: u64 = 24;

/// The pins of reset(): one pi-rukas commit, forty a.
fn pins() -> String {
    format!(
        "PI_RUKAS_REPO=https://github.com/Up-Owner/pi-rukas.git\nPI_RUKAS_FOLLOW=commit:{}\n",
        "a".repeat(40)
    )
}

/// A full image id of 64 `c`.
#[allow(dead_code)] // each test file uses a part of this module
pub fn id_of(c: char) -> String {
    format!("sha256:{}", c.to_string().repeat(64))
}

/// The ids of the base's three pulls: the two of earlier releases, and the
/// commit followed now.
#[allow(dead_code)] // each test file uses a part of this module
pub fn sha_old1() -> String {
    id_of('b')
}
#[allow(dead_code)] // each test file uses a part of this module
pub fn sha_old2() -> String {
    id_of('c')
}
#[allow(dead_code)] // each test file uses a part of this module
pub fn sha_now() -> String {
    id_of('d')
}

/// A time `secs` ago in the shape docker answers a creation with.
#[allow(dead_code)] // each test file uses a part of this module
pub fn docker_time(secs: u64) -> String {
    let t = tehas_core::time::now() - secs as i64;
    tehas_core::time::iso(t).replace('Z', ".000000000Z")
}

/// Three days ago, as docker dates an image.
#[allow(dead_code)] // each test file uses a part of this module
pub fn old_time() -> String {
    docker_time(3 * 86400)
}

/// Now, as docker dates an image.
#[allow(dead_code)] // each test file uses a part of this module
pub fn young_time() -> String {
    docker_time(0)
}

/// The free kilobytes of that many gibibytes, as df prints them.
#[allow(dead_code)] // each test file uses a part of this module
pub fn kb(gib: u64) -> String {
    (gib * 1_048_576).to_string()
}

/// The checkout of this source.
#[allow(dead_code)] // each test file uses a part of this module
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

fn which(name: &str) -> String {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .map(|d| Path::new(d).join(name))
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("/usr/bin/{name}"))
}

/// The end of one run: its exit status and both streams as one.
#[derive(Debug)]
pub struct Run {
    pub rc: i32,
    pub out: String,
}

/// One host, as reset() leaves it.
pub struct Host {
    t: TestDir,
    th: PathBuf,
    bash: String,
    store: PathBuf,
    instance: RefCell<bool>,
    env: RefCell<Vec<(String, String)>>,
}

#[allow(dead_code)] // each test file uses a part of this module
impl Host {
    pub fn new(name: &str) -> Host {
        let t = TestDir::new(&format!("clean-{name}"));
        for d in [
            "bin",
            "calls",
            "deny",
            "tmp",
            "fx",
            "tree/bin",
            "tree/libexec",
        ] {
            fs::create_dir_all(t.join(d)).unwrap();
        }
        let store = t.join("store-bin");
        link_exe(store_binary(), &store).unwrap();
        std::os::unix::fs::symlink(root().join("lib"), t.join("tree/lib")).unwrap();
        std::os::unix::fs::symlink(root().join("bin/tehasbot"), t.join("tree/bin/tehasbot"))
            .unwrap();
        let h = Host {
            th: t.join("home/tehas"),
            bash: which("bash"),
            store,
            t,
            instance: RefCell::new(false),
            env: RefCell::new(vec![]),
        };
        h.put_console();
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
        h.stub("docker", DOCKER);
        h.stub("df", DF);
        h.reset();
        h
    }

    /// This build's console beside the command.
    pub fn put_console(&self) {
        link_exe(env!("CARGO_BIN_EXE_console"), self.console()).unwrap();
    }

    /// Where the command looks for its console.
    pub fn console(&self) -> PathBuf {
        self.t.join("tree/libexec/console")
    }

    /// The host of a case before it says anything.
    pub fn reset(&self) {
        let _ = fs::remove_dir_all(self.t.join("calls"));
        let _ = fs::remove_dir_all(self.t.join("fx"));
        let _ = fs::remove_dir_all(&self.th);
        for d in [
            self.home().cycles_dir(),
            self.th.join("logs"),
            self.tmp("/dl").into(),
            self.tmp("/node-compile-cache").into(),
            self.tmp("/other").into(),
            self.wd().into(),
            self.t.join("fx"),
            self.t.join("calls"),
            self.th.join("lib"),
        ] {
            fs::create_dir_all(d).unwrap();
        }
        fs::write(self.th.join("versions.env"), pins()).unwrap();
        self.minimal(&[]);
        self.store_home();
        self.set_env(&[&format!("BASE_NAME={BASE_REPO}")]);
        self.df_reset();
        self.fx("docker-dir", "/srv/docker\n");
    }

    fn home(&self) -> Home {
        Home::new(&self.th)
    }

    /// The real store in the home, with a fresh database (t_store_home).
    fn store_home(&self) {
        let libexec = self.th.join("libexec");
        fs::create_dir_all(&libexec).unwrap();
        link_exe(&self.store, libexec.join("store")).unwrap();
        fs::create_dir_all(self.home().state_dir()).unwrap();
        let out = Command::new(&self.store)
            .env("TEHAS_HOME", &self.th)
            .args([
                "init",
                "--version",
                "v0.0.0",
                "--commit",
                "0123456789abcdef0123456789abcdef01234567",
                "--instance",
                "instance",
                "--repo",
                "acme/widget",
            ])
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "the store's init failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// The store is not installed.
    pub fn no_store(&self) {
        fs::remove_file(self.th.join("libexec/store")).unwrap();
    }

    /// `$T`.
    pub fn t(&self, rel: &str) -> String {
        format!("{}{rel}", self.t.display())
    }

    /// `~/tehas` and a path under it.
    pub fn th(&self, rel: &str) -> String {
        format!("{}{rel}", self.th.display())
    }

    /// The work dir of the repository, `work/widget`, and what follows it:
    /// `wd("-7/target")` is the build output of issue 7.
    pub fn wd(&self) -> String {
        self.th("/work/widget")
    }

    pub fn ws(&self, rest: &str) -> String {
        format!("{}{rest}", self.wd())
    }

    /// `tmp/` of the home.
    pub fn tmp(&self, rel: &str) -> String {
        self.th(&format!("/tmp{rel}"))
    }

    /// `logs/` of the home.
    pub fn logs(&self, rel: &str) -> String {
        self.th(&format!("/logs{rel}"))
    }

    /// The lock of issue `n`'s cycle.
    pub fn cycle_lock(&self, n: u32) -> PathBuf {
        self.home().cycles_dir().join(format!("{n}.lock"))
    }

    /// The lock of the cleaner itself.
    pub fn clean_lock(&self) -> PathBuf {
        self.home().state(State::CleanLock)
    }

    /// The lock `tehas dev` holds for a clone's session.
    pub fn dev_lock(&self, name: &str) -> PathBuf {
        self.home().dev_lock(name)
    }

    /// Where the store keeps its backups.
    pub fn backups(&self) -> PathBuf {
        self.home().state(State::Backups)
    }

    /// The installed versions.env.
    pub fn versions(&self, text: &str) {
        fs::write(self.th.join("versions.env"), text).unwrap();
    }

    pub fn no_versions(&self) {
        fs::remove_file(self.th.join("versions.env")).unwrap();
    }

    /// `tehasbot clean` with these words.
    pub fn clean(&self, words: &[&str]) -> Run {
        let mut c = Command::new(self.t.join("tree/bin/tehasbot"));
        c.arg("clean").args(words);
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
        if *self.instance.borrow() {
            c.env("TEHAS_INSTANCE_DIR", self.t.join("instance"));
        }
        c.envs(self.env.borrow().iter().map(|(k, v)| (k, v)));
        run(c)
    }

    /// A stub program on the PATH of a run that records its arguments, then
    /// runs `body`.
    fn stub(&self, name: &str, body: &str) {
        let text = format!(
            "#!{}\nprintf '%s\\n' \"$*\" >>\"$T/calls/{name}\"\n{body}\n",
            self.bash
        );
        write_exe(self.t.join("bin").join(name), text).unwrap();
    }

    /// What the stub docker was asked, one call a line.
    pub fn docker(&self) -> String {
        fs::read_to_string(self.t.join("calls/docker")).unwrap_or_default()
    }

    /// The calls to docker that start with `prefix`.
    pub fn docker_count(&self, prefix: &str) -> usize {
        self.docker()
            .lines()
            .filter(|l| l.starts_with(prefix))
            .count()
    }

    /// The variables of every later run until reset(), in place of those
    /// before (`T_ENV=(K=V ...)`).
    pub fn set_env(&self, pairs: &[&str]) {
        *self.env.borrow_mut() = pairs
            .iter()
            .map(|p| {
                let (k, v) = p.split_once('=').expect("K=V");
                (k.to_string(), v.to_string())
            })
            .collect();
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
        if !self.home().state(State::BotLogin).exists() {
            self.app_world();
        }
    }

    /// The minimal instance, with more settings after it.
    pub fn minimal(&self, more: &[&str]) {
        let mut lines = MINIMAL.to_vec();
        lines.extend_from_slice(more);
        self.instance(&lines);
    }

    /// No instance directory, and none named to a run.
    pub fn no_instance(&self) {
        *self.instance.borrow_mut() = false;
        fs::remove_dir_all(self.t.join("instance")).unwrap();
    }

    /// The factory's login and tokens with hours left (t_app_world).
    fn app_world(&self) {
        let until = tehas_core::time::now() + 86400;
        for (dir, token) in [
            (TokenDir::App, "app"),
            (TokenDir::Cycle, "cycle"),
            (TokenDir::Session, "session"),
        ] {
            let d = self.home().token_dir(dir);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("token"), format!("ghs_{token}\n")).unwrap();
            fs::write(d.join("token.expires"), format!("{until}\n")).unwrap();
        }
        fs::write(self.home().state(State::BotLogin), "tehas-bot[bot]\n").unwrap();
    }

    /// A fixture file under `$T/fx`.
    pub fn fx(&self, name: &str, text: &str) {
        write(self.t.join("fx").join(name), text);
    }

    /// One more line at the end of a fixture file.
    pub fn fx_line(&self, name: &str, line: &str) {
        let p = self.t.join("fx").join(name);
        let mut text = fs::read_to_string(&p).unwrap_or_default();
        text.push_str(line);
        text.push('\n');
        fs::write(p, text).unwrap();
    }

    pub fn rm_fx(&self, name: &str) {
        let _ = fs::remove_file(self.t.join("fx").join(name));
    }

    // The free disk.

    /// No figure of its own: 40 GiB everywhere, and docker naming nothing.
    pub fn df_reset(&self) {
        for f in [
            "free",
            "free-docker",
            "df-fails",
            "df-fails-of",
            "docker-dir",
            "docker-down",
        ] {
            self.rm_fx(f);
        }
    }

    /// The same free figure on every file system the cleaner asks.
    pub fn df_free(&self, gib: u64) {
        self.df_free_of(gib, gib);
    }

    /// `~/tehas` and docker's /srv/docker apart.
    pub fn df_free_of(&self, home: u64, docker: u64) {
        self.df_reset();
        self.fx("docker-dir", "/srv/docker\n");
        self.fx("free", &format!("{}\n", kb(home)));
        self.fx("free-docker", &format!("{}\n", kb(docker)));
    }

    // The images.

    /// Six dated builds of the sandbox image, oldest first, each its own
    /// image `sha256:<n>`.
    pub fn six_tags(&self) {
        let lines: String = (1..=6)
            .map(|n| format!("2610{:02}-1000 sha256:{n}\n", 8 + n))
            .collect();
        self.fx("images", &lines);
    }

    /// Every one of the six built three days ago.
    pub fn all_old(&self) {
        for n in 1..=6 {
            self.created(&format!("sha256:{n}"), &old_time());
        }
    }

    pub fn no_containers(&self) {
        self.rm_fx("containers");
    }

    /// The containers of the host, "<id> <image>" each.
    pub fn containers(&self, lines: &[&str]) {
        let mut text = lines.join("\n");
        text.push('\n');
        self.fx("containers", &text);
    }

    /// Six old builds held by nothing.
    pub fn old_tags(&self) {
        self.six_tags();
        self.all_old();
        self.no_containers();
    }

    /// When docker says image `id` was made.
    pub fn created(&self, id: &str, when: &str) {
        self.fx(&format!("created-{id}"), &format!("{when}\n"));
    }

    /// The bytes docker says image `id` holds.
    pub fn size(&self, id: &str, bytes: u64) {
        self.fx(&format!("size-{id}"), &format!("{bytes}\n"));
    }

    /// The three sha- tags of the base, oldest first.
    pub fn base_tags(&self) {
        self.fx(
            "base-images",
            &format!(
                "{BASE_REPO} sha-bbbbbbbbbbbb {}\n{BASE_REPO} sha-cccccccccccc {}\n{BASE_REPO} sha-aaaaaaaaaaaa {}\n",
                sha_old1(),
                sha_old2(),
                sha_now()
            ),
        );
    }

    /// The bases pulled three days ago.
    pub fn old_base(&self, ids: &[&str]) {
        for id in ids {
            self.created(id, &old_time());
        }
    }

    /// The image a kept release ran with, as the sandbox listing has it.
    pub fn kept_release(&self, version: &str, id: &str) {
        self.fx_line("images", &format!("{IMAGE} release-v{version} {id}"));
    }

    // The trees.

    /// Issue `n`'s kept workspace, three days old (`workspace` of the shell).
    pub fn workspace(&self, n: u32) {
        self.tree(n, false, true, &[]);
    }

    /// As `workspace`, with these paths made young.
    pub fn workspace_young(&self, n: u32, young: &[&str]) {
        self.tree(n, false, true, young);
    }

    /// The tree a retry moved aside, three days old.
    pub fn retry_tree(&self, n: u32) {
        self.tree(n, true, true, &[]);
    }

    /// A workspace a person would read and nothing a toolchain made.
    pub fn bare_workspace(&self, n: u32) {
        self.tree(n, false, false, &[]);
    }

    fn tree(&self, n: u32, retry: bool, output: bool, young: &[&str]) {
        let suffix = if retry { ".before-retry" } else { "" };
        let d = self.ws(&format!("-{n}{suffix}"));
        let s = self.ws(&format!("-{n}.state"));
        let dirs: &[&str] = if output {
            &[
                "/target/sub",
                "/node_modules/pkg",
                "/src",
                "/sub/target",
                "/nested/target",
                "/.git",
                "/keep",
            ]
        } else {
            &["/src", "/.git", "/keep"]
        };
        for sub in dirs {
            mkdir(format!("{d}{sub}"));
        }
        if !retry {
            write(format!("{s}/memory/learned"), "m\n");
            write(format!("{s}/sessions/one.jsonl"), "p\n");
            if output {
                write(format!("{s}/cache/index.db"), "idx\n");
            }
        }
        write(format!("{d}/src/main.sh"), "keep\n");
        write(format!("{d}/uncommitted"), "keep\n");
        write(format!("{d}/.git/HEAD"), "obj\n");
        write(format!("{d}/keep/notes"), "keep\n");
        if output {
            for f in [
                "/target/sub/app",
                "/node_modules/pkg/index.js",
                "/sub/target/app",
                "/nested/target/app",
            ] {
                write(format!("{d}{f}"), "big\n");
            }
        }
        let lock = self.cycle_lock(n);
        write(&lock, "");
        let mut aged = vec![d.clone(), lock.display().to_string()];
        if Path::new(&s).exists() {
            aged.push(s.clone());
        }
        if Path::new(&format!("{s}/cache")).exists() {
            aged.push(format!("{s}/cache"));
        }
        if output {
            for sub in [
                "/target",
                "/node_modules",
                "/sub",
                "/sub/target",
                "/nested",
                "/nested/target",
            ] {
                aged.push(format!("{d}{sub}"));
            }
        }
        age(&aged);
        for p in young {
            if !Path::new(p).exists() {
                mkdir(Path::new(p).parent().unwrap());
            }
            touch(p);
        }
    }

    /// A git worktree of a cycle under issue 7's workspace (`suffix` ""), or
    /// the tree a retry moved aside (`.before-retry`), three days old, made
    /// by a real `git worktree add` from a clone of its own beside it.
    pub fn worktree(&self, suffix: &str, name: &str) -> String {
        let name = if name.is_empty() {
            "issue-7-default"
        } else {
            name
        };
        let root = self.ws(&format!("-7{suffix}"));
        let t = format!("{root}/{WORKTREES}/{name}");
        let from = self.th(&format!("/work/.clean-fixture-7{suffix}"));
        if !Path::new(&from).exists() {
            mkdir(&from);
            self.git(&from, &["init", "-q"]);
            self.git(&from, &["config", "user.email", "clean@tehas"]);
            self.git(&from, &["config", "user.name", "clean"]);
            write(format!("{from}/seed"), "seed\n");
            self.git(&from, &["add", "seed"]);
            self.git(
                &from,
                &["-c", "core.hooksPath=/dev/null", "commit", "-qm", "seed"],
            );
        }
        mkdir(format!("{root}/{WORKTREES}"));
        self.git(&from, &["worktree", "add", "-q", "--detach", &t]);
        write(format!("{t}/console/target/debug/console"), "big\n");
        write(format!("{t}/.pi/agent/state.json"), "st\n");
        write(format!("{t}/src/main.sh"), "keep\n");
        let mut aged: Vec<String> = [
            root.clone(),
            format!("{root}/{WORKTREES}"),
            t.clone(),
            format!("{t}/console"),
            format!("{t}/console/target"),
            format!("{t}/console/target/debug"),
            format!("{t}/src"),
            format!("{t}/.git"),
            format!("{t}/.pi"),
            format!("{t}/.pi/agent"),
        ]
        .into();
        aged.push(self.gitdir(&t));
        age(&aged);
        t
    }

    /// The gitdir git names for worktree `t`.
    pub fn gitdir(&self, t: &str) -> String {
        self.git_out(t, &["rev-parse", "--absolute-git-dir"])
            .expect("git names the gitdir of a worktree")
    }

    /// The commit of worktree `t`, None when git names none.
    pub fn worktree_head(&self, t: &str) -> Option<String> {
        self.git_out(t, &["rev-parse", "HEAD"])
            .filter(|s| !s.is_empty())
    }

    pub fn git(&self, dir: &str, args: &[&str]) {
        let out = self.git_cmd(dir, args).output().unwrap();
        assert!(
            out.status.success(),
            "git {args:?} in {dir}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn git_out(&self, dir: &str, args: &[&str]) -> Option<String> {
        let out = self.git_cmd(dir, args).output().unwrap();
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    fn git_cmd(&self, dir: &str, args: &[&str]) -> Command {
        let mut c = Command::new("git");
        c.arg("-C")
            .arg(dir)
            .args(args)
            .env("HOME", self.t.join("home"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .stdin(Stdio::null());
        c
    }

    /// A worktree a cycle made in its container: its .git names a gitdir
    /// under the shared checkout's path, and the gitdir stands under issue
    /// 7's workspace as `Gitdir` says.
    pub fn container_worktree(&self, gitdir: Gitdir) -> String {
        let w7 = self.ws("-7");
        let t = format!("{w7}/{WORKTREES}/issue-7-default");
        let gd = format!("{w7}/.git/worktrees/issue-7-default");
        mkdir(format!("{t}/target"));
        mkdir(format!("{w7}/.git/worktrees"));
        write(format!("{t}/target/app"), "big\n");
        write(
            format!("{t}/.git"),
            &format!("gitdir: {}/.git/worktrees/issue-7-default\n", self.wd()),
        );
        match &gitdir {
            Gitdir::Dir => mkdir(&gd),
            Gitdir::None => {}
            Gitdir::Link(to) => std::os::unix::fs::symlink(to, &gd).unwrap(),
        }
        age(&[
            t.clone(),
            format!("{t}/target"),
            format!("{t}/.git"),
            format!("{w7}/{WORKTREES}"),
            format!("{w7}/.git"),
            format!("{w7}/.git/worktrees"),
            w7.clone(),
        ]);
        if matches!(gitdir, Gitdir::Dir) {
            age(&[gd]);
        }
        t
    }

    /// The build output of clone `name` under `dev`, its memory, sessions,
    /// cache and author, all old (`clone_state` of the shell).
    pub fn clone_state(&self, name: &str) {
        let devs = self.th("/dev");
        let st = format!("{devs}/{name}.state");
        write(format!("{st}/target/debug/console"), "big\n");
        write(format!("{st}/memory/learned"), "m\n");
        write(format!("{st}/sessions/one.jsonl"), "s\n");
        write(format!("{st}/cache/index.db"), "i\n");
        write(format!("{devs}/{name}.author"), "alice\n");
        age(&[
            devs.clone(),
            st.clone(),
            format!("{st}/target"),
            format!("{st}/target/debug"),
        ]);
    }

    /// The clean log of the home.
    pub fn log(&self) -> String {
        fs::read_to_string(self.logs("/clean.log")).unwrap_or_default()
    }
}

/// What the gitdir of a container's worktree is.
#[allow(dead_code)] // each test file uses a part of this module
pub enum Gitdir {
    Dir,
    None,
    Link(String),
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

/// A directory and its parents.
#[allow(dead_code)] // each test file uses a part of this module
pub fn mkdir(p: impl AsRef<Path>) {
    fs::create_dir_all(p).unwrap();
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

/// touch(1) dated `ago`: an empty file where there is nothing, and a link
/// followed.
#[allow(dead_code)] // each test file uses a part of this module
pub fn touch_ago(p: impl AsRef<Path>, ago: Duration) {
    let p = p.as_ref();
    let f = if p.exists() {
        File::open(p).unwrap()
    } else {
        File::create(p).unwrap_or_else(|e| panic!("touch {}: {e}", p.display()))
    };
    let when = SystemTime::now() - ago;
    f.set_times(FileTimes::new().set_accessed(when).set_modified(when))
        .unwrap();
}

#[allow(dead_code)] // each test file uses a part of this module
pub fn touch(p: impl AsRef<Path>) {
    touch_ago(p, Duration::ZERO);
}

/// Dated three days ago, as `age` of the shell.
#[allow(dead_code)] // each test file uses a part of this module
pub fn age<P: AsRef<Path>>(paths: &[P]) {
    for p in paths {
        touch_ago(p, Duration::from_secs(3 * 86400));
    }
}

/// A lock held by this process until the value drops (t_hold).
#[allow(dead_code)] // each test file uses a part of this module
pub fn hold(p: impl AsRef<Path>) -> File {
    let p = p.as_ref();
    if let Some(d) = p.parent() {
        fs::create_dir_all(d).unwrap();
    }
    let f = File::create(p).unwrap();
    f.lock().unwrap();
    f
}

/// The path is there, a link followed (`[ -e ]`).
#[track_caller]
#[allow(dead_code)] // each test file uses a part of this module
pub fn there(p: impl AsRef<Path>) {
    let p = p.as_ref();
    assert!(p.exists(), "{} should be there", p.display());
}

/// The path is not there (`[ -e ]` fails).
#[track_caller]
#[allow(dead_code)] // each test file uses a part of this module
pub fn gone(p: impl AsRef<Path>) {
    let p = p.as_ref();
    assert!(!p.exists(), "{} should be gone", p.display());
}

/// The path is a link.
#[track_caller]
#[allow(dead_code)] // each test file uses a part of this module
pub fn link(p: impl AsRef<Path>) {
    let p = p.as_ref();
    assert!(p.is_symlink(), "{} should be a link", p.display());
}

/// The path is a directory.
#[track_caller]
#[allow(dead_code)] // each test file uses a part of this module
pub fn dir(p: impl AsRef<Path>) {
    let p = p.as_ref();
    assert!(p.is_dir(), "{} should be a directory", p.display());
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

/// How many lines of `text` hold `want` (`grep -c`).
#[allow(dead_code)] // each test file uses a part of this module
pub fn count(text: &str, want: &str) -> usize {
    text.lines().filter(|l| l.contains(want)).count()
}

/// The exit status of a run, with its output when it is another.
#[track_caller]
#[allow(dead_code)] // each test file uses a part of this module
pub fn rc(r: &Run, want: i32) {
    assert_eq!(r.rc, want, "exit {} and not {want}:\n{}", r.rc, r.out);
}

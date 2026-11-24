//! `tehas dev` where it reaches no GitHub: the clone a manual session works
//! in, whose name its commits carry, what its container is given, and the
//! slice it is started in. A stub docker records its arguments, one per line,
//! and exits at once, so no session runs; gh is not a stub, so a call to it
//! fails the case. What the container is given is pinned here as strictly as
//! the shell pinned it (AGENTS.md rule 4): every mount, every variable and
//! every option, against the literal of console/cycle/tests/docker_args/.
//! Moved from test/dev_test.sh, whose cases of an issue's attachments stay
//! there while their gh stub stands for console/src/attach.rs, with the cases
//! of the session's lock after them.

mod common;
#[path = "../cycle/tests/docker_args/mod.rs"]
mod docker_args;

use common::*;
use docker_args::*;
use std::fs::{self, OpenOptions};
use std::os::unix::fs::{symlink, MetadataExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Once;
use std::time::Duration;
use tehas_core::home::{Live, TokenDir};
use tehas_core::testing::link_exe;

const GIB: u64 = 1 << 30;
/// The container path of the session's cargo build output: the mount, the
/// variable and the manual name it.
const TARGET: &str = "/home/vscode/.tehas-target";
/// The host alias of the instance most cases run with.
const ALIAS: &str = "PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9";

/// One record per call: a counter file gives each call a file of its own,
/// `calls/docker.args.<n>`, and the token it was given beside it.
const DOCKER: &str = r#"if [ "$1" = info ]; then cat "$T/docker.info"; exit; fi
if [ "$1" = image ]; then cat "$T/image.label" 2>/dev/null; exit; fi
n=0
[ -e "$T/docker.n" ] && n=$(<"$T/docker.n")
n=$((n + 1))
printf "%s\n" "$n" >"$T/docker.n"
printf "%s\n" "$@" >"$T/calls/docker.args.$n"
printf "%s\n" "${GH_TOKEN:-}" >"$T/calls/docker.token.$n""#;

/// As DOCKER, and a run whose build output source is not a directory is
/// refused: no line, no container.
const GUARD: &str = r#"if [ "$1" = info ]; then cat "$T/docker.info"; exit; fi
if [ "$1" = image ]; then cat "$T/image.label" 2>/dev/null; exit; fi
prev=""
for a in "$@"; do
  if [ "$prev" = -v ]; then
    case $a in
      *".tehas-target")
        [ -d "${a%%:*}" ] || {
          echo "stub docker: mount source ${a%%:*} is not a directory" >&2
          exit 2
        } ;;
    esac
  fi
  prev=$a
done
n=0
[ -e "$T/docker.n" ] && n=$(<"$T/docker.n")
n=$((n + 1))
printf "%s\n" "$n" >"$T/docker.n"
printf "%s\n" "$@" >"$T/calls/docker.args.$n""#;

/// The session's variables, by name: exactly a cycle's, plus the terminal's
/// two.
const WANT_ENV: &[&str] = &[
    "CARGO_TARGET_DIR",
    "COLORTERM",
    "GH_TOKEN",
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_KEY_0",
    "GIT_CONFIG_KEY_1",
    "GIT_CONFIG_KEY_2",
    "GIT_CONFIG_KEY_3",
    "GIT_CONFIG_VALUE_0",
    "GIT_CONFIG_VALUE_1",
    "GIT_CONFIG_VALUE_2",
    "GIT_CONFIG_VALUE_3",
    "GIT_TERMINAL_PROMPT",
    "CBM_CACHE_DIR",
    "PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS",
    "PI_ENSEMBLE_MAX_WORKSTREAMS",
    "PI_ENSEMBLE_SANDBOX_MODE",
    "PI_ENSEMBLE_SUBAGENT_MODEL",
    "PI_ENSEMBLE_SUBAGENT_PROVIDER",
    "PI_ENSEMBLE_VERIFY_SUITE_TIMEOUT_MS",
    "PI_ENSEMBLE_VERIFY_TIMEOUT_MS",
    "PI_ENSEMBLE_WORKSPACE_ROOT",
    "TERM",
];

/// The variables of a run that has a token, and the terminal's and one more
/// that must not reach the container.
const ENV: &[&str] = &[
    "GH_TOKEN=test-token",
    "TERM=xterm-256color",
    "COLORTERM=truecolor",
    "SECRET_THING=leak",
];

/// A host with the root step done, a product checkout of acme/widget, Pi's
/// settings and a git identity: the slice loaded from its file with 8 GiB, so
/// a container's ceiling is 2048m, Docker on the systemd driver and cgroup
/// v2, and nothing in the slice. The CPU quota follows the host's CPUs, 16.
struct Dev {
    w: World,
    /// The product repository.
    upstream: PathBuf,
    /// The host's checkout of it.
    checkout: PathBuf,
    /// Where the clones of the sessions are.
    devs: PathBuf,
    /// The clone of the session `main`.
    clone: PathBuf,
    /// Its own state on the host: what this session writes, and nothing a
    /// cycle wrote.
    state: PathBuf,
}

/// git in `dir`, its output without the last newline.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

fn id(flag: &str) -> String {
    let out = Command::new("id").arg(flag).output().unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// The image a session runs.
fn image() -> String {
    format!("tehas/pi-rukas:uid{}", id("-u"))
}

/// A word as bash reads it back, in single quotes.
fn quote(word: &str) -> String {
    format!("'{}'", word.replace('\'', r"'\''"))
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

/// The names in `dir` that `keep` takes, none when there is no `dir`.
fn names(dir: &Path, keep: impl Fn(&str) -> bool) -> Vec<String> {
    let Ok(rd) = fs::read_dir(dir) else {
        return vec![];
    };
    sorted(
        rd.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| keep(n))
            .collect(),
    )
}

/// How many entries are under `dir`, at any depth.
fn under(dir: &Path) -> usize {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            1 + if p.is_dir() && !p.is_symlink() {
                under(&p)
            } else {
                0
            }
        })
        .sum()
}

fn write(p: impl AsRef<Path>, text: &str) {
    let p = p.as_ref();
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

fn read(p: impl AsRef<Path>) -> String {
    fs::read_to_string(p.as_ref()).unwrap_or_default()
}

impl Dev {
    fn new(name: &str) -> Dev {
        static CYCLE: Once = Once::new();
        let w = World::new(&format!("dev-{name}"));
        fs::remove_file(w.t().join("bin/gh")).unwrap();
        w.stub("nproc", "echo 16");
        let t = w.t().to_path_buf();
        let upstream = t.join("git/acme/widget");
        let checkout = w.home.join("repos/widget");
        fs::create_dir_all(&upstream).unwrap();
        git(&upstream, &["init", "-q", "-b", "main"]);
        git(
            &upstream,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "first",
            ],
        );
        fs::create_dir_all(checkout.parent().unwrap()).unwrap();
        git(
            &t,
            &[
                "clone",
                "-q",
                &upstream.to_string_lossy(),
                &checkout.to_string_lossy(),
            ],
        );
        w.stub("docker", DOCKER);
        // The command beside the console, as a host has it: the session is
        // the console's (console/src/dev.rs), with tehas-cycle beside it.
        symlink(root().join("bin/tehas"), t.join("tree/bin/tehas")).unwrap();
        link_exe(
            built("tehas-cycle", &CYCLE),
            t.join("tree/libexec/tehas-cycle"),
        )
        .unwrap();
        w.fx(
            "home/.gitconfig",
            "[user]\n\tname = Test Bot\n\temail = bot@example.com\n",
        );
        // Pi's settings as install.sh leaves them on a host.
        w.fx(
            "home/.pi/agent/settings.json",
            "{\"retry\":{\"provider\":{\"timeoutMs\":600000,\"maxRetries\":8,\"maxRetryDelayMs\":60000}}}\n",
        );
        w.fx("home/.pi/agent/models.json", "{\"providers\":{}}\n");
        w.stub("systemctl", r#"cat "$T/slice.show""#);
        let devs = w.home.join("dev");
        let d = Dev {
            clone: devs.join("main"),
            state: devs.join("main.state"),
            devs,
            upstream,
            checkout,
            w,
        };
        d.slice_good();
        d.w.minimal(&[ALIAS]);
        d.env(ENV);
        d
    }

    fn t(&self) -> &Path {
        self.w.t()
    }

    /// The variables of every later run: these, and the cgroup root of the
    /// slice.
    fn env(&self, pairs: &[&str]) {
        self.w.set_env(pairs);
        let root = self.t().join("cgroup");
        self.w.env("TEHAS_CGROUP_ROOT", &root.to_string_lossy());
    }

    /// tehas dev with these words, as a host runs it. No program the gate
    /// denies is reached.
    fn dev(&self, words: &[&str]) -> Run {
        let words: Vec<String> = words.iter().map(|w| quote(w)).collect();
        let cmd = self.t().join("tree/bin/tehas");
        let r = self
            .w
            .sh(&format!("{} dev {}", cmd.display(), words.join(" ")));
        assert_eq!(read(self.t().join("denied")), "", "{}", r.out);
        r
    }

    /// The first run of the session `main`, which makes its clone.
    fn first(name: &str) -> (Dev, Run) {
        let d = Dev::new(name);
        let r = d.dev(&[]);
        assert_eq!(r.rc, 0, "{}", r.out);
        (d, r)
    }

    /// The calls are forgotten.
    fn again(&self) {
        self.w.clear_calls();
        let _ = fs::remove_file(self.t().join("docker.n"));
    }

    /// The arguments of the docker calls that started a session, in order.
    fn session_args(&self) -> Vec<String> {
        (1..)
            .map(|n| self.t().join(format!("calls/docker.args.{n}")))
            .take_while(|p| p.exists())
            .flat_map(|p| read(p).lines().map(String::from).collect::<Vec<_>>())
            .collect()
    }

    /// How many containers were started.
    fn containers(&self) -> usize {
        names(&self.t().join("calls"), |n| n.starts_with("docker.args.")).len()
    }

    /// The values that follow a flag, e.g. every -v or every -e.
    fn after(&self, flag: &str) -> Vec<String> {
        security_of(&self.session_args(), flag)
    }

    /// The value of one variable passed as NAME=value.
    fn passed(&self, name: &str) -> String {
        let pre = format!("{name}=");
        self.after("-e")
            .iter()
            .filter_map(|e| e.strip_prefix(&pre).map(String::from))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The words from the image to the end: what the container runs.
    fn run_words(&self) -> Vec<String> {
        let args = self.session_args();
        let at = args.iter().position(|a| *a == image()).expect("no image");
        args[at..].to_vec()
    }

    /// How many times this exact word stands in the arguments.
    fn exactly(&self, word: &str) -> usize {
        self.session_args().iter().filter(|a| *a == word).count()
    }

    /// The entries of the clones' directory whose name has `part`.
    fn devs_with(&self, part: &str) -> Vec<String> {
        names(&self.devs, |n| n.contains(part))
    }

    /// What `systemctl show` of the slice answers.
    fn slice_show(&self, lines: &[&str]) {
        self.w.fx("slice.show", &format!("{}\n", lines.join("\n")));
    }

    fn slice_good(&self) {
        self.slice_show(&[
            "FragmentPath=/etc/systemd/system/tehas.slice",
            "ActiveState=active",
            "MemoryMax=8589934592",
        ]);
        self.w.fx("docker.info", "systemd 2\n");
        self.slice_state("0.00");
    }

    fn slice_dir(&self) -> PathBuf {
        self.t().join("cgroup/tehas.slice")
    }

    /// The cgroup tree, started again: the controllers, and the slice's
    /// pressure with `full` as its avg60. No child.
    fn slice_state(&self, full: &str) {
        let _ = fs::remove_dir_all(self.t().join("cgroup"));
        write(
            self.t().join("cgroup/cgroup.controllers"),
            "cpuset cpu io memory hugetlb pids rdma misc\n",
        );
        write(
            self.slice_dir().join("memory.pressure"),
            &format!(
                "some avg10=0.00 avg60=0.00 avg300=0.00 total=0\nfull avg10=0.00 avg60={full} avg300=0.00 total=0\n"
            ),
        );
    }

    /// One container in the slice, with its anon, its reclaimable slab and
    /// its floor.
    fn slice_child(&self, id: &str, anon: u64, slab: u64, low: u64) {
        let dir = self.slice_dir().join(format!("docker-{id}.scope"));
        write(
            dir.join("memory.stat"),
            &format!(
                "anon {anon}\nfile 0\nkernel {slab}\nkernel_stack 0\nshmem 0\nslab_reclaimable {slab}\nslab_unreclaimable 0\n"
            ),
        );
        write(dir.join("memory.low"), &format!("{low}\n"));
        write(
            dir.join("memory.events"),
            "low 0\nhigh 0\nmax 0\noom 0\noom_kill 0\noom_group_kill 0\n",
        );
    }

    /// Exactly a cycle's mounts, with the clone in place of the workspace,
    /// and the clone's own cargo target directory.
    fn mounts(&self) -> Vec<String> {
        let (clone, state) = (self.clone.display(), self.state.display());
        let pi = self.t().join("home/.pi/agent");
        let pi = pi.display();
        let session = self.w.at().token_dir(TokenDir::Session);
        sorted(vec![
            format!("{clone}:{clone}"),
            format!("{state}/runs:/home/vscode/.pi/agent/ensemble-runs"),
            format!("{state}/cache:/home/vscode/.tehas-cache"),
            format!("{state}/target:{TARGET}"),
            format!("{state}/memory:/home/vscode/.vipune"),
            format!("{state}/sessions:/home/vscode/.pi/agent/sessions"),
            format!("{pi}/mcp.json:/home/vscode/.pi/agent/mcp.json:ro"),
            format!("{state}/models.json:/home/vscode/.pi/agent/models.json:ro"),
            format!("{pi}/settings.json:/home/vscode/.pi/agent/settings.json:ro"),
            format!("{}:/run/tehas/github:ro", session.display()),
        ])
    }

    /// Refused with 78: neither a clone nor a container was made.
    #[track_caller]
    fn refused(&self, r: &Run) {
        assert_eq!(r.rc, 78, "{}", r.out);
        assert_eq!(self.containers(), 0);
        lacks(&self.w.calls("docker"), "run ");
        assert_eq!(self.devs_with("zed"), Vec::<String>::new());
    }
}

// --- Refusals before anything is made -------------------------------------

#[test]
fn help_prints_the_usage_with_no_instance_and_starts_nothing() {
    let d = Dev::new("help");
    d.w.no_instance();
    d.env(&[]);
    let r = d.dev(&["--help"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "Usage: tehas dev");
    assert_eq!(d.containers(), 0);
}

#[test]
fn an_unknown_argument_starts_no_container_and_makes_no_clone() {
    let d = Dev::new("unknown");
    d.w.minimal(&[]);
    d.env(&["GH_TOKEN=test-token"]);
    let r = d.dev(&["--resume"]);
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "unknown argument: --resume");
    let r = d.dev(&["main", "other"]);
    assert_eq!(r.rc, 64, "{}", r.out);
    assert_eq!(d.containers(), 0);
    assert!(!d.devs.exists());
}

#[test]
fn a_name_that_is_not_one_plain_word_is_refused() {
    let d = Dev::new("name");
    for bad in ["../repos/widget", "a/b", "a.author", "-x", ""] {
        let r = d.dev(&[bad]);
        assert_eq!(r.rc, 64, "{bad}: {}", r.out);
    }
    assert_eq!(d.containers(), 0);
    assert!(!d.devs.exists());
}

#[test]
fn a_malformed_author_is_refused_and_not_stored() {
    let d = Dev::new("author-bad");
    for bad in [
        "Alice",
        "Alice alice@example.com",
        "<alice@example.com>",
        "Alice <alice>",
        "A <a@b> <c@d>",
        "",
    ] {
        let r = d.dev(&["main", "--author", bad]);
        assert_eq!(r.rc, 64, "{bad}: {}", r.out);
    }
    let r = d.dev(&["main", "--author"]);
    assert_eq!(r.rc, 64, "{}", r.out);
    assert_eq!(d.containers(), 0);
    assert!(!d.devs.exists());
}

#[test]
fn an_invalid_instance_starts_no_container() {
    let d = Dev::new("instance");
    d.w.instance(&["TEHAS_CONTROL_ISSUE=1"]);
    let r = d.dev(&[]);
    assert_eq!(r.rc, 78, "{}", r.out);
    assert_eq!(d.containers(), 0);
    assert!(!d.devs.exists());
}

#[test]
fn no_token_to_be_had_starts_no_container() {
    let d = Dev::new("token");
    for dir in [TokenDir::App, TokenDir::Cycle, TokenDir::Session] {
        fs::remove_dir_all(d.w.at().token_dir(dir)).unwrap();
    }
    let key = d.t().join("no-such-key.pem");
    d.env(&[&format!("TEHAS_GITHUB_APP_KEY={}", key.display())]);
    let r = d.dev(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "tehas dev: no GitHub token for the app");
    assert_eq!(d.containers(), 0);
    assert!(!d.devs.exists());
}

#[test]
fn a_checkout_of_another_repository_starts_no_container() {
    let d = Dev::new("other");
    git(
        &d.checkout,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/acme/other.git",
        ],
    );
    let r = d.dev(&[]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "not of acme/widget");
    assert_eq!(d.containers(), 0);
}

#[test]
fn without_pis_settings_no_container_starts_and_no_clone_is_made() {
    let d = Dev::new("settings");
    let settings = d.t().join("home/.pi/agent/settings.json");
    fs::remove_file(&settings).unwrap();
    let r = d.dev(&[]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        &format!("no {}: run install.sh", settings.display()),
    );
    assert_eq!(d.w.calls("docker"), "");
    assert_eq!(d.containers(), 0);
    assert!(!d.devs.exists());
}

// --- The first run, and what its container is given ------------------------

#[test]
fn the_first_run_makes_the_clone_at_the_branchs_tip_with_the_products_origin() {
    let (d, _) = Dev::first("first");
    assert_eq!(d.containers(), 1);
    assert_eq!(
        git(&d.clone, &["rev-parse", "HEAD"]),
        git(&d.upstream, &["rev-parse", "main"])
    );
    assert_eq!(git(&d.clone, &["branch", "--show-current"]), "main");
    assert_eq!(
        git(&d.clone, &["config", "--get", "remote.origin.url"]),
        git(&d.checkout, &["config", "--get", "remote.origin.url"])
    );
    assert_eq!(
        git(
            &d.clone,
            &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]
        ),
        "origin/main"
    );
}

#[test]
fn without_an_author_the_bots_identity_is_used_and_the_command_says_so() {
    let (d, r) = Dev::first("bot");
    has(&r.out, "Test Bot");
    assert_eq!(d.passed("GIT_CONFIG_VALUE_2"), "Test Bot");
    assert_eq!(d.passed("GIT_CONFIG_VALUE_3"), "bot@example.com");
}

#[test]
fn the_session_gets_the_slice_a_cycles_limits_and_no_privileges() {
    let (d, _) = Dev::first("limits");
    let one = |flag: &str| d.after(flag).join("\n");
    assert_eq!(one("--cgroup-parent"), "tehas.slice");
    assert_eq!(one("--memory"), "2048m");
    assert_eq!(one("--memory-swap"), "2048m");
    assert_eq!(one("--memory-reservation"), "2g");
    assert_eq!(one("--pids-limit"), "4096");
    assert_eq!(one("--cpu-shares"), "1024");
    assert_eq!(one("--cpus"), "15.00");
    let args = d.session_args();
    assert_eq!(security_line(&args), format!("{SECURITY_OPTS} "));
    assert_eq!(sorted(security_of(&args, "--cap-add")), ["CHOWN", "FOWNER"]);
    assert_eq!(
        security_of(&args, "--security-opt"),
        ["no-new-privileges:true"]
    );
    // One bounded /tmp, as a cycle's container has. The whole line is counted
    // beside security_line, so a second mount outside the block it matches
    // fails, and the equals form docker also accepts counts as a mount too.
    assert_eq!(security_of(&args, "--tmpfs"), [security_tmpfs()]);
    assert_eq!(count_of(&args, "--tmpfs"), 1);
    // The image's entrypoint drops from root with setpriv, which the dropped
    // SETUID and SETGID refuse: the container starts as the host's user.
    assert_eq!(one("--user"), format!("{}:{}", id("-u"), id("-g")));
    assert!(!args.iter().any(|a| a.contains("--cidfile")));
}

#[test]
fn the_slice_is_read_once_and_the_session_is_told_its_room() {
    let (d, r) = Dev::first("room");
    assert_eq!(
        d.w.calls("systemctl").trim_end(),
        "show tehas.slice -p FragmentPath -p ActiveState -p MemoryMax"
    );
    assert_eq!(count_lines(&d.w.calls("docker"), "info "), 1);
    has(&r.out, "tehas dev: tehas.slice has room for 4 more cycles");
    assert_eq!(d.after("-w"), [d.clone.display().to_string()]);
    assert_eq!(d.after("--name"), ["tehas-dev-main"]);
    assert_eq!(d.run_words(), [image(), "pi".into()]);
}

#[test]
fn exactly_a_cycles_mounts_with_the_clone_and_its_own_target() {
    let (d, _) = Dev::first("mounts");
    let want = d.mounts();
    assert_eq!(sorted(d.after("-v")), want);
    // The count of -v is asked of the docker line, not of the list: an
    // option written --volume or --mount is a mount too, and the sorted list
    // would not see it.
    assert_eq!(count_of(&d.session_args(), "-v"), want.len());
}

#[test]
fn the_sessions_container_is_given_the_narrowed_token_never_the_apps() {
    let (d, _) = Dev::first("narrowed");
    assert_eq!(read(d.t().join("calls/docker.token.1")), "ghs_session\n");
}

#[test]
fn the_sessions_state_is_its_own_made_beside_the_clone_and_kept() {
    let (d, _) = Dev::first("state");
    // target is made before the container starts, so it is there to hold a
    // build.
    fs::create_dir_all(d.state.join("target/debug")).unwrap();
    let dirs = names(&d.state, |_| true)
        .into_iter()
        .filter(|n| d.state.join(n).is_dir())
        .collect::<Vec<_>>();
    assert_eq!(dirs, ["cache", "memory", "runs", "sessions", "target"]);
    let mounts = d.after("-v").join("\n");
    lacks(&mounts, "pi-ensemble");
    let home = d.t().join("home");
    lacks(&mounts, &home.join(".pi/agent/sessions").to_string_lossy());
    lacks(&mounts, &home.join(".vipune").to_string_lossy());
    write(d.state.join("sessions/one.jsonl"), "mine\n");
    write(d.state.join("target/debug/console"), "built\n");
    d.again();
    let r = d.dev(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(read(d.state.join("sessions/one.jsonl")), "mine\n");
    assert_eq!(read(d.state.join("target/debug/console")), "built\n");
}

#[test]
fn exactly_a_cycles_variables_plus_the_terminals_two() {
    let (d, _) = Dev::first("variables");
    let names: Vec<String> = d
        .after("-e")
        .iter()
        .map(|e| e.split('=').next().unwrap().to_string())
        .collect();
    let want = sorted(WANT_ENV.iter().map(|s| s.to_string()).collect());
    assert_eq!(sorted(names), want);
    assert_eq!(
        d.passed("PI_ENSEMBLE_WORKSPACE_ROOT"),
        d.clone.display().to_string()
    );
    assert_eq!(d.passed("CARGO_TARGET_DIR"), TARGET);
    // One mount of it, with no option on it: a mount given :ro, one given
    // :rw,exec, or a second -v of the same source is not the mount the
    // session needs.
    assert_eq!(
        d.exactly(&format!("{}/target:{TARGET}", d.state.display())),
        1
    );
}

#[test]
fn the_sessions_container_gets_no_option_beyond_the_documented_ones() {
    let (d, _) = Dev::first("options");
    let args = d.session_args();
    let valued = format!("--name {DOCKER_OPTS} --user -w -v -e");
    assert_eq!(
        unknown_options(&args, &image(), &valued, "--rm -it --init"),
        Vec::<String>::new()
    );
    assert_eq!(count_of(&args, "--tmpfs"), 1);
    assert_eq!(d.exactly("--init"), 1);
    assert!(args.iter().any(|a| a == "--add-host=models:192.0.2.9"));
}

#[test]
fn the_token_and_the_environment_are_not_in_the_arguments_and_nothing_controls_the_host() {
    let (d, _) = Dev::first("host");
    let args = d.session_args().join("\n");
    for word in ["test-token", "leak", "xterm-256color"] {
        lacks(&args, word);
    }
    lacks(&args, &d.checkout.to_string_lossy());
    for word in ["docker.sock", ".ssh", ".config", "--privileged"] {
        lacks(&args, word);
    }
}

// --- Later runs of a clone ---------------------------------------------------

#[test]
fn a_later_run_keeps_what_is_in_the_clone_and_the_host_runs_none_of_it() {
    let (d, _) = Dev::first("later");
    let t = d.t().to_path_buf();
    // Everything an agent could leave to be run later, and a person's own
    // work.
    let head = git(&d.clone, &["rev-parse", "HEAD"]);
    write(d.clone.join("in-progress.txt"), "mine\n");
    let hook = format!("#!/bin/sh\ntouch \"{}\"\n", t.join("ran-hook").display());
    for name in ["post-checkout", "reference-transaction"] {
        let p = d.clone.join(".git/hooks").join(name);
        write(&p, &hook);
        fs::set_permissions(&p, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    }
    let mut config = OpenOptions::new()
        .append(true)
        .open(d.clone.join(".git/config"))
        .unwrap();
    std::io::Write::write_all(
        &mut config,
        format!(
            "[core]\n\tfsmonitor = touch {}; false\n\tsshCommand = touch {}\n",
            t.join("ran-fsmonitor").display(),
            t.join("ran-ssh").display()
        )
        .as_bytes(),
    )
    .unwrap();
    drop(config);
    git(
        &d.upstream,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "second",
        ],
    );
    git(&d.checkout, &["fetch", "-q", "origin"]);
    d.again();
    let r = d.dev(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(d.containers(), 1);
    assert_eq!(read(d.clone.join("in-progress.txt")), "mine\n");
    assert_eq!(names(&t, |n| n.starts_with("ran-")), Vec::<String>::new());
    assert_eq!(
        git(
            &d.clone,
            &["-c", "core.fsmonitor=false", "rev-parse", "HEAD"]
        ),
        head
    );
}

#[test]
fn an_author_is_stored_next_to_the_clone_used_again_and_checked() {
    let (d, _) = Dev::first("author");
    d.again();
    let r = d.dev(&["main", "--author", "Alice Example <alice@example.com>"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    let stored = d.devs.join("main.author");
    assert_eq!(
        read(&stored).trim_end(),
        "Alice Example <alice@example.com>"
    );
    assert_eq!(d.passed("GIT_CONFIG_VALUE_2"), "Alice Example");
    assert_eq!(d.passed("GIT_CONFIG_VALUE_3"), "alice@example.com");
    lacks(&r.out, "Test Bot");
    let out = Command::new("find")
        .arg(&d.clone)
        .args(["-name", "*.author"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout), "");
    // The stored author is used on the next run without the option.
    d.again();
    let r = d.dev(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(d.passed("GIT_CONFIG_VALUE_2"), "Alice Example");
    assert_eq!(d.passed("GIT_CONFIG_VALUE_3"), "alice@example.com");
    // A stored author that is not Name <email> starts no container.
    fs::write(&stored, "not an author\n").unwrap();
    d.again();
    let r = d.dev(&[]);
    assert_eq!(r.rc, 64, "{}", r.out);
    has(&r.out, "main.author");
    assert_eq!(d.containers(), 0);
}

#[test]
fn a_second_name_is_a_second_clone_with_its_own_author() {
    let d = Dev::new("second");
    let r = d.dev(&["main", "--author", "Alice Example <alice@example.com>"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    d.again();
    let r = d.dev(&["bob"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    let bob = d.devs.join("bob");
    assert_eq!(d.after("-w"), [bob.display().to_string()]);
    assert_eq!(d.after("--name"), ["tehas-dev-bob"]);
    assert_eq!(d.passed("GIT_CONFIG_VALUE_2"), "Test Bot");
    assert_eq!(
        git(&bob, &["rev-parse", "HEAD"]),
        git(&d.checkout, &["rev-parse", "origin/main"])
    );
}

#[test]
fn a_clone_that_could_not_be_finished_is_not_left_to_pass_for_one() {
    let d = Dev::new("unfinished");
    // The clone is made by a fetch of the product's own remote, so a git that
    // will not fetch is the failure the rule is for. The tree is moved into
    // its place whole, or not at all: nothing of a name is left behind to
    // pass for a clone.
    let out = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    let real = String::from_utf8(out.stdout).unwrap().trim().to_string();
    d.w.stub(
        "git",
        &format!(r#"case " $* " in *' fetch '*) exit 1 ;; esac; exec {real} "$@""#),
    );
    let r = d.dev(&["carol"]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "could not make the clone");
    assert_eq!(d.containers(), 0);
    assert_eq!(d.devs_with("carol"), Vec::<String>::new());
    // A state a person kept from an earlier clone is theirs: only what is
    // empty goes.
    let kept = d.devs.join("dora.state/sessions/one.jsonl");
    write(&kept, "mine\n");
    d.again();
    let r = d.dev(&["dora"]);
    assert_eq!(r.rc, 78, "{}", r.out);
    assert_eq!(read(&kept), "mine\n");
    fs::remove_dir_all(d.devs.join("dora.state")).unwrap();
    fs::remove_file(d.t().join("bin/git")).unwrap();
    d.again();
    let r = d.dev(&["carol"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        git(&d.devs.join("carol"), &["rev-parse", "HEAD"]),
        git(&d.checkout, &["rev-parse", "origin/main"])
    );
    assert_eq!(names(&d.devs, |n| n.starts_with('.')), Vec::<String>::new());
}

#[test]
fn the_arguments_after_the_dashes_reach_pi() {
    let (d, _) = Dev::first("dashes");
    d.again();
    let r = d.dev(&["main", "--", "-r", "--model", "a b"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(
        d.run_words(),
        [
            image(),
            "pi".into(),
            "-r".into(),
            "--model".into(),
            "a b".into()
        ]
    );
    d.again();
    let r = d.dev(&["--", "-r"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(d.after("-w"), [d.clone.display().to_string()]);
    let args = d.session_args();
    assert_eq!(args[args.len() - 2..], ["pi", "-r"]);
}

#[test]
fn a_directory_that_is_not_a_clone_is_left_alone() {
    let d = Dev::new("notes");
    let file = d.devs.join("notes/file.txt");
    write(&file, "keep\n");
    let r = d.dev(&["notes"]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "not a clone");
    assert_eq!(read(&file), "keep\n");
    assert_eq!(d.containers(), 0);
}

// --- What the session is told, and what it ends with -----------------------

#[test]
fn running_cycles_are_announced_and_the_session_starts() {
    let d = Dev::new("running");
    let running = d.w.at().live(Live::Running);
    write(
        &running,
        r#"{"cycles":[{"issue":12,"running":true}],"last":null}"#,
    );
    let r = d.dev(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(&r.out, "a cycle is running on issue 12");
    assert_eq!(d.containers(), 1);
    write(
        &running,
        r#"{"cycles":[],"last":{"issue":12,"running":false}}"#,
    );
    lacks(&d.dev(&[]).out, "issue 12");
    // Several running cycles are all named.
    write(
        &running,
        r#"{"cycles":[{"issue":12,"running":true},{"issue":14,"running":true}],"last":null}"#,
    );
    has(&d.dev(&[]).out, "cycles are running on issues 12, 14");
    // The one object an earlier version wrote is still read.
    write(&running, r#"{"issue":12,"running":true}"#);
    has(&d.dev(&[]).out, "a cycle is running on issue 12");
    write(&running, r#"{"issue":12,"running":false}"#);
    lacks(&d.dev(&[]).out, "issue 12");
}

#[test]
fn the_sessions_exit_status_is_passed_on() {
    let d = Dev::new("exit");
    d.w.stub(
        "docker",
        "[ \"$1\" != info ] || { cat \"$T/docker.info\"; exit; }\nexit 7",
    );
    assert_eq!(d.dev(&[]).rc, 7);
}

#[test]
fn the_room_counts_another_container_at_its_use_and_a_cycle_with_none_at_one_reserve() {
    let d = Dev::new("counts");
    d.slice_child(&"b".repeat(64), 3 * GIB, 0, 2 * GIB);
    let r = d.dev(&[]);
    has(&r.out, "tehas dev: tehas.slice has room for 2 more cycles");
    let at = d.w.at();
    fs::create_dir_all(at.cycles_dir()).unwrap();
    let lock = OpenOptions::new()
        .append(true)
        .create(true)
        .open(at.cycle_note(12, "lock"))
        .unwrap();
    lock.lock().unwrap();
    let r = d.dev(&[]);
    has(&r.out, "tehas dev: tehas.slice has room for 1 more cycles");
    // The cycle's container is in the slice: it is counted once, at its
    // floor.
    let c = "c".repeat(64);
    fs::write(at.cycle_note(12, "cid"), format!("{c}\n")).unwrap();
    d.slice_child(&c, 1000, 0, 2 * GIB);
    let r = d.dev(&[]);
    has(&r.out, "tehas dev: tehas.slice has room for 1 more cycles");
    drop(lock);
}

#[test]
fn a_slice_under_pressure_or_unread_has_no_room_and_the_session_still_starts() {
    let d = Dev::new("pressure");
    d.slice_state("12.00");
    d.slice_child(&"b".repeat(64), 5 * GIB, 0, 2 * GIB);
    let r = d.dev(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "tehas dev: tehas.slice has room for 0 more cycles (it is under memory pressure)",
    );
    d.slice_good();
    let pressure = d.slice_dir().join("memory.pressure");
    fs::remove_file(&pressure).unwrap();
    let r = d.dev(&[]);
    has(
        &r.out,
        &format!(
            "tehas dev: tehas.slice has room for 0 more cycles (its use could not be read: {} cannot be read)",
            pressure.display()
        ),
    );
    assert_eq!(d.containers(), 2);
}

// --- The session's build output ---------------------------------------------

#[test]
fn a_build_output_that_is_not_a_directory_starts_no_session() {
    let (d, _) = Dev::first("target-file");
    let target = d.state.join("target");
    let says = format!("{} is not a directory", target.display());
    fs::remove_dir_all(&target).unwrap();
    fs::write(&target, "").unwrap();
    d.again();
    let r = d.dev(&[]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, &says);
    assert_eq!(d.containers(), 0);
    // A symlink to a directory is no mount source either: the session would
    // build where it points, outside the clone's state, and mkdir -p answers
    // 0 for it.
    fs::remove_file(&target).unwrap();
    let outside = d.t().join("outside-target");
    fs::create_dir_all(&outside).unwrap();
    symlink(&outside, &target).unwrap();
    d.again();
    let r = d.dev(&[]);
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, &says);
    assert_eq!(d.containers(), 0);
}

#[test]
fn the_build_output_of_a_clone_the_command_made_is_owned_empty_and_there_when_docker_is_called() {
    let d = Dev::new("target-made");
    let r = d.dev(&["bob3"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    let target = d.devs.join("bob3.state/target");
    // Empty, so what a session builds is its own, and owned by the user the
    // container runs as, not by root: Docker would make a missing source
    // root-owned.
    assert_eq!(under(&target), 0);
    assert_eq!(fs::metadata(&target).unwrap().uid().to_string(), id("-u"));
    assert_eq!(d.exactly(&format!("{}:{TARGET}", target.display())), 1);
}

#[test]
fn the_build_of_the_last_session_of_a_clone_survives_the_next_session() {
    let d = Dev::new("target-kept");
    // The gate of the next session builds on what the last one built: the
    // file the earlier build left is still there after a run of the command,
    // which neither empties the directory it was handed nor replaces it.
    let built = d.devs.join("bob2.state/target/debug/console");
    let session = d.devs.join("bob2.state/sessions/one.jsonl");
    write(&built, "built\n");
    write(&session, "mine\n");
    let r = d.dev(&["bob2"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(read(&built), "built\n");
    assert_eq!(read(&session), "mine\n");
    d.again();
    let r = d.dev(&["bob2"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(read(&built), "built\n");
}

#[test]
fn a_session_whose_build_output_docker_would_have_to_make_is_refused() {
    let d = Dev::new("target-guard");
    let r = d.dev(&["bob2"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    // Two claims about the mount source, both read from the recorded call.
    // The stub refuses a container whose build output source is not a
    // directory, so a run that got here with none answers nothing and the
    // case fails. And the source is older than the record of the call, so
    // this command made it and Docker was not left to. The comparison is only
    // as sharp as the clock: the tree is put a minute behind before the run.
    d.again();
    d.w.stub("docker", GUARD);
    let state = d.devs.join("bob2.state");
    let target = state.join("target");
    fs::remove_dir_all(&target).unwrap();
    let out = Command::new("find")
        .arg(&state)
        .args(["-exec", "touch", "-d", "1 minute ago", "{}", "+"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    std::thread::sleep(Duration::from_millis(1100));
    let r = d.dev(&["bob2"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(d.containers(), 1);
    assert_eq!(under(&target), 0);
    assert_eq!(d.exactly(&format!("{}:{TARGET}", target.display())), 1);
    // The directory predates the call that mounted it.
    let made = fs::metadata(&target).unwrap().modified().unwrap();
    let call = fs::metadata(d.t().join("calls/docker.args.1"))
        .unwrap()
        .modified()
        .unwrap();
    assert!(made < call, "{made:?} is not before {call:?}");
}

// --- Without a usable slice ---------------------------------------------------

#[test]
fn refuses_with_78_without_the_slice_before_any_docker_run() {
    let d = Dev::new("no-slice");
    d.slice_show(&[
        "FragmentPath=",
        "ActiveState=inactive",
        "MemoryMax=infinity",
    ]);
    let r = d.dev(&["zed", "--author", "Zed Example <zed@example.com>"]);
    d.refused(&r);
    has(
        &r.out,
        "tehas dev: no slice file on this host (sudo ./bootstrap.sh slice)",
    );
}

#[test]
fn a_cgroupfs_driver_cgroup_v1_and_a_silent_docker_are_refused_the_same_way() {
    let d = Dev::new("driver");
    for answer in ["cgroupfs 2", "systemd 1", ""] {
        d.again();
        d.slice_good();
        d.w.fx("docker.info", &format!("{answer}\n"));
        let r = d.dev(&["zed"]);
        d.refused(&r);
        has(&r.out, "tehas dev: ");
        has(&r.out, "ocker");
    }
}

#[test]
fn a_reserve_above_the_ceiling_is_refused() {
    let d = Dev::new("reserve");
    d.w.minimal(&["TEHAS_CYCLE_RESERVE=3g"]);
    let r = d.dev(&["zed"]);
    d.refused(&r);
    has(
        &r.out,
        "tehas dev: TEHAS_CYCLE_RESERVE=3g is more than a container may use here (2048m",
    );
}

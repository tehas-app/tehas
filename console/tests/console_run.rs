//! `console run`, the way into every command under the settings: the order of
//! the check, the load and the verb, and that bad settings stop each command
//! with 78 before GitHub, Docker or git and before any write (rule 2). Run as
//! a host runs it: bin/tehas and bin/tehasbot of a tree with this build's
//! console and tehas-dispatch in libexec/, and gh, docker and git stubbed to
//! fail. Moved from test/console_run_test.sh.

mod common;

use common::{built, has, lacks, root, run, Run, MINIMAL};
use std::cell::RefCell;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Once;
use tehas_core::testdir::TestDir;
use tehas_core::testing::{copy_exe, link_exe, write_exe};

/// A tree as install.sh leaves it, a scratch home and the stubs. The
/// instance names no GitHub App of its own (the shell's T_APP_WORLD=0): the
/// home stays empty, so a write shows.
struct Host {
    t: TestDir,
    tree: PathBuf,
    home: PathBuf,
    bash: String,
    instance: RefCell<bool>,
    env: RefCell<Vec<(String, String)>>,
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

fn dispatch_binary() -> PathBuf {
    static BUILT: Once = Once::new();
    built("tehas-dispatch", &BUILT)
}

impl Host {
    fn new(name: &str) -> Host {
        let t = TestDir::new(&format!("console-run-{name}"));
        for d in ["bin", "calls", "deny", "tmp", "tree/bin", "tree/libexec"] {
            fs::create_dir_all(t.join(d)).unwrap();
        }
        let tree = t.join("tree");
        std::os::unix::fs::symlink(root().join("lib"), tree.join("lib")).unwrap();
        for f in ["tehas", "tehasbot"] {
            copy_exe(root().join("bin").join(f), tree.join("bin").join(f)).unwrap();
        }
        for e in fs::read_dir(root().join("libexec")).unwrap() {
            let e = e.unwrap();
            copy_exe(e.path(), tree.join("libexec").join(e.file_name())).unwrap();
        }
        let console = Path::new(env!("CARGO_BIN_EXE_console"));
        link_exe(console, tree.join("libexec/console")).unwrap();
        link_exe(dispatch_binary(), tree.join("libexec/tehas-dispatch")).unwrap();
        link_exe(console, t.join("console-bin")).unwrap();
        let h = Host {
            home: t.join("home/tehas"),
            tree,
            bash: which("bash"),
            t,
            instance: RefCell::new(false),
            env: RefCell::new(vec![]),
        };
        for name in ["docker", "gh", "systemctl", "curl", "ssh"] {
            write_exe(
                h.t.join("deny").join(name),
                format!(
                    "#!{}\necho \"{name} $*\" >>\"$T/denied\"\necho \"no stub stands in for {name}\" >&2\nexit 97\n",
                    h.bash
                ),
            )
            .unwrap();
        }
        for tool in ["gh", "docker", "git"] {
            h.stub(tool, "exit 1");
        }
        h.reset();
        h
    }

    /// The home is gone, the calls are forgotten, no variables are set and
    /// the instance is the minimal one.
    fn reset(&self) {
        let _ = fs::remove_dir_all(&self.home);
        let calls = self.t.join("calls");
        let _ = fs::remove_dir_all(&calls);
        fs::create_dir_all(calls).unwrap();
        self.env.borrow_mut().clear();
        self.instance(MINIMAL);
    }

    /// instance.env, as given (t_instance).
    fn instance(&self, lines: &[&str]) {
        let dir = self.t.join("instance");
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        let mut text = lines.join("\n");
        if !text.is_empty() {
            text.push('\n');
        }
        fs::write(dir.join("instance.env"), text).unwrap();
        *self.instance.borrow_mut() = true;
    }

    /// The minimal instance, with more settings after it.
    fn minimal(&self, more: &[&str]) {
        let mut lines = MINIMAL.to_vec();
        lines.extend_from_slice(more);
        self.instance(&lines);
    }

    /// No instance directory, and none named to a run (t_no_instance).
    fn no_instance(&self) {
        *self.instance.borrow_mut() = false;
        fs::remove_dir_all(self.t.join("instance")).unwrap();
    }

    /// The variables of every later run until reset() (T_ENV).
    fn set_env(&self, pairs: &[(&str, &str)]) {
        *self.env.borrow_mut() = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
    }

    /// A stub on the PATH of a run that records its arguments, then runs
    /// `body` (t_stub).
    fn stub(&self, name: &str, body: &str) {
        write_exe(
            self.t.join("bin").join(name),
            format!(
                "#!{}\nprintf '%s\\n' \"$*\" >>\"$T/calls/{name}\"\n{body}\n",
                self.bash
            ),
        )
        .unwrap();
    }

    /// What a stub was called with, one call per line.
    fn calls(&self, name: &str) -> String {
        fs::read_to_string(self.t.join("calls").join(name)).unwrap_or_default()
    }

    /// `tehas` of the tree, as a shell word.
    fn tehas(&self) -> String {
        self.tree.join("bin/tehas").display().to_string()
    }

    /// `tehasbot` of the tree, as a shell word.
    fn bot(&self) -> String {
        self.tree.join("bin/tehasbot").display().to_string()
    }

    /// A command of bin/ of the tree with its words, as the shell's list
    /// names it: "tehas state pack -".
    fn cmd(&self, command: &str) -> Run {
        self.sh(&format!("{}/bin/{command}", self.tree.display()))
    }

    /// A bash snippet in the environment of a run (t_run).
    fn sh(&self, snippet: &str) -> Run {
        let mut c = Command::new(&self.bash);
        c.args(["-c", snippet])
            .env_clear()
            .env(
                "PATH",
                format!(
                    "{}:{}:{}",
                    self.t.join("bin").display(),
                    self.t.join("deny").display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
            .env("HOME", self.t.join("home"))
            .env("TEHAS_HOME", &self.home)
            .env("T", self.t.path())
            .env("T_ROOT", root())
            .env("CARGO_TARGET_DIR", self.t.join("target"))
            .env("TEHAS_SETTINGS_CMD", self.t.join("console-bin"))
            .env("TEHAS_TEST_STORE_ANSWERS", self.t.join("store-answers"))
            .env("TMPDIR", self.t.join("tmp"));
        if *self.instance.borrow() {
            c.env("TEHAS_INSTANCE_DIR", self.t.join("instance"));
        }
        c.envs(self.env.borrow().iter().map(|(k, v)| (k, v)));
        run(c)
    }

    /// Nothing outside was asked and nothing was written.
    #[track_caller]
    fn untouched(&self) {
        assert_eq!(
            format!(
                "{}{}{}",
                self.calls("gh"),
                self.calls("docker"),
                self.calls("git")
            ),
            ""
        );
        assert_eq!(entries(&self.home), 0, "the home holds files");
    }

    /// A program at `path` that records the umask it runs under and its words.
    fn recording(&self, path: &Path) {
        write_exe(
            path,
            "#!/usr/bin/env bash\numask >>\"$T/calls/umask\"\nprintf \"%s\\n\" \"$*\" >>\"$T/calls/console\"\n",
        )
        .unwrap();
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

/// The files and directories under `dir`, 0 when there is no `dir`.
fn entries(dir: &Path) -> usize {
    let Ok(read) = fs::read_dir(dir) else {
        return 0;
    };
    read.map(|e| {
        let e = e.unwrap();
        let below = if e.file_type().unwrap().is_dir() {
            entries(&e.path())
        } else {
            0
        };
        1 + below
    })
    .sum()
}

#[track_caller]
fn rc(r: &Run, want: i32) {
    assert_eq!(r.rc, want, "the exit of a run that printed:\n{}", r.out);
}

/// One test per row: `name: arguments of check;`.
macro_rules! cases {
    ($check:ident { $($name:ident: $($arg:expr),+;)* }) => {
        $(
            #[test]
            fn $name() {
                $check($($arg),+);
            }
        )*
    };
}

// Every command that went through a script or with-settings, with no
// instance.env, a setting no rule allows and an instance file that ends its
// shell.

fn with_no_instance_env(command: &str) {
    let h = Host::new("no-instance");
    h.no_instance();
    let r = h.cmd(command);
    rc(&r, 78);
    has(&r.out, "no instance.env");
    h.untouched();
}

fn with_a_setting_no_rule_allows(command: &str) {
    let h = Host::new("no-rule");
    h.minimal(&["TEHAS_ANALYTICS=sometimes"]);
    let r = h.cmd(command);
    rc(&r, 78);
    has(&r.out, "TEHAS_ANALYTICS");
    h.untouched();
}

fn with_an_instance_file_that_ends_its_shell(command: &str) {
    let h = Host::new("ends-shell");
    h.minimal(&["exit 3"]);
    let r = h.cmd(command);
    rc(&r, 78);
    has(&r.out, "could not be read as shell");
    h.untouched();
}

/// What each command cannot run without, and the name it stops on.
fn with_no_tehas_repo(command: &str) {
    let h = Host::new("no-repo");
    h.instance(&[
        "TEHAS_GITHUB_APP_ID=1",
        "TEHAS_CONTROL_ISSUE=1",
        "TEHAS_ALLOWED_LABELERS=alice",
        "PI_ENSEMBLE_SUBAGENT_MODEL=test-model",
        "PI_ENSEMBLE_SUBAGENT_PROVIDER=test-provider",
    ]);
    let r = h.cmd(command);
    rc(&r, 78);
    has(&r.out, "TEHAS_REPO is not set (see ");
    h.untouched();
}

/// The commands that write or call GitHub stop on any of the three settings
/// of the instance, missing or malformed, before the write or the call.
fn writer_with(command: &str, setting: &str, says: &str) {
    let h = Host::new("writer");
    h.minimal(&[setting]);
    let r = h.cmd(command);
    rc(&r, 78);
    has(&r.out, says);
    h.untouched();
}

#[test]
fn a_refresh_with_no_tehas_dispatch_to_run_is_78_and_says_to_install() {
    let h = Host::new("refresh-no-dispatch");
    for words in ["--pr 50", "--pr 50 --json", ""] {
        h.reset();
        let none = h.t.join("no-dispatch");
        h.set_env(&[("TEHAS_DISPATCH_CMD", &none.to_string_lossy())]);
        let r = h.sh(&format!("{} refresh {words}", h.tehas()));
        rc(&r, 78);
        has(&r.out, "cannot run (run install.sh)");
        h.untouched();
    }
}

#[test]
fn an_order_with_no_state_store_is_78_and_says_to_install_as_history_is() {
    let h = Host::new("order-no-store");
    for words in ["", "--json", "set --issues 5", "clear"] {
        let r = h.sh(&format!("{} order {words}", h.tehas()));
        rc(&r, 78);
        has(&r.out, "no state store");
    }
    h.untouched();
}

#[test]
fn a_refresh_stops_on_the_settings_it_needs() {
    let h = Host::new("refresh-settings");
    h.instance(&[
        "TEHAS_GITHUB_APP_ID=1",
        "TEHAS_CONTROL_ISSUE=1",
        "TEHAS_ALLOWED_LABELERS=alice",
    ]);
    let r = h.sh(&format!("{} refresh --pr 50", h.bot()));
    rc(&r, 78);
    has(&r.out, "TEHAS_REPO is not set (see ");
    h.untouched();
}

#[test]
fn tehasbot_improve_is_gone_an_unknown_command_64_and_the_settings_are_not_read() {
    let h = Host::new("improve");
    h.instance(&[r#"echo "settings were read" >&2; exit 9"#]);
    let r = h.sh(&format!("{} improve", h.bot()));
    rc(&r, 64);
    has(&r.out, "unknown command: improve");
    lacks(&r.out, "settings were read");
    h.untouched();
}

#[test]
fn refresh_with_no_pr_n_is_64_before_the_settings_are_read() {
    let h = Host::new("refresh-no-pr");
    h.instance(&[r#"echo "settings were read" >&2; exit 9"#]);
    let r = h.sh(&format!("{} refresh", h.bot()));
    rc(&r, 64);
    has(
        &r.out,
        "tehas refresh: --pr takes the number of one pull request",
    );
    lacks(&r.out, "settings were read");
    h.untouched();
}

#[test]
fn tehas_refresh_the_public_name_is_64_before_the_settings_are_read_too() {
    let h = Host::new("refresh-public");
    h.instance(&[r#"echo "settings were read" >&2; exit 9"#]);
    let r = h.sh(&format!("{} refresh", h.tehas()));
    rc(&r, 64);
    has(
        &r.out,
        "tehas refresh: --pr takes the number of one pull request",
    );
    lacks(&r.out, "settings were read");
    h.untouched();
}

#[test]
fn tehas_github_check_with_another_word_is_64_before_the_settings_are_read() {
    let h = Host::new("github-check");
    h.instance(&[r#"echo "settings were read" >&2; exit 9"#]);
    let r = h.sh(&format!("{} github check --print", h.tehas()));
    rc(&r, 64);
    lacks(&r.out, "settings were read");
    h.untouched();
}

#[test]
fn an_instance_with_previews_off_is_78_and_says_so() {
    let h = Host::new("previews-off");
    let r = h.sh(&format!("{} preview", h.bot()));
    rc(&r, 78);
    assert_eq!(
        r.out,
        "tehas: TEHAS_PREVIEWS is not 1: this instance has no previews"
    );
    h.untouched();
}

// A bad argument is 64 and a text 0, whatever the settings hold.
#[test]
fn a_check_that_refuses_is_its_own_status_and_the_settings_are_not_read() {
    let h = Host::new("check-refuses");
    h.instance(&[r#"echo "settings were read" >&2; exit 9"#]);
    let (tehas, bot) = (h.tehas(), h.bot());
    for bad in [
        format!("{tehas} limit show"),
        format!("{tehas} pause a b"),
        format!("{tehas} stop"),
        format!("{tehas} check-product --what"),
        format!("{tehas} init-product --what"),
        format!("{tehas} dispatch --dryrun"),
        format!("{bot} dispatch --dry-run --help"),
        format!("{bot} preview --what"),
        format!("{bot} clean --what"),
        format!("{bot} github-token --print"),
        format!("{bot} refresh --what"),
    ] {
        let r = h.sh(&bad);
        rc(&r, 64);
        lacks(&r.out, "settings were read");
    }
    h.untouched();
}

#[test]
fn a_check_that_was_the_whole_answer_ends_0_and_neither_the_settings_nor_the_verb_are_reached() {
    let h = Host::new("check-answers");
    h.instance(&[r#"echo "settings were read" >&2; exit 9"#]);
    let (tehas, bot) = (h.tehas(), h.bot());
    for help in [
        format!("{tehas} stop 5 --help"),
        format!("{bot} dispatch --help"),
        format!("{bot} clean -h"),
        format!("{bot} refresh --help"),
    ] {
        let r = h.sh(&help);
        rc(&r, 0);
        lacks(&r.out, "settings were read");
    }
    h.untouched();
}

#[test]
fn a_word_that_is_not_text_is_refused_by_the_check_of_its_command_else_as_text() {
    let h = Host::new("not-text");
    let r = h.sh(&format!(r"{} stop $'\xff'", h.tehas()));
    rc(&r, 64);
    let r = h.sh(&format!(r"{} limit set $'\xff'", h.tehas()));
    rc(&r, 2);
    has(&r.out, "console: an argument is not UTF-8");
    h.untouched();
}

#[test]
fn the_status_is_the_verbs() {
    let h = Host::new("status");
    let r = h.sh(&format!("{} api --json", h.tehas()));
    rc(&r, 0);
    let r = h.sh(&format!("{} refresh --what", h.bot()));
    rc(&r, 64);
}

// What a command starts has what a shell's load exports, no more and no fewer.
#[test]
fn the_poll_is_given_what_a_shells_load_exports_no_more_and_no_fewer() {
    let h = Host::new("poll-env");
    h.stub(
        "env-of-a-child",
        r#"env -0 | sort -z | tr "\0" "\n" | grep -E "^(TEHAS|PI_ENSEMBLE|PI_RUKAS|GH_|GIT_)" | grep -v "^TEHAS_DISPATCH_CMD=" >"$T/child-env""#,
    );
    let checkout = h.home.join("repos/widget");
    h.minimal(&[
        "TEHAS_REFRESH_SLOTS=2",
        "SOMETHING_ELSE=x",
        &format!("TEHAS_WORK_REPO={}", checkout.display()),
    ]);
    fs::create_dir_all(&checkout).unwrap();
    for args in [
        &["init", "-q"][..],
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/acme/widget.git",
        ],
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    ] {
        let out = Command::new("git")
            .arg("-C")
            .arg(&checkout)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}");
    }
    fs::remove_file(h.t.join("bin/git")).unwrap();
    let child = h.t.join("bin/env-of-a-child");
    h.set_env(&[
        ("TEHAS_DISPATCH_CMD", &child.to_string_lossy()),
        ("TEHAS_STALE", "old"),
        ("PI_ENSEMBLE_STALE", "old"),
    ]);
    let r = h.sh(&format!("{} dispatch", h.tehas()));
    rc(&r, 0);
    let child_env = fs::read_to_string(h.t.join("child-env")).unwrap();
    lacks(&child_env, "STALE");
    has(&child_env, "TEHAS_REPO=acme/widget");
    has(&child_env, "PI_ENSEMBLE_IMAGE=");
    h.sh(&format!(
        r#". '{}/lib/config.sh'; tehas_load; env -0 | sort -z | tr '\0' '\n' | grep -E '^(TEHAS|PI_ENSEMBLE|PI_RUKAS|GH_|GIT_)' | grep -v '^TEHAS_DISPATCH_CMD=' >'{}/shell-env'"#,
        root().display(),
        h.t.display()
    ));
    let shell_env = fs::read_to_string(h.t.join("shell-env")).unwrap();
    assert_eq!(shell_env, child_env);
}

// The two the shell hands in, and the tree the console was told it stands in.
#[test]
fn tehas_and_tehasbot_hand_the_front_their_libexec_the_uid_and_the_bash_and_every_word() {
    let h = Host::new("front-words");
    let other = h.t.join("other");
    h.recording(&other);
    h.set_env(&[("TEHAS_CONSOLE", &other.to_string_lossy())]);
    let r = h.sh(&format!(
        r#"{} limit set --reason 'a b' && {} clean --dry-run && echo "$EUID $BASH""#,
        h.tehas(),
        h.bot()
    ));
    rc(&r, 0);
    let say = r.out.rsplit('\n').next().unwrap();
    let (uid, bash) = (
        say.rsplit_once(' ').unwrap().0,
        say.split_once(' ').unwrap().1,
    );
    let tree = h.tree.display();
    assert_eq!(
        h.calls("console").trim_end_matches('\n'),
        format!(
            "front {tree}/libexec {uid} {bash} tehas limit set --reason a b\nfront {tree}/libexec {uid} {bash} tehasbot clean --dry-run"
        )
    );
}

#[test]
fn the_pages_and_the_previews_are_written_by_a_umask_their_server_can_read() {
    let h = Host::new("umask");
    let other = h.t.join("other");
    h.recording(&other);
    h.set_env(&[("TEHAS_CONSOLE", &other.to_string_lossy())]);
    let bot = h.bot();
    h.sh(&format!(
        "umask 077; {bot} preview; {bot} protolab; {bot} clean"
    ));
    assert_eq!(h.calls("umask").trim_end_matches('\n'), "0022\n0022\n0077");
}

#[test]
fn a_reader_that_left_before_the_text_is_141_as_for_every_command_and_says_nothing() {
    let h = Host::new("reader-left");
    let r = h.sh(&format!(
        "(sleep 0.2; {} help dispatch; echo $? >&2) | true",
        h.tehas()
    ));
    assert_eq!(r.out, "141");
    let r = h.sh(&format!(
        "(sleep 0.2; {} dispatch --help; echo $? >&2) | true",
        h.bot()
    ));
    assert_eq!(r.out, "141");
}

#[test]
fn no_console_is_78_names_the_command_and_the_fix_and_reads_nothing() {
    let h = Host::new("no-console");
    h.instance(&[r#"echo "settings were read" >&2; exit 9"#]);
    fs::rename(h.tree.join("libexec/console"), h.t.join("held")).unwrap();
    let tree = h.tree.display();
    let r = h.sh(&format!("{} limit show", h.tehas()));
    rc(&r, 78);
    assert_eq!(
        r.out,
        format!("tehas: no console at {tree}/libexec/console: run install.sh")
    );
    let r = h.sh(&format!("{} refresh", h.bot()));
    rc(&r, 78);
    assert_eq!(
        r.out,
        format!("tehasbot: no console at {tree}/libexec/console: run install.sh")
    );
}

cases!(with_no_instance_env {
    tehas_state_pack_with_no_instance_env_is_78_and_calls_and_writes_nothing: "tehas state pack -";
    tehas_limit_with_no_instance_env_is_78_and_calls_and_writes_nothing: "tehas limit";
    tehas_pause_with_no_instance_env_is_78_and_calls_and_writes_nothing: "tehas pause";
    tehas_resume_with_no_instance_env_is_78_and_calls_and_writes_nothing: "tehas resume";
    tehas_api_with_no_instance_env_is_78_and_calls_and_writes_nothing: "tehas api";
    tehas_labels_with_no_instance_env_is_78_and_calls_and_writes_nothing: "tehas labels";
    tehas_analytics_report_with_no_instance_env_is_78_and_calls_and_writes_nothing: "tehas analytics report";
    tehas_drain_with_no_instance_env_is_78_and_calls_and_writes_nothing: "tehas drain";
    tehas_history_with_no_instance_env_is_78_and_calls_and_writes_nothing: "tehas history";
    tehas_stop_7_with_no_instance_env_is_78_and_calls_and_writes_nothing: "tehas stop 7";
    tehas_mcp_serve_with_no_instance_env_is_78_and_calls_and_writes_nothing: "tehas mcp serve";
    tehas_dispatch_with_no_instance_env_is_78_and_calls_and_writes_nothing: "tehas dispatch";
    tehas_check_product_with_no_instance_env_is_78_and_calls_and_writes_nothing: "tehas check-product";
    tehas_init_product_with_no_instance_env_is_78_and_calls_and_writes_nothing: "tehas init-product";
    tehasbot_health_with_no_instance_env_is_78_and_calls_and_writes_nothing: "tehasbot health";
    tehasbot_status_with_no_instance_env_is_78_and_calls_and_writes_nothing: "tehasbot status";
    tehasbot_top_with_no_instance_env_is_78_and_calls_and_writes_nothing: "tehasbot top";
    tehasbot_mcp_serve_with_no_instance_env_is_78_and_calls_and_writes_nothing: "tehasbot mcp serve";
    tehasbot_dispatch_with_no_instance_env_is_78_and_calls_and_writes_nothing: "tehasbot dispatch";
    tehasbot_protolab_with_no_instance_env_is_78_and_calls_and_writes_nothing: "tehasbot protolab";
    tehasbot_preview_with_no_instance_env_is_78_and_calls_and_writes_nothing: "tehasbot preview";
    tehasbot_refresh_pr_50_with_no_instance_env_is_78_and_calls_and_writes_nothing: "tehasbot refresh --pr 50";
    tehasbot_clean_with_no_instance_env_is_78_and_calls_and_writes_nothing: "tehasbot clean";
    tehasbot_github_token_with_no_instance_env_is_78_and_calls_and_writes_nothing: "tehasbot github-token";
});

cases!(with_a_setting_no_rule_allows {
    tehas_state_pack_with_a_setting_no_rule_allows_is_78_and_calls_and_writes_nothing: "tehas state pack -";
    tehas_limit_with_a_setting_no_rule_allows_is_78_and_calls_and_writes_nothing: "tehas limit";
    tehas_pause_with_a_setting_no_rule_allows_is_78_and_calls_and_writes_nothing: "tehas pause";
    tehas_resume_with_a_setting_no_rule_allows_is_78_and_calls_and_writes_nothing: "tehas resume";
    tehas_api_with_a_setting_no_rule_allows_is_78_and_calls_and_writes_nothing: "tehas api";
    tehas_labels_with_a_setting_no_rule_allows_is_78_and_calls_and_writes_nothing: "tehas labels";
    tehas_analytics_report_with_a_setting_no_rule_allows_is_78_and_calls_and_writes_nothing: "tehas analytics report";
    tehas_drain_with_a_setting_no_rule_allows_is_78_and_calls_and_writes_nothing: "tehas drain";
    tehas_history_with_a_setting_no_rule_allows_is_78_and_calls_and_writes_nothing: "tehas history";
    tehas_stop_7_with_a_setting_no_rule_allows_is_78_and_calls_and_writes_nothing: "tehas stop 7";
    tehas_mcp_serve_with_a_setting_no_rule_allows_is_78_and_calls_and_writes_nothing: "tehas mcp serve";
    tehas_dispatch_with_a_setting_no_rule_allows_is_78_and_calls_and_writes_nothing: "tehas dispatch";
    tehas_check_product_with_a_setting_no_rule_allows_is_78_and_calls_and_writes_nothing: "tehas check-product";
    tehas_init_product_with_a_setting_no_rule_allows_is_78_and_calls_and_writes_nothing: "tehas init-product";
    tehasbot_health_with_a_setting_no_rule_allows_is_78_and_calls_and_writes_nothing: "tehasbot health";
    tehasbot_status_with_a_setting_no_rule_allows_is_78_and_calls_and_writes_nothing: "tehasbot status";
    tehasbot_top_with_a_setting_no_rule_allows_is_78_and_calls_and_writes_nothing: "tehasbot top";
    tehasbot_mcp_serve_with_a_setting_no_rule_allows_is_78_and_calls_and_writes_nothing: "tehasbot mcp serve";
    tehasbot_dispatch_with_a_setting_no_rule_allows_is_78_and_calls_and_writes_nothing: "tehasbot dispatch";
    tehasbot_protolab_with_a_setting_no_rule_allows_is_78_and_calls_and_writes_nothing: "tehasbot protolab";
    tehasbot_preview_with_a_setting_no_rule_allows_is_78_and_calls_and_writes_nothing: "tehasbot preview";
    tehasbot_refresh_pr_50_with_a_setting_no_rule_allows_is_78_and_calls_and_writes_nothing: "tehasbot refresh --pr 50";
    tehasbot_clean_with_a_setting_no_rule_allows_is_78_and_calls_and_writes_nothing: "tehasbot clean";
    tehasbot_github_token_with_a_setting_no_rule_allows_is_78_and_calls_and_writes_nothing: "tehasbot github-token";
});

cases!(with_an_instance_file_that_ends_its_shell {
    tehas_state_pack_with_an_instance_file_that_ends_its_shell_is_78_and_calls_and_writes_nothing: "tehas state pack -";
    tehas_limit_with_an_instance_file_that_ends_its_shell_is_78_and_calls_and_writes_nothing: "tehas limit";
    tehas_pause_with_an_instance_file_that_ends_its_shell_is_78_and_calls_and_writes_nothing: "tehas pause";
    tehas_resume_with_an_instance_file_that_ends_its_shell_is_78_and_calls_and_writes_nothing: "tehas resume";
    tehas_api_with_an_instance_file_that_ends_its_shell_is_78_and_calls_and_writes_nothing: "tehas api";
    tehas_labels_with_an_instance_file_that_ends_its_shell_is_78_and_calls_and_writes_nothing: "tehas labels";
    tehas_analytics_report_with_an_instance_file_that_ends_its_shell_is_78_and_calls_and_writes_nothing: "tehas analytics report";
    tehas_drain_with_an_instance_file_that_ends_its_shell_is_78_and_calls_and_writes_nothing: "tehas drain";
    tehas_history_with_an_instance_file_that_ends_its_shell_is_78_and_calls_and_writes_nothing: "tehas history";
    tehas_stop_7_with_an_instance_file_that_ends_its_shell_is_78_and_calls_and_writes_nothing: "tehas stop 7";
    tehas_mcp_serve_with_an_instance_file_that_ends_its_shell_is_78_and_calls_and_writes_nothing: "tehas mcp serve";
    tehas_dispatch_with_an_instance_file_that_ends_its_shell_is_78_and_calls_and_writes_nothing: "tehas dispatch";
    tehas_check_product_with_an_instance_file_that_ends_its_shell_is_78_and_calls_and_writes_nothing: "tehas check-product";
    tehas_init_product_with_an_instance_file_that_ends_its_shell_is_78_and_calls_and_writes_nothing: "tehas init-product";
    tehasbot_health_with_an_instance_file_that_ends_its_shell_is_78_and_calls_and_writes_nothing: "tehasbot health";
    tehasbot_status_with_an_instance_file_that_ends_its_shell_is_78_and_calls_and_writes_nothing: "tehasbot status";
    tehasbot_top_with_an_instance_file_that_ends_its_shell_is_78_and_calls_and_writes_nothing: "tehasbot top";
    tehasbot_mcp_serve_with_an_instance_file_that_ends_its_shell_is_78_and_calls_and_writes_nothing: "tehasbot mcp serve";
    tehasbot_dispatch_with_an_instance_file_that_ends_its_shell_is_78_and_calls_and_writes_nothing: "tehasbot dispatch";
    tehasbot_protolab_with_an_instance_file_that_ends_its_shell_is_78_and_calls_and_writes_nothing: "tehasbot protolab";
    tehasbot_preview_with_an_instance_file_that_ends_its_shell_is_78_and_calls_and_writes_nothing: "tehasbot preview";
    tehasbot_refresh_pr_50_with_an_instance_file_that_ends_its_shell_is_78_and_calls_and_writes_nothing: "tehasbot refresh --pr 50";
    tehasbot_clean_with_an_instance_file_that_ends_its_shell_is_78_and_calls_and_writes_nothing: "tehasbot clean";
    tehasbot_github_token_with_an_instance_file_that_ends_its_shell_is_78_and_calls_and_writes_nothing: "tehasbot github-token";
});

cases!(with_no_tehas_repo {
    tehas_api_with_no_tehas_repo_is_78_and_calls_and_writes_nothing: "tehas api";
    tehas_labels_with_no_tehas_repo_is_78_and_calls_and_writes_nothing: "tehas labels";
    tehas_history_with_no_tehas_repo_is_78_and_calls_and_writes_nothing: "tehas history";
    tehas_resume_with_no_tehas_repo_is_78_and_calls_and_writes_nothing: "tehas resume";
    tehasbot_status_with_no_tehas_repo_is_78_and_calls_and_writes_nothing: "tehasbot status";
    tehas_dispatch_with_no_tehas_repo_is_78_and_calls_and_writes_nothing: "tehas dispatch";
    tehasbot_dispatch_with_no_tehas_repo_is_78_and_calls_and_writes_nothing: "tehasbot dispatch";
    tehas_check_product_with_no_tehas_repo_is_78_and_calls_and_writes_nothing: "tehas check-product";
    tehas_init_product_with_no_tehas_repo_is_78_and_calls_and_writes_nothing: "tehas init-product";
    tehasbot_protolab_with_no_tehas_repo_is_78_and_calls_and_writes_nothing: "tehasbot protolab";
    tehasbot_preview_with_no_tehas_repo_is_78_and_calls_and_writes_nothing: "tehasbot preview";
    tehasbot_clean_with_no_tehas_repo_is_78_and_calls_and_writes_nothing: "tehasbot clean";
});

cases!(writer_with {
    tehas_start_5_with_tehas_repo_bad_is_78_and_calls_and_writes_nothing: "tehas start 5", "TEHAS_REPO=bad", "TEHAS_REPO is not ";
    tehas_start_5_with_tehas_repo_is_78_and_calls_and_writes_nothing: "tehas start 5", "TEHAS_REPO=", "TEHAS_REPO is not ";
    tehas_start_5_with_tehas_control_issue_x_is_78_and_calls_and_writes_nothing: "tehas start 5", "TEHAS_CONTROL_ISSUE=x", "TEHAS_CONTROL_ISSUE is not ";
    tehas_start_5_with_tehas_allowed_labelers_alice_bob_is_78_and_calls_and_writes_nothing: "tehas start 5", "TEHAS_ALLOWED_LABELERS='alice  bob'", "TEHAS_ALLOWED_LABELERS is not ";
    tehas_start_5_with_tehas_allowed_labelers_is_78_and_calls_and_writes_nothing: "tehas start 5", "TEHAS_ALLOWED_LABELERS=", "TEHAS_ALLOWED_LABELERS is not ";
    tehas_limit_set_cycles_2_with_tehas_repo_bad_is_78_and_calls_and_writes_nothing: "tehas limit set --cycles 2", "TEHAS_REPO=bad", "TEHAS_REPO is not ";
    tehas_limit_set_cycles_2_with_tehas_repo_is_78_and_calls_and_writes_nothing: "tehas limit set --cycles 2", "TEHAS_REPO=", "TEHAS_REPO is not ";
    tehas_limit_set_cycles_2_with_tehas_control_issue_x_is_78_and_calls_and_writes_nothing: "tehas limit set --cycles 2", "TEHAS_CONTROL_ISSUE=x", "TEHAS_CONTROL_ISSUE is not ";
    tehas_limit_set_cycles_2_with_tehas_allowed_labelers_alice_bob_is_78_and_calls_and_writes_nothing: "tehas limit set --cycles 2", "TEHAS_ALLOWED_LABELERS='alice  bob'", "TEHAS_ALLOWED_LABELERS is not ";
    tehas_limit_set_cycles_2_with_tehas_allowed_labelers_is_78_and_calls_and_writes_nothing: "tehas limit set --cycles 2", "TEHAS_ALLOWED_LABELERS=", "TEHAS_ALLOWED_LABELERS is not ";
    tehas_limit_clear_with_tehas_repo_bad_is_78_and_calls_and_writes_nothing: "tehas limit clear", "TEHAS_REPO=bad", "TEHAS_REPO is not ";
    tehas_limit_clear_with_tehas_repo_is_78_and_calls_and_writes_nothing: "tehas limit clear", "TEHAS_REPO=", "TEHAS_REPO is not ";
    tehas_limit_clear_with_tehas_control_issue_x_is_78_and_calls_and_writes_nothing: "tehas limit clear", "TEHAS_CONTROL_ISSUE=x", "TEHAS_CONTROL_ISSUE is not ";
    tehas_limit_clear_with_tehas_allowed_labelers_alice_bob_is_78_and_calls_and_writes_nothing: "tehas limit clear", "TEHAS_ALLOWED_LABELERS='alice  bob'", "TEHAS_ALLOWED_LABELERS is not ";
    tehas_limit_clear_with_tehas_allowed_labelers_is_78_and_calls_and_writes_nothing: "tehas limit clear", "TEHAS_ALLOWED_LABELERS=", "TEHAS_ALLOWED_LABELERS is not ";
    tehas_order_set_issues_5_with_tehas_repo_bad_is_78_and_calls_and_writes_nothing: "tehas order set --issues 5", "TEHAS_REPO=bad", "TEHAS_REPO is not ";
    tehas_order_set_issues_5_with_tehas_repo_is_78_and_calls_and_writes_nothing: "tehas order set --issues 5", "TEHAS_REPO=", "TEHAS_REPO is not ";
    tehas_order_set_issues_5_with_tehas_control_issue_x_is_78_and_calls_and_writes_nothing: "tehas order set --issues 5", "TEHAS_CONTROL_ISSUE=x", "TEHAS_CONTROL_ISSUE is not ";
    tehas_order_set_issues_5_with_tehas_allowed_labelers_alice_bob_is_78_and_calls_and_writes_nothing: "tehas order set --issues 5", "TEHAS_ALLOWED_LABELERS='alice  bob'", "TEHAS_ALLOWED_LABELERS is not ";
    tehas_order_set_issues_5_with_tehas_allowed_labelers_is_78_and_calls_and_writes_nothing: "tehas order set --issues 5", "TEHAS_ALLOWED_LABELERS=", "TEHAS_ALLOWED_LABELERS is not ";
    tehas_order_clear_with_tehas_repo_bad_is_78_and_calls_and_writes_nothing: "tehas order clear", "TEHAS_REPO=bad", "TEHAS_REPO is not ";
    tehas_order_clear_with_tehas_repo_is_78_and_calls_and_writes_nothing: "tehas order clear", "TEHAS_REPO=", "TEHAS_REPO is not ";
    tehas_order_clear_with_tehas_control_issue_x_is_78_and_calls_and_writes_nothing: "tehas order clear", "TEHAS_CONTROL_ISSUE=x", "TEHAS_CONTROL_ISSUE is not ";
    tehas_order_clear_with_tehas_allowed_labelers_alice_bob_is_78_and_calls_and_writes_nothing: "tehas order clear", "TEHAS_ALLOWED_LABELERS='alice  bob'", "TEHAS_ALLOWED_LABELERS is not ";
    tehas_order_clear_with_tehas_allowed_labelers_is_78_and_calls_and_writes_nothing: "tehas order clear", "TEHAS_ALLOWED_LABELERS=", "TEHAS_ALLOWED_LABELERS is not ";
    tehas_panel_set_chat_label_chat_url_https_chat_example_org_with_tehas_repo_bad_is_78_and_calls_and_writes_nothing: "tehas panel set chat --label Chat --url https://chat.example.org", "TEHAS_REPO=bad", "TEHAS_REPO is not ";
    tehas_panel_set_chat_label_chat_url_https_chat_example_org_with_tehas_repo_is_78_and_calls_and_writes_nothing: "tehas panel set chat --label Chat --url https://chat.example.org", "TEHAS_REPO=", "TEHAS_REPO is not ";
    tehas_panel_set_chat_label_chat_url_https_chat_example_org_with_tehas_control_issue_x_is_78_and_calls_and_writes_nothing: "tehas panel set chat --label Chat --url https://chat.example.org", "TEHAS_CONTROL_ISSUE=x", "TEHAS_CONTROL_ISSUE is not ";
    tehas_panel_set_chat_label_chat_url_https_chat_example_org_with_tehas_allowed_labelers_alice_bob_is_78_and_calls_and_writes_nothing: "tehas panel set chat --label Chat --url https://chat.example.org", "TEHAS_ALLOWED_LABELERS='alice  bob'", "TEHAS_ALLOWED_LABELERS is not ";
    tehas_panel_set_chat_label_chat_url_https_chat_example_org_with_tehas_allowed_labelers_is_78_and_calls_and_writes_nothing: "tehas panel set chat --label Chat --url https://chat.example.org", "TEHAS_ALLOWED_LABELERS=", "TEHAS_ALLOWED_LABELERS is not ";
    tehas_host_message_set_up_level_info_text_hi_with_tehas_repo_bad_is_78_and_calls_and_writes_nothing: "tehas host-message set up --level info --text hi", "TEHAS_REPO=bad", "TEHAS_REPO is not ";
    tehas_host_message_set_up_level_info_text_hi_with_tehas_repo_is_78_and_calls_and_writes_nothing: "tehas host-message set up --level info --text hi", "TEHAS_REPO=", "TEHAS_REPO is not ";
    tehas_host_message_set_up_level_info_text_hi_with_tehas_control_issue_x_is_78_and_calls_and_writes_nothing: "tehas host-message set up --level info --text hi", "TEHAS_CONTROL_ISSUE=x", "TEHAS_CONTROL_ISSUE is not ";
    tehas_host_message_set_up_level_info_text_hi_with_tehas_allowed_labelers_alice_bob_is_78_and_calls_and_writes_nothing: "tehas host-message set up --level info --text hi", "TEHAS_ALLOWED_LABELERS='alice  bob'", "TEHAS_ALLOWED_LABELERS is not ";
    tehas_host_message_set_up_level_info_text_hi_with_tehas_allowed_labelers_is_78_and_calls_and_writes_nothing: "tehas host-message set up --level info --text hi", "TEHAS_ALLOWED_LABELERS=", "TEHAS_ALLOWED_LABELERS is not ";
    tehas_freeze_5_with_tehas_repo_bad_is_78_and_calls_and_writes_nothing: "tehas freeze 5", "TEHAS_REPO=bad", "TEHAS_REPO is not ";
    tehas_freeze_5_with_tehas_repo_is_78_and_calls_and_writes_nothing: "tehas freeze 5", "TEHAS_REPO=", "TEHAS_REPO is not ";
    tehas_freeze_5_with_tehas_control_issue_x_is_78_and_calls_and_writes_nothing: "tehas freeze 5", "TEHAS_CONTROL_ISSUE=x", "TEHAS_CONTROL_ISSUE is not ";
    tehas_freeze_5_with_tehas_allowed_labelers_alice_bob_is_78_and_calls_and_writes_nothing: "tehas freeze 5", "TEHAS_ALLOWED_LABELERS='alice  bob'", "TEHAS_ALLOWED_LABELERS is not ";
    tehas_freeze_5_with_tehas_allowed_labelers_is_78_and_calls_and_writes_nothing: "tehas freeze 5", "TEHAS_ALLOWED_LABELERS=", "TEHAS_ALLOWED_LABELERS is not ";
    tehas_thaw_5_with_tehas_repo_bad_is_78_and_calls_and_writes_nothing: "tehas thaw 5", "TEHAS_REPO=bad", "TEHAS_REPO is not ";
    tehas_thaw_5_with_tehas_repo_is_78_and_calls_and_writes_nothing: "tehas thaw 5", "TEHAS_REPO=", "TEHAS_REPO is not ";
    tehas_thaw_5_with_tehas_control_issue_x_is_78_and_calls_and_writes_nothing: "tehas thaw 5", "TEHAS_CONTROL_ISSUE=x", "TEHAS_CONTROL_ISSUE is not ";
    tehas_thaw_5_with_tehas_allowed_labelers_alice_bob_is_78_and_calls_and_writes_nothing: "tehas thaw 5", "TEHAS_ALLOWED_LABELERS='alice  bob'", "TEHAS_ALLOWED_LABELERS is not ";
    tehas_thaw_5_with_tehas_allowed_labelers_is_78_and_calls_and_writes_nothing: "tehas thaw 5", "TEHAS_ALLOWED_LABELERS=", "TEHAS_ALLOWED_LABELERS is not ";
    tehas_pause_with_tehas_repo_bad_is_78_and_calls_and_writes_nothing: "tehas pause", "TEHAS_REPO=bad", "TEHAS_REPO is not ";
    tehas_pause_with_tehas_repo_is_78_and_calls_and_writes_nothing: "tehas pause", "TEHAS_REPO=", "TEHAS_REPO is not ";
    tehas_pause_with_tehas_control_issue_x_is_78_and_calls_and_writes_nothing: "tehas pause", "TEHAS_CONTROL_ISSUE=x", "TEHAS_CONTROL_ISSUE is not ";
    tehas_pause_with_tehas_allowed_labelers_alice_bob_is_78_and_calls_and_writes_nothing: "tehas pause", "TEHAS_ALLOWED_LABELERS='alice  bob'", "TEHAS_ALLOWED_LABELERS is not ";
    tehas_pause_with_tehas_allowed_labelers_is_78_and_calls_and_writes_nothing: "tehas pause", "TEHAS_ALLOWED_LABELERS=", "TEHAS_ALLOWED_LABELERS is not ";
    tehas_drain_with_tehas_repo_bad_is_78_and_calls_and_writes_nothing: "tehas drain", "TEHAS_REPO=bad", "TEHAS_REPO is not ";
    tehas_drain_with_tehas_repo_is_78_and_calls_and_writes_nothing: "tehas drain", "TEHAS_REPO=", "TEHAS_REPO is not ";
    tehas_drain_with_tehas_control_issue_x_is_78_and_calls_and_writes_nothing: "tehas drain", "TEHAS_CONTROL_ISSUE=x", "TEHAS_CONTROL_ISSUE is not ";
    tehas_drain_with_tehas_allowed_labelers_alice_bob_is_78_and_calls_and_writes_nothing: "tehas drain", "TEHAS_ALLOWED_LABELERS='alice  bob'", "TEHAS_ALLOWED_LABELERS is not ";
    tehas_drain_with_tehas_allowed_labelers_is_78_and_calls_and_writes_nothing: "tehas drain", "TEHAS_ALLOWED_LABELERS=", "TEHAS_ALLOWED_LABELERS is not ";
});

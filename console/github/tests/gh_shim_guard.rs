//! The gh shim's write guard, pinned: the lines and refusals that
//! `console gh` leaves today, which host/shim/gh and the host depend on
//! (TF-B52 plan PR 14). The guard decides which call is a write, what one
//! line the write leaves in the log, and what a read-only host says when it
//! refuses. The move of the shim into the GitHub adapter must change none of
//! what is said here, not one byte.
//!
//! The real gh is a stub that keeps each call's words on a line of a file
//! and answers from its environment, or from the answer script of its call
//! number when a case writes one: that is how a retry case answers twice.
//! TEHAS_GH_RETRY=0 keeps a case that does not test the retry from waiting,
//! and TEHAS_GH_RETRY_WAIT=0 keeps the retry case from waiting.
//!
//! The cases run the `console` of this build as `console gh <real gh> <log>
//! [words]`, the way host/shim/gh execs it: the guard is pinned where the
//! host's calls meet it, above whichever module of which crate runs it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Once;
use tehas_core::home::{Home, State};
use tehas_core::testdir::TestDir;
use tehas_core::testing::write_exe;

/// The `console` of this build, built when the test's own build did not
/// build it: the shape of `console/tests/common/mod.rs`'s helper, for a
/// test of the adapter that execs the program the host's shim execs.
fn console_bin() -> PathBuf {
    static BUILT: Once = Once::new();
    let above = || {
        let exe = std::env::current_exe().unwrap();
        exe.ancestors()
            .skip(1)
            .map(|d| d.join("console"))
            .find(|p| p.is_file())
    };
    if let Some(found) = above() {
        return found;
    }
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
    above().expect("the console binary after its build")
}

/// The console's subcommand under test.
const GH: &str = "gh";
/// The usage line, as the shim says it.
const USAGE: &str = "usage: console gh <real gh> <log> [words]";
/// A rate limit as gh's error stream says it, and a refusal as GitHub says
/// it, each named once here.
const LIMITED: &str = "HTTP 429: Too Many Requests";
const FORBIDDEN: &str = "HTTP 403: rate limit exceeded";
/// The words a read-only host says for a refused write and for a command it
/// does not know, each named once here.
const WOULD_WRITE: &str = "read-only: would gh issue comment o/r#7";
const WOULD_UNKNOWN: &str = "read-only: would gh workflow run";

struct Shim {
    t: TestDir,
    /// The stub that stands for the real gh.
    gh: PathBuf,
    /// The write log, as `console gh` is given it.
    log: PathBuf,
    /// The stub's own call count and its answers, named to it in the
    /// environment of each run.
    n: PathBuf,
    calls: PathBuf,
    answers: PathBuf,
}

impl Shim {
    fn new(name: &str) -> Shim {
        let t = TestDir::new(&format!("gh-shim-guard-{name}"));
        let s = Shim {
            gh: t.join("real-gh"),
            log: t.join("writes.log"),
            n: t.join("n"),
            calls: t.join("calls"),
            answers: t.join("answers"),
            t,
        };
        fs::create_dir_all(&s.answers).unwrap();
        write_exe(
            &s.gh,
            r#"#!/usr/bin/env bash
n=$(($(cat "$N" 2>/dev/null || echo 0) + 1)); echo "$n" > "$N"
{ for a in "$@"; do printf "%s " "$a"; done; printf "\n"; } >> "$CALLS"
if [ -f "$A/$n" ]; then . "$A/$n"; exit $?; fi
[ -z "${GH_OUT:-}" ] || printf "%s\n" "$GH_OUT"
[ -z "${GH_ERR:-}" ] || printf "%s\n" "$GH_ERR" >&2
exit "${GH_RC:-0}"
"#,
        )
        .unwrap();
        s
    }

    /// The answer of the nth call, as a script the stub reads: it prints
    /// what it is told and ends with the status it is told.
    fn answer(&self, call: u32, out: &str, err: &str, rc: u8) {
        let mut body = String::new();
        if !out.is_empty() {
            body += &format!("printf '%s\\n' \"{out}\"\n");
        }
        if !err.is_empty() {
            body += &format!("printf '%s\\n' \"{err}\" >&2\n");
        }
        body += &format!("exit {rc}\n");
        fs::write(self.answers.join(call.to_string()), body).unwrap();
    }

    /// `console gh <words>`, with nothing added: for a run that must end on
    /// the usage line before the shim takes a log path and a gh.
    fn run_plain(&self, words: &[&str]) -> Output {
        Command::new(console_bin())
            .arg(GH)
            .args(words)
            .env("N", &self.n)
            .env("CALLS", &self.calls)
            .env("A", &self.answers)
            .env_remove("TEHAS_READ_ONLY")
            .env_remove("TEHAS_GH_RETRY")
            .env_remove("TEHAS_GH_RETRY_WAIT")
            .env_remove("TEHAS_HOME")
            .output()
            .unwrap()
    }

    /// `console gh <stub> <log> <words>`, with the settings each case names
    /// and the ones no case may inherit cleared.
    fn run(&self, words: &[&str], envs: &[(&str, &str)]) -> Output {
        Command::new(console_bin())
            .arg(GH)
            .arg(&self.gh)
            .arg(&self.log)
            .args(words)
            .env("N", &self.n)
            .env("CALLS", &self.calls)
            .env("A", &self.answers)
            .env_remove("TEHAS_READ_ONLY")
            .env_remove("TEHAS_GH_RETRY")
            .env_remove("TEHAS_GH_RETRY_WAIT")
            .env_remove("TEHAS_HOME")
            .envs(envs.iter().map(|(k, v)| (k, *v)))
            .output()
            .unwrap()
    }

    /// The lines of the write log, as it stands.
    fn log_lines(&self) -> Vec<String> {
        fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// What the stub was called with, one call per line; none when it ran
    /// not at all.
    fn calls(&self) -> Vec<String> {
        fs::read_to_string(&self.calls)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }
}

#[test]
fn too_few_words_end_64_with_the_usage_line() {
    let s = Shim::new("usage");
    for words in [&[][..], &["one-word"][..]] {
        let o = s.run_plain(words);
        assert_eq!(o.status.code(), Some(64), "{words:?}");
        assert_eq!(
            String::from_utf8_lossy(&o.stderr),
            format!("{USAGE}\n"),
            "{words:?}"
        );
    }
    assert_eq!(s.calls().len(), 0);
    assert!(!s.log.exists());
}

#[test]
fn a_read_runs_the_real_gh_at_once_and_leaves_no_line() {
    let s = Shim::new("read");
    let o = s.run(
        &["pr", "view", "7"],
        &[("GH_OUT", "the issue"), ("GH_RC", "3")],
    );
    assert_eq!(o.status.code(), Some(3));
    assert_eq!(String::from_utf8_lossy(&o.stdout), "the issue\n");
    assert_eq!(s.calls(), ["pr view 7 ".to_string()]);
    assert!(!s.log.exists());
}

#[test]
fn a_write_leaves_one_line_of_six_one_word_fields() {
    let s = Shim::new("line");
    let o = s.run(
        &["issue", "comment", "12", "-R", "o/r", "--body", "secret"],
        &[("TEHAS_GH_RETRY", "0"), ("GH_OUT", "ok"), ("GH_RC", "1")],
    );
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&o.stdout), "ok\n");
    assert_eq!(
        s.calls(),
        ["issue comment 12 -R o/r --body secret ".to_string()]
    );
    let lines = s.log_lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    let f: Vec<&str> = lines[0].split('\t').collect();
    assert_eq!(f.len(), 6, "{lines:?}");
    assert!(
        tehas_core::time::from_iso(f[0]).is_some(),
        "the time is the log's own shape: {}",
        f[0]
    );
    assert_eq!(&f[1..], &["issue comment", "o/r", "12", "1", "-"][..]);
}

#[test]
fn a_write_gh_refused_for_a_rate_limit_is_marked_403() {
    let s = Shim::new("marker");
    let o = s.run(
        &["issue", "comment", "12", "-R", "o/r", "--body", "x"],
        &[
            ("TEHAS_GH_RETRY", "0"),
            ("GH_ERR", FORBIDDEN),
            ("GH_RC", "1"),
        ],
    );
    assert_eq!(o.status.code(), Some(1));
    let lines = s.log_lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    let f: Vec<&str> = lines[0].split('\t').collect();
    assert_eq!(&f[1..], &["issue comment", "o/r", "12", "1", "403"][..]);
}

#[test]
fn a_read_only_host_refuses_the_write_and_says_what_it_would_have_been() {
    let s = Shim::new("refused");
    let home = s.t.join("home");
    let o = s.run(
        &["issue", "comment", "7", "-R", "o/r"],
        &[
            ("TEHAS_READ_ONLY", "1"),
            ("TEHAS_HOME", home.to_str().unwrap()),
        ],
    );
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&o.stderr),
        format!("{WOULD_WRITE}\n")
    );
    // Nothing reached gh, and nothing reached the write log: the log counts
    // what reached GitHub.
    assert_eq!(s.calls().len(), 0);
    assert!(!s.log.exists());
    let kept = fs::read_to_string(Home::new(&home).state(State::ReadOnlyLog)).unwrap();
    let l: Vec<&str> = kept.trim_end().split('\t').collect();
    assert_eq!(l.len(), 2, "{kept:?}");
    assert!(tehas_core::time::from_iso(l[0]).is_some(), "{kept:?}");
    assert_eq!(l[1], WOULD_WRITE);
}

#[test]
fn a_read_only_host_refuses_a_command_it_does_not_know_by_its_first_words() {
    let s = Shim::new("unknown");
    let o = s.run(&["workflow", "run", "ci.yml"], &[("TEHAS_READ_ONLY", "1")]);
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&o.stderr),
        format!("{WOULD_UNKNOWN}\n")
    );
    assert_eq!(s.calls().len(), 0);
    assert!(!s.log.exists());
}

#[test]
fn a_read_only_host_runs_the_reads_it_knows() {
    let s = Shim::new("read-only-read");
    let o = s.run(
        &["pr", "view", "7"],
        &[("TEHAS_READ_ONLY", "1"), ("GH_OUT", "the issue")],
    );
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&o.stdout), "the issue\n");
    assert_eq!(String::from_utf8_lossy(&o.stderr), "");
    assert_eq!(s.calls(), ["pr view 7 ".to_string()]);
    assert!(!s.log.exists());
}

#[test]
fn a_real_gh_that_cannot_be_run_ends_127() {
    let s = Shim::new("missing");
    let gone = s.t.join("no-such-gh");
    let o = Command::new(console_bin())
        .arg(GH)
        .arg(&gone)
        .arg(&s.log)
        .args(["pr", "view", "1"])
        .env_remove("TEHAS_READ_ONLY")
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(127));
    let err = String::from_utf8_lossy(&o.stderr).to_string();
    assert!(
        err.starts_with(&format!("gh: {}: ", gone.display())),
        "{err}"
    );
}

#[test]
fn a_write_refused_for_a_rate_limit_is_sent_once_more_and_answered_once() {
    let s = Shim::new("retry");
    // The first answer prints what only it prints: the shim must hold that
    // back, since only the last attempt's answer reaches the caller.
    s.answer(1, "the first answer", LIMITED, 1);
    s.answer(2, "created", "", 0);
    let o = s.run(
        &["pr", "comment", "8", "-R", "o/r", "--body", "hi"],
        &[("TEHAS_GH_RETRY_WAIT", "0")],
    );
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(s.calls().len(), 2);
    let out = String::from_utf8_lossy(&o.stdout).to_string();
    assert_eq!(out, "created\n");
    assert!(!out.contains("the first answer"), "{out}");
    let err = String::from_utf8_lossy(&o.stderr).to_string();
    assert_eq!(err, "gh: rate limited, trying once more in 0 s\n");
    let lines = s.log_lines();
    assert_eq!(lines.len(), 2, "{lines:?}");
    let first: Vec<&str> = lines[0].split('\t').collect();
    assert_eq!(&first[1..], &["pr comment", "o/r", "8", "1", "429"][..]);
    let second: Vec<&str> = lines[1].split('\t').collect();
    assert_eq!(&second[1..], &["pr comment", "o/r", "8", "0", "-"][..]);
}

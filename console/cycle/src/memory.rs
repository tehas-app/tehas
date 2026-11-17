//! The factory's vipune memory, behind the issue relay (#2362, behind the
//! outsource flag). A cycle's container calls `vipune`; the stand-in in the
//! image (host/sandbox/vipune-shim) leaves the call in the relay directory as a
//! request of kind `vipune`, and the factory runs the pinned vipune for it with
//! the cycle's own database. In-house the host's relay answers it
//! ([`crate::relay`]); on a contractor the cycle's relay sends it to the
//! factory over the job channel's relay route, which answers it the same way.
//!
//! A request is read as hostile, as every relay request is:
//!   - the verb is one of [`VERBS`]; `--db-path` in any spelling is refused,
//!     and so are arguments of more than [`MAX_ARGS`] bytes;
//!   - a `-p`/`--project` of the caller is dropped, not refused: the project is
//!     always `TEHAS_REPO`, given as `--project` before the verb;
//!   - of the variables only `VIPUNE_RECENCY_WEIGHT` (a decimal number) and
//!     `VIPUNE_PROJECT` are taken, and the project is replaced the same way.
//!
//! A refusal answers as vipune answers a usage error: [`USAGE_EXIT`] with
//! nothing on stdout, so pi-rukas reads it as it reads vipune's own.
//!
//! Which database a call uses comes from the relay directory or the job it
//! arrived with, never from the request: `issue-<n>` for an in-house cycle and
//! `job-<id>` for a contractor's, as `<name>.db` under `state/memory/cycles`.
//! A cycle's database starts empty and goes when the cycle ends, as the
//! container's own store did; keeping what a cycle learnt is #2366 and #2367.
//! It holds at most [`MAX_DB`] bytes: an `add` beyond that fails with 1.
//!
//! The container of a call runs as the factory's user with vscode's home, as
//! every container of the image does, within [`LIMITS`].

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The status the pinned vipune ([`PINNED`]) exits with on a usage error.
pub const USAGE_EXIT: i32 = 64;

/// The vipune version [`USAGE_EXIT`] was read from. A test holds it to
/// host/versions.env, so a new pin asks for the status to be read again.
pub const PINNED: &str = "0.20.3";

/// The verbs the factory runs.
pub const VERBS: &[&str] = &["add", "search", "get", "list", "validate", "version"];

/// The most bytes the arguments of one call may hold.
pub const MAX_ARGS: usize = 16 * 1024;

/// The variables a request may carry.
pub const VARS: &[&str] = &["VIPUNE_RECENCY_WEIGHT", "VIPUNE_PROJECT"];

/// The file the host leaves in a cycle's relay directory while the factory's
/// memory answers: the stand-in sends its calls only when it is there.
pub const MARK: &str = "vipune";

/// Where the real vipune is in the image: the stand-in takes its old path.
pub const REAL: &str = "/usr/local/lib/tehas/vipune";

/// How long one vipune call may take, before the 5 s docker is given to
/// stop. The stand-in waits 28 s.
pub const WAIT: u64 = 15;

/// The most bytes a cycle's database may hold, with its write-ahead log: an
/// `add` beyond it is refused. A job's database lives on the factory, and a
/// contractor's cycle could otherwise fill its disk one call at a time.
pub const MAX_DB: u64 = 64 << 20;

/// The limits of the container one call runs in.
pub const LIMITS: &[&str] = &[
    "--memory=2g",
    "--memory-swap=2g",
    "--pids-limit=256",
    "--cpus=2",
];

fn no(why: &str) -> String {
    format!("vipune: the factory's memory {why}\n")
}

/// What runs for a request that passes: the arguments without the caller's
/// project, the project and the variables vipune is given.
#[derive(Debug, PartialEq, Eq)]
pub struct Planned {
    pub args: Vec<String>,
    pub vars: Vec<(String, String)>,
}

/// The call a request makes, or why it is refused. `args` start after
/// `vipune`; `vars` are the request's variables.
pub fn plan(args: &[String], vars: &[(String, String)], repo: &str) -> Result<Planned, String> {
    if !crate::relay::repo_shaped(repo) {
        return Err(no("has no TEHAS_REPO of the form <owner>/<name>"));
    }
    let size: usize = args.iter().map(|a| a.len() + 1).sum();
    if size > MAX_ARGS {
        return Err(no(&format!(
            "takes arguments of at most {} KiB",
            MAX_ARGS / 1024
        )));
    }
    let mut out = Vec::with_capacity(args.len());
    let mut verb: Option<&str> = None;
    let mut rest = args.iter();
    while let Some(a) = rest.next() {
        if a == "--" {
            if verb.is_none() {
                return Err(no("takes a verb before --"));
            }
            out.push(a.clone());
            out.extend(rest.cloned());
            break;
        }
        if a == "--db-path" || a.starts_with("--db-path=") {
            return Err(no("chooses the database itself: --db-path is refused"));
        }
        if a == "-p" || a == "--project" {
            rest.next();
            continue;
        }
        if a.starts_with("--project=") || (a.starts_with("-p") && !a.starts_with("--")) {
            continue;
        }
        if verb.is_none() && !a.starts_with('-') {
            if !VERBS.contains(&a.as_str()) {
                return Err(no(&format!("runs {} only", VERBS.join(", "))));
            }
            verb = Some(a);
        }
        out.push(a.clone());
    }
    if verb.is_none() {
        return Err(no(&format!("runs {} only", VERBS.join(", "))));
    }
    let mut weight = None;
    for (name, value) in vars {
        match name.as_str() {
            "VIPUNE_PROJECT" => {}
            "VIPUNE_RECENCY_WEIGHT" if decimal(value) => weight = Some(value.clone()),
            "VIPUNE_RECENCY_WEIGHT" => {
                return Err(no("takes a VIPUNE_RECENCY_WEIGHT that is a decimal number"))
            }
            _ => return Err(no(&format!("takes the variables {} only", VARS.join(", ")))),
        }
    }
    let mut planned = vec![(String::from("VIPUNE_PROJECT"), repo.to_string())];
    planned.extend(weight.map(|w| (String::from("VIPUNE_RECENCY_WEIGHT"), w)));
    Ok(Planned {
        args: out,
        vars: planned,
    })
}

fn decimal(v: &str) -> bool {
    let (whole, part) = v.split_once('.').unwrap_or((v, ""));
    !v.is_empty()
        && v.len() <= 16
        && !whole.is_empty()
        && whole.bytes().all(|b| b.is_ascii_digit())
        && part.bytes().all(|b| b.is_ascii_digit())
}

/// A request's variables from the relay's `env` file: `NAME=VALUE`, each
/// ended by a NUL.
pub fn vars_of(text: &str) -> Result<Vec<(String, String)>, String> {
    let Some(text) = text.strip_suffix('\0').or(text.is_empty().then_some("")) else {
        return Err(no("takes variables each ended by a NUL"));
    };
    if text.is_empty() {
        return Ok(Vec::new());
    }
    text.split('\0')
        .map(|line| {
            line.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .ok_or_else(|| no("takes variables of the form NAME=VALUE"))
        })
        .collect()
}

/// The name of a cycle's database: from the issue of an in-house cycle, or
/// from the job of a contractor's.
pub fn of_issue(issue: u64) -> String {
    format!("issue-{issue}")
}

pub fn of_job(job: i64) -> String {
    format!("job-{job}")
}

/// Where vipune runs for the factory.
pub trait Vipune {
    /// One planned call for the cycle database `cycle`: vipune's stdout,
    /// stderr and status, unchanged.
    fn call(&self, cycle: &str, planned: &Planned, repo: &str) -> (String, String, i32);
    /// The cycle has ended: its database goes.
    fn forget(&self, cycle: &str);
}

/// The pinned vipune of the sandbox image, in a container of its own with no
/// network and only the cycles' databases mounted.
pub struct InImage {
    pub docker: PathBuf,
    pub image: String,
    /// `<uid>:<gid>`, as every container of the image runs.
    pub user: String,
    /// `state/memory/cycles`.
    pub dir: PathBuf,
}

impl InImage {
    /// The host's: docker from PATH, the image of `PI_ENSEMBLE_IMAGE`.
    pub fn of_home(home: &Path, image: &str) -> InImage {
        InImage {
            docker: PathBuf::from("docker"),
            image: image.to_string(),
            user: tehas_core::sandbox::user(),
            dir: home.join("state/memory/cycles"),
        }
    }

    /// The arguments of docker for one call, after `docker`.
    pub fn argv(&self, cycle: &str, planned: &Planned, repo: &str) -> Vec<String> {
        let mut argv: Vec<String> = [
            "run",
            "--rm",
            "--network=none",
            "--cap-drop=ALL",
            "--security-opt=no-new-privileges",
        ]
        .iter()
        .chain(LIMITS)
        .map(|s| s.to_string())
        .collect();
        // The image ends as root, which without capabilities cannot write the
        // directory, and the embedding model is in vscode's home.
        argv.push(format!("--user={}", self.user));
        argv.push("--env=HOME=/home/vscode".to_string());
        argv.push(format!("--volume={}:/memory", self.dir.display()));
        argv.push("--env=HF_HUB_OFFLINE=1".to_string());
        for (k, v) in &planned.vars {
            argv.push(format!("--env={k}={v}"));
        }
        argv.push(format!("--entrypoint={REAL}"));
        argv.push(self.image.clone());
        argv.push(format!("--db-path=/memory/{cycle}.db"));
        argv.push(format!("--project={repo}"));
        argv.extend(planned.args.iter().cloned());
        argv
    }
}

impl Vipune for InImage {
    fn call(&self, cycle: &str, planned: &Planned, repo: &str) -> (String, String, i32) {
        if self.image.is_empty() {
            return (String::new(), no("has no sandbox image"), 1);
        }
        if std::fs::create_dir_all(&self.dir).is_err() {
            return (String::new(), no("has no directory for its databases"), 1);
        }
        if full(&self.dir, cycle, planned) {
            return (
                String::new(),
                no(&format!("of this cycle is full ({} MiB)", MAX_DB >> 20)),
                1,
            );
        }
        let out = Command::new("timeout")
            .args(["-k", "5", &WAIT.to_string()])
            .arg(&self.docker)
            .args(self.argv(cycle, planned, repo))
            .stdin(Stdio::null())
            .output();
        let Ok(out) = out else {
            return (String::new(), no("could not start vipune"), 1);
        };
        let mut err = String::from_utf8_lossy(&out.stderr).into_owned();
        let rc = out.status.code().unwrap_or(1);
        if rc == 124 {
            err.push_str(&no(&format!("stopped vipune after {WAIT} s")));
        }
        (String::from_utf8_lossy(&out.stdout).into_owned(), err, rc)
    }

    fn forget(&self, cycle: &str) {
        forget_in(&self.dir, cycle);
    }
}

/// Whether `planned` is an `add` and the database of `cycle` in `dir` holds
/// [`MAX_DB`] bytes or more.
pub fn full(dir: &Path, cycle: &str, planned: &Planned) -> bool {
    let adds = planned
        .args
        .iter()
        .take_while(|a| *a != "--")
        .find(|a| !a.starts_with('-'))
        .is_some_and(|v| v == "add");
    let size: u64 = ["", "-wal"]
        .iter()
        .filter_map(|end| std::fs::symlink_metadata(dir.join(format!("{cycle}.db{end}"))).ok())
        .map(|m| m.len())
        .sum();
    adds && size >= MAX_DB
}

/// The database of `cycle` in `dir` gone, with the files SQLite keeps beside it.
pub fn forget_in(dir: &Path, cycle: &str) {
    for end in ["", "-wal", "-shm", "-journal"] {
        let _ = std::fs::remove_file(dir.join(format!("{cycle}.db{end}")));
    }
}

/// One request of kind `vipune`, planned and run: vipune's answer, or the
/// refusal answered as a usage error.
pub fn answer(
    args: &[String],
    vars: &[(String, String)],
    repo: &str,
    run: &dyn Fn(&Planned) -> (String, String, i32),
) -> (String, String, i32) {
    match plan(args, vars, repo) {
        Ok(planned) => run(&planned),
        Err(why) => {
            eprintln!("tehas relay: refused a vipune call: {}", why.trim_end());
            (String::new(), why, USAGE_EXIT)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPO: &str = "acme/widget";

    fn args(line: &[&str]) -> Vec<String> {
        line.iter().map(|s| s.to_string()).collect()
    }

    fn vars(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn the_usage_exit_is_the_pinned_vipunes() {
        let pins = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../host/versions.env"),
        )
        .unwrap();
        let pinned = pins
            .lines()
            .find_map(|l| l.strip_prefix("VIPUNE_VERSION="))
            .unwrap();
        // A new pin: read its usage exit again (`vipune frob; echo $?`), then
        // move PINNED and USAGE_EXIT together.
        assert_eq!(pinned, PINNED);
        assert_eq!(USAGE_EXIT, 64);
    }

    #[test]
    fn the_allowed_verbs_run_with_the_project_set() {
        for verb in VERBS {
            let p = plan(&args(&[verb, "x"]), &[], REPO).unwrap();
            assert_eq!(p.args, args(&[verb, "x"]));
            assert_eq!(p.vars, vars(&[("VIPUNE_PROJECT", REPO)]));
        }
        let p = plan(
            &args(&["--json", "search", "the cache", "--limit", "5"]),
            &vars(&[("VIPUNE_RECENCY_WEIGHT", "0.3"), ("VIPUNE_PROJECT", "x/y")]),
            REPO,
        )
        .unwrap();
        assert_eq!(
            p.args,
            args(&["--json", "search", "the cache", "--limit", "5"])
        );
        assert_eq!(
            p.vars,
            vars(&[("VIPUNE_PROJECT", REPO), ("VIPUNE_RECENCY_WEIGHT", "0.3")])
        );
    }

    #[test]
    fn every_other_verb_is_refused() {
        for line in [
            &["delete", "x"][..],
            &["update", "x"],
            &["import", "-"],
            &["export", "/tmp/x"],
            &["mcp"],
            &["backup", "x"],
            &["hook"],
            &["--json"],
            &[],
            &["--", "add", "x"],
        ] {
            let why = plan(&args(line), &[], REPO).unwrap_err();
            assert!(
                why.starts_with("vipune: the factory's memory"),
                "{line:?}: {why}"
            );
        }
    }

    #[test]
    fn a_db_path_from_the_caller_is_refused() {
        for line in [
            &["add", "x", "--db-path", "/etc/x"][..],
            &["--db-path=/x", "list"],
            &["search", "q", "--db-path=/x"],
        ] {
            let why = plan(&args(line), &[], REPO).unwrap_err();
            assert!(why.contains("--db-path"), "{line:?}: {why}");
        }
        // After --, it is the memory's text.
        let p = plan(&args(&["add", "--", "--db-path"]), &[], REPO).unwrap();
        assert_eq!(p.args, args(&["add", "--", "--db-path"]));
    }

    #[test]
    fn a_callers_project_is_replaced_not_refused() {
        for line in [
            &["-p", "someone/else", "list"][..],
            &["list", "--project", "someone/else"],
            &["list", "--project=someone/else"],
            &["list", "-psomeone/else"],
        ] {
            let p = plan(&args(line), &[], REPO).unwrap();
            assert_eq!(p.args, args(&["list"]), "{line:?}");
        }
        let p = plan(&args(&["add", "--", "-p", "x"]), &[], REPO).unwrap();
        assert_eq!(p.args, args(&["add", "--", "-p", "x"]));
    }

    #[test]
    fn arguments_over_sixteen_kib_are_refused() {
        let big = "x".repeat(MAX_ARGS);
        let why = plan(&args(&["add", &big]), &[], REPO).unwrap_err();
        assert!(why.contains("16 KiB"), "{why}");
        let fits = "x".repeat(MAX_ARGS - 10);
        assert!(plan(&args(&["add", &fits]), &[], REPO).is_ok());
    }

    #[test]
    fn only_the_two_variables_are_taken() {
        for v in [
            vars(&[("PATH", "/x")]),
            vars(&[("VIPUNE_RECENCY_WEIGHT", "1; rm")]),
            vars(&[("VIPUNE_RECENCY_WEIGHT", "")]),
            vars(&[("LD_PRELOAD", "/x")]),
        ] {
            assert!(plan(&args(&["list"]), &v, REPO).is_err(), "{v:?}");
        }
    }

    #[test]
    fn the_env_file_is_read_as_the_relay_writes_it() {
        assert_eq!(
            vars_of("VIPUNE_PROJECT=a/b\0VIPUNE_RECENCY_WEIGHT=0.2\0"),
            Ok(vars(&[
                ("VIPUNE_PROJECT", "a/b"),
                ("VIPUNE_RECENCY_WEIGHT", "0.2")
            ]))
        );
        assert_eq!(vars_of(""), Ok(Vec::new()));
        assert!(vars_of("A=b").is_err());
        assert!(vars_of("Ab\0").is_err());
    }

    #[test]
    fn a_refusal_answers_as_a_usage_error_with_nothing_on_stdout() {
        let never = |_: &Planned| panic!("ran");
        let (out, err, rc) = answer(&args(&["delete", "x"]), &[], REPO, &never);
        assert_eq!((out.as_str(), rc), ("", USAGE_EXIT));
        assert!(err.ends_with('\n'), "{err:?}");
        let (out, err, rc) = answer(&args(&["list"]), &[], REPO, &|p: &Planned| {
            (format!("{:?}", p.args), "noise".into(), 3)
        });
        assert_eq!((out.as_str(), err.as_str(), rc), ("[\"list\"]", "noise", 3));
    }

    #[test]
    fn the_container_is_pinned() {
        let i = InImage {
            docker: "docker".into(),
            image: "tehas/pi-rukas:uid1000".into(),
            user: "1000:1000".into(),
            dir: "/h/state/memory/cycles".into(),
        };
        let p = plan(
            &args(&["search", "q"]),
            &vars(&[("VIPUNE_RECENCY_WEIGHT", "0.3")]),
            REPO,
        )
        .unwrap();
        assert_eq!(
            i.argv("job-7", &p, REPO),
            args(&[
                "run",
                "--rm",
                "--network=none",
                "--cap-drop=ALL",
                "--security-opt=no-new-privileges",
                "--memory=2g",
                "--memory-swap=2g",
                "--pids-limit=256",
                "--cpus=2",
                "--user=1000:1000",
                "--env=HOME=/home/vscode",
                "--volume=/h/state/memory/cycles:/memory",
                "--env=HF_HUB_OFFLINE=1",
                "--env=VIPUNE_PROJECT=acme/widget",
                "--env=VIPUNE_RECENCY_WEIGHT=0.3",
                "--entrypoint=/usr/local/lib/tehas/vipune",
                "tehas/pi-rukas:uid1000",
                "--db-path=/memory/job-7.db",
                "--project=acme/widget",
                "search",
                "q",
            ])
        );
    }

    #[test]
    fn an_add_to_a_full_database_is_refused_and_a_search_is_not() {
        let d = tehas_core::testdir::TestDir::new("memory-full");
        let dir = d.join("");
        let add = plan(&args(&["--json", "add", "x"]), &[], REPO).unwrap();
        let search = plan(&args(&["search", "add"]), &[], REPO).unwrap();
        assert!(!full(&dir, "job-7", &add));
        std::fs::write(d.join("job-7.db"), vec![0u8; (MAX_DB / 2) as usize]).unwrap();
        assert!(!full(&dir, "job-7", &add));
        std::fs::write(d.join("job-7.db-wal"), vec![0u8; (MAX_DB / 2) as usize]).unwrap();
        assert!(full(&dir, "job-7", &add));
        assert!(!full(&dir, "job-7", &search));
        assert!(!full(&dir, "job-8", &add));
        let i = InImage {
            docker: "/nonexistent/docker".into(),
            image: "img".into(),
            user: "1000:1000".into(),
            dir,
        };
        let (out, err, rc) = i.call("job-7", &add, REPO);
        assert_eq!((out.as_str(), rc), ("", 1));
        assert!(err.contains("full"), "{err}");
    }

    #[test]
    fn a_cycles_database_goes_with_its_side_files() {
        let d = tehas_core::testdir::TestDir::new("memory-forget");
        for f in ["job-7.db", "job-7.db-wal", "job-7.db-shm", "job-8.db"] {
            std::fs::write(d.join(f), "x").unwrap();
        }
        forget_in(&d.join(""), "job-7");
        let mut left: Vec<String> = std::fs::read_dir(d.join(""))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, ["job-8.db"]);
    }
}

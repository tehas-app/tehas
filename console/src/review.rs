//! `tehasbot review`: what the default branch gained since the last reviewed
//! commit, with pi-rukas's lens reviewer running headless in the sandbox image.
//! libexec/review loads the settings, asks whether the loop is on and starts
//! this; src/usage/review.txt says what a person is told.
//!
//!   console review main   one review, as that command's exit status says
//!   console review table  its row of the table of runs
//!
//! The review writes nothing to the tracker: a container of the review kind has
//! no token and no app directory (tehas_core::sandbox). The findings of a run
//! are filed by the store, whole (`store review-file`,
//! console/store/src/review_issue.rs): this asks for the filing and says what it
//! answered, and nothing else.

use crate::store;
use serde_json::Value;
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, Instant};
use tehas_core::models::{self, host_path};
use tehas_core::sandbox::{self, Identity, Kind, Paths};
use tehas_core::slice::{Host, Load};
use tehas_core::{mask, settings, time};

// How far back a host that never reviewed anything reads.
const DAYS: u32 = 7;
const TICK: Duration = Duration::from_millis(100);
const RUNNER: &str = "/opt/pi-rukas/bin/pi-rukas-headless";
const NAME: &str = "tehas-review";
const ASK: &str = "Apply `tehas:refine` to plan this into parts sized for one cycle.";

// The names a review's container may be given: the builder's own, by name, and
// the two settings every instance sets.
fn wanted() -> Vec<String> {
    let who = Identity {
        tree: "",
        name: "",
        email: "",
    };
    let mut names: Vec<String> = sandbox::env_options(Kind::Review, &who, &[], &[])
        .iter()
        .filter(|w| *w != "-e")
        .map(|w| w.split('=').next().unwrap_or("").to_string())
        .collect();
    names.extend(
        [
            "PI_ENSEMBLE_SUBAGENT_MODEL",
            "PI_ENSEMBLE_SUBAGENT_PROVIDER",
        ]
        .map(String::from),
    );
    names
}

pub fn main(given: &[String]) -> u8 {
    match given {
        [verb] if verb == "main" => review(),
        [verb] if verb == "table" => {
            println!("{}", table());
            0
        }
        _ => {
            eprintln!("console review: usage: main|table, see console/src/review.rs");
            2
        }
    }
}

/// The arguments of `tehasbot review`, as `console check` asks. Err is the exit
/// status: 99 when the help was printed.
pub fn words(given: &[String], usage: &str) -> Result<(), u8> {
    let unknown = |word: &str| {
        eprintln!("tehasbot review: unknown argument: {word}");
        eprint!("{usage}");
        Err(64)
    };
    match given {
        [] => Ok(()),
        [one] if one == "-h" || one == "--help" => {
            print!("{usage}");
            Err(99)
        }
        [one] if one.is_empty() => Ok(()),
        [first, second, ..] if first.is_empty() || first == "-h" || first == "--help" => {
            unknown(second)
        }
        [first, ..] => unknown(first),
    }
}

// The docker line of a review, in order.
fn docker_words(image: &str, c: &Container, since: &str) -> Vec<String> {
    let mut words: Vec<String> = ["docker", "run", "--rm", "--init", "--name", NAME]
        .map(String::from)
        .to_vec();
    words.extend_from_slice(&c.limits);
    words.extend_from_slice(&c.security);
    words.extend(["--user".to_string(), c.user.clone(), "-w".to_string()]);
    words.push(c.repo.clone());
    words.extend_from_slice(&c.mounts);
    words.extend_from_slice(&c.envs);
    words.extend_from_slice(&c.hosts);
    words.extend([image, RUNNER, "lens-review", "--since", since, "--json"].map(String::from));
    words
}

#[derive(Default)]
struct Container {
    limits: Vec<String>,
    security: Vec<String>,
    user: String,
    repo: String,
    mounts: Vec<String>,
    envs: Vec<String>,
    hosts: Vec<String>,
}

// This file's row of the table of runs, read off the docker line.
fn table() -> String {
    let image = "PI_ENSEMBLE_IMAGE";
    let words = docker_words(image, &Container::default(), "");
    let at = words.iter().position(|w| w == image);
    let entry = at.map_or("", |i| &words[i + 1]);
    format!("review\t{image}\t{entry}")
}

fn git_global(key: &str) -> String {
    Command::new("git")
        .args(["config", "--global", key])
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim_end_matches('\n')
                .to_string()
        })
        .unwrap_or_default()
}

fn account() -> String {
    Command::new("id")
        .arg("-un")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim_end().to_string())
        .unwrap_or_default()
}

// What the image's runner says it can do. Err is the status of the call, so a
// docker that does not answer is said apart from an image with no verb.
fn capabilities(image: &str) -> Result<String, i32> {
    let wait = env::var("TEHAS_CAPS_WAIT").unwrap_or_default();
    let shape = !wait.is_empty()
        && wait.len() <= 4
        && !wait.starts_with('0')
        && wait.bytes().all(|b| b.is_ascii_digit());
    let wait = if shape { wait.as_str() } else { "120" };
    let out = Command::new("timeout")
        .args(["-k", "10", wait, "docker", "run", "--rm", "--init"])
        .args([image, RUNNER, "--capabilities"])
        .stderr(Stdio::null())
        .output();
    match out {
        Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).into_owned()),
        Ok(o) => Err(exit_of(o.status)),
        Err(_) => Err(127),
    }
}

// The findings of a run, filed by the store. One run of the store says all of a
// filing, so one line is read from it: what it did, or why it did not. A store
// that did not answer says nothing here, as it never did.
fn file_findings(home: &Path, doc: &str, branch: &str, head: &str, names: &[String]) {
    let repo = env::var("TEHAS_REVIEW_REPO").unwrap_or_default();
    if repo.is_empty() {
        say(&format!(
            "TEHAS_REVIEW_REPO is not set, so the findings of {} are nowhere",
            &head[..head.len().min(7)]
        ));
        return;
    }
    let mut args = vec![
        "review-file",
        "--repo",
        &repo,
        "--doc",
        doc,
        "--branch",
        branch,
        "--commit",
        head,
        "--ask",
        ASK,
    ];
    for name in names {
        args.extend(["--private", name]);
    }
    let Ok(answer) = store::try_call(home, 3, &args) else {
        return;
    };
    let text = |k: &str| answer.get(k).and_then(Value::as_str).unwrap_or("");
    let line = match (text("said"), text("why")) {
        ("", "") => return,
        ("", why) => format!("the findings were not filed: {why}"),
        (said, _) => said.to_string(),
    };
    say(&line);
}

// Everything from the settings to the end of a run.
fn review() -> u8 {
    let var = |k: &str| env::var(k).unwrap_or_default();
    let refuse = |line: String| {
        eprintln!("{line}");
        78
    };
    if let Err(line) = sandbox::settings_ok() {
        return refuse(line);
    }
    // Docker would create a missing mount source as a root-owned directory.
    if let Err(line) = settings::checkout() {
        return refuse(line);
    }
    let branch = match settings::branch() {
        Ok(branch) => branch,
        Err(line) => return refuse(line),
    };
    // No container starts outside the slice, and none with half its limits.
    let taken = sandbox::take(&Host::here(), &Load::from_env(), "tehas review", "a review");
    let taken = match taken {
        Ok(taken) => taken,
        Err(line) => return refuse(line),
    };

    let home = var("TEHAS_HOME");
    let src = var("TEHAS_WORK_REPO");
    // The workspace inside the container, the directory on the host behind it,
    // and the run's own state beside that, not in it.
    let repo = var("TEHAS_WORK_DIR");
    let tree = format!("{repo}-review");
    let container = format!("{tree}.state");
    let image = var("PI_ENSEMBLE_IMAGE");
    let stamp = time::stamp(time::now());
    let log = format!("{home}/logs/review-{stamp}.jsonl");
    // The runner's one JSON document, beside its NDJSON progress.
    let doc = format!("{home}/logs/review-{stamp}.json");
    let runs = format!("{}/.pi/agent/ensemble-runs/review", var("HOME"));
    let record = format!("{home}/state/review.json");

    // The committer of the clone: git needs an identity for the branch it checks
    // out, and the container is handed one. The bot's is the tehas user's own,
    // and the runner commits nothing.
    let (name, email) = (git_global("user.name"), git_global("user.email"));
    if name.is_empty() || email.is_empty() {
        warn(&format!(
            "git user.name and user.email are not set for {}",
            account()
        ));
        return 64;
    }

    // The names the console hides in the two files this run leaves in logs/,
    // and the store in what it files: what a review agent could print that
    // belongs to the host and nobody else.
    let private = mask::host_names(&host_path(), &email);

    let who = Identity {
        tree: &repo,
        name: &name,
        email: &email,
    };
    let envs = sandbox::env_options(
        Kind::Review,
        &who,
        &sandbox::ensemble(Kind::Review),
        &sandbox::tokens(),
    );
    let hosts = sandbox::hosts(&var("PI_ENSEMBLE_HOST_ALIASES"));

    // A docker that does not answer is said apart from an image with no
    // lens-review verb, so nobody rebuilds an image that is fine.
    let caps = match capabilities(&image) {
        Ok(caps) => caps,
        Err(rc) => {
            return refuse(format!(
                "tehas review: {image} gave no --capabilities answer (exit {rc}), not starting"
            ))
        }
    };
    if !tehas_cycle::caps::verb(&caps, "lens-review", "--since --json") {
        return refuse(format!(
            "tehas review: {image} runs no review (its --capabilities has no verbs lens-review), not starting"
        ));
    }
    // Which PI_ENSEMBLE_* the image does not list for a host to set, and so a
    // lever of the runner this run was never vetted for.
    let refused = refused(&caps, &envs);
    if !refused.is_empty() {
        warn(&format!(
            "{image} does not take what this review would be given, not starting:"
        ));
        for name in refused {
            eprintln!("    {name}");
        }
        return 78;
    }

    // The range, read before anything is made: nothing new starts nothing.
    let (head, start, from) = match since(&src, &branch, &record) {
        Ok(range) => range,
        Err(3) => return 0,
        Err(_) => return 2,
    };

    // One review at a time: the container, the workspace and the record are one
    // per host; the home's directories are made only now.
    if fs::create_dir_all(format!("{home}/state")).is_err() {
        return refuse(format!(
            "tehas review: could not make the directories under {home}, nothing was reviewed"
        ));
    }
    let lock_file = format!("{home}/state/review.lock");
    let Ok(lock) = fs::File::create(&lock_file) else {
        return refuse(format!(
            "tehas review: could not open {lock_file}, nothing was reviewed"
        ));
    };
    if lock.try_lock().is_err() {
        say("another review is going, nothing was reviewed");
        return 0;
    }

    // A fresh workspace for this run, as a cycle's turn gets one. Only a
    // directory tehas made is ever removed.
    let works = Path::new(&tree).parent().unwrap_or(Path::new("/"));
    if works.exists() && !works.join(".tehas-workspaces").is_file() {
        return refuse(format!(
            "tehas review: {} exists and was not made by tehas, not touching it",
            works.display()
        ));
    }
    // From here the two directories are this run's. A return removes them, and
    // a signal is one at the next step.
    let signalled = Arc::new(AtomicUsize::new(0));
    for signal in [SIGTERM, SIGINT, SIGHUP] {
        let flag = Arc::clone(&signalled);
        if signal_hook::flag::register_usize(signal, flag, signal as usize).is_err() {
            warn("cannot listen for signals, not starting a review");
            return 2;
        }
    }
    let workspace = Workspace(vec![tree.clone(), container.clone()]);
    let gone = |dir: &str| fs::remove_dir_all(dir).is_ok() || !Path::new(dir).exists();
    let made = fs::create_dir_all(works).is_ok()
        && fs::write(works.join(".tehas-workspaces"), "").is_ok()
        && gone(&tree)
        && gone(&container)
        && sandbox::clone(&src, &tree, &branch);
    if !made {
        return refuse(format!(
            "tehas review: could not make the workspace {tree}, not starting a review"
        ));
    }
    if let Some(rc) = ended(&signalled) {
        return rc;
    }

    // The run's own copy of the host's code index, and a word when it is not of
    // this commit, since the search then falls back to reading files.
    let indexed = fs::read_to_string(format!("{home}/state/index.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.get("commit")?.as_str().map(str::to_string));
    if indexed.as_deref() != Some(head.as_str()) {
        say(&format!(
            "the host's code index is not of {}, so this run may search an older one",
            short(&head)
        ));
    }
    if !sandbox::seed(&container, &format!("{home}/state/index")) {
        say(&format!(
            "the code index could not be copied into {container}/cache, this run searches without one"
        ));
    }
    // The mounts name directories that only exist now.
    let paths = Paths {
        tree: &repo,
        host: &tree,
        state: &container,
        runs: &runs,
    };
    let mounts = sandbox::prepared_mounts(Kind::Review, &paths).map_err(|why| {
        if !why.is_empty() {
            eprintln!("{why}");
        }
    });
    let at = Path::new(&container);
    let models = models::write_for_container(&at.join("models.json"), &at.join("endpoints"));
    let (Ok(mounts), Some(())) = (mounts, models) else {
        return refuse(
            "tehas review: could not prepare the sandbox, not starting a review".to_string(),
        );
    };
    // The log's directory is made here; the document beside it by the run.
    if fs::create_dir_all(format!("{home}/logs")).is_err() {
        return refuse(format!(
            "tehas review: could not make {home}/logs, not starting a review"
        ));
    }
    say(&format!(
        "{branch} from {} ({from}), log {log}",
        short(&start)
    ));
    let box_of = Container {
        limits: taken.limits,
        security: taken.security,
        user: sandbox::user(),
        repo: repo.clone(),
        mounts,
        envs,
        hosts,
    };
    let (Ok(limit), Ok(stop_wait)) = (
        var("TEHAS_PLAN_TIMEOUT").parse(),
        env::var("TEHAS_STOP_WAIT")
            .unwrap_or_else(|_| "60".into())
            .parse(),
    ) else {
        warn("TEHAS_PLAN_TIMEOUT or TEHAS_STOP_WAIT is not a number of seconds");
        return 2;
    };
    // The run removes the two directories when it is over, and this holds the
    // lock while it goes. TEHAS_PLAN_TIMEOUT bounds the run, as it bounds one
    // headless turn; TEHAS_STOP_WAIT bounds each docker call and the client.
    let args = RunArgs {
        command: docker_words(&image, &box_of, &start),
        private: private.clone(),
        branch: branch.clone(),
        head: head.clone(),
        since: start,
        record,
        limit,
        stop_wait,
        name: NAME.to_string(),
        doc: doc.clone(),
        log,
        signalled: Arc::clone(&signalled),
        remove: workspace.0.clone(),
    };
    if let Some(rc) = ended(&signalled) {
        return rc;
    }
    let rc = run(&args);
    if rc == 10 || rc == 20 {
        file_findings(Path::new(&home), &doc, &branch, &head, &private);
    }
    rc
}

fn say(line: &str) {
    let _ = writeln!(io::stdout(), "tehas review: {line}");
}

fn warn(line: &str) {
    let _ = writeln!(io::stderr(), "tehas review: {line}");
}

fn short(commit: &str) -> &str {
    &commit[..commit.len().min(12)]
}

/// git's standard output, trimmed; None when git fails. What it said on
/// standard error is added to `said`.
fn git(repo: &str, args: &[&str], said: &mut String) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    said.push_str(&String::from_utf8_lossy(&out.stderr));
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The commit of a record: a full commit id, else nothing.
fn recorded(record: &str) -> Option<String> {
    let doc: Value = serde_json::from_str(&fs::read_to_string(record).ok()?).ok()?;
    let commit = doc.get("commit")?.as_str()?;
    (commit.len() == 40
        && commit
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
    .then(|| commit.to_string())
}

// The start is the recorded commit when the tip still reaches it (a
// force-push makes it no start), else the commit 7 days back, else the root.
fn since(repo: &str, branch: &str, record: &str) -> Result<(String, String, String), u8> {
    let mut ignored = String::new();
    let Some(head) = git(
        repo,
        &[
            "rev-parse",
            "-q",
            "--verify",
            &format!("refs/remotes/origin/{branch}^{{commit}}"),
        ],
        &mut ignored,
    ) else {
        warn(&format!(
            "{repo} has no origin/{branch}, nothing was reviewed"
        ));
        return Err(2);
    };
    let mut start = recorded(record);
    if let Some(commit) = &start {
        if git(
            repo,
            &["merge-base", "--is-ancestor", commit, &head],
            &mut ignored,
        )
        .is_none()
        {
            warn(&format!(
                "{record} names {}, which origin/{branch} no longer reaches: reading the {DAYS} days instead",
                short(commit)
            ));
            start = None;
        }
    }
    let (start, from) = match start {
        Some(commit) => (commit, "record".to_string()),
        None => {
            // A git that fails here is said as itself, not read as a young
            // branch.
            let mut said = String::new();
            let before = format!("--before={DAYS}.days ago");
            let older =
                git(repo, &["rev-list", "-1", &before, &head], &mut said).unwrap_or_default();
            let found = if !older.is_empty() && older != head {
                (older, format!("{DAYS} days"))
            } else {
                let root = git(repo, &["rev-list", "--max-parents=0", &head], &mut said)
                    .and_then(|roots| roots.lines().last().map(str::to_string))
                    .unwrap_or_else(|| head.clone());
                let from = if root == head {
                    "root".to_string()
                } else {
                    format!("{DAYS} days")
                };
                (root, from)
            };
            let said = said.replace('\n', " ");
            if !said.trim().is_empty() {
                warn(&format!(
                    "git said: {}",
                    said.chars().take(200).collect::<String>()
                ));
            }
            found
        }
    };
    if start == head && Path::new(record).is_file() {
        warn(&format!(
            "branch={branch} nothing new since {}",
            short(&head)
        ));
        return Err(3);
    }
    Ok((head, start, from))
}

/// The variables, by name, that the image does not list as an input a host
/// may set. A name that is neither wanted nor of the PI_ENSEMBLE_* family is
/// not the runner's to refuse; nor are secrets and the inputs of a cycle or a
/// planning turn, which a container of the review kind is not given.
fn refused(caps: &str, names: &[String]) -> Vec<String> {
    let caps = serde_json::from_str::<Value>(caps).unwrap_or(Value::Null);
    let inputs: Vec<&str> = caps
        .get("inputs")
        .and_then(Value::as_array)
        .map(|inputs| {
            inputs
                .iter()
                .filter(|i| {
                    matches!(
                        i.get("scope").and_then(Value::as_str),
                        Some("cycle" | "sandbox")
                    )
                })
                .filter_map(|i| i.get("name").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    let wanted = wanted();
    names
        .iter()
        .map(|n| n.split('=').next().unwrap_or(""))
        .filter(|n| *n != "-e" && !n.is_empty())
        .filter(|n| !wanted.iter().any(|w| w == n))
        .filter(|n| !(n.contains("TOKEN") || n.contains("SECRET") || n.ends_with("KEY")))
        .filter(|n| {
            !(n.starts_with("PI_ENSEMBLE_PLAN_")
                || n.starts_with("PI_ENSEMBLE_HANDOFF_")
                || matches!(
                    *n,
                    "PI_ENSEMBLE_EXTRA_IMAGE_DIRS"
                        | "PI_ENSEMBLE_CYCLE_DEADLINE"
                        | "PI_ENSEMBLE_VERIFY_CONCURRENCY"
                ))
        })
        .filter(|n| n.starts_with("PI_ENSEMBLE_"))
        .filter(|n| !inputs.contains(n))
        .map(str::to_string)
        .collect()
}

struct RunArgs {
    private: Vec<String>,
    branch: String,
    head: String,
    since: String,
    record: String,
    limit: u64,
    stop_wait: u64,
    name: String,
    doc: String,
    log: String,
    signalled: Arc<AtomicUsize>,
    remove: Vec<String>,
    command: Vec<String>,
}

/// The directories of a run, removed on every way out of it.
struct Workspace(Vec<String>);

impl Drop for Workspace {
    fn drop(&mut self) {
        for dir in &self.0 {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

/// The exit status of a run a signal ended, as a shell gives it.
fn ended(signalled: &AtomicUsize) -> Option<u8> {
    match signalled.load(Ordering::Relaxed) {
        0 => None,
        signal => Some(128 + signal as u8),
    }
}

/// A docker call that may not answer: None when it was still going after
/// `wait` seconds, and then it is killed.
fn bounded(args: &[&str], wait: u64) -> Option<ExitStatus> {
    let mut child = Command::new("docker")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    match wait_for(&mut child, Duration::from_secs(wait)) {
        Some(status) => Some(status),
        None => {
            let _ = child.kill();
            let _ = child.wait();
            None
        }
    }
}

fn wait_for(child: &mut Child, wait: Duration) -> Option<ExitStatus> {
    let until = Instant::now() + wait;
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Some(status);
        }
        if Instant::now() >= until {
            return None;
        }
        sleep(TICK);
    }
}

// docker stop, then docker kill, both bounded.
fn stop(args: &RunArgs, grace: u64) {
    if bounded(
        &["stop", "-t", &grace.to_string(), &args.name],
        args.stop_wait,
    )
    .is_some()
    {
        return;
    }
    warn(&format!(
        "docker stop of {} did not end within {}s, killing it",
        args.name, args.stop_wait
    ));
    if !bounded(&["kill", &args.name], args.stop_wait).is_some_and(|s| s.success()) {
        warn(&format!("docker kill of {} did not work either", args.name));
    }
}

// The client may outlive its container: after the stop's grace it is sent
// TERM, so one that traps it can clean up, then KILL.
fn reap(child: &mut Child, args: &RunArgs) -> ExitStatus {
    if let Some(status) = wait_for(child, Duration::from_secs(args.stop_wait)) {
        return status;
    }
    warn("the docker client of the review would not end, killing it");
    let _ = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status();
    if let Some(status) = wait_for(child, Duration::from_secs(2)) {
        return status;
    }
    let _ = child.kill();
    // A client that cannot be waited for is counted as killed.
    child.wait().unwrap_or_else(|_| ExitStatus::from_raw(9))
}

fn exit_of(status: ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
}

/// A file an agent wrote, masked in place. An empty mask keeps the file as
/// it was, as tehas export does.
fn mask_file(path: &str) {
    let Ok(bytes) = fs::read(path) else { return };
    if bytes.is_empty() {
        return;
    }
    let masked = mask::mask(&String::from_utf8_lossy(&bytes));
    if masked.is_empty() {
        return;
    }
    let to = format!("{path}.masking");
    if fs::write(&to, masked)
        .and_then(|()| fs::rename(&to, path))
        .is_err()
    {
        let _ = fs::remove_file(&to);
        warn(&format!("could not replace {path}"));
    }
}

/// The document's own exitCode, when it has one.
fn doc_exit(doc: &str) -> Option<String> {
    let doc: Value = serde_json::from_str(&fs::read_to_string(doc).ok()?).ok()?;
    doc.as_object()?
        .get("exitCode")
        .filter(|c| c.is_number())
        .map(Value::to_string)
}

fn write_record(args: &RunArgs, rc: i32) -> io::Result<()> {
    if let Some(dir) = Path::new(&args.record).parent() {
        fs::create_dir_all(dir)?;
    }
    let record = serde_json::json!({
        "commit": args.head,
        "at": time::iso(time::now()),
        "exit": rc,
    });
    let new = format!("{}.new", args.record);
    let written =
        fs::write(&new, format!("{record}\n")).and_then(|()| fs::rename(&new, &args.record));
    if written.is_err() {
        let _ = fs::remove_file(&new);
    }
    written
}

fn run(args: &RunArgs) -> u8 {
    if let Err(e) = mask::set_private(&args.private) {
        eprintln!("{e}");
        return 2;
    }
    // A container under this name outlived its client: the caller holds the
    // lock, so it is no review's.
    bounded(&["rm", "-f", &args.name], args.stop_wait);
    let opened =
        fs::File::create(&args.doc).and_then(|doc| Ok((doc, fs::File::create(&args.log)?)));
    let spawned = opened.and_then(|(doc, log)| {
        Command::new(&args.command[0])
            .args(&args.command[1..])
            .stdin(Stdio::null())
            .stdout(doc)
            .stderr(log)
            .spawn()
    });
    let mut child = match spawned {
        Ok(child) => child,
        Err(e) => {
            warn(&format!("could not start the review: {e}"));
            remove(args);
            return 2;
        }
    };
    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Ok(Some(status)) = child.try_wait() {
            break status;
        }
        if started.elapsed().as_secs() >= args.limit {
            say(&format!("time limit of {}s passed", args.limit));
            timed_out = true;
            stop(args, 20);
            break reap(&mut child, args);
        }
        if ended(&args.signalled).is_some() {
            stop(args, 10);
            break reap(&mut child, args);
        }
        sleep(TICK);
    };
    let rc = exit_of(status);
    remove(args);
    mask_file(&args.doc);
    mask_file(&args.log);

    let branch = &args.branch;
    if timed_out {
        say(&format!(
            "branch={branch} runner_exit={rc} recorded=no, the run ran out of time, see {}",
            args.log
        ));
        return 2;
    }
    // The document and the container must agree before the range counts as
    // read.
    let doc_rc = doc_exit(&args.doc);
    if let Some(doc_rc) = doc_rc.as_ref().filter(|d| **d != rc.to_string()) {
        say(&format!(
            "branch={branch} runner_exit={rc} document_exit={doc_rc} recorded=no, the two disagree, see {}",
            args.doc
        ));
        return 2;
    }
    if ![0, 10, 20].contains(&rc) {
        say(&format!(
            "branch={branch} runner_exit={} recorded=no, see {}",
            doc_rc.unwrap_or_else(|| rc.to_string()),
            args.log
        ));
        return 2;
    }
    if write_record(args, rc).is_err() {
        warn(&format!(
            "could not write {}, the record of what was reviewed is unchanged",
            args.record
        ));
        return 2;
    }
    say(&format!(
        "branch={branch} from={} to={} runner_exit={rc} recorded=yes seconds={}",
        short(&args.since),
        short(&args.head),
        started.elapsed().as_secs()
    ));
    rc as u8
}

fn remove(args: &RunArgs) {
    let mut failed = None;
    for dir in &args.remove {
        if fs::remove_dir_all(dir).is_err() && Path::new(dir).exists() {
            failed.get_or_insert(dir.clone());
        }
    }
    if let Some(dir) = failed {
        warn(&format!("could not remove the workspace {dir}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_workspace_goes_on_every_way_out_and_a_missing_directory_is_no_fault() {
        let at = env::temp_dir().join(format!("review-workspace-{}", std::process::id()));
        let (tree, state) = (at.join("tree"), at.join("tree.state"));
        fs::create_dir_all(tree.join("sub")).unwrap();
        fs::create_dir_all(&state).unwrap();
        let name = |p: &Path| p.display().to_string();
        let way_out = |dirs: Vec<String>| -> u8 {
            let _workspace = Workspace(dirs);
            2
        };
        let gone = name(&at.join("never"));
        assert_eq!(way_out(vec![name(&tree), gone, name(&state)]), 2);
        assert!(!tree.exists() && !state.exists());
        fs::remove_dir_all(at).unwrap();
    }

    #[test]
    fn a_signal_ends_a_run_as_a_shell_says_it() {
        assert_eq!(ended(&AtomicUsize::new(0)), None);
        assert_eq!(ended(&AtomicUsize::new(15)), Some(143));
        assert_eq!(ended(&AtomicUsize::new(2)), Some(130));
    }

    #[test]
    fn the_wanted_names_are_the_builders_and_hold_no_token() {
        let wanted = wanted();
        assert!(wanted.iter().any(|w| w == "GIT_TERMINAL_PROMPT"));
        assert!(wanted.iter().any(|w| w == "PI_ENSEMBLE_WORKSPACE_ROOT"));
        assert!(wanted.iter().any(|w| w == "PI_ENSEMBLE_SUBAGENT_MODEL"));
        assert!(!wanted.iter().any(|w| w == "GH_TOKEN"));
    }

    #[test]
    fn the_docker_line_ends_with_the_image_and_the_verb() {
        let line = docker_words("img", &Container::default(), "abc");
        assert_eq!(
            line[line.len() - 6..],
            ["img", RUNNER, "lens-review", "--since", "abc", "--json"].map(String::from)
        );
        assert_eq!(table(), format!("review\tPI_ENSEMBLE_IMAGE\t{RUNNER}"));
    }

    #[test]
    fn an_argument_is_refused_and_the_help_answers() {
        let given = |w: &[&str]| w.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(words(&given(&[]), ""), Ok(()));
        assert_eq!(words(&given(&[""]), ""), Ok(()));
        assert_eq!(words(&given(&["-h"]), ""), Err(99));
        assert_eq!(words(&given(&["--help"]), ""), Err(99));
        assert_eq!(words(&given(&["--help", "x"]), ""), Err(64));
        assert_eq!(words(&given(&["7"]), ""), Err(64));
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    const CAPS: &str = r#"{"inputs":[{"name":"PI_ENSEMBLE_LENS_NOTE","scope":"cycle"},
        {"name":"PI_ENSEMBLE_HOST_ONLY","scope":"host"}]}"#;

    #[test]
    fn a_lever_the_image_does_not_list_is_refused() {
        let given = names(&[
            "-e",
            "PI_ENSEMBLE_POLICY_JUDGE=off",
            "-e",
            "PI_ENSEMBLE_LENS_NOTE=short",
            "-e",
            "PI_ENSEMBLE_HOST_ONLY",
        ]);
        assert_eq!(
            refused(CAPS, &given),
            ["PI_ENSEMBLE_POLICY_JUDGE", "PI_ENSEMBLE_HOST_ONLY"]
        );
    }

    #[test]
    fn the_wanted_names_secrets_and_others_are_not_the_runners_to_refuse() {
        let given = names(&[
            "-e",
            "PI_ENSEMBLE_SANDBOX_MODE=x",
            "-e",
            "PI_ENSEMBLE_MODEL_TOKEN",
            "-e",
            "PI_ENSEMBLE_API_KEY",
            "-e",
            "PI_ENSEMBLE_PLAN_ISSUE=7",
            "-e",
            "PI_ENSEMBLE_EXTRA_IMAGE_DIRS",
            "-e",
            "HOME=/x",
        ]);
        assert!(refused(CAPS, &given).is_empty());
    }

    #[test]
    fn an_answer_that_is_no_object_lists_nothing() {
        let given = names(&["-e", "PI_ENSEMBLE_LENS_NOTE"]);
        assert_eq!(refused("not json", &given), ["PI_ENSEMBLE_LENS_NOTE"]);
        assert_eq!(refused("[]", &given), ["PI_ENSEMBLE_LENS_NOTE"]);
    }

    #[test]
    fn a_record_counts_only_with_a_full_commit() {
        let dir = std::env::temp_dir().join(format!("review-record-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("review.json");
        let path = file.to_str().unwrap();
        let full = "0123456789abcdef0123456789abcdef01234567";
        for (text, want) in [
            (format!(r#"{{"commit":"{full}"}}"#), Some(full.to_string())),
            (r#"{"commit":"abc1"}"#.to_string(), None),
            (r#"{"commit":null}"#.to_string(), None),
            (format!(r#"{{"commit":"{}"}}"#, full.to_uppercase()), None),
            ("not json".to_string(), None),
        ] {
            fs::write(&file, text).unwrap();
            assert_eq!(recorded(path), want);
        }
        fs::remove_dir_all(&dir).unwrap();
    }
}

//! The product contract, for `tehas check-product` and `tehas init-product`.
//! The commands load the settings and hand over: this reads TEHAS_HOME,
//! TEHAS_REPO, TEHAS_WORK_REPO, TEHAS_INSTANCE_DIR and PI_ENSEMBLE_IMAGE from
//! the environment tehas_load left. What each does is in the header of its
//! command, libexec/check-product and libexec/init-product.
//!
//!   console check-product [--sandbox]
//!     The checks, one line each, and state/check-product.json. Exit 0 every
//!     check passed, 1 the sandbox could not be run, 64 usage, 78 a check
//!     failed or a setting is missing.
//!   console check-product table
//!     The row of test/sandbox_test.sh's table of runs for the one container
//!     started here, read off the arguments it is started with.
//!   console init-product <branch> [--dry-run]
//!     The missing contract files, on tehas/init-product off origin/<branch>,
//!     pushed with gh's credentials and proposed in a pull request. Exit 0
//!     the pull request is open, or nothing is missing, 1 git or gh failed or
//!     the branch is already on origin, 64 usage, 78 <branch> is that branch
//!     or a setting is missing.

use serde_json::Value;
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};
use tehas_core::time;

const HEAD: &str = "tehas/init-product";
const COMMIT: &str = "The tehas contract files";

const AGENTS: &str = "# AGENTS.md

Rules for agents working in this repository.

## What this is

<What the product is and how the repository is laid out.>

## Rules

- <What never to touch, architecture rules, anything agents get wrong.>
- Open a pull request and stop. Never push to the default branch.

## Verify

`.pi/verify-cmd` is the fast gate. Run it before you say a change is done.

## Commits

<Commit style.>";

const CHECK: &str = "# Guessed by tehas init-product from the repository's files. A person should
# check it: the first line that is not empty or a comment is run.";

const NO_GUESS: &str = "# tehas init-product could not guess a gate from the repository's files.
# Write the command that tests the product on the first line that is not a comment.";

fn say(line: &str) {
    let _ = writeln!(io::stdout(), "{line}");
}

fn complain(line: &str) {
    let _ = writeln!(io::stderr(), "{line}");
}

fn setting(name: &str) -> Result<String, u8> {
    match std::env::var(name) {
        Ok(value) if !value.is_empty() => Ok(value),
        _ => {
            complain(&format!("tehas: {name} is not set"));
            Err(78)
        }
    }
}

fn git(dir: &str) -> Command {
    let mut command = Command::new("git");
    command.arg("-C").arg(dir);
    command
}

// What a command printed, without its last newlines, when it worked.
fn read(command: &mut Command) -> Option<String> {
    let done = command.stderr(Stdio::null()).output().ok()?;
    done.status.success().then(|| {
        let text = String::from_utf8_lossy(&done.stdout);
        text.trim_end_matches('\n').to_string()
    })
}

fn ran(command: &mut Command) -> bool {
    command.status().is_ok_and(|s| s.success())
}

fn quiet(command: &mut Command) -> bool {
    command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

// A clone made under $TEHAS_HOME/tmp. It goes when this ends, on a signal too.
struct Scratch(String);

impl Scratch {
    fn new(home: &str, name: &str) -> Option<Scratch> {
        let tmp = format!("{home}/tmp");
        if let Err(e) = fs::create_dir_all(&tmp) {
            complain(&format!("tehas: {tmp}: {e}"));
            return None;
        }
        let done = Command::new("mktemp")
            .arg("-d")
            .arg(format!("{tmp}/{name}.XXXXXX"))
            .stderr(Stdio::inherit())
            .output()
            .ok()?;
        if !done.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&done.stdout);
        let path = text.trim_end_matches('\n').to_string();
        remove_on_signal(PathBuf::from(&path));
        Some(Scratch(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn remove_on_signal(path: PathBuf) {
    let Ok(mut signals) = Signals::new([SIGHUP, SIGINT, SIGTERM]) else {
        return;
    };
    std::thread::spawn(move || {
        if let Some(signal) = signals.forever().next() {
            let _ = fs::remove_dir_all(&path);
            process::exit(128 + signal);
        }
    });
}

pub fn check(given: &[String]) -> u8 {
    let sandbox = match given {
        [] => false,
        [a] if a == "--sandbox" => true,
        [a] if a == "table" => {
            say(&table());
            return 0;
        }
        _ => {
            complain("usage: tehas check-product [--sandbox]");
            return 64;
        }
    };
    checks(sandbox).unwrap_or_else(|code| code)
}

// One run's lines and the failures among them.
#[derive(Default)]
struct Run {
    failures: Vec<String>,
}

impl Run {
    fn pass(&self, name: &str, words: &str) {
        say(&format!("ok    {name:<8} {words}"));
    }
    fn fail(&mut self, name: &str, words: String) {
        say(&format!("FAIL  {name:<8} {words}"));
        self.failures.push(words);
    }
}

fn checks(sandbox: bool) -> Result<u8, u8> {
    let home = setting("TEHAS_HOME")?;
    let name = setting("TEHAS_REPO")?;
    let repo = setting("TEHAS_WORK_REPO")?;
    let models_file = format!("{}/models.json", setting("TEHAS_INSTANCE_DIR")?);

    let mut run = Run::default();
    let head = "refs/remotes/origin/HEAD";
    let mut known = false;
    if !Path::new(&repo).join(".git").is_dir() {
        run.fail("origin", format!("no product checkout at {repo}"));
    } else {
        let url = read(git(&repo).args(["config", "--get", "remote.origin.url"]));
        match origin_mismatch(&repo, &name, url.as_deref().unwrap_or("")) {
            None => run.pass("origin", &format!("{repo} is a clone of {name}")),
            Some(words) => run.fail("origin", words),
        }
        known = quiet(git(&repo).args(["symbolic-ref", "-q", head]));
        if known {
            let short = read(git(&repo).args(["symbolic-ref", "-q", "--short", head]));
            let short = short.unwrap_or_default();
            run.pass("default", &format!("origin/HEAD is {short}"));
        } else {
            run.fail(
                "default",
                format!("origin/HEAD is not set: run git -C {repo} remote set-head origin --auto"),
            );
        }
    }

    let mut sandboxed = None;
    if known {
        let command_of = |path: &str| {
            let file = read(git(&repo).args(["show", &format!("{head}:{path}")]));
            first_command(&file.unwrap_or_default())
        };
        if in_commit(&repo, head, "AGENTS.md") {
            run.pass("agents", "AGENTS.md");
        } else {
            let words = "no AGENTS.md: tehas init-product proposes one";
            run.fail("agents", words.to_string());
        }
        let quick = command_of(".pi/verify-cmd");
        if quick.is_empty() {
            let words = "no command in .pi/verify-cmd: tehas init-product proposes one";
            run.fail("gate", words.to_string());
        } else {
            run.pass("gate", &format!(".pi/verify-cmd: {quick}"));
        }
        let settings = read(git(&repo).args(["show", &format!("{head}:.pi/settings.json")]));
        let models = fs::read_to_string(&models_file).ok();
        match model_check(settings.as_deref(), models.as_deref(), &models_file) {
            Ok(words) => run.pass("model", &words),
            Err(words) => run.fail("model", words),
        }
        if sandbox {
            let full = command_of(".pi/verify-cmd-full");
            let gates = Gates {
                gate: if full.is_empty() { quick } else { full },
                smoke: command_of(".pi/smoke-cmd"),
                setup: in_commit(&repo, head, ".pi/worktree-setup"),
            };
            sandboxed = in_sandbox(&home, &repo, gates, &mut run)?;
        }
    }

    write_result(&home, &run.failures, sandboxed);
    Ok(if run.failures.is_empty() { 0 } else { 78 })
}

fn in_commit(repo: &str, head: &str, path: &str) -> bool {
    quiet(git(repo).args(["cat-file", "-e", &format!("{head}:{path}")]))
}

// What is wrong with the checkout's origin, the words of tehas_checkout.
fn origin_mismatch(dir: &str, repo: &str, url: &str) -> Option<String> {
    let url = url.strip_suffix('/').unwrap_or(url);
    let url = url.strip_suffix(".git").unwrap_or(url);
    if url.ends_with(&format!("/{repo}")) || url.ends_with(&format!(":{repo}")) {
        return None;
    }
    let found = if url.is_empty() { "nothing" } else { url };
    Some(format!("{dir} is a checkout of {found}, not of {repo}"))
}

fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\x0b' | '\x0c' | '\r')
}

// The first line that is not empty or a comment.
fn first_command(file: &str) -> String {
    file.split('\n')
        .find(|line| {
            let line = line.trim_start_matches(is_space);
            !line.is_empty() && !line.starts_with('#')
        })
        .unwrap_or("")
        .to_string()
}

// A value of .pi/settings.json as a string in jq prints it: false and null
// are the empty word.
fn word(value: &Value) -> String {
    match value {
        Value::Null | Value::Bool(false) => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

// jq's `any(.id == $m)`: it stops at the first match, and an entry before one
// that is neither an object nor null stops the search unlisted.
fn listed_in(models: &Value, model: &str) -> bool {
    let entries: Vec<&Value> = match models {
        Value::Array(list) => list.iter().collect(),
        Value::Object(map) => map.values().collect(),
        _ => return false,
    };
    for entry in entries {
        if !matches!(entry, Value::Object(_) | Value::Null) {
            return false;
        }
        if entry["id"] == model {
            return true;
        }
    }
    false
}

// The model check: the words of a pass, or of a failure.
fn model_check(
    settings: Option<&str>,
    models: Option<&str>,
    models_file: &str,
) -> Result<String, String> {
    let Some(settings) = settings else {
        return Ok("no .pi/settings.json, the instance's model is used".to_string());
    };
    let invalid = || ".pi/settings.json is not valid JSON".to_string();
    let parsed: Value = serde_json::from_str(settings).map_err(|_| invalid())?;
    if !matches!(parsed, Value::Object(_) | Value::Null) {
        return Err(invalid());
    }
    let pm = format!(
        "{}/{}",
        word(&parsed["defaultProvider"]),
        word(&parsed["defaultModel"])
    );
    if pm == "/" {
        return Ok(".pi/settings.json names no model, the instance's is used".to_string());
    }
    let (provider, model) = pm.split_once('/').unwrap_or((&pm, ""));
    let listed = models
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .is_some_and(|models| listed_in(&models["providers"][provider]["models"], model));
    if listed {
        Ok(format!(".pi/settings.json names {pm}"))
    } else {
        Err(format!(
            ".pi/settings.json names {pm}, which {models_file} does not list"
        ))
    }
}

struct Gates {
    gate: String,
    smoke: String,
    setup: bool,
}

// The arguments of the container. test/check_product_test.sh pins them in
// order, and table() reads this file's row of the table of runs off them.
fn gates_run(tree: &str, image: &str, user: &str, gates: &Gates) -> Vec<String> {
    let mut script = String::new();
    if gates.setup {
        script.push_str("sh .pi/worktree-setup && ");
    }
    script.push_str("sh -c \"$1\"");
    if !gates.smoke.is_empty() {
        script.push_str(" && sh -c \"$2\"");
    }
    [
        "run",
        "--rm",
        "--cgroup-parent",
        "tehas.slice",
        "--memory",
        "4g",
        "--memory-swap",
        "4g",
        "--user",
        user,
        "-e",
        "HOME=/home/vscode",
        "-v",
        &format!("{tree}:{tree}"),
        "-w",
        tree,
        "--entrypoint",
        "bash",
        image,
        "-c",
        &script,
        "gates",
        &gates.gate,
        &gates.smoke,
    ]
    .map(String::from)
    .to_vec()
}

fn table() -> String {
    let image = "PI_ENSEMBLE_IMAGE";
    let gates = Gates {
        gate: String::new(),
        smoke: String::new(),
        setup: false,
    };
    let argv = gates_run("", image, "", &gates);
    let at = argv.iter().position(|a| a == "--entrypoint");
    let entry = at.map_or("", |i| &argv[i + 1]);
    format!("check-product\t{image}\t{entry}")
}

// The gates on a fresh clone in the image: whether they passed, or nothing
// when there was no gate to run.
fn in_sandbox(home: &str, repo: &str, gates: Gates, run: &mut Run) -> Result<Option<bool>, u8> {
    let image = setting("PI_ENSEMBLE_IMAGE")?;
    if gates.gate.is_empty() {
        run.fail("sandbox", "no gate to run in the sandbox".to_string());
        return Ok(None);
    }
    if !quiet(Command::new("docker").args(["image", "inspect", &image])) {
        let build = format!("{home}/sandbox/build.sh");
        complain(&format!(
            "tehas check-product: no sandbox image {image}: build it with {build}"
        ));
        return Err(1);
    }
    let tree = Scratch::new(home, "check-product").ok_or(1u8)?;
    let branch =
        read(git(repo).args(["symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD"]))
            .unwrap_or_default();
    let branch = branch.strip_prefix("origin/").unwrap_or(&branch);
    // A cycle's gate runs on its work branch, ahead of the default branch. An
    // empty commit puts the clone there too, so a gate that diffs against
    // origin/main compares a proper ancestor and finds no change.
    let path = tree.0.as_str();
    if !(sandbox_clone(repo, path, branch)
        && ran(git(path).args(["checkout", "-q", "-b", "tehas/check-product"]))
        && ran(git(path).args([
            "-c",
            "user.name=tehas",
            "-c",
            "user.email=tehas@localhost",
            "commit",
            "-q",
            "--allow-empty",
            "--no-verify",
            "-m",
            "tehas check-product: no change",
        ])))
    {
        complain(&format!(
            "tehas check-product: could not clone {repo} into {path}"
        ));
        return Err(1);
    }
    let user = format!(
        "{}:{}",
        read(Command::new("id").arg("-u")).unwrap_or_default(),
        read(Command::new("id").arg("-g")).unwrap_or_default()
    );
    let passed = ran(Command::new("docker").args(gates_run(path, &image, &user, &gates)));
    if passed {
        run.pass("sandbox", &format!("the gates passed in {image}"));
    } else {
        run.fail("sandbox", format!("the gates failed in {image}"));
    }
    Ok(Some(passed))
}

// tehas_sandbox_clone of lib/sandbox.sh: the tip the checkout last fetched,
// with the product's address as origin. Nothing comes from the network.
fn sandbox_clone(src: &str, tree: &str, branch: &str) -> bool {
    let tip = format!("refs/remotes/origin/{branch}");
    let verify = format!("{tip}^{{commit}}");
    let Some(base) = read(git(src).args(["rev-parse", "-q", "--verify", &verify])) else {
        complain(&format!("tehas: {src} has no origin/{branch}"));
        return false;
    };
    let Some(url) = read(git(src).args(["config", "--get", "remote.origin.url"])) else {
        return false;
    };
    let all = "+refs/remotes/origin/*:refs/remotes/origin/*";
    let upstream = format!("--set-upstream-to=origin/{branch}");
    ran(Command::new("git").args(["init", "-q", "-b", branch, tree]))
        && ran(git(tree).args(["remote", "add", "origin", &url]))
        && ran(Command::new("timeout")
            .args(["-k", "10", "300", "git", "-C", tree, "fetch"])
            .args(["-q", "--no-tags", src, all]))
        && ran(git(tree).args(["symbolic-ref", "refs/remotes/origin/HEAD", &tip]))
        && ran(git(tree).args(["checkout", "-q", "-B", branch, &base]))
        && ran(git(tree).args(["branch", "-q", &upstream, branch]))
}

// One string as jq prints it: serde_json and jq differ on DEL alone.
fn json_string(s: &str) -> String {
    serde_json::to_string(s)
        .unwrap_or_default()
        .replace('\x7f', "\\u007f")
}

// The file as `jq -n` pretty-prints it.
fn result_text(at: &str, failures: &[String], sandbox: Option<bool>) -> String {
    let list = if failures.is_empty() {
        "[]".to_string()
    } else {
        let items: Vec<String> = failures
            .iter()
            .map(|f| format!("    {}", json_string(f)))
            .collect();
        format!("[\n{}\n  ]", items.join(",\n"))
    };
    let sandbox = sandbox.map_or("null".to_string(), |b| b.to_string());
    format!(
        "{{\n  \"v\": 1,\n  \"at\": \"{at}\",\n  \"ok\": {},\n  \"failures\": {list},\n  \"sandbox\": {sandbox}\n}}\n",
        failures.is_empty()
    )
}

fn write_result(home: &str, failures: &[String], sandbox: Option<bool>) {
    let state = format!("{home}/state");
    let file = format!("{state}/check-product.json");
    let new = format!("{file}.new");
    let text = result_text(&time::iso(time::now()), failures, sandbox);
    let written = fs::create_dir_all(&state)
        .and_then(|()| fs::write(&new, text))
        .and_then(|()| fs::rename(&new, &file));
    if written.is_err() {
        complain(&format!("tehas check-product: could not write {file}"));
    }
}

pub fn init(given: &[String]) -> u8 {
    let (branch, dry) = match given {
        [b] => (b, false),
        [b, d] if d == "--dry-run" => (b, true),
        _ => {
            complain("usage: tehas init-product [--dry-run]");
            return 64;
        }
    };
    if branch == HEAD {
        complain(&format!("tehas init-product: the default branch is {HEAD}"));
        return 78;
    }
    propose(branch, dry).unwrap_or_else(|code| code)
}

fn propose(branch: &str, dry: bool) -> Result<u8, u8> {
    let home = setting("TEHAS_HOME")?;
    let name = setting("TEHAS_REPO")?;
    let src = setting("TEHAS_WORK_REPO")?;

    let scratch = Scratch::new(&home, "init-product").ok_or(1u8)?;
    let tree = scratch.0.as_str();
    let url = read(git(&src).args(["config", "--get", "remote.origin.url"])).unwrap_or_default();
    let tip = format!("refs/remotes/origin/{branch}");
    if !(ran(git(tree).args(["init", "-q"]))
        && ran(git(tree).args(["remote", "add", "origin", &url]))
        && ran(git(tree).args(["fetch", "-q", "--no-tags", &src, &tip]))
        && ran(git(tree).args(["checkout", "-q", "-b", HEAD, "FETCH_HEAD"])))
    {
        complain(&format!(
            "tehas init-product: {src} has no origin/{branch} to start from"
        ));
        return Err(1);
    }

    let guessed = guess(Path::new(tree));
    let stack = guessed.stack;
    let mut files = vec![("AGENTS.md", AGENTS.to_string())];
    files.push((
        ".pi/verify-cmd",
        match &guessed.gate {
            Some(gate) => format!("{CHECK}\n# Stack: {stack}.\n{gate}"),
            None => NO_GUESS.to_string(),
        },
    ));
    if let Some(install) = &guessed.install {
        files.push((
            ".pi/worktree-setup",
            format!("#!/bin/sh\n{CHECK}\n# Stack: {stack}.\nset -e\n{install}"),
        ));
    }

    let mut wrote = Vec::new();
    for (path, text) in &files {
        if quiet(git(tree).args(["cat-file", "-e", &format!("HEAD:{path}")])) {
            continue;
        }
        wrote.push(*path);
        if dry {
            say(&format!("=== {path}\n{text}"));
            continue;
        }
        let file = Path::new(tree).join(path);
        let parent = file.parent().unwrap_or(Path::new(tree));
        let put = fs::create_dir_all(parent).and_then(|()| fs::write(&file, format!("{text}\n")));
        if let Err(e) = put {
            complain(&format!("tehas init-product: {path}: {e}"));
            return Err(1);
        }
    }

    if wrote.is_empty() {
        say(&format!(
            "tehas init-product: {name} has every file this proposes"
        ));
        return Ok(0);
    }
    let list = wrote.join(" ");
    say(&format!(
        "tehas init-product: stack {stack}; proposing {list}"
    ));
    if dry {
        return Ok(0);
    }

    let on_origin = read(git(tree).args(["ls-remote", "--heads", "origin", HEAD]));
    if !on_origin.unwrap_or_default().is_empty() {
        complain(&format!(
            "tehas init-product: {HEAD} is already on origin: review or close its pull request first"
        ));
        return Err(1);
    }
    let setup = Path::new(tree).join(".pi/worktree-setup");
    if setup.is_file() {
        ran(Command::new("chmod").arg("+x").arg(&setup));
    }
    let body = format!(
        "The files of the tehas contract that the default branch does not have yet: {list}.\n\n\
         The gate and the install step are guessed from the repository's files (stack: {stack}). \
         Check them, and fill in AGENTS.md, before you merge. \
         `tehas check-product` says what is still missing."
    );
    if !(ran(git(tree).args(["add", "--"]).args(&wrote))
        && ran(git(tree).args(["commit", "-q", "-m", COMMIT]))
        && ran(git(tree)
            .args(["-c", "credential.helper="])
            .args(["-c", "credential.helper=!gh auth git-credential"])
            .args(["push", "-q", "origin", HEAD])))
    {
        complain(&format!(
            "tehas init-product: could not commit and push {HEAD}"
        ));
        return Err(1);
    }
    let opened = ran(Command::new("gh")
        .args(["pr", "create", "--repo", &name, "--base", branch])
        .args(["--head", HEAD, "--title", COMMIT, "--body", &body]));
    Ok(if opened { 0 } else { 1 })
}

#[derive(Debug, PartialEq)]
struct Guess {
    stack: &'static str,
    gate: Option<String>,
    install: Option<String>,
}

impl Guess {
    fn of(stack: &'static str, gate: Option<&str>, install: Option<&str>) -> Guess {
        Guess {
            stack,
            gate: gate.map(String::from),
            install: install.map(String::from),
        }
    }
}

// The stack, the gate and the install step, from the tree's files.
fn guess(tree: &Path) -> Guess {
    let has = |name: &str| tree.join(name).is_file();
    if let Some(target) = make_target(tree) {
        return Guess {
            stack: "Makefile",
            gate: Some(format!("make {target}")),
            install: None,
        };
    }
    if has("package.json") {
        let (tool, install) = node_tool(tree);
        let json = fs::read_to_string(tree.join("package.json")).unwrap_or_default();
        let json: Value = serde_json::from_str(&json).unwrap_or(Value::Bool(false));
        let script = ["test", "check", "build"]
            .into_iter()
            .find(|t| !matches!(json["scripts"][t], Value::Null | Value::Bool(false)));
        let gate = script.map(|t| match t {
            "test" => format!("{tool} test"),
            t => format!("{tool} run {t}"),
        });
        return Guess {
            stack: "package.json",
            gate,
            install: Some(install.to_string()),
        };
    }
    if has("Cargo.toml") {
        return Guess::of("Cargo.toml", Some("cargo test"), Some("cargo fetch"));
    }
    if has("go.mod") {
        return Guess::of("go.mod", Some("go test ./..."), Some("go mod download"));
    }
    if has("pyproject.toml") {
        return if has("uv.lock") {
            Guess::of("pyproject.toml", Some("uv run pytest"), Some("uv sync"))
        } else {
            let (test, install) = ("python -m pytest", "pip install -e .");
            Guess::of("pyproject.toml", Some(test), Some(install))
        };
    }
    match workflow_run(tree) {
        Some(run) => Guess {
            stack: "a CI workflow",
            gate: Some(run),
            install: None,
        },
        None => Guess::of("nothing", None, None),
    }
}

// The Makefile's target of these names, by the order of the names.
fn make_target(tree: &Path) -> Option<&'static str> {
    let file = fs::read(tree.join("Makefile")).ok()?;
    let text = String::from_utf8_lossy(&file);
    ["test", "check", "verify"]
        .into_iter()
        .find(|t| text.split('\n').any(|l| l.starts_with(&format!("{t}:"))))
}

// The tool a package.json's lockfile says, and its install command.
fn node_tool(tree: &Path) -> (&'static str, &'static str) {
    let has = |name: &str| tree.join(name).is_file();
    if has("pnpm-lock.yaml") {
        ("pnpm", "pnpm install --frozen-lockfile")
    } else if has("yarn.lock") {
        ("yarn", "yarn install --frozen-lockfile")
    } else if has("bun.lockb") || has("bun.lock") {
        ("bun", "bun install --frozen-lockfile")
    } else if has("package-lock.json") {
        ("npm", "npm ci")
    } else {
        ("npm", "npm install")
    }
}

// The first run step of the CI workflows that tests anything: the .yml files,
// then the .yaml files, each in name order, read as one text.
fn workflow_run(tree: &Path) -> Option<String> {
    let mut names: Vec<PathBuf> = fs::read_dir(tree.join(".github/workflows"))
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| !path.is_dir())
        .collect();
    names.sort();
    let mut text = Vec::new();
    for extension in ["yml", "yaml"] {
        for path in &names {
            let name = path.file_name().map(|n| n.to_string_lossy());
            let hidden = name.is_some_and(|n| n.starts_with('.'));
            if !hidden && path.extension().is_some_and(|e| e == extension) {
                text.extend(fs::read(path).unwrap_or_default());
            }
        }
    }
    let text = String::from_utf8_lossy(&text);
    text.split('\n').find_map(run_step)
}

fn run_step(line: &str) -> Option<String> {
    let rest = line.trim_start_matches(|c| is_space(c) || c == '-');
    let run = rest.strip_prefix("run:")?.trim_matches(is_space);
    let lower = run.to_ascii_lowercase();
    let tests = ["test", "check", "verify", "lint"]
        .iter()
        .any(|w| lower.contains(w));
    tests.then(|| run.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // The files of a tree and what is guessed from them.
    type Case<'a> = (
        &'a [(&'a str, &'a str)],
        &'a str,
        Option<&'a str>,
        Option<&'a str>,
    );

    fn guessed(files: &[(&str, &str)]) -> Guess {
        static CASE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = CASE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("product-{}-{n}", process::id()));
        fs::create_dir_all(&dir).unwrap();
        for (name, text) in files {
            let file = dir.join(name);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, text).unwrap();
        }
        let said = guess(&dir);
        fs::remove_dir_all(dir).unwrap();
        said
    }

    #[test]
    fn one_guess_for_each_stack() {
        let cases: &[Case] = &[
            (
                &[("Makefile", "test:\n")],
                "Makefile",
                Some("make test"),
                None,
            ),
            (
                &[("Makefile", "x:\ncheck: x\n")],
                "Makefile",
                Some("make check"),
                None,
            ),
            (
                &[("Makefile", "verify:\n")],
                "Makefile",
                Some("make verify"),
                None,
            ),
            (
                &[
                    ("package.json", r#"{"scripts":{"test":"x"}}"#),
                    ("package-lock.json", ""),
                ],
                "package.json",
                Some("npm test"),
                Some("npm ci"),
            ),
            (
                &[
                    ("package.json", r#"{"scripts":{"check":"x"}}"#),
                    ("yarn.lock", ""),
                ],
                "package.json",
                Some("yarn run check"),
                Some("yarn install --frozen-lockfile"),
            ),
            (
                &[
                    ("package.json", r#"{"scripts":{"build":"x"}}"#),
                    ("pnpm-lock.yaml", ""),
                ],
                "package.json",
                Some("pnpm run build"),
                Some("pnpm install --frozen-lockfile"),
            ),
            (
                &[("package.json", "{}"), ("bun.lockb", "")],
                "package.json",
                None,
                Some("bun install --frozen-lockfile"),
            ),
            (
                &[("package.json", "not json")],
                "package.json",
                None,
                Some("npm install"),
            ),
            (
                &[("Cargo.toml", "")],
                "Cargo.toml",
                Some("cargo test"),
                Some("cargo fetch"),
            ),
            (
                &[("go.mod", "")],
                "go.mod",
                Some("go test ./..."),
                Some("go mod download"),
            ),
            (
                &[("pyproject.toml", "")],
                "pyproject.toml",
                Some("python -m pytest"),
                Some("pip install -e ."),
            ),
            (
                &[("pyproject.toml", ""), ("uv.lock", "")],
                "pyproject.toml",
                Some("uv run pytest"),
                Some("uv sync"),
            ),
            (
                &[(
                    ".github/workflows/ci.yml",
                    "- run: ./configure\n  - run: ./Run-TESTS.sh  \n",
                )],
                "a CI workflow",
                Some("./Run-TESTS.sh"),
                None,
            ),
            (&[("README", "")], "nothing", None, None),
        ];
        for (files, stack, gate, install) in cases {
            assert_eq!(
                guessed(files),
                Guess::of(stack, *gate, *install),
                "{files:?}"
            );
        }
    }

    #[test]
    fn a_makefile_without_a_target_is_no_stack() {
        let files = [("Makefile", "all:\n xtest:\n"), ("go.mod", "")];
        assert_eq!(guessed(&files).stack, "go.mod");
    }

    #[test]
    fn workflows_read_yml_before_yaml_as_one_text() {
        let files = [
            (".github/workflows/a.yaml", "- run: make lint\n"),
            (".github/workflows/z.yml", "- run: make check\n"),
        ];
        assert_eq!(guessed(&files).gate.as_deref(), Some("make check"));
        let files = [
            (".github/workflows/a.yml", "- ru"),
            (".github/workflows/b.yml", "n: make test\n"),
        ];
        assert_eq!(guessed(&files).gate.as_deref(), Some("make test"));
    }

    #[test]
    fn the_origin_compare_is_tehas_checkout() {
        for url in [
            "https://github.com/acme/widget",
            "https://github.com/acme/widget.git",
            "https://github.com/acme/widget/",
            "https://github.com/acme/widget.git/",
            "git@github.com:acme/widget.git",
        ] {
            assert_eq!(origin_mismatch("/r", "acme/widget", url), None, "{url}");
        }
        let said = |url| origin_mismatch("/r", "acme/widget", url).unwrap();
        assert_eq!(
            said("https://x/acme/other.git"),
            "/r is a checkout of https://x/acme/other, not of acme/widget"
        );
        assert_eq!(said(""), "/r is a checkout of nothing, not of acme/widget");
        assert_eq!(
            said("https://x/notacme/widget2"),
            "/r is a checkout of https://x/notacme/widget2, not of acme/widget"
        );
        assert_eq!(
            said("https://x.y/widget.git.git"),
            "/r is a checkout of https://x.y/widget.git, not of acme/widget"
        );
    }

    #[test]
    fn the_first_command_skips_comments_and_blanks_only() {
        assert_eq!(
            first_command("# one\n\n  \t\n  # two\n  make t \nmore"),
            "  make t "
        );
        assert_eq!(first_command("# only\n"), "");
        assert_eq!(first_command(""), "");
    }

    #[test]
    fn the_model_is_read_as_jq_reads_it() {
        let models = json!({"providers": {"lab": {"models": [{"id": "big"}]}}}).to_string();
        let check = |s: &str| model_check(Some(s), Some(&models), "m.json");
        assert_eq!(
            model_check(None, None, "m.json").unwrap(),
            "no .pi/settings.json, the instance's model is used"
        );
        let none = ".pi/settings.json names no model, the instance's is used";
        assert_eq!(check("{}").unwrap(), none);
        assert_eq!(check("null").unwrap(), none);
        assert_eq!(check(r#"{"defaultModel": false}"#).unwrap(), none);
        for bad in ["", "[1]", "1", "\"x\"", "true", "{"] {
            assert_eq!(
                check(bad).unwrap_err(),
                ".pi/settings.json is not valid JSON"
            );
        }
        assert_eq!(
            check(r#"{"defaultProvider": "lab", "defaultModel": "big"}"#).unwrap(),
            ".pi/settings.json names lab/big"
        );
        assert_eq!(
            check(r#"{"defaultModel": "big"}"#).unwrap_err(),
            ".pi/settings.json names /big, which m.json does not list"
        );
        assert!(model_check(Some(r#"{"defaultModel": "big"}"#), None, "m.json").is_err());
    }

    #[test]
    fn a_malformed_model_before_the_match_stops_the_search() {
        let id = |models: Value| listed_in(&models, "big");
        assert!(id(json!([null, {"id": "big"}])));
        assert!(id(json!({"a": {"id": "big"}})));
        assert!(id(json!([{"id": "big"}, 1])));
        assert!(!id(json!([1, {"id": "big"}])));
        assert!(!id(json!(["x", {"id": "big"}])));
        assert!(!id(json!("big")));
    }

    #[test]
    fn the_result_is_printed_as_jq_prints_it() {
        assert_eq!(
            result_text("T", &[], None),
            "{\n  \"v\": 1,\n  \"at\": \"T\",\n  \"ok\": true,\n  \"failures\": [],\n  \"sandbox\": null\n}\n"
        );
        let failures = ["a \"b\"\u{7f}\n".to_string(), "é".to_string()];
        assert_eq!(
            result_text("T", &failures, Some(false)),
            "{\n  \"v\": 1,\n  \"at\": \"T\",\n  \"ok\": false,\n  \"failures\": [\n    \"a \\\"b\\\"\\u007f\\n\",\n    \"é\"\n  ],\n  \"sandbox\": false\n}\n"
        );
    }

    #[test]
    fn the_container_is_given_what_it_was_given_in_order() {
        let gates = Gates {
            gate: "g".to_string(),
            smoke: String::new(),
            setup: true,
        };
        let said = gates_run("/t", "img", "1:2", &gates).join(" ");
        assert_eq!(
            said,
            "run --rm --cgroup-parent tehas.slice --memory 4g --memory-swap 4g --user 1:2 \
             -e HOME=/home/vscode -v /t:/t -w /t --entrypoint bash img \
             -c sh .pi/worktree-setup && sh -c \"$1\" gates g "
        );
        assert_eq!(table(), "check-product\tPI_ENSEMBLE_IMAGE\tbash");
    }
}

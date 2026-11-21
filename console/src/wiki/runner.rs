//! `tehasbot wiki build [--full]`: one build of the product wiki with the
//! model, pi-rukas's `wiki` verb run headless in a container of the wiki kind
//! (tehas_core::sandbox::Kind::Wiki): the code read-only, the raw sources
//! read-only, the pages it writes, no token. The host's part is
//! [`super::build::run`]; this is its writer.
//!
//! The model has no time limit (docs/specs/2026-11-21-wiki.md, decision 11).
//! The container runs at a low CPU weight, so cycles come first.
//!
//! Exit: 0 built or up to date, or the feature is off; 1 the build failed and
//! the current one stays; 2 a signal or a fault of the host; 78 the settings,
//! the sandbox or the image cannot run a build.

use super::build::{self, Ask, Done, Work};
use super::publish;
use crate::review::{self, bounded, ended, exit_of, mask_file, Workspace};
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;
use tehas_core::models::{self, host_path};
use tehas_core::sandbox::{self, Identity, Kind, Paths, WIKI_OUT, WIKI_RAW};
use tehas_core::slice::{Host, Load};
use tehas_core::{mask, settings, time};

const NAME: &str = "tehas-wiki";
/// The verb and the flags an image must list for a build.
pub const VERB: (&str, &str) = ("wiki", "--raw --out --json");
/// The CPU weight of the container, against a cycle's 1024.
const SHARES: &str = "64";

fn say(line: &str) {
    tehas_core::outln!("tehasbot wiki: {line}");
}

/// Whether the arguments ask for a full build, or 64 with the usage.
pub fn words(given: &[String]) -> Result<bool, u8> {
    match given {
        [] => Ok(false),
        [f] if f == "--full" => Ok(true),
        _ => {
            eprintln!("usage: tehasbot wiki build [--full]");
            Err(64)
        }
    }
}

/// The container's limits with its CPU weight lowered.
pub fn lowered(limits: &[String]) -> Vec<String> {
    let mut out = limits.to_vec();
    match out.iter().position(|w| w == "--cpu-shares") {
        Some(i) if i + 1 < out.len() => out[i + 1] = SHARES.to_string(),
        _ => out.extend(["--cpu-shares".to_string(), SHARES.to_string()]),
    }
    out
}

/// The docker line of a build, in order.
pub fn docker_words(image: &str, c: &Container, full: bool) -> Vec<String> {
    let mut words: Vec<String> = [
        "docker",
        "run",
        sandbox::NEVER_PULL,
        "--rm",
        "--init",
        "--name",
        NAME,
    ]
    .map(String::from)
    .to_vec();
    words.extend(lowered(&c.limits));
    words.extend_from_slice(&c.security);
    words.extend([
        "--user".to_string(),
        c.user.clone(),
        "-w".to_string(),
        c.repo.clone(),
    ]);
    words.extend_from_slice(&c.mounts);
    words.extend_from_slice(&c.envs);
    words.extend_from_slice(&c.hosts);
    words.extend(
        [
            image,
            sandbox::RUNNER,
            VERB.0,
            "--raw",
            WIKI_RAW,
            "--out",
            WIKI_OUT,
        ]
        .map(String::from),
    );
    if full {
        words.push("--full".to_string());
    }
    words.push("--json".to_string());
    words
}

#[derive(Default)]
pub struct Container {
    pub limits: Vec<String>,
    pub security: Vec<String>,
    pub user: String,
    pub repo: String,
    pub mounts: Vec<String>,
    pub envs: Vec<String>,
    pub hosts: Vec<String>,
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    let pages = publish::read(from).map_err(|why| why.join("; "))?;
    publish::seed(to, &pages).map_err(|e| e.to_string())
}

fn copy_files(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)?.flatten() {
        if e.file_type()?.is_file() {
            fs::copy(e.path(), to.join(e.file_name()))?;
        }
    }
    Ok(())
}

/// The container's run: Ok when the runner wrote the pages and said so.
fn run_container(
    command: &[String],
    doc: &str,
    log: &str,
    stop_wait: u64,
    signalled: &AtomicUsize,
) -> Result<(), String> {
    bounded(&["rm", "-f", NAME], stop_wait);
    let opened = fs::File::create(doc).and_then(|d| Ok((d, fs::File::create(log)?)));
    let mut child = opened
        .and_then(|(d, l)| {
            Command::new(&command[0])
                .args(&command[1..])
                .stdin(Stdio::null())
                .stdout(d)
                .stderr(l)
                .spawn()
        })
        .map_err(|e| format!("could not start the container: {e}"))?;
    let status = loop {
        if let Ok(Some(s)) = child.try_wait() {
            break s;
        }
        if ended(signalled).is_some() {
            bounded(&["stop", "-t", "10", NAME], stop_wait);
            break child.wait().map_err(|e| e.to_string())?;
        }
        sleep(Duration::from_millis(100));
    };
    mask_file(doc);
    mask_file(log);
    match exit_of(status) {
        0 => Ok(()),
        rc => Err(format!("the runner exited {rc}, see {log}")),
    }
}

pub fn main(given: &[String]) -> u8 {
    let full = match words(given) {
        Ok(f) => f,
        Err(rc) => return rc,
    };
    if !tehas_core::features::on("wiki") {
        return 0;
    }
    let var = |k: &str| env::var(k).unwrap_or_default();
    // An ask of tehas wiki build is taken now: one ask, one build.
    let asked = super::take_ask(Path::new(&var("TEHAS_HOME")));
    let full = full || asked.as_deref() == Some("full");
    let refuse = |line: String| {
        eprintln!("{line}");
        78
    };
    if let Err(line) = sandbox::settings_ok().and_then(|()| settings::checkout()) {
        return refuse(line);
    }
    let branch = match settings::branch() {
        Ok(b) => b,
        Err(line) => return refuse(line),
    };
    let depth = match tehas_core::depth::from_env() {
        Ok(d) => d,
        Err(line) => return refuse(line),
    };
    let image = var("PI_ENSEMBLE_IMAGE");
    match review::capabilities(&image) {
        Ok(caps) if tehas_cycle::caps::verb(&caps, VERB.0, VERB.1) => {}
        Ok(_) => {
            return refuse(format!(
                "tehasbot wiki: {image} has no wiki verb (its --capabilities lists no verbs.wiki with {}; trail-openers/pi-rukas#2456), not starting",
                VERB.1
            ))
        }
        Err(rc) => return refuse(format!("tehasbot wiki: {image} gave no --capabilities answer (exit {rc}), not starting")),
    }
    let taken = match sandbox::take(
        &Host::here(),
        &Load::from_env(),
        "tehasbot wiki",
        "a wiki build",
    ) {
        Ok(t) => t,
        Err(line) => return refuse(line),
    };
    let home = var("TEHAS_HOME");
    let src = var("TEHAS_WORK_REPO");
    let repo = var("TEHAS_WORK_DIR");
    let tree = format!("{repo}-wiki");
    let state = format!("{tree}.state");
    let (name, email) = (
        review::git_global("user.name"),
        review::git_global("user.email"),
    );
    if name.is_empty() || email.is_empty() {
        return refuse("tehasbot wiki: git user.name and user.email are not set".to_string());
    }
    let who = Identity {
        tree: &repo,
        name: &name,
        email: &email,
    };
    let envs = sandbox::env_options(
        Kind::Wiki,
        &who,
        &sandbox::ensemble(Kind::Wiki),
        &sandbox::tokens(),
    );
    let hosts = match sandbox::network(&var("PI_ENSEMBLE_HOST_ALIASES")) {
        Ok(h) => h,
        Err(why) => return refuse(format!("tehasbot wiki: {why}, not starting")),
    };
    let stop_wait: u64 = var("TEHAS_STOP_WAIT").parse().unwrap_or(60);
    if fs::create_dir_all(format!("{home}/state"))
        .and_then(|()| fs::create_dir_all(format!("{home}/logs")))
        .is_err()
    {
        return refuse(format!(
            "tehasbot wiki: could not make the directories under {home}"
        ));
    }
    let Ok(lock) = fs::File::create(format!("{home}/state/wiki.lock")) else {
        return refuse("tehasbot wiki: could not open the lock".to_string());
    };
    if lock.try_lock().is_err() {
        say("another build is going");
        return 0;
    }
    let signalled = Arc::new(AtomicUsize::new(0));
    for signal in [SIGTERM, SIGINT, SIGHUP] {
        if signal_hook::flag::register_usize(signal, Arc::clone(&signalled), signal as usize)
            .is_err()
        {
            return 2;
        }
    }
    let works = Path::new(&tree).parent().unwrap_or(Path::new("/"));
    if works.exists() && !works.join(".tehas-workspaces").is_file() {
        return refuse(format!(
            "tehasbot wiki: {} exists and was not made by tehas",
            works.display()
        ));
    }
    let _gone = Workspace(vec![tree.clone(), state.clone()]);
    let private = mask::host_names(&host_path(), &email);
    if let Err(e) = mask::set_private(&private) {
        eprintln!("{e}");
        return 2;
    }
    let stamp = time::stamp(time::now());
    let log = format!("{home}/logs/wiki-{stamp}.jsonl");
    let doc = format!("{home}/logs/wiki-{stamp}.json");
    let runs = format!("{}/.pi/agent/ensemble-runs/wiki", var("HOME"));
    let now = time::iso(time::now());
    let date = now.get(..10).unwrap_or("").to_string();
    let ask = Ask {
        home: Path::new(&home),
        checkout: Path::new(&src),
        branch: &branch,
        full,
        date: &date,
        now: &now,
    };
    let writer = |w: &Work| -> Result<(), String> {
        let _ = fs::remove_dir_all(&tree);
        let _ = fs::remove_dir_all(&state);
        let made = fs::create_dir_all(works).is_ok()
            && fs::write(works.join(".tehas-workspaces"), "").is_ok()
            && sandbox::clone(&src, &tree, &branch, depth);
        if !made {
            return Err(format!("could not make the workspace {tree}"));
        }
        let at = head_of(&tree);
        if at.as_deref() != Some(w.head.commit.as_str()) {
            return Err(format!(
                "the workspace {tree} is not at {}",
                &w.head.commit[..8]
            ));
        }
        let out = Path::new(&state).join("out");
        copy_tree(w.stage, &out)?;
        copy_files(w.raw, &Path::new(&state).join("raw")).map_err(|e| e.to_string())?;
        let paths = Paths {
            tree: &repo,
            host: &tree,
            state: &state,
            runs: &runs,
        };
        let mounts = sandbox::prepared_mounts(Kind::Wiki, &paths)?;
        let st = Path::new(&state);
        models::write_for_container(&st.join("models.json"), &st.join("endpoints"))
            .ok_or("could not write the container's models.json")?;
        let c = Container {
            limits: taken.limits.clone(),
            security: taken.security.clone(),
            user: sandbox::user(),
            repo: repo.clone(),
            mounts,
            envs: envs.clone(),
            hosts: hosts.clone(),
        };
        say(&format!("building at {}, log {log}", &w.head.commit[..8]));
        run_container(
            &docker_words(&image, &c, full),
            &doc,
            &log,
            stop_wait,
            &signalled,
        )?;
        // What the model wrote comes back only as pages; anything else fails.
        fs::remove_dir_all(w.stage).map_err(|e| e.to_string())?;
        fs::create_dir_all(w.stage).map_err(|e| e.to_string())?;
        copy_tree(&out, w.stage)
    };
    let done = build::run(&ask, writer);
    // The state written below is no longer this build's.
    drop(lock);
    if let Some(rc) = ended(&signalled) {
        return rc;
    }
    match done {
        Ok(Done::UpToDate(c)) => {
            say(&format!("up to date at {}", &c[..8]));
            0
        }
        Ok(Done::Built { id, changed }) => {
            say(&format!("built {id}, {changed} pages changed"));
            super::publish(&[]);
            0
        }
        Err(why) => {
            eprintln!("tehasbot wiki: the build failed, the current one stays: {why}");
            1
        }
    }
}

fn head_of(tree: &str) -> Option<String> {
    let out = Command::new("git")
        .args(["-C", tree, "rev-parse", "HEAD"])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn it_takes_full_alone() {
        assert_eq!(words(&[]), Ok(false));
        assert_eq!(words(&w(&["--full"])), Ok(true));
        assert_eq!(words(&w(&["--json"])), Err(64));
    }

    #[test]
    fn the_container_runs_at_a_low_cpu_weight() {
        assert_eq!(
            lowered(&w(&["--cpus", "2", "--cpu-shares", "1024"])),
            w(&["--cpus", "2", "--cpu-shares", "64"])
        );
        assert_eq!(
            lowered(&w(&["--cpus", "2"])),
            w(&["--cpus", "2", "--cpu-shares", "64"])
        );
    }

    #[test]
    fn the_docker_line_names_the_verb_its_two_directories_and_no_token() {
        let line = docker_words("img", &Container::default(), true).join(" ");
        assert!(line.ends_with("img /opt/pi-rukas/bin/pi-rukas-headless wiki --raw /run/tehas/wiki/raw --out /run/tehas/wiki/out --full --json"), "{line}");
        assert!(
            line.starts_with("docker run --pull=never --rm --init --name tehas-wiki"),
            "{line}"
        );
        assert!(!line.contains("GH_TOKEN"));
    }
}

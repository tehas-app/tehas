//! tehas-contractor: the contractor program of tehas outsource (#2274).
//!
//!   tehas-contractor join <factory> <name> <code>
//!       Join the factory at <factory> (host:port) as <name>, with the
//!       one-time code `tehas outsource invite` printed. A key is made here;
//!       the factory keeps its hash. The identity is kept in
//!       ~/.config/tehas/contractor.json, mode 0600.
//!   tehas-contractor run
//!       Ask the factory for jobs and run them, until drained or revoked.
//!       The workspace is wiped first and after every job.
//!   tehas-contractor drain
//!       Take no new job; a running one has the grace to end, then it is
//!       handed back with its branch.
//!   tehas-contractor doctor
//!       Say what this host holds that a contractor should not, and whether
//!       its disk is encrypted.
//!   tehas-contractor token | relay
//!       The cycle's renewer and relay call these while a job runs: a fresh
//!       cycle token from the factory, written where the container reads it
//!       and printed; one issue write, JSON on standard input and output.
//!   tehas-contractor own --as <uid:gid> <folder>...
//!       The cycle calls this before its container starts, naming the account
//!       the container runs as: the folders the container writes, handed to
//!       the factory's account the job names when the container runs as it.
//!       Only folders under $TEHAS_HOME/work and ~/.pi/agent/ensemble-runs.
//!
//! The settings: TEHAS_HOME, the contractor's tehas install [~/tehas];
//! TEHAS_OUTSOURCE, which must be on, as the feature registry reads it.
//! Outsource is still being built: a release build refuses every verb.
//! From the environment, for tests: TEHAS_WORK_CMD, the command that runs a
//! cycle [$TEHAS_HOME/bin/tehas work]; TEHAS_WORK_GRACE, the seconds a cycle
//! has after its stop signal [300]; TEHAS_CONTRACTOR_GIT, where a product is
//! cloned from [https://github.com].
//!
//! Exit: 0 done (run: drained); 1 failed, refused or revoked; 3 doctor
//! found something; 64 a word it does not know; 78 outsource is off, this
//! host has not joined, or (run) TEHAS_HOME holds a factory or the user's
//! models.json is not this program's.

use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{json, Value};
use tehas_contractor::host::{self, Host, Paths};
use tehas_contractor::http::{self, As, Reply};
use tehas_contractor::{doctor, identity, job};

const USAGE: &str =
    "usage: tehas-contractor join <factory> <name> <code> | run | drain | doctor | token | relay | own --as <uid:gid> <folder>...";

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

fn say(line: &str) {
    eprintln!("tehas outsource contractor: {line}");
}

fn stop(line: &str, code: i32) -> ! {
    say(line);
    std::process::exit(code)
}

fn paths() -> Paths {
    let user_home = PathBuf::from(env("HOME"));
    let home = match env("TEHAS_HOME") {
        h if h.is_empty() => user_home.join("tehas"),
        h => PathBuf::from(h),
    };
    Paths { home, user_home }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    if matches!(words.as_slice(), [] | ["--help" | "-h"]) {
        tehas_core::outln!("tehas outsource contractor: runs cycles for one factory\n{USAGE}");
        std::process::exit(if words.is_empty() { 64 } else { 0 });
    }
    if !tehas_core::features::on("outsource") {
        stop(
            "outsource is off: TEHAS_OUTSOURCE is not on, or this release is still building it",
            78,
        );
    }
    let p = paths();
    let code = match words.as_slice() {
        ["join", factory, name, code] => join(&p, factory, name, code),
        ["run"] => run(&p),
        ["drain"] => drain(&p),
        ["doctor"] => check(&p),
        ["token"] => token(&p),
        ["relay"] => relay(&p),
        ["own", "--as", user, folders @ ..] if !folders.is_empty() => match job::Owner::parse(user)
        {
            Some(container) => own(&p, container, folders),
            None => stop(USAGE, 64),
        },
        _ => stop(USAGE, 64),
    };
    std::process::exit(code)
}

fn join(p: &Paths, factory: &str, name: &str, code: &str) -> i32 {
    match identity::join(&identity::path(&p.user_home), factory, name, code) {
        Ok(id) => {
            say(&format!("joined {} as {}", id.factory, id.name));
            0
        }
        Err(why) => stop(&why, 1),
    }
}

fn run(p: &Paths) -> i32 {
    let Some(id) = identity::read(&identity::path(&p.user_home)) else {
        stop(
            "this host has not joined a factory: run tehas-contractor join",
            78,
        );
    };
    let work: Vec<String> = match env("TEHAS_WORK_CMD") {
        c if c.trim().is_empty() => vec![
            p.home.join("bin/tehas").display().to_string(),
            "work".to_string(),
        ],
        c => c.split_whitespace().map(str::to_string).collect(),
    };
    // This program, which the cycle calls back for its token and its relay.
    let program = std::fs::read_link("/proc/self/exe")
        .unwrap_or_else(|_| PathBuf::from(std::env::args().next().unwrap_or_default()));
    let grace = env("TEHAS_WORK_GRACE").trim().parse().unwrap_or(300);
    let git_base = match env("TEHAS_CONTRACTOR_GIT") {
        b if b.is_empty() => "https://github.com".to_string(),
        b => b,
    };
    let host = Host {
        id,
        paths: p.clone(),
        program,
        work,
        git_base,
    };
    // Refused before the first wipe, which would remove a factory's files.
    if let Some(why) = host::foreign(p) {
        stop(&why, 78);
    }
    let _ = std::fs::remove_file(p.drain_file());
    job::run(&host, grace, 30)
}

fn drain(p: &Paths) -> i32 {
    match host::write_private(&p.drain_file(), b"drain\n") {
        Ok(()) => {
            say("draining: no new job is taken, a running one ends or is handed back");
            0
        }
        Err(why) => stop(&why, 1),
    }
}

fn check(p: &Paths) -> i32 {
    let vars: Vec<(String, String)> = std::env::vars().collect();
    let found = doctor::findings(p, &vars, std::path::Path::new("/sys"));
    for f in &found {
        say(f);
    }
    if found.is_empty() {
        say("nothing to say");
        0
    } else {
        3
    }
}

fn token(p: &Paths) -> i32 {
    let Some((factory, job_token)) = host::job_of(p) else {
        stop("no job is running here", 1);
    };
    match host::fetch_token(&factory, &job_token, &p.token_dir()) {
        Ok(t) => {
            tehas_core::outln!("{t}");
            0
        }
        Err(why) => stop(&why, 1),
    }
}

fn own(p: &Paths, container: job::Owner, folders: &[&str]) -> i32 {
    let asked: Vec<PathBuf> = folders.iter().map(PathBuf::from).collect();
    match host::own(p, container, &asked) {
        Ok(host::Handed::To(o)) => {
            say(&format!(
                "the cycle's folders are handed to {}:{}",
                o.uid, o.gid
            ));
            0
        }
        Ok(host::Handed::Unnamed) => {
            say("the job names no account: the cycle's folders stay as they are");
            0
        }
        Ok(host::Handed::Elsewhere { job, container }) => {
            say(&format!(
                "the container runs as {}:{}, not the job's {}:{}: the cycle's folders stay as they are",
                container.uid, container.gid, job.uid, job.gid
            ));
            0
        }
        Err(why) => stop(&why, 1),
    }
}

fn relay(p: &Paths) -> i32 {
    let mut text = String::new();
    let read = std::io::stdin()
        .take(4 << 20)
        .read_to_string(&mut text)
        .is_ok();
    let ask = serde_json::from_str::<Value>(&text).ok();
    // A vipune call (kind vipune) is passed on as it is; a failure reads as
    // vipune's, so the cycle goes on without memory.
    let who = match ask.as_ref().map(|a| &a["kind"]) {
        Some(k) if k == "vipune" => "vipune: the factory's memory",
        _ => "gh-shim: the relay",
    };
    let failed = |why: &str| {
        tehas_core::outln!(
            "{}",
            json!({ "out": "", "err": format!("{who} {why}"), "rc": 1 })
        );
        0
    };
    if !read {
        return failed("could not read the request");
    }
    let Some(ask) = ask else {
        return failed("got no request");
    };
    let Some((factory, job_token)) = host::job_of(p) else {
        return failed("has no job running");
    };
    let wait = Duration::from_secs(120);
    match http::post(&factory, "relay", As::Job(&job_token), &ask, wait) {
        Reply::Ok(v) => {
            tehas_core::outln!("{v}");
            0
        }
        Reply::No(_, word) => failed(&format!("was refused by the factory: {word}")),
        Reply::Down(why) => failed(&format!("got no answer from the factory: {why}")),
    }
}

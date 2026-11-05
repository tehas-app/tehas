//! `tehas-cycle sandbox`: the docker options of a container, for the commands
//! that still hold their docker line in bash. The lists are
//! tehas_core::sandbox's. Each verb prints its words NUL separated, so a
//! command reads them with `mapfile -d ''`.
//!
//! `smoke` and `smoke-verdict` are the build's: the words of one offline work
//! turn started inside an image that would take the uid tag, and the decision
//! on what that turn printed.

use crate::runner;
use std::env;
use std::io::{self, Read, Write};
use std::path::Path;
use tehas_core::models;
use tehas_core::sandbox::{self, Identity, Kind, Paths};
use tehas_core::slice::{self, Host, Load};

/// What the output of a sandbox smoke run says of the image. `Started` is the
/// one answer that lets the uid tag move; the other two both refuse it, and are
/// told apart only for the line the build says.
#[derive(Debug, PartialEq)]
enum Smoke {
    /// A stable `step-started` line: the runner reached its first step.
    Started,
    /// A result document whose `error.code` is `crash`: the runner died and
    /// said so.
    Crashed,
    /// Neither. Of an output with no `step-started` line the build cannot tell
    /// a runner that printed nothing from one that died without a document.
    Silent,
}

impl Smoke {
    fn word(&self) -> &'static str {
        match self {
            Smoke::Started => "started",
            Smoke::Crashed => "crashed",
            Smoke::Silent => "silent",
        }
    }
}

fn crash_document(v: &serde_json::Value) -> bool {
    v.get("schemaVersion").and_then(serde_json::Value::as_u64) == Some(runner::RESULT_SCHEMA)
        && v.pointer("/error/code").and_then(serde_json::Value::as_str) == Some("crash")
}

/// What the whole output of one offline `work` turn in an image says of it.
///
/// A `step-started` line counts wherever it stands, read as a cycle's log
/// reader reads it (`runner::stable`): a line of other text is a warning a tool
/// wrote to the same descriptor. A crash is read only from a document that is
/// the whole output, one line or printed indented, so a crash document after
/// `docker: command not found` answers nothing.
fn smoke_verdict(text: &str) -> Smoke {
    let started = runner::stable(text).any(|v| {
        v.get("kind").and_then(serde_json::Value::as_str) == Some("step-started")
            && !runner::hides(v.get("event"))
    });
    if started {
        return Smoke::Started;
    }
    let head = text.trim_start();
    let mut de = serde_json::Deserializer::from_str(head).into_iter::<serde_json::Value>();
    match de.next() {
        Some(Ok(v)) if head[de.byte_offset()..].trim().is_empty() && crash_document(&v) => {
            Smoke::Crashed
        }
        _ => Smoke::Silent,
    }
}

/// The words of the one offline turn the build runs inside a new image before
/// the uid tag moves onto it. The container has no network and small bounds of
/// its own, since it runs outside tehas.slice. The turn makes a commit in
/// `/tmp`, asks the runner for issue 1 with its NDJSON (stderr) and its result
/// (stdout) on one stream, and `sed` ends the stream at the first `step-started`
/// line, so the turn stops there rather than reach a step that needs a model.
fn smoke_run(image: &str) -> Vec<String> {
    let turn = "cd /tmp && git init -q r && cd r \
 && git -c user.email=a@b -c user.name=a commit -q --allow-empty -m x \
 && timeout 20 /opt/pi-rukas/bin/pi-rukas-headless work 1 --json 2>&1 \
 | sed '/\"kind\":\"step-started\"/q'";
    [
        "run",
        "--rm",
        "--init",
        "--network",
        "none",
        "--memory",
        "2g",
        "--pids-limit",
        "512",
        "--entrypoint",
        "bash",
        image,
        "-c",
        turn,
    ]
    .map(String::from)
    .to_vec()
}

/// The `tehas-cycle sandbox smoke-verdict` verb: the output of a smoke run
/// comes on standard input, at most `runner::LOG_TAIL` bytes of it. Prints
/// `started`, `crashed` or `silent` and ends 0 only for `started`. It reads no
/// settings: the words hold none and the decision reads only its input.
fn smoke_verdict_verb() -> i32 {
    let mut bytes = vec![];
    if let Err(e) = io::stdin().take(runner::LOG_TAIL).read_to_end(&mut bytes) {
        eprintln!("tehas-cycle: the smoke output cannot be read: {e}");
        return 1;
    }
    let verdict = smoke_verdict(&String::from_utf8_lossy(&bytes));
    tehas_core::outln!("{}", verdict.word());
    i32::from(verdict != Smoke::Started)
}

fn print(words: &[String]) -> i32 {
    let mut out = io::stdout().lock();
    for word in words {
        if out.write_all(word.as_bytes()).is_err() || out.write_all(b"\0").is_err() {
            return 1;
        }
    }
    0
}

fn limits(who: &str, what: &str) -> i32 {
    let taken = match sandbox::take(&Host::here(), &Load::from_env(), who, what) {
        Ok(taken) => taken,
        Err(line) => {
            eprintln!("{line}");
            return 78;
        }
    };
    let mut words = taken.limits;
    words.extend(taken.security);
    print(&words)
}

fn env_options(kind: Kind, tree: &str, name: &str, email: &str) -> i32 {
    let who = Identity { tree, name, email };
    print(&sandbox::env_options(
        kind,
        &who,
        &sandbox::ensemble(kind),
        &sandbox::tokens(),
    ))
}

// The options are held until the container's models.json and endpoints are
// written, so a run that could not write them is given none.
fn mounts(kind: Kind, p: &Paths) -> i32 {
    match sandbox::prepared_mounts(kind, p) {
        Ok(words) => {
            let state = Path::new(p.state);
            let out = state.join("models.json");
            match models::write_for_container(&out, &state.join("endpoints")) {
                Some(()) => print(&words),
                None => 1,
            }
        }
        Err(why) => {
            if !why.is_empty() {
                eprintln!("{why}");
            }
            1
        }
    }
}

fn verify(words: &[String]) -> i32 {
    match slice::verify(words, &Load::from_env()) {
        Some(n) => {
            tehas_core::outln!("{n}");
            0
        }
        None => 1,
    }
}

fn ok() -> i32 {
    match sandbox::settings_ok() {
        Ok(()) => 0,
        Err(line) => {
            eprintln!("{line}");
            78
        }
    }
}

/// The containers tehas work starts, each row its command, its image and what
/// it runs, read off the words of the run itself. The runner stands as the
/// libexec scripts name it. The build's smoke run is no row of this table: it is
/// started in the image this build has just made, whose tag no host's name is in,
/// so a row could name no image of it.
pub fn table() -> String {
    let image = "PI_ENSEMBLE_IMAGE";
    let none: [String; 0] = [];
    let runs = [
        sandbox::capabilities(image),
        sandbox::push_check(&none, "", &none, "", "", image),
        sandbox::install_run(&none, &none, "", "", "", image, ""),
        sandbox::cycle_run(&sandbox::CycleRun {
            name: "",
            limits: &none,
            security: &none,
            user: "",
            cid_file: "",
            tree: "",
            mounts: &none,
            images: &none,
            env: &none,
            hosts: &none,
            image,
            issue: 1,
            flags: &none,
        }),
    ];
    let rows = runs.iter().map(|run| {
        let entry = run.iter().position(|w| w == "--entrypoint");
        let at = entry.or_else(|| run.iter().position(|w| w == image));
        let cmd = match at.and_then(|i| run.get(i + 1)).map(String::as_str) {
            Some(sandbox::RUNNER) => "$runner",
            Some(other) => other,
            None => "",
        };
        format!("work\t{image}\t{cmd}")
    });
    rows.collect::<Vec<_>>().join("\n")
}

/// The exit status of one verb, None for words it does not take.
pub fn main(args: &[String]) -> Option<i32> {
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    Some(match a.as_slice() {
        ["ok"] => ok(),
        ["limits", who, what] => limits(who, what),
        ["hosts"] => print(&sandbox::hosts(
            &env::var("PI_ENSEMBLE_HOST_ALIASES").unwrap_or_default(),
        )),
        ["env", kind, tree, name, email] => env_options(Kind::parse(kind)?, tree, name, email),
        ["mounts", kind, tree, host, state, runs] => mounts(
            Kind::parse(kind)?,
            &Paths {
                tree,
                host,
                state,
                runs,
            },
        ),
        ["verify", ..] => verify(&args[1..]),
        ["table"] => {
            tehas_core::outln!("{}", table());
            0
        }
        ["retry", host] => {
            tehas_core::outln!("{}", sandbox::retry_line(host));
            0
        }
        ["clone", src, tree, branch] => i32::from(!sandbox::clone(src, tree, branch)),
        ["seed", state, index] => i32::from(!sandbox::seed(state, index)),
        ["smoke", image] => print(&smoke_run(image)),
        ["smoke-verdict"] => smoke_verdict_verb(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn started(step: &str) -> String {
        format!("{{\"v\":1,\"kind\":\"step-started\",\"event\":{{\"step\":\"{step}\"}}}}\n")
    }

    fn crash() -> String {
        "{\"schemaVersion\":5,\"issues\":[],\"error\":{\"code\":\"crash\",\"message\":\"the runner process exited 0 without a result\"}}\n".to_string()
    }

    #[test]
    fn a_step_started_line_is_a_started_run() {
        assert_eq!(smoke_verdict(&started("explore")), Smoke::Started);
        // A line of the runner's own text between two documents says nothing
        // about either, so the line below it counts.
        assert_eq!(
            smoke_verdict(&format!(
                "not json\n{}\n{}",
                "y".repeat(40),
                started("develop")
            )),
            Smoke::Started
        );
        // Whatever came before it, and whatever the line names.
        assert_eq!(
            smoke_verdict(&format!("{}{}", crash(), started("x"))),
            Smoke::Started
        );
        assert_eq!(
            smoke_verdict("{\"v\":1,\"kind\":\"step-started\",\"event\":null}\n"),
            Smoke::Started
        );
    }

    #[test]
    fn a_line_the_runner_marks_unstable_starts_nothing() {
        for marked in [
            "{\"v\":1,\"kind\":\"step-started\",\"unstable\":true,\"event\":{\"step\":\"x\"}}",
            "{\"v\":1,\"kind\":\"step-started\",\"unstable\":\"true\",\"event\":{\"step\":\"x\"}}",
            "{\"kind\":\"step-started\",\"event\":{\"step\":\"x\"}}",
            "{\"v\":2,\"kind\":\"step-started\",\"event\":{\"step\":\"x\"}}",
            "{\"v\":1,\"kind\":\"step-started\",\"event\":7}",
            "{\"v\":1,\"kind\":\"step-started\",\"event\":\"x\"}",
            "{\"v\":1,\"kind\":\"step-started\",\"event\":[]}",
            "{\"v\":1,\"kind\":\"log\",\"message\":\"step-started\"}",
        ] {
            assert_eq!(
                smoke_verdict(&format!("{marked}\n")),
                Smoke::Silent,
                "{marked}"
            );
        }
        // A mark of false or null leaves the line stable.
        for ok in [
            "{\"v\":1,\"kind\":\"step-started\",\"unstable\":false,\"event\":{\"step\":\"x\"}}",
            "{\"v\":1,\"kind\":\"step-started\",\"unstable\":null,\"event\":{\"step\":\"x\"}}",
        ] {
            assert_eq!(smoke_verdict(&format!("{ok}\n")), Smoke::Started, "{ok}");
        }
    }

    #[test]
    fn a_crash_document_is_named_as_one() {
        assert_eq!(smoke_verdict(&crash()), Smoke::Crashed);
        // The one result document of a turn that died, and the same document
        // printed indented: both are the shape of a runner that died and said
        // so, and in both the document is the first thing the output said.
        assert_eq!(smoke_verdict(crash().trim_end()), Smoke::Crashed);
        assert_eq!(
            smoke_verdict("{\n  \"schemaVersion\": 5,\n  \"error\": { \"code\": \"crash\" }\n}\n"),
            Smoke::Crashed
        );
    }

    #[test]
    fn no_other_document_is_a_crash() {
        for other in [
            // Another code, or no code at all.
            "{\"schemaVersion\":5,\"error\":{\"code\":\"timeout\"}}",
            "{\"schemaVersion\":5,\"error\":{}}",
            // An error of the wrong type stops this reader too.
            "{\"schemaVersion\":5,\"error\":\"crash\"}",
            "{\"schemaVersion\":5,\"error\":7}",
            // Not the schema tehas reads.
            "{\"schemaVersion\":3,\"error\":{\"code\":\"crash\"}}",
            "{\"error\":{\"code\":\"crash\"}}",
            // A number, not a string.
            "{\"schemaVersion\":5,\"error\":{\"code\":7}}",
        ] {
            assert_eq!(
                smoke_verdict(&format!("{other}\n")),
                Smoke::Silent,
                "{other}"
            );
        }
    }

    #[test]
    fn an_output_with_nothing_to_say_is_silent() {
        for nothing in ["", "\n", "   \n\n", "docker: command not found\n"] {
            assert_eq!(smoke_verdict(nothing), Smoke::Silent, "{nothing:?}");
        }
        // Text before the first document means the document is part of what
        // something else printed, and is read as no document.
        assert_eq!(
            smoke_verdict(&format!("bash: line 1: jq: not found\n{}", crash())),
            Smoke::Silent
        );
        // A line that is no JSON, where a started line would have counted.
        assert_eq!(
            smoke_verdict("{\"v\":1,\"kind\":\"step-started\",\"event\":{\"step\":\"x\"}\n"),
            Smoke::Silent
        );
    }

    #[test]
    fn the_run_is_one_offline_work_turn_with_its_own_bound() {
        let words = smoke_run("tehas/pi-rukas:260101-0000");
        let held: Vec<&str> = words.iter().map(String::as_str).collect();
        assert_eq!(
            held[..12],
            [
                "run",
                "--rm",
                "--init",
                "--network",
                "none",
                "--memory",
                "2g",
                "--pids-limit",
                "512",
                "--entrypoint",
                "bash",
                "tehas/pi-rukas:260101-0000",
            ]
        );
        assert_eq!(held[12], "-c");
        assert_eq!(held.len(), 14);
        let turn = held[13];
        assert!(turn.contains("git init"), "{turn}");
        assert!(turn.contains("--allow-empty"), "{turn}");
        assert!(
            turn.contains(
                "timeout 20 /opt/pi-rukas/bin/pi-rukas-headless work 1 --json 2>&1 | sed "
            ),
            "{turn}"
        );
        for secret in ["--model", "api_key", "token", "http"] {
            assert!(
                !turn.contains(secret),
                "{secret} is in the smoke turn: {turn}"
            );
        }
    }

    #[test]
    fn the_shared_fixture_is_a_started_run() {
        let fixture = include_str!("../../../test/fixtures/smoke-started.json");
        assert_eq!(smoke_verdict(fixture), Smoke::Started);
    }
}

//! `tehas-cycle sandbox`: the docker options of a container, for the commands
//! that still hold their docker line in bash. The lists are
//! tehas_core::sandbox's. Each verb prints its words NUL separated, so a
//! command reads them with `mapfile -d ''`.
//!
//! `smoke` is the other thing this program answers for the build: the words of
//! one offline work turn started inside an image that would take the uid tag,
//! and the decision on what that turn printed ([`smoke_verdict`]).

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
pub enum Smoke {
    /// A stable `step-started` line: the runner reached its first step.
    Started,
    /// A result document whose `error.code` is `crash`: the runner died and
    /// said so.
    Crashed,
    /// Neither. Of an output with no `step-started` line the build cannot tell
    /// a runner that printed nothing from one that died without a document.
    Silent,
}

/// The result schema a smoke run's crash document is read at: the one tehas
/// reads everywhere else, held here rather than taken from `runner`, which is
/// the reader of a cycle's log and nothing of a build's.
const RESULT_SCHEMA: u64 = 5;

/// The NDJSON schema a `step-started` line is read at.
const NDJSON_SCHEMA: u64 = 1;

fn crash_document(v: &serde_json::Value) -> bool {
    v.get("schemaVersion").and_then(serde_json::Value::as_f64) == Some(RESULT_SCHEMA as f64)
        && v.get("error")
            .and_then(|e| e.as_object())
            .and_then(|e| e.get("code"))
            .and_then(serde_json::Value::as_str)
            == Some("crash")
}

/// Whether one document of the runner's NDJSON is a stable `step-started` line:
/// a JSON object at v 1, not marked unstable, whose `event` holds no step of its
/// own to refuse. A line the runner marks unstable publishes nothing, so it
/// starts no run; a line whose version or event is neither of what the schema
/// says is no answer, and is read as silence, as `runner::run` reads it for a
/// cycle.
fn started_line(v: &serde_json::Value) -> bool {
    v.is_object()
        && v.get("v").and_then(serde_json::Value::as_f64) == Some(NDJSON_SCHEMA as f64)
        && v.get("kind").and_then(serde_json::Value::as_str) == Some("step-started")
        && matches!(
            v.get("unstable"),
            None | Some(serde_json::Value::Null) | Some(serde_json::Value::Bool(false))
        )
        && matches!(
            v.get("event"),
            None | Some(serde_json::Value::Null) | Some(serde_json::Value::Object(_))
        )
}

/// What the whole output of one offline `work` turn in an image says of it.
///
/// The two questions about the output are asked of the same reading of it, the
/// stream of JSON documents it is, and answered apart. One document per line is
/// how a turn prints and one document printed indented is what a runner that
/// died leaves, and the stream reads both, so no line has to hold a document
/// whole.
///
/// A `step-started` document counts wherever the stream reads it: a line of
/// text between two of them is a warning a tool wrote to the same descriptor,
/// and says nothing about the documents above or below it. A document the runner
/// marks `unstable` publishes nothing and starts no run.
///
/// A crash is read from the document that begins the output and from no other.
/// Where the stream stopped says whether the output was all documents or text
/// stood at its head: text there says the bytes are no turn's NDJSON at all, so
/// a crash document that follows `docker: command not found` answers nothing.
///
/// Whether the run was cut off at its bound is no part of this: a document read
/// whole and matching is an answer whoever read it managed to bring back.
pub fn smoke_verdict(text: &str) -> Smoke {
    // The answer is read twice, from the same lines, because a line of text in
    // a turn's output means two different things.
    //
    // A `step-started` line is looked for line by line, whatever stands around
    // it: the runner prints one document per line, and a line of text between
    // two of them is a warning a tool wrote to the same descriptor, which says
    // nothing about the documents above or below it. A line the runner marks
    // `unstable` publishes nothing and starts no run.
    //
    // A crash is read from the document that begins the output and from no
    // other, which is a question about the head of the output alone: text there
    // says the bytes are no turn's NDJSON at all, so a crash document that
    // follows `docker: command not found` answers nothing. The whole of the head
    // is asked of a stream of JSON rather than of one line, since one document
    // printed indented - what a runner that died leaves behind - is no document
    // to any single line of it.
    for line in text.lines() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) {
            if started_line(&v) {
                return Smoke::Started;
            }
        }
    }
    // The document that begins the output: one document per line is read line
    // by line, and one document printed indented - no document to any single
    // line of it, and what a runner that died leaves behind - is read as the one
    // document it is. The walk ends where text ends it, so a document after text
    // began nothing and names nothing.
    let head = text.trim_start();
    let mut de = serde_json::Deserializer::from_str(head).into_iter::<serde_json::Value>();
    let first = match de.next() {
        // The stream answers one document and where it stopped; everything the
        // bytes left are whitespace, an output of nothing but documents begins
        // with a document, and anything else says a document follows text.
        Some(Ok(v)) if head[de.byte_offset()..].chars().all(char::is_whitespace) => Some(v),
        _ => None,
    };
    match first {
        Some(v) if crash_document(&v) => Smoke::Crashed,
        _ => Smoke::Silent,
    }
}

/// The words of the one offline turn the build runs inside a new image before
/// the uid tag moves onto it: the words of `docker run`, the image, and the
/// script asked of bash. The image is a word the caller gives, so no host's tag
/// is held here. The turn needs no token, no model and no network: it makes a
/// commit in the container's `/tmp` and asks the runner for issue 1, and the
/// first `step-started` line is the answer, which is why the inner bound cuts the
/// turn there rather than letting it reach a step that needs a model.
pub fn smoke_run(image: &str) -> Vec<String> {
    let turn = "cd /tmp && git init -q r && cd r \
 && git -c user.email=a@b -c user.name=a commit -q --allow-empty -m x \
 && timeout 20 /opt/pi-rukas/bin/pi-rukas-headless work 1 --json";
    vec![
        "run".into(),
        "--rm".into(),
        "--init".into(),
        "--entrypoint".into(),
        "bash".into(),
        image.into(),
        "-c".into(),
        turn.into(),
    ]
}

/// The `tehas-cycle smoke` verb: the output of a smoke run comes on standard
/// input, as the `caps` verbs take their answer. Ends 0 on a `step-started` line
/// and 1 on a crash or on no such line, and says which of the three it saw, so a
/// build that refuses an image says why.
pub fn smoke_verb() -> ! {
    let mut bytes = vec![];
    match io::stdin().read_to_end(&mut bytes) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("tehas-cycle: the smoke output cannot be read: {e}");
            std::process::exit(1);
        }
    }
    match smoke_verdict(&String::from_utf8_lossy(&bytes)) {
        Smoke::Started => {
            tehas_core::outln!("started");
            std::process::exit(0)
        }
        Smoke::Crashed => {
            tehas_core::outln!("crashed");
            std::process::exit(1)
        }
        Smoke::Silent => {
            tehas_core::outln!("silent");
            std::process::exit(1)
        }
    }
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
        let at = run.iter().position(|w| w == image).unwrap_or(0);
        let cmd = match run.get(at + 1).map(String::as_str) {
            Some(sandbox::RUNNER) => "$runner",
            Some("push") => "git",
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
            &held[..5],
            ["run", "--rm", "--init", "--entrypoint", "bash"]
        );
        assert_eq!(held[5], "tehas/pi-rukas:260101-0000");
        assert_eq!(held[6], "-c");
        let turn = held[7];
        // The turn stands on its own: a commit of its own making, and one ask of
        // the runner, bounded inside the container so a runner that hangs is no
        // reason for the build to wait.
        assert!(turn.contains("git init"), "{turn}");
        assert!(turn.contains("--allow-empty"), "{turn}");
        assert!(turn.contains("timeout 20 "), "{turn}");
        assert!(
            turn.contains("/opt/pi-rukas/bin/pi-rukas-headless work 1 --json"),
            "{turn}"
        );
        // No token, no model and no endpoint stands in the words: the turn is
        // offline, and a run that reached a model would be a different check.
        for secret in ["--model", "api_key", "token", "http"] {
            assert!(
                !turn.contains(secret),
                "{secret} is in the smoke turn: {turn}"
            );
        }
    }
}

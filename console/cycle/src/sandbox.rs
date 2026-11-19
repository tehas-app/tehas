//! `tehas-cycle sandbox`: the docker options of a container, for the commands
//! that still hold their docker line in bash. The lists are
//! tehas_core::sandbox's. Each verb prints its words NUL separated, so a
//! command reads them with `mapfile -d ''`.
//!
//! `smoke-run` is the build's: one offline work turn started inside an image
//! that would take the uid tag, and the decision on what that turn printed.

use crate::runner;
use std::io::{self, Read, Write};
use std::process::{Command, Stdio};
use tehas_core::sandbox;
use tehas_core::slice::{self, Host, Load};

/// What the output of a sandbox smoke run says of the image. `Started` is the
/// one answer that lets the uid tag move; the others refuse it, and are told
/// apart only for the line the build says.
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
    /// The runner started, but vipune wrote to stderr on one CPU of a cpuset.
    Noisy,
}

impl Smoke {
    fn word(&self) -> &'static str {
        match self {
            Smoke::Started => "started",
            Smoke::Crashed => "crashed",
            Smoke::Silent => "silent",
            Smoke::Noisy => "noisy",
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
        sandbox::NEVER_PULL,
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

/// The words of the build's vipune check: an add and a search on a scratch
/// store, as the cycle's user, offline, under a cpuset of one CPU. vipune sets
/// no onnxruntime thread count of its own, and onnxruntime then pins a thread
/// to each host core; under a cpuset every pin outside it fails and is written
/// to stderr on every call. `taskset` does not show it: only a cpuset refuses
/// the pin. The thread count the image sets stops it, so the run sets none
/// of its own. Their stdout goes nowhere, so the whole output of a good
/// image is the `quiet` line.
fn vipune_run(image: &str, uid: &str, cpu: u32) -> Vec<String> {
    let turn = "db=$(mktemp -d) && export HF_HUB_OFFLINE=1 \
 && vipune add 'the sandbox smoke run' --json --db-path \"$db/smoke.db\" >/dev/null \
 && vipune search 'sandbox smoke run' --json --limit 1 --db-path \"$db/smoke.db\" >/dev/null \
 && echo quiet";
    let cpu = cpu.to_string();
    [
        "run",
        sandbox::NEVER_PULL,
        "--rm",
        "--init",
        "--network",
        "none",
        "--memory",
        "2g",
        "--pids-limit",
        "512",
        "--cpuset-cpus",
        &cpu,
        "--user",
        uid,
        "-e",
        "HOME=/home/vscode",
        "--entrypoint",
        "bash",
        image,
        "-c",
        turn,
    ]
    .map(String::from)
    .to_vec()
}

fn vipune_quiet(text: &str) -> bool {
    text == "quiet\n"
}

/// The CPUs of a list such as `0-3,8,10-11`, as the kernel and docker write
/// them. Anything else is no list.
fn cpu_list(list: &str) -> Option<Vec<u32>> {
    let mut cpus = vec![];
    for part in list.trim().split(',') {
        let (lo, hi) = part.split_once('-').unwrap_or((part, part));
        let (lo, hi) = (lo.parse::<u32>().ok()?, hi.parse::<u32>().ok()?);
        if lo > hi {
            return None;
        }
        cpus.extend(lo..=hi);
    }
    Some(cpus)
}

/// The CPUs of `Cpus_allowed_list` in a `/proc/<pid>/status`.
fn allowed_cpus(status: &str) -> Option<Vec<u32>> {
    cpu_list(
        status
            .lines()
            .find_map(|l| l.strip_prefix("Cpus_allowed_list:"))?,
    )
}

/// The highest CPU of `Cpus_allowed_list` in a `/proc/<pid>/status`.
fn highest_cpu(status: &str) -> Option<u32> {
    allowed_cpus(status)?.into_iter().max()
}

/// The CPUs docker says it has, when it refused a `--cpuset-cpus`. dockerd
/// checks the option against the CPUs it saw when it started, so in a
/// container whose cpuset moved since (Incus with `limits.cpu`) it refuses a
/// CPU the build may run on.
fn docker_cpus(out: &str) -> Option<Vec<u32>> {
    let line = out
        .lines()
        .find(|l| l.contains("Requested CPUs are not available"))?;
    cpu_list(line.split_once("available: ")?.1)
}

/// The highest CPU both this process and docker take, for a second try.
fn shared_cpu(ours: &[u32], docker: &[u32]) -> Option<u32> {
    ours.iter().copied().filter(|c| docker.contains(c)).max()
}

/// What a command printed on both streams, as `2>&1` puts them on one pipe,
/// at most `runner::LOG_TAIL` bytes of it. The read end closes there, so a
/// command that goes on printing ends on its next write.
fn merged_head(cmd: &mut Command) -> io::Result<Vec<u8>> {
    let (reader, writer) = io::pipe()?;
    cmd.stdin(Stdio::null())
        .stdout(writer.try_clone()?)
        .stderr(writer);
    let child = cmd.spawn();
    // The command holds the writers; the read ends only once they are closed.
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    let mut child = child?;
    let mut bytes = vec![];
    let read = reader.take(runner::LOG_TAIL).read_to_end(&mut bytes);
    let _ = child.wait();
    read.map(|_| bytes)
}

/// The `tehas-cycle sandbox smoke-run <image> <uid>` verb: the turn of
/// `smoke_run` in <image>, bounded at 40 s, and the verdict on what it printed;
/// then, on a started run, the vipune check of `vipune_run` as <uid>, bounded
/// the same, which must print `quiet` alone. Prints `started`, `crashed`,
/// `silent` or `noisy` and ends 0 only for `started`. Docker's own status is
/// not read: the output decides. It reads no settings.
fn smoke_run_verb(image: &str, uid: &str) -> i32 {
    if uid.is_empty() || !uid.bytes().all(|b| b.is_ascii_digit()) {
        eprintln!("tehas-cycle: sandbox smoke-run takes a numeric uid, not {uid:?}");
        return 64;
    }
    let bounded = |words: Vec<String>| {
        let mut cmd = Command::new("timeout");
        cmd.args(["-k", "5", "40", "docker"]).args(words);
        merged_head(&mut cmd)
    };
    let bytes = match bounded(smoke_run(image)) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("tehas-cycle: the smoke run of {image} could not be started: {e}");
            return 1;
        }
    };
    let mut verdict = smoke_verdict(&String::from_utf8_lossy(&bytes));
    if verdict == Smoke::Started {
        let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        let Some(cpu) = highest_cpu(&status) else {
            eprintln!("tehas-cycle: no CPU this process may run on is named in /proc/self/status");
            return 1;
        };
        let vipune = |cpu: u32| match bounded(vipune_run(image, uid, cpu)) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(e) => format!("{e}\n"),
        };
        let mut cpu = cpu;
        let mut out = vipune(cpu);
        // dockerd may refuse a CPU the build runs on: its list is from its
        // start. Any one CPU shows a lost thread count, so try one it takes.
        if let Some(docker) = docker_cpus(&out) {
            let ours = allowed_cpus(&status).unwrap_or_default();
            match shared_cpu(&ours, &docker) {
                Some(other) => {
                    eprintln!("tehas-cycle: docker refused CPU {cpu}; the vipune check runs on CPU {other}");
                    cpu = other;
                    out = vipune(cpu);
                }
                None => eprintln!(
                    "tehas-cycle: docker takes none of the CPUs this process may run on; restart docker so it reads its cpuset again"
                ),
            }
        }
        if !vipune_quiet(&out) {
            eprintln!("tehas-cycle: vipune in {image} on CPU {cpu} of a cpuset printed:");
            eprint!("{out}");
            verdict = Smoke::Noisy;
        }
    }
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

fn verify(words: &[String]) -> i32 {
    match slice::verify(words, &Load::from_env()) {
        Some(n) => {
            tehas_core::outln!("{n}");
            0
        }
        None => 1,
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
        sandbox::push_check(&none, "", &none, &none, "", "", image),
        sandbox::install_run(&none, "", &none, "", "", image, ""),
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
            verb: "work",
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
        ["limits", who, what] => limits(who, what),
        ["verify", ..] => verify(&args[1..]),
        ["table"] => {
            tehas_core::outln!("{}", table());
            0
        }
        ["retry", host] => {
            tehas_core::outln!("{}", sandbox::retry_line(host));
            0
        }
        ["smoke-run", image, uid] => smoke_run_verb(image, uid),
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
            held[..13],
            [
                "run",
                "--pull=never",
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
        assert_eq!(held[13], "-c");
        assert_eq!(held.len(), 15);
        let turn = held[14];
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
    fn the_vipune_run_is_on_one_cpu_as_the_cycle_user_and_offline() {
        let words = vipune_run("tehas/pi-rukas:260101-0000", "1000", 7);
        let held: Vec<&str> = words.iter().map(String::as_str).collect();
        assert_eq!(
            held[..20],
            [
                "run",
                "--pull=never",
                "--rm",
                "--init",
                "--network",
                "none",
                "--memory",
                "2g",
                "--pids-limit",
                "512",
                "--cpuset-cpus",
                "7",
                "--user",
                "1000",
                "-e",
                "HOME=/home/vscode",
                "--entrypoint",
                "bash",
                "tehas/pi-rukas:260101-0000",
                "-c",
            ]
        );
        assert_eq!(held.len(), 21);
        let turn = held[20];
        assert!(turn.contains("vipune add "), "{turn}");
        assert!(turn.contains("vipune search "), "{turn}");
        assert_eq!(
            turn.matches("--db-path \"$db/smoke.db\" >/dev/null")
                .count(),
            2
        );
        assert!(turn.ends_with("&& echo quiet"), "{turn}");
        // The thread count has to come from the image, or the run proves nothing.
        assert!(!turn.contains("ORT_"), "{turn}");
        assert!(!held.iter().any(|w| w.contains("ORT_")));
    }

    #[test]
    fn only_the_marker_alone_is_quiet() {
        assert!(vipune_quiet("quiet\n"));
        for noisy in [
            "",
            "quiet",
            "pthread_setaffinity_np failed for thread: 12, index: 3, mask: {4, }, error code: 22\nquiet\n",
            "quiet\nwarning\n",
            include_str!("../../../test/fixtures/smoke-started.json"),
        ] {
            assert!(!vipune_quiet(noisy), "{noisy:?}");
        }
    }

    #[test]
    fn the_cpu_is_the_highest_one_this_process_may_run_on() {
        let status = "Name:\tx\nCpus_allowed:\tffff\nCpus_allowed_list:\t0-3,8,10-11\nMems_allowed_list:\t0\n";
        assert_eq!(highest_cpu(status), Some(11));
        assert_eq!(highest_cpu("Cpus_allowed_list:\t0-15\n"), Some(15));
        assert_eq!(highest_cpu("Cpus_allowed_list:\t0,2,4\n"), Some(4));
        assert_eq!(highest_cpu("Cpus_allowed_list:\t3\n"), Some(3));
        for none in [
            "",
            "Cpus_allowed:\tffff\n",
            "Cpus_allowed_list:\t\n",
            "Cpus_allowed_list:\tx\n",
        ] {
            assert_eq!(highest_cpu(none), None, "{none:?}");
        }
    }

    /// An Incus container with `limits.cpu` moves across CPUs; dockerd keeps
    /// the list it read at its start and refuses a CPU outside it. The lists
    /// are the ones a factory saw (#2519).
    #[test]
    fn a_cpu_docker_refuses_is_tried_again_on_one_both_take() {
        let out = "docker: Error response from daemon: Requested CPUs are not available - requested 79, available: 12,19-20,29-30,32,35,39,46,49,55,65,69,74,87,95\n\nRun 'docker run --help' for more information\n";
        let docker = docker_cpus(out).unwrap();
        assert_eq!(docker.len(), 16);
        let status = "Cpus_allowed_list:\t4,13,17,37,40,47-48,50-51,53,63,67-69,72,79\n";
        let ours = allowed_cpus(status).unwrap();
        assert_eq!(ours.len(), 16);
        assert_eq!(highest_cpu(status), Some(79));
        assert_eq!(shared_cpu(&ours, &docker), Some(69));
        assert_eq!(shared_cpu(&[1, 2], &docker), None);
        for other in [
            "quiet\n",
            "pthread_setaffinity_np failed for thread: 12, index: 3, mask: {4, }, error code: 22\n",
            "docker: Error response from daemon: Requested CPUs are not available - requested 79, available: x\n",
        ] {
            assert_eq!(docker_cpus(other), None, "{other:?}");
        }
    }

    #[test]
    fn a_cpu_list_is_ranges_and_single_cpus() {
        assert_eq!(cpu_list("0-3,8,10-11"), Some(vec![0, 1, 2, 3, 8, 10, 11]));
        assert_eq!(cpu_list(" 7\n"), Some(vec![7]));
        for none in ["", "x", "3-1", "1,,2", "1-"] {
            assert_eq!(cpu_list(none), None, "{none:?}");
        }
    }

    #[test]
    fn the_verb_takes_an_image_and_a_numeric_uid() {
        let args = |w: &[&str]| w.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(main(&args(&["smoke-run", "img"])), None);
        assert_eq!(main(&args(&["smoke-run", "img", "vscode"])), Some(64));
        assert_eq!(main(&args(&["smoke-run", "img", ""])), Some(64));
    }

    #[test]
    fn both_streams_are_read_as_one() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo out; echo err >&2; echo end"]);
        let bytes = merged_head(&mut cmd).unwrap();
        assert_eq!(String::from_utf8_lossy(&bytes), "out\nerr\nend\n");
    }

    #[test]
    fn an_endless_output_is_read_to_the_tail_and_not_waited_on() {
        let mut cmd = Command::new("yes");
        let bytes = merged_head(&mut cmd).unwrap();
        assert_eq!(bytes.len() as u64, runner::LOG_TAIL);
    }

    #[test]
    fn a_command_that_cannot_start_is_an_error() {
        let mut cmd = Command::new("/nonexistent/tehas-no-such-program");
        assert!(merged_head(&mut cmd).is_err());
    }

    #[test]
    fn the_shared_fixture_is_a_started_run() {
        let fixture = include_str!("../../../test/fixtures/smoke-started.json");
        assert_eq!(smoke_verdict(fixture), Smoke::Started);
    }
}

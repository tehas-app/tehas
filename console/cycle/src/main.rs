//! tehas-cycle: a cycle's report in Rust, beside libexec/cycle. It acts on
//! nothing on GitHub or in the state store.
//!
//! Usage:
//!   tehas-cycle report <facts.json>   print the report of an ended cycle,
//!                                     as JSON, from the facts in the file
//!                                     (src/facts.rs) and the answers of
//!                                     GitHub it holds under "github"
//!   tehas-cycle shadow <issue> --attempt <n> --rc <n> --started <s>
//!       --signalled 0|1 --by <login> --how <how> --approval-at <s>
//!       --requeued 0|1
//!                                     just before libexec/cycle reports:
//!                                     decide the report from the host and
//!                                     keep it in state/cycles/shadow-<issue>.json
//!   tehas-cycle compare <issue>       after the report: set the bash's
//!                                     beside it and add one line to
//!                                     logs/cycle-shadow.log
//!   tehas-cycle last-attempt <issue> <result.json> <workspace>
//!                                     the issue as GitHub's REST API answers
//!                                     it on standard input: what a cycle
//!                                     that ended at child-timeout:develop
//!                                     does with its body, as JSON
//!                                     {"action": "none"|"edit"|"repeat",
//!                                     "body": <the body to write>,
//!                                     "say": <a repeat's words for the park>}
//!                                     (src/last_attempt.rs)
//!   tehas-cycle run <issue>           what tehas-cycle@.service starts:
//!                                     copy the install's code for this
//!                                     cycle and run tehasbot cycle from the
//!                                     copy (src/pin.rs)
//!   tehas-cycle pin plan <issue> <program> [args...]
//!                                     what the dispatcher starts a planning
//!                                     turn with: the same copy, and run
//!                                     libexec/<program> of it with the args
//!   tehas-cycle clock now             print the seconds of the monotonic
//!                                     clock: seconds since boot, read from
//!                                     the first number of /proc/uptime
//!                                     (src/clock.rs)
//!   tehas-cycle clock mark <file>     write that number into <file>, for
//!                                     the mark of a cycle's start and the
//!                                     mark of its last log growth
//!   tehas-cycle clock check <file> --limit <s> --stall <s> --since <s>
//!                                     <file> holds the mark of the start and
//!                                     <s> after --since is a number from
//!                                     `clock now`, the mark of the last growth
//!                                     of the cycle's logs: print `ok`,
//!                                     `stalled` or `over` and exit 0, 1 or 2.
//!                                     The limit is counted before the stall.
//!                                     A mark that is missing or not a number,
//!                                     or a time that would go backwards,
//!                                     prints `unknown` and exits 3: nothing
//!                                     is stopped on a guess. It reads no wall
//!                                     clock, so a VM that was saved is not
//!                                     counted against a cycle
//!   tehas-cycle describe <dir>        write <dir>/images.md, a model's
//!                                     description of each image in
//!                                     <dir>/manifest.json, and name it in the
//!                                     manifest (src/describe.rs). Exits 1 only
//!                                     when a file could not be written
//!   tehas-cycle private <models.json> <email>
//!                                     the names a mask hides, two lines each:
//!                                     `--private` and the name, for a command
//!                                     to read into its arguments
//!   tehas-cycle renew --cmd <path> --every <s> --retry <s>
//!                                     the renewer of a cycle's GitHub token:
//!                                     run <path> with its output discarded,
//!                                     wait --every seconds after a success
//!                                     and --retry after a failure, and say
//!                                     one line on standard error for each
//!                                     failure, never the command's output
//!                                     (src/renew.rs). It ends 0 on TERM.
//!   tehas-cycle finish <issue> --home <dir> --repo <owner/name> --default <branch>
//!       --reason <text>
//!                                     the factory opens the draft pull
//!                                     request of a stopped cycle: the kept
//!                                     workspace `<home>/work/<name>-<issue>`,
//!                                     its branch `feature/issue-<issue>-*`
//!                                     rebased onto `origin/<branch>` in a
//!                                     temporary worktree, pushed as
//!                                     `finish/issue-<issue>`, and opened with
//!                                     `gh pr create --draft`. The workspace and
//!                                     its branch never move. It prints the URL,
//!                                     or one line on standard error and exits 1
//!                                     for a refusal that changed nothing
//!                                     (src/finish.rs)
//!   tehas-cycle caps unsupported <flags> <names> [<features>]
//!                                     the image's --capabilities answer on
//!                                     standard input, as every caps verb
//!                                     takes it: what it does not support,
//!                                     one line each (src/caps.rs). <flags>
//!                                     are the work arguments, <names> the
//!                                     inputs, <features> the features, each
//!                                     space separated. Exits 1 on an answer
//!                                     that cannot be read
//!   tehas-cycle caps expired <workarounds>
//!                                     which NAME@VERSION entries the image's
//!                                     pi-rukas no longer needs, one line each
//!   tehas-cycle caps mode <feature>   how tehas gets a feature: `native`,
//!                                     `polyfill` when lib/polyfill-<feature>.sh
//!                                     stands beside this program's libexec/,
//!                                     or `off`. `error` and exit 1 on an
//!                                     answer that cannot be read, `off` and
//!                                     exit 1 on no feature name
//!   tehas-cycle caps verb <verb> <flags>
//!                                     exit 0 when the runner takes <verb>
//!                                     with all of <flags>, else 1
//!   tehas-cycle caps reads-images     1 when the runner reads the image
//!                                     manifest tehas writes, else 0
//!   tehas-cycle runner outcome <result> <issue>
//!                                     the issue's entry of a schema 5 result
//!                                     as one JSON object. Exits 4 when there
//!                                     is none, 2 when the file or the number
//!                                     cannot be read
//!   tehas-cycle runner run <ndjson> <issue>
//!                                     three lines of a run log: the last
//!                                     step started, the branch of its last
//!                                     push when it is the issue's, and
//!                                     `whole` or, past 200 KB, `truncated`.
//!                                     Exits 1 with nothing printed when
//!                                     <issue> is no number or the log is no
//!                                     file it can read
//!   tehas-cycle -h|--help             print this text
//!
//! shadow and compare do nothing while $TEHAS_HOME/state/cycle-shadow.on is
//! not there. They read $TEHAS_HOME, TEHAS_REPO and TEHAS_WORK_DIR from the
//! environment, as tehas_load leaves them, and gh and the store.
//!
//! last-attempt reads what it is given and changes nothing.
//!
//! run hardlinks $TEHAS_HOME's bin/, libexec/ and lib/ into
//! $TEHAS_HOME/run/cycle-<issue>-<pid>/, removes the copies of processes that
//! are gone, and execs <copy>/bin/tehasbot cycle <issue> with TEHAS_CODE set to
//! the copy. A copy it cannot make is said on standard error and the install's
//! tehasbot runs instead. It returns only when neither could be run. pin plan
//! does the same under $TEHAS_HOME/run/plan-<issue>-<pid>/.
//!
//! Exit: 0 it printed or kept its answer; 1 what it reads could not be
//! read, said on standard error, or run could not exec, or `clock check` said
//! `stalled`, or `finish` refused, or a caps or runner verb said no as above;
//! 2 `clock check` said `over`; 3 `clock check` said `unknown`; 64 an argument
//! it does not know; 78 run without an absolute TEHAS_HOME.
//!
//! finish is asked the whole of its words and reads no setting of its own, so it
//! takes no TEHAS_HOME: part 2's poll step, the only caller today, passes the
//! four it needs. Its reason is one argument and never a file of the asker's.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tehas_cycle::host::Start;
use tehas_cycle::last_attempt::{self, Action};
use tehas_cycle::report::{to_json, Given};
use tehas_cycle::{caps, clock, runner};
use tehas_cycle::{finish, pin, renew, report, shadow, Facts};

fn usage() -> String {
    include_str!("main.rs")
        .lines()
        .skip(2)
        .take_while(|l| l.starts_with("//!"))
        .map(|l| l.trim_start_matches("//!").strip_prefix(' ').unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

fn wrong(args: &[String]) -> ! {
    eprintln!(
        "tehas-cycle: unknown invocation: {}\n\n{}",
        args.join(" "),
        usage()
    );
    std::process::exit(64);
}

fn fail(e: String) -> ! {
    eprintln!("tehas-cycle: {e}");
    std::process::exit(1);
}

fn issue(s: &str, args: &[String]) -> u64 {
    s.parse()
        .ok()
        .filter(|n| *n > 0)
        .unwrap_or_else(|| wrong(args))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let home = PathBuf::from(std::env::var("TEHAS_HOME").unwrap_or_default());
    let on = || home.join("state/cycle-shadow.on").exists();
    match a.as_slice() {
        ["-h" | "--help"] => println!("{}", usage()),
        ["report", file] => print_report(file),
        ["last-attempt", n, result, workspace] => {
            last_attempt(issue(n, &args), result, Path::new(workspace))
        }
        ["shadow", n, rest @ ..] => {
            let n = issue(n, &args);
            let mut start = Start {
                issue: n,
                attempt: 0,
                rc: 0,
                started: 0,
                signalled: false,
                by: String::new(),
                how: String::new(),
                approval_at: String::new(),
                requeued: false,
            };
            let mut it = rest.iter();
            while let Some(flag) = it.next() {
                let v = it.next().unwrap_or_else(|| wrong(&args)).to_string();
                let num = |v: &str| v.parse::<i64>().unwrap_or_else(|_| wrong(&args));
                match *flag {
                    "--attempt" => {
                        start.attempt = num(&v).try_into().unwrap_or_else(|_| wrong(&args))
                    }
                    "--rc" => start.rc = num(&v),
                    "--started" => start.started = num(&v),
                    "--signalled" => start.signalled = v == "1",
                    "--by" => start.by = v,
                    "--how" => start.how = v,
                    "--approval-at" => start.approval_at = v,
                    "--requeued" => start.requeued = v == "1",
                    _ => wrong(&args),
                }
            }
            if start.attempt == 0 {
                wrong(&args);
            }
            if on() {
                shadow::begin(&home, &start).unwrap_or_else(|e| fail(e));
            }
        }
        ["clock", "now"] => {
            if clock::print_now(clock::boot_seconds).is_none() {
                fail("the clock could not be read".to_string());
            }
        }
        ["clock", "mark", file] => {
            clock::mark(Path::new(file), clock::boot_seconds).unwrap_or_else(|e| fail(e));
        }
        ["clock", "check", file, rest @ ..] => {
            let mut limit = None;
            let mut stall = None;
            let mut since = None;
            let secs = |v: &str| v.parse::<u64>().unwrap_or_else(|_| wrong(&args));
            let mut it = rest.iter();
            while let Some(flag) = it.next() {
                let v = it.next().unwrap_or_else(|| wrong(&args));
                match *flag {
                    "--limit" => limit = Some(secs(v)),
                    "--stall" => stall = Some(secs(v)),
                    "--since" => since = Some(secs(v)),
                    _ => wrong(&args),
                }
            }
            let (Some(limit), Some(stall), Some(since)) = (limit, stall, since) else {
                wrong(&args)
            };
            let v = clock::check(Path::new(file), limit, stall, since, clock::boot_seconds);
            println!("{}", v.word());
            std::process::exit(i32::from(v.exit()));
        }
        ["run", n] => {
            let n = issue(n, &args);
            pinned(
                &home,
                "cycle",
                n,
                "bin/tehasbot",
                &["cycle", &n.to_string()],
            )
        }
        ["pin", "plan", n, program, rest @ ..] => {
            let n = issue(n, &args);
            if program.is_empty() || program.contains('/') || program.starts_with('.') {
                wrong(&args);
            }
            pinned(&home, "plan", n, &format!("libexec/{program}"), rest)
        }
        ["renew", rest @ ..] => {
            let mut cmd = None;
            let mut every = None;
            let mut retry = None;
            let secs = |v: &str| v.parse::<u64>().unwrap_or_else(|_| wrong(&args));
            let mut it = rest.iter();
            while let Some(flag) = it.next() {
                let v = it.next().unwrap_or_else(|| wrong(&args));
                match *flag {
                    "--cmd" => cmd = Some(PathBuf::from(v)),
                    "--every" => every = Some(secs(v)),
                    "--retry" => retry = Some(secs(v)),
                    _ => wrong(&args),
                }
            }
            let (Some(cmd), Some(every), Some(retry)) = (cmd, every, retry) else {
                wrong(&args)
            };
            renew_verb(&cmd, every, retry);
        }
        ["finish", n, rest @ ..] => finish_verb(issue(n, &args), &args, rest),
        ["caps", rest @ ..] => caps_verb(rest, &args),
        ["runner", "outcome", result, n] => outcome_verb(result, n),
        ["runner", "run", log, n] => match runner::run_log(Path::new(log), n) {
            Some(r) => println!("{}\n{}\n{}", r.step, r.branch, r.read),
            None => std::process::exit(1),
        },
        ["compare", n] => {
            let n = issue(n, &args);
            if on() {
                if let Some(line) = shadow::compare(&home, n).unwrap_or_else(|e| fail(e)) {
                    println!("{line}");
                }
            }
        }
        ["describe", dir] => {
            if tehas_cycle::describe::describe(Path::new(dir)).is_err() {
                std::process::exit(1);
            }
        }
        ["private", models, email] => {
            for name in tehas_core::mask::host_names(models, email) {
                println!("--private\n{name}");
            }
        }
        _ => wrong(&args),
    }
}

/// The caps verbs. The answer comes on standard input: it can pass what one
/// argument may hold.
fn caps_verb(rest: &[&str], args: &[String]) {
    let mut bytes = vec![];
    let read = std::io::Read::read_to_end(&mut std::io::stdin(), &mut bytes);
    read.unwrap_or_else(|e| fail(format!("the answer cannot be read: {e}")));
    let text = String::from_utf8_lossy(&bytes);
    match rest {
        ["unsupported", flags, names, features @ ..] if features.len() < 2 => {
            let features = features.first().copied().unwrap_or("");
            match caps::unsupported(&text, flags, names, features) {
                Some(lines) => print!("{lines}"),
                None => std::process::exit(1),
            }
        }
        ["expired", workarounds] => {
            for line in caps::expired(&text, workarounds) {
                println!("{line}");
            }
        }
        ["mode", feature] => {
            let lib = std::env::current_exe()
                .ok()
                .and_then(|p| Some(p.parent()?.parent()?.join("lib")));
            let (word, code) = caps::mode(&text, feature, lib.as_deref());
            println!("{word}");
            std::process::exit(code);
        }
        ["verb", verb, flags] => std::process::exit(i32::from(!caps::verb(&text, verb, flags))),
        ["reads-images"] => println!("{}", u8::from(caps::reads_images(&text))),
        _ => wrong(args),
    }
}

/// The statuses are jq's, which is what this verb replaced.
fn outcome_verb(result: &str, n: &str) {
    let Ok(n) = n.parse::<u64>() else {
        std::process::exit(2)
    };
    let Ok(bytes) = std::fs::read(result) else {
        std::process::exit(2)
    };
    match runner::outcome(&String::from_utf8_lossy(&bytes), n) {
        Some(o) => println!("{o}"),
        None => std::process::exit(4),
    }
}

/// The ask of part 2's poll step: the words of one finish, and the run of them.
///
/// The kept workspace is `TEHAS_WORK_DIR` with the issue's number on the end,
/// the name `tehas work` gave the tree the cycle worked in and the name the poll
/// keeps when the cycle stops. The rest are given, so the command reads no
/// setting but that one, and it is refused when the host has none — the poll
/// step of part 2 has it, as every command that works on a workspace has.
fn finish_verb(n: u64, args: &[String], rest: &[&str]) -> ! {
    let mut home = None;
    let mut repo = None;
    let mut default = None;
    let mut reason = None;
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let Some(v) = it.next() else { wrong(args) };
        let v = v.to_string();
        match *flag {
            "--home" => home = Some(PathBuf::from(v)),
            "--repo" => repo = Some(v),
            "--default" => default = Some(v),
            "--reason" => reason = Some(v),
            _ => wrong(args),
        }
    }
    let (Some(home), Some(repo), Some(default), Some(reason)) = (home, repo, default, reason)
    else {
        wrong(args)
    };
    let work_dir = match std::env::var("TEHAS_WORK_DIR") {
        Ok(v) if !v.is_empty() => PathBuf::from(v),
        _ => fail("TEHAS_WORK_DIR is not set".to_string()),
    };
    let plan = finish::Plan {
        issue: n,
        repo,
        home,
        work_dir,
        default,
        reason,
    };
    let mut host = match finish::Live::new() {
        Ok(host) => host,
        Err(e) => fail(e),
    };
    let done = finish::finish(&plan, &mut host);
    if done.url.is_empty() {
        finish::say(n, &done.why);
        std::process::exit(1);
    }
    println!("{}", done.url);
    std::process::exit(0);
}

/// The renewer of a cycle's GitHub token: it runs to the end of the cycle and
/// ends 0 when a signal asks it to stop. The flag is what `renew` counts its
/// waits against, as `console`'s review does.
fn renew_verb(cmd: &Path, every: u64, retry: u64) -> ! {
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        if signal_hook::flag::register(signal, Arc::clone(&stop)).is_err() {
            fail(format!("cannot listen for signal {signal}"));
        }
    }
    renew::renew(cmd, every, retry, &stop, renew::epoch_seconds);
    std::process::exit(0);
}

/// Runs `program` of the copy of the install made for this `kind` of `n`,
/// or the install's own when the copy cannot be made: `bin/tehasbot` for a
/// cycle, a program of libexec/ for a planning turn.
fn pinned(home: &Path, kind: &str, n: u64, program: &str, args: &[&str]) -> ! {
    use std::os::unix::process::CommandExt;
    let say = |s: String| {
        eprintln!(
            "{} tehas-cycle: {s}",
            tehas_core::time::iso(tehas_core::time::now())
        )
    };
    if !home.is_absolute() {
        eprintln!("tehas-cycle: TEHAS_HOME is not an absolute path");
        std::process::exit(78);
    }
    let what = if kind == "plan" {
        "the planning turn of "
    } else {
        ""
    };
    pin::sweep(home, pin::alive);
    let root = match pin::make(home, kind, n, std::process::id()) {
        Ok(dir) => {
            say(format!("{what}#{n} runs from {}", dir.display()));
            dir
        }
        Err(e) => {
            say(format!(
                "{what}#{n} runs from the install: its own copy could not be made ({e})"
            ));
            home.to_path_buf()
        }
    };
    let e = std::process::Command::new(root.join(program))
        .args(args)
        .env("TEHAS_CODE", &root)
        .exec();
    fail(format!("{}: {e}", root.join(program).display()));
}

fn print_report(file: &str) {
    let v: Value = std::fs::read_to_string(file)
        .map_err(|e| e.to_string())
        .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
        .unwrap_or_else(|e| fail(format!("the facts in {file} cannot be read: {e}")));
    let facts = Facts::from_json(&v).unwrap_or_else(|e| fail(e));
    let mut gh = Given(v.get("github").cloned().unwrap_or(json!({})));
    let r = report(&facts, &mut gh);
    println!("{}", to_json(&r, facts.issue));
}

fn last_attempt(n: u64, result: &str, workspace: &Path) {
    let mut stdin = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut stdin)
        .unwrap_or_else(|e| fail(format!("the issue cannot be read: {e}")));
    let issue: Value = serde_json::from_str(&stdin)
        .unwrap_or_else(|e| fail(format!("the issue cannot be read: {e}")));
    let r: Value = std::fs::read_to_string(result)
        .map_err(|e| e.to_string())
        .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
        .unwrap_or_else(|e| fail(format!("the result in {result} cannot be read: {e}")));
    let entry = r
        .get("issues")
        .and_then(Value::as_array)
        .and_then(|l| {
            l.iter()
                .find(|e| e.get("issue").and_then(Value::as_u64) == Some(n))
        })
        .cloned()
        .unwrap_or(Value::Null);
    let body = match issue.get("body") {
        Some(Value::String(b)) => b.as_str(),
        Some(Value::Null) => "",
        _ => fail("the issue has no body".to_string()),
    };
    let action = last_attempt::decide(body, &entry, last_attempt::state(workspace, n).as_ref());
    println!(
        "{}",
        match action {
            Action::Nothing => json!({"action": "none"}),
            Action::Edit(body) => json!({"action": "edit", "body": body}),
            Action::Repeat => json!({"action": "repeat", "say": last_attempt::REPEAT}),
        }
    );
}

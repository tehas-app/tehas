//! tehas-cycle: a cycle's start, its supervision and its report.
//!
//! Usage:
//!   tehas-cycle report <facts.json>   print the report of an ended cycle,
//!                                     as JSON, from the facts in the file
//!                                     (src/facts.rs) and the answers of
//!                                     GitHub it holds under "github"
//!   tehas-cycle report <issue> --attempt <n> --rc <n> --started <s>
//!       --signalled 0|1 --by <login> --how <how> --approval-at <s>
//!       --requeued 0|1 [--now <s>] --console <path> --status-cmd <path>
//!       [--status-arg <word>]...
//!                                     report an ended cycle, every GitHub
//!                                     write through the action journal, then
//!                                     what follows its end (src/apply.rs), as
//!                                     `cycle` does once tehas work is over
//!   tehas-cycle approved <issue> <attempt>
//!                                     print {at, how, driver} of the approval
//!                                     that attempt used, or null: read
//!                                     before cycle-end, which removes the
//!                                     record of a start that never ran
//!   tehas-cycle after <issue>         standard input holds, each word ended
//!                                     by a NUL as a status can pass what one
//!                                     argument may hold: --attempt <n>
//!       --started <s> --rc <n> --status <word> --step <step> --cap <cap> --park-reason <word>
//!       --failure-kind <word> --failure-cause <word> --provider 0|1
//!       --parked true|false --pr <url> --by <login> --how <how>
//!       --approved <json> --image <id> --image-built <time>
//!       --rukas-image <commit> --oom <n> --forks <n> --reason <text>
//!       --build <commit> --handoff-delivered true|false --signalled 0|1
//!       --workdir <dir> --console <path>
//!       --status-cmd <path>
//!                                     what follows a cycle's record, each
//!                                     step's failure a WARN line: the live
//!                                     view, the run and its end in the
//!                                     analytics, the improvement record, the
//!                                     workspace's build output, the status
//!                                     unless a signal stopped the cycle, and
//!                                     the next poll (src/after.rs). A flag
//!                                     with nothing to say may be left out
//!   tehas-cycle last-attempt <issue> <result.json> <workspace>
//!                                     the issue as GitHub's REST API answers
//!                                     it on standard input: what a cycle
//!                                     that ended at child-timeout:develop
//!                                     does with its body, as JSON
//!                                     {"action": "none"|"edit"|"repeat",
//!                                     "body": <the body to write>,
//!                                     "say": <a repeat's words for the park>}
//!                                     (src/last_attempt.rs)
//!   tehas-cycle cycle <issue> --work <path> [--work-arg <word>]...
//!       --status-cmd <path> [--status-arg <word>]... --console <path>
//!                                     tehasbot cycle, which the console execs
//!                                     with the settings loaded: the issue's
//!                                     lock, waited for 5 s (3 when another
//!                                     holds it), the start on record, what
//!                                     the number is, tehas work to its end and
//!                                     the report (src/supervise.rs). Exits as
//!                                     tehasbot cycle does
//!   tehas-cycle run <issue>           what tehas-cycle@.service starts:
//!                                     copy the install's code for this
//!                                     cycle and run tehasbot cycle from the
//!                                     copy (src/pin.rs)
//!   tehas-cycle pin plan <issue> <command> [args...]
//!                                     what the dispatcher starts a planning
//!                                     turn with: the same copy, and run
//!                                     bin/tehas <command> of it with the args
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
//!   tehas-cycle describe <dir> <issue>
//!                                     write <dir>/images.md, a model's
//!                                     description of each image in
//!                                     <dir>/manifest.json, and name it in the
//!                                     manifest (src/describe.rs). <issue> is
//!                                     the issue number, sent as X-Tehas-Issue
//!                                     on each request when it is digits. Exits
//!                                     1 only when a file could not be written
//!   tehas-cycle private <models.json> <email>
//!                                     the names a mask hides, two lines each:
//!                                     `--private` and the name, for a command
//!                                     to read into its arguments
//!   tehas-cycle renew --cmd <path> [--arg <word>]... --every <s> --retry <s>
//!                                     the renewer of a cycle's GitHub token:
//!                                     run <path> with the words of --arg, in
//!                                     their order, with its output discarded,
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
//!   tehas-cycle runner progress <ndjson> [<result> <issue>]
//!                                     the summary of a run, one JSON object:
//!                                     what state/cycles/<issue>.progress.json
//!                                     holds (src/progress.rs)
//!   tehas-cycle runner error-only <result>
//!                                     exit 0 when <result> is a schema 5
//!                                     result with no issue entry at all
//!   tehas-cycle runner run <ndjson> <issue>
//!                                     three lines of a run log: the last
//!                                     step started, the branch of its last
//!                                     push when it is the issue's, and
//!                                     `whole` or, past 200 KB read from its
//!                                     end, `truncated`. Exits 1 with nothing
//!                                     printed when <issue> is no number or
//!                                     the log is no file it can read
//!   tehas-cycle sandbox ok            Pi's settings.json and models.json are
//!                                     there and the tokens models.json names
//!                                     are set, else one line on standard
//!                                     error and exit 78
//!   tehas-cycle sandbox limits <who> <what>
//!                                     one systemctl and one docker info: the
//!                                     14 limit words, the 10 security words,
//!                                     NUL separated. Without a usable slice
//!                                     one line on standard error
//!                                     that starts with <who>, and exit 78;
//!                                     <what> names what does not start
//!   tehas-cycle sandbox env <kind> <tree> <git name> <git email>
//!                                     the -e options of a cycle or turn, NUL
//!                                     separated
//!   tehas-cycle sandbox hosts         the --add-host options, NUL separated
//!   tehas-cycle sandbox mounts <kind> <tree> <host> <state> <runs>
//!                                     make what the -v options name, write
//!                                     <state>/models.json, the host's with each
//!                                     provider on the endpoint picked now, and
//!                                     <state>/endpoints, the providers on their
//!                                     backup, and print the options, NUL
//!                                     separated. Exit 1 with nothing printed
//!                                     when it cannot
//!   tehas-cycle sandbox table         the rows of the containers tehas work
//!                                     starts, for the table of runs
//!   tehas-cycle sandbox retry <host>  the provider retry a container's Pi runs
//!                                     with, as one line
//!   tehas-cycle sandbox clone <src> <tree> <branch>
//!                                     make <tree>, a clone of <src>
//!   tehas-cycle sandbox seed <state> <index>
//!                                     give <state> the host's code index
//!   tehas-cycle sandbox smoke-run <image>
//!                                     one offline work turn in <image>, at
//!                                     most 40 s, for `host/sandbox/build.sh`
//!                                     before it moves the uid tag. Prints
//!                                     `started`, `crashed` or `silent`. Exit 0
//!                                     only for a stable `step-started` line
//!   tehas-cycle sandbox verify <limit words>
//!                                     how many runs of the gate one container
//!                                     runs at once. Exit 1 with nothing
//!                                     printed when the words have no quota or
//!                                     ceiling
//!   tehas-cycle settings <file> <count> [<docker options>...]
//!                                     write what a cycle ran with to <file>
//!   tehas-cycle work <issue> --console <path> [--restart] [--driver lean|classic]
//!                                     tehas work, which the console execs
//!                                     with the settings loaded: the issue's
//!                                     lock, or the one tehasbot cycle handed
//!                                     down (5 when another holds it), the
//!                                     start held to its approval, the issue's files, the
//!                                     workspace, the container watched to its
//!                                     end and the cycle's notes
//!                                     (src/work.rs). Exits as tehas work does
//!   tehas-cycle index [--before-cycle]
//!                                     tehas index, which the console execs
//!                                     with the settings loaded: the run
//!                                     below at the tip of the product's
//!                                     branch the checkout last fetched; 78
//!                                     for a checkout of another repository
//!                                     or a branch it does not know
//!   tehas-cycle index <branch> <commit> [--before-cycle]
//!                                     one run of the code indexer at <commit>,
//!                                     the tip of <branch>, under
//!                                     state/index.lock, which its docker
//!                                     client inherits (src/index.rs). 3 when
//!                                     another run holds it. Exit 0, 1, 3, 78
//!                                     as tehas index says
//!   tehas-cycle index table           its row of the table of runs
//!   tehas-cycle -h|--help             print this text
//!
//! last-attempt reads what it is given and changes nothing.
//!
//! run hardlinks $TEHAS_HOME's bin/, libexec/ and lib/ into
//! $TEHAS_HOME/run/cycle-<issue>-<pid>/, removes the copies of processes that
//! are gone, and execs <copy>/bin/tehasbot cycle <issue>. A copy it cannot make is said on standard error and the install's
//! tehasbot runs instead. It returns only when neither could be run. pin plan
//! does the same under $TEHAS_HOME/run/plan-<issue>-<pid>/.
//!
//! cycle and report <issue> read $TEHAS_HOME, TEHAS_REPO and TEHAS_WORK_DIR
//! from the environment, as tehas_load leaves them, what after reads, and
//! TEHAS_CYCLE_RETRY_WAIT and TEHAS_CYCLE_STOP_GH_WAIT; they call gh and the
//! store.
//!
//! after and approved read $TEHAS_HOME, TEHAS_ANALYTICS, TEHAS_ANALYTICS_WAIT,
//! TEHAS_IMPROVE_REPO, TEHAS_CLEAN_DIRS and TEHAS_SCRATCH_DIR.
//!
//! Exit: 0 it printed or kept its answer; 70 report: the end of the cycle is
//! not on record; 1 what it reads could not be
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
use std::time::Duration;
use tehas_cycle::after::{self, After};
use tehas_cycle::host::Start;
use tehas_cycle::last_attempt::{self, Action};
use tehas_cycle::report::{last_attempts, to_json, Given};
use tehas_cycle::{apply, finish, lock, pin, progress, renew, report};
use tehas_cycle::{caps, clock, improve, index, runner, sandbox, start, supervise, work, Facts};

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
    match a.as_slice() {
        ["-h" | "--help"] => tehas_core::outln!("{}", usage()),
        ["report", file] => print_report(file),
        ["index", "table"] => tehas_core::outln!("{}", index::table()),
        ["index"] => index_verb(&home, || index::here(false)),
        ["index", "--before-cycle"] => index_verb(&home, || index::here(true)),
        ["index", branch, base] => index_verb(&home, || index::main(branch, base, false)),
        ["index", branch, base, "--before-cycle"] => {
            index_verb(&home, || index::main(branch, base, true))
        }
        ["sandbox", ..] => match sandbox::main(&args[1..]) {
            Some(status) => std::process::exit(status),
            None => wrong(&args),
        },
        ["last-attempt", n, result, workspace] => {
            last_attempt(issue(n, &args), result, Path::new(workspace))
        }
        ["approved", n, attempt] => {
            let attempt = attempt
                .parse()
                .ok()
                .filter(|a| *a > 0)
                .unwrap_or_else(|| wrong(&args));
            tehas_core::outln!("{}", after::approved(&home, issue(n, &args), attempt));
        }
        ["after", n] => after_verb(&home, issue(n, &args), &args),
        ["report", n, rest @ ..] if !rest.is_empty() => {
            let start = start(issue(n, &args), &args, rest);
            if start.console.is_empty() || start.status_cmd.is_empty() {
                wrong(&args);
            }
            std::process::exit(apply::run(&home, &start));
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
            tehas_core::outln!("{}", v.word());
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
            let mut words = vec![*program];
            words.extend_from_slice(rest);
            pinned(&home, "plan", n, "bin/tehas", &words)
        }
        ["renew", rest @ ..] => {
            let mut cmd = None;
            let mut words = Vec::new();
            let mut every = None;
            let mut retry = None;
            let secs = |v: &str| v.parse::<u64>().unwrap_or_else(|_| wrong(&args));
            let mut it = rest.iter();
            while let Some(flag) = it.next() {
                let v = it.next().unwrap_or_else(|| wrong(&args));
                match *flag {
                    "--cmd" => cmd = Some(PathBuf::from(v)),
                    "--arg" => words.push(v.to_string()),
                    "--every" => every = Some(secs(v)),
                    "--retry" => retry = Some(secs(v)),
                    _ => wrong(&args),
                }
            }
            let (Some(cmd), Some(every), Some(retry)) = (cmd, every, retry) else {
                wrong(&args)
            };
            renew_verb(&cmd, &words, every, retry);
        }
        ["finish", n, rest @ ..] => finish_verb(issue(n, &args), &args, rest),
        ["caps", rest @ ..] => caps_verb(rest, &args),
        ["runner", "outcome", result, n] => outcome_verb(result, n),
        ["runner", "progress", log] => {
            tehas_core::outln!("{}", progress::summary(Path::new(log), None))
        }
        ["runner", "progress", log, result, n] => tehas_core::outln!(
            "{}",
            progress::summary(
                Path::new(log),
                n.parse().ok().map(|n| (Path::new(*result), n))
            )
        ),
        ["runner", "error-only", result] => {
            let text = std::fs::read(result).unwrap_or_default();
            let only = progress::error_only(&String::from_utf8_lossy(&text));
            std::process::exit(i32::from(!only));
        }
        ["cycle", n, rest @ ..] => cycle_verb(&home, issue(n, &args), &args, rest),
        ["work", n, rest @ ..] => work_verb(&home, issue(n, &args), &args, rest),
        ["settings", file, count, options @ ..] => {
            let words: Vec<String> = options.iter().map(|w| w.to_string()).collect();
            let text = improve::settings(count, &words);
            if std::fs::write(file, format!("{text}\n")).is_err() {
                fail(format!("{file} cannot be written"));
            }
        }
        ["runner", "run", log, n] => match runner::run_log(Path::new(log), n) {
            Some(r) => tehas_core::outln!("{}\n{}\n{}", r.step, r.branch, r.read),
            None => std::process::exit(1),
        },
        ["describe", dir, issue] => {
            if tehas_cycle::describe::describe(Path::new(dir), issue).is_err() {
                std::process::exit(1);
            }
        }
        ["private", models, email] => {
            for name in tehas_core::mask::host_names(models, email) {
                tehas_core::outln!("--private\n{name}");
            }
        }
        _ => wrong(&args),
    }
}

/// One run of the indexer under state/index.lock, which its docker client
/// inherits: a client that outlives the run still writes the index.
fn index_verb(home: &Path, run: impl FnOnce() -> u8) -> ! {
    let state = home.join("state");
    let _ = std::fs::create_dir_all(&state);
    let held = match lock::take(&state.join("index.lock"), Duration::ZERO) {
        Ok(Some(f)) => f,
        Ok(None) => {
            eprintln!("tehas index: another index run holds its lock, nothing was indexed; run it again in a moment");
            std::process::exit(3)
        }
        Err(e) => fail(e),
    };
    lock::hand_down(&held).unwrap_or_else(|e| fail(e));
    std::process::exit(run().into());
}

/// The words of a cycle's supervision. The store is asked for before the
/// lock is opened, and the lock is waited for up to five seconds: a second
/// start of the same issue is 3.
fn cycle_verb(home: &Path, n: u64, args: &[String], rest: &[&str]) -> ! {
    let mut g = supervise::Given {
        issue: n,
        work: String::new(),
        work_args: Vec::new(),
        status_cmd: String::new(),
        status_args: Vec::new(),
        console: String::new(),
    };
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let v = it.next().unwrap_or_else(|| wrong(args)).to_string();
        match *flag {
            "--work" => g.work = v,
            "--work-arg" => g.work_args.push(v),
            "--status-cmd" => g.status_cmd = v,
            "--status-arg" => g.status_args.push(v),
            "--console" => g.console = v,
            _ => wrong(args),
        }
    }
    if g.work.is_empty() || g.status_cmd.is_empty() || g.console.is_empty() {
        wrong(args);
    }
    if !work::store_ready(home) {
        after::log("ERROR: the state store is not installed (run install.sh)");
        std::process::exit(78);
    }
    let cycles = home.join("state/cycles");
    let _ = std::fs::create_dir_all(&cycles);
    let _ = std::fs::create_dir_all(home.join("logs"));
    // tehas work and the docker client it starts inherit this lock. If this
    // process is killed outright the place stays taken until they are gone too.
    let held = match lock::take(&cycles.join(format!("{n}.lock")), Duration::from_secs(5)) {
        Ok(Some(f)) => f,
        Ok(None) => {
            after::log(&format!("a cycle for #{n} is already running"));
            std::process::exit(3)
        }
        Err(e) => fail(e),
    };
    lock::hand_down(&held).unwrap_or_else(|e| fail(e));
    std::process::exit(supervise::run(home, &g));
}

/// The words of tehas work.
fn work_verb(home: &Path, n: u64, args: &[String], rest: &[&str]) -> ! {
    let mut g = work::Given {
        issue: n,
        restart: false,
        driver: None,
        console: String::new(),
    };
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        match *flag {
            "--restart" => g.restart = true,
            "--driver" => {
                let word = it.next().unwrap_or_else(|| wrong(args));
                g.driver = Some(start::Driver::named(word).unwrap_or_else(|| {
                    eprintln!("tehas work: --driver takes lean or classic, not {word}");
                    std::process::exit(64)
                }));
            }
            "--console" => g.console = it.next().unwrap_or_else(|| wrong(args)).to_string(),
            _ => wrong(args),
        }
    }
    if g.console.is_empty() || !home.is_absolute() {
        wrong(args);
    }
    // Under tehasbot cycle the lock comes down already held. Taken here, it is
    // handed down: every program of the cycle, the docker client among them,
    // holds it, so a run by hand and the unit's cycle never share a workspace.
    let path = home.join(format!("state/cycles/{n}.lock"));
    let _ = std::fs::create_dir_all(home.join("state/cycles"));
    let _held = if lock::held_here(&path) {
        None
    } else {
        match lock::take(&path, Duration::ZERO) {
            Ok(Some(f)) => {
                lock::hand_down(&f).unwrap_or_else(|e| fail(e));
                Some(f)
            }
            Ok(None) => {
                eprintln!("tehas work: a cycle for #{n} is running, not starting another");
                std::process::exit(5)
            }
            Err(e) => fail(e),
        }
    };
    std::process::exit(work::run(home, &g));
}

/// The start and end of a cycle, as tehas-cycle holds them.
fn start(n: u64, args: &[String], rest: &[&str]) -> Start {
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
        now: None,
        console: String::new(),
        status_cmd: String::new(),
        status_args: Vec::new(),
    };
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let v = it.next().unwrap_or_else(|| wrong(args)).to_string();
        let num = |v: &str| v.parse::<i64>().unwrap_or_else(|_| wrong(args));
        match *flag {
            "--attempt" => start.attempt = num(&v).try_into().unwrap_or_else(|_| wrong(args)),
            "--rc" => start.rc = num(&v),
            "--started" => start.started = num(&v),
            "--signalled" => start.signalled = v == "1",
            "--by" => start.by = v,
            "--how" => start.how = v,
            "--approval-at" => start.approval_at = v,
            "--requeued" => start.requeued = v == "1",
            "--now" => start.now = Some(num(&v)),
            "--console" => start.console = v,
            "--status-cmd" => start.status_cmd = v,
            "--status-arg" => start.status_args.push(v),
            _ => wrong(args),
        }
    }
    if start.attempt == 0 {
        wrong(args);
    }
    start
}

/// What follows a cycle's record, from what tehas-cycle holds of its end.
fn after_verb(home: &Path, n: u64, args: &[String]) {
    let mut bytes = vec![];
    let read = std::io::Read::read_to_end(&mut std::io::stdin(), &mut bytes);
    read.unwrap_or_else(|e| fail(format!("the words cannot be read: {e}")));
    let text = String::from_utf8_lossy(&bytes);
    let mut a = After::default();
    a.ended.issue = n;
    let num = |v: &str| v.parse::<i64>().unwrap_or_else(|_| wrong(args));
    let mut it = text.split_terminator('\0');
    while let Some(flag) = it.next() {
        let v = it.next().unwrap_or_else(|| wrong(args)).to_string();
        let e = &mut a.ended;
        match flag {
            "--attempt" => a.attempt = num(&v).try_into().unwrap_or_else(|_| wrong(args)),
            "--started" => e.started = num(&v),
            "--rc" => e.rc = num(&v),
            "--status" => e.status = v,
            "--step" => e.step = v,
            "--cap" => e.cap = v,
            "--park-reason" => a.park_reason = v,
            "--failure-kind" => a.failure_kind = v,
            "--failure-cause" => a.failure_cause = v,
            "--provider" => a.provider = v == "1",
            "--parked" => a.parked = v == "true",
            "--pr" => e.pr = v,
            "--by" => a.by = v,
            "--how" => a.how = v,
            "--approved" => a.approved = v,
            "--image" => e.image = v,
            "--image-built" => a.image_built = v,
            "--rukas-image" => e.rukas = v,
            "--oom" => e.kills = v,
            "--forks" => a.forks = v,
            "--reason" => e.reason = v,
            "--build" => e.build = v,
            "--handoff-delivered" => e.handoff_delivered = Some(v == "true"),
            "--signalled" => a.signalled = v == "1",
            "--workdir" => a.workdir = v,
            "--console" => a.console = v,
            "--status-cmd" => a.status_cmd = v,
            "--status-arg" => a.status_args.push(v),
            _ => wrong(args),
        }
    }
    if a.attempt == 0 || a.ended.started == 0 || a.console.is_empty() || a.status_cmd.is_empty() {
        wrong(args);
    }
    after::after(home, &a);
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
            match caps::unsupported(&text, "work", flags, names, features) {
                Some(lines) => tehas_core::out!("{lines}"),
                None => std::process::exit(1),
            }
        }
        ["expired", workarounds] => {
            for line in caps::expired(&text, workarounds) {
                tehas_core::outln!("{line}");
            }
        }
        ["mode", feature] => {
            let lib = std::env::current_exe()
                .ok()
                .and_then(|p| Some(p.parent()?.parent()?.join("lib")));
            let (word, code) = caps::mode(&text, feature, lib.as_deref());
            tehas_core::outln!("{word}");
            std::process::exit(code);
        }
        ["verb", verb, flags] => std::process::exit(i32::from(!caps::verb(&text, verb, flags))),
        ["reads-images"] => tehas_core::outln!("{}", u8::from(caps::reads_images(&text))),
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
        Some(o) => tehas_core::outln!("{o}"),
        None => std::process::exit(4),
    }
}

/// The ask of part 2's poll step: the words of one finish, and the run of them.
///
/// The kept workspace is `TEHAS_WORK_DIR` with the issue's number on the end,
/// the name `tehas work` gave the tree the cycle worked in and the name the poll
/// keeps when the cycle stops. The rest are given, so the command reads no
/// setting but that one, and it is refused when the host has none — the poll
/// step of part 2 has it, as every command that works on a workspace has. The
/// poll's environment also gives `TEHAS_BOT_LOGIN` and `TEHAS_ALLOWED_LABELERS`,
/// whose open pull requests count; a run with no allowed account is refused.
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
    let setting = |k: &str| std::env::var(k).unwrap_or_default();
    // A finish run by hand has no poll environment: the bot is the login the
    // App's mint kept, as the poll reads it.
    let bot = match setting("TEHAS_BOT_LOGIN") {
        b if b.is_empty() => tehas_core::settings::bot_login(&home).unwrap_or_default(),
        b => b,
    };
    let plan = finish::Plan {
        issue: n,
        repo,
        home,
        work_dir,
        default,
        reason,
        remote: None,
        bot,
        allowed: setting("TEHAS_ALLOWED_LABELERS")
            .split_whitespace()
            .map(str::to_string)
            .collect(),
    };
    let mut host = match finish::Live::new() {
        Ok(host) => host,
        Err(e) => fail(e),
    };
    let done = finish::finish(&plan, &mut host);
    if done.url.is_empty() {
        if !done.said.is_empty() {
            finish::say(n, &format!("{}{}", finish::SAID, done.said));
        }
        finish::say(n, &done.why);
        std::process::exit(1);
    }
    tehas_core::outln!("{}", done.url);
    std::process::exit(0);
}

/// The renewer of a cycle's GitHub token: it runs to the end of the cycle and
/// ends 0 when a signal asks it to stop. The flag is what `renew` counts its
/// waits against, as `console`'s review does.
fn renew_verb(cmd: &Path, words: &[String], every: u64, retry: u64) -> ! {
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        if signal_hook::flag::register(signal, Arc::clone(&stop)).is_err() {
            fail(format!("cannot listen for signal {signal}"));
        }
    }
    renew::renew(cmd, words, every, retry, &stop, renew::epoch_seconds);
    std::process::exit(0);
}

/// Runs `program` of the copy of the install made for this `kind` of `n`,
/// or the install's own when the copy cannot be made: `bin/tehasbot` for a
/// cycle, `bin/tehas` and a command of it for a planning turn.
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
    let mut r = report(&facts, &mut gh);
    last_attempts(&mut r, &facts, &mut gh);
    tehas_core::outln!("{}", to_json(&r, facts.issue));
}

fn last_attempt(n: u64, result: &str, workspace: &Path) {
    let mut stdin = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut stdin)
        .unwrap_or_else(|e| fail(format!("the issue cannot be read: {e}")));
    let issue: Value = serde_json::from_str(&stdin)
        .unwrap_or_else(|e| fail(format!("the issue cannot be read: {e}")));
    let text = std::fs::read_to_string(result)
        .unwrap_or_else(|e| fail(format!("the result in {result} cannot be read: {e}")));
    let state = last_attempt::state(workspace, n);
    let action =
        last_attempt::of_issue(n, &issue, Some(&text), state.as_ref()).unwrap_or_else(|| {
            fail(format!(
                "the issue or the result in {result} cannot be read"
            ))
        });
    tehas_core::outln!(
        "{}",
        match action {
            Action::Nothing => json!({"action": "none"}),
            Action::Edit(body) => json!({"action": "edit", "body": body}),
            Action::Repeat => json!({"action": "repeat", "say": last_attempt::REPEAT}),
        }
    );
}

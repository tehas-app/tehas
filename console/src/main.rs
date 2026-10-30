//! Turn a running /work cycle into one readable, append-only console file
//! for the dashboard.
//!
//!   console --progress <work-N.ndjson> --runs <ensemble-runs dir>
//!     --out <console.txt> --since <epoch seconds> [--private <name>]...
//!
//! Follows two sources until it is told to stop (TERM or INT):
//!   - the headless runner's NDJSON progress, its stable kinds only: driver
//!     notices, step boundaries, agents started and done, caps, the handoff,
//!     CI and the merge,
//!   - every agent transcript touched since the cycle began: what each agent
//!     said, the commands it ran and the start of what came back.
//!
//! Output is plain text. Tokens, database passwords and private network
//! addresses are masked, see core/src/mask.rs, and so is each name given with
//! --private, as text in any case. A name is not a secret: it is on the
//! command line.
//!
//!   console --mask [--private <name>]...
//!
//! The mask alone: standard input to standard output, for libexec/work's own
//! line on the dashboard, so that the mask is written in one place.
//!
//!   console preview tell ...
//!   console preview choose <repo> [max] <pull requests
//!
//! The notice tehasbot preview keeps in a pull request's description, see
//! src/notice.rs, and which open pull requests get a preview, see
//! src/preview.rs.
//!
//!   console plan outcome|update ...
//!
//! What a planning turn of libexec/plan says, see src/plan.rs.
//!
//!   console top reading|snapshot|frame ...
//!
//! The snapshot behind tehas top, its host section and the frame drawn from
//! it, see src/top.rs, src/snapshot.rs and src/frame.rs.
//!
//!   console health report ...
//!
//! The health file tehasbot health writes, see src/health.rs.
//!
//!   console status show|json ...
//!
//! The lines and the object of tehas status, see src/status.rs.
//!
//!   console review since|refused|run ...
//!
//! What tehasbot review decides and how it runs its container, see
//! src/review.rs.
//!
//!   console clean run ...
//!
//! What tehasbot clean removes, see src/clean.rs.
//!
//!   console epics <owner/name> <out>
//!
//! The open epics and their sub-issues, live/epics.json, for tehasbot status,
//! see src/epics.rs.
//!
//!   console flags limit show|set|clear|read ...
//!
//! The flags under state/ of one instance: the limit tehas limit sets, see
//! src/flags.rs.
//!
//!   console export run ...
//!
//! What tehas export copies, see src/export.rs.
//!
//!   console protolab page|front|live|refs ...
//!
//! The pages of the Protolab and the links of a ticket, for libexec/protolab,
//! see src/protolab.rs.
//!
//!   console setup files ...
//!
//! The settings, the checks and the files of tehas setup, see src/setup.rs.
//!
//!   console shipped page|query|answer|list|has|whole ...
//!
//! What closed an issue: the page and the GraphQL query of the read, and what its
//! answer says, for tehas tidy and tehasbot status (lib/shipped.sh holds the names
//! of these verbs and makes the call), see src/shipped.rs.
//!
//!   console tidy states|plan ...
//!
//! Which of tehas's state labels come off which closed issue, and which issue the
//! read of what closed them is asked about, for tehas tidy (libexec/tidy makes the
//! two searches and the edits), see src/tidy.rs.
//!
//!   console forge labels|label-state|approval|add-label|remove-label ...
//!
//! The forge as a command bash can call: an issue's labels, the newest event of
//! one label, who applied the label that lets a cycle start, and a label put on
//! or taken off, asked of GitHub through tehas_core::forge, see src/forge.rs.
//!
//! Exit: 0 stopped by a signal or masked, 1 the console file or standard
//! output cannot be written, 2 usage.

mod clean;
mod epics;
mod export;
mod flags;
mod forge;
mod frame;
mod health;
mod jq;
mod notice;
mod plan;
mod preview;
mod protolab;
mod render;
mod review;
mod setup;
mod shipped;
mod snapshot;
mod status;
mod store;
mod tail;
mod tidy;
mod top;

use render::Entry;
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tehas_core::mask;

const POLL: Duration = Duration::from_millis(1500);
// How often a stop is looked for between two polls.
const TICK: Duration = Duration::from_millis(100);

struct Args {
    progress: String,
    runs: String,
    out: String,
    since: SystemTime,
}

// Takes every `--private <name>` out of the arguments.
fn private(given: &mut Vec<String>) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    while let Some(i) = given.iter().position(|a| a == "--private") {
        if i + 1 >= given.len() {
            return Err("console: --private needs a name".to_string());
        }
        names.push(given.remove(i + 1));
        given.remove(i);
    }
    Ok(names)
}

fn args(given: &[String]) -> Result<Args, String> {
    let value = |name: &str| -> Result<String, String> {
        let flag = format!("--{name}");
        given
            .iter()
            .position(|a| *a == flag)
            .and_then(|i| given.get(i + 1))
            .filter(|v| !v.is_empty())
            .cloned()
            .ok_or(format!("console: {flag} is required"))
    };
    let since = value("since")?;
    let since = since
        .parse()
        .ok()
        .and_then(|seconds| UNIX_EPOCH.checked_add(Duration::from_secs(seconds)))
        .ok_or(format!(
            "console: --since is not a time in seconds: {since}"
        ))?;
    Ok(Args {
        progress: value("progress")?,
        runs: value("runs")?,
        out: value("out")?,
        since,
    })
}

fn write(out: &mut File, entry: &Entry) -> io::Result<()> {
    let now = tehas_core::time::now().rem_euclid(86_400);
    out.write_all(render::format(entry, now as u32).as_bytes())
}

fn open_out(path: &str) -> io::Result<File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o644)
        .open(path)
}

// A console file removed or replaced while the cycle runs is opened again by
// its name, or the rest of the cycle would go to a file no one can read.
fn reopened(out: File, path: &str) -> io::Result<File> {
    let same = match (fs::metadata(path), out.metadata()) {
        (Ok(named), Ok(held)) => named.dev() == held.dev() && named.ino() == held.ino(),
        _ => false,
    };
    if same {
        Ok(out)
    } else {
        open_out(path)
    }
}

fn run(args: &Args, stop: &AtomicBool) -> io::Result<()> {
    let mut out = open_out(&args.out)?;
    let mut follower = tail::Follower::default();
    let mut seen: HashSet<String> = HashSet::new();
    write(
        &mut out,
        &Entry::new("tehas", "console attached, waiting for the cycle", None),
    )?;
    loop {
        // Read before the lines are: a stop that arrives during a pass is
        // followed by one more pass, so the last lines are not lost.
        let stopping = stop.load(Ordering::Relaxed);
        out = reopened(out, &args.out)?;
        for line in follower.new_lines(&args.progress) {
            if let Some(key) = render::progress_key(&line) {
                if !seen.insert(key) {
                    continue;
                }
            }
            if let Some(entry) = render::progress_line(&line) {
                write(&mut out, &entry)?;
            }
        }
        for transcript in follower.transcripts(&args.runs, args.since) {
            // A stop that arrives now goes straight to the last pass.
            if !stopping && stop.load(Ordering::Relaxed) {
                break;
            }
            let role = tail::role(&transcript.path);
            let who = mask::mask(&role);
            for line in follower.transcript_lines(&transcript) {
                for entry in render::transcript_line(&who, &line) {
                    write(&mut out, &entry)?;
                }
            }
        }
        let skipped = follower.take_skipped();
        if skipped > 0 {
            let said = format!("log lines over 1 MiB left out: {skipped}");
            write(&mut out, &Entry::new("tehas", said, None))?;
        }
        let unheld = follower.take_unheld();
        if unheld > 0 {
            let said = format!("unfinished log lines left out, 16 MiB of them held: {unheld}");
            write(&mut out, &Entry::new("tehas", said, None))?;
        }
        if stopping {
            break;
        }
        let mut waited = Duration::ZERO;
        while waited < POLL && !stop.load(Ordering::Relaxed) {
            sleep(TICK);
            waited += TICK;
        }
    }
    write(
        &mut out,
        &Entry::new("tehas", "cycle ended, console detached", None),
    )
}

// To stderr, and no matter if it cannot be written: with a full disk it
// cannot, and the exit status must still be the one this file promises.
fn complain(message: &str) {
    let _ = writeln!(io::stderr(), "{message}");
}

fn mask_stdin() -> io::Result<()> {
    let mut given = Vec::new();
    io::stdin().read_to_end(&mut given)?;
    let masked = mask::mask(&String::from_utf8_lossy(&given));
    io::stdout().write_all(masked.as_bytes())
}

fn main() -> ExitCode {
    let given = std::env::args_os()
        .skip(1)
        .map(|a| a.into_string())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "console: an argument is not UTF-8".to_string());
    if let Ok(given) = &given {
        match given.first().map(String::as_str) {
            Some("clean") => return ExitCode::from(clean::main(&given[1..])),
            Some("epics") => return ExitCode::from(epics::main(&given[1..])),
            Some("export") => return ExitCode::from(export::main(&given[1..])),
            Some("flags") => return ExitCode::from(flags::main(&given[1..])),
            Some("forge") => return ExitCode::from(forge::main(&given[1..])),
            Some("review") => return ExitCode::from(review::main(&given[1..])),
            Some("preview") => return ExitCode::from(preview::main(&given[1..])),
            Some("setup") => return ExitCode::from(setup::main(&given[1..])),
            Some("health") => return ExitCode::from(health::main(&given[1..])),
            Some("status") => return ExitCode::from(status::main(&given[1..])),
            Some("shipped") => return ExitCode::from(shipped::main(&given[1..])),
            Some("tidy") => return ExitCode::from(tidy::main(&given[1..])),
            Some("plan") => return ExitCode::from(plan::main(&given[1..])),
            Some("protolab") => return ExitCode::from(protolab::main(&given[1..])),
            Some("top") => return ExitCode::from(top::main(&given[1..])),
            _ => {}
        }
    }
    let given = given.and_then(|mut given| {
        mask::set_private(&private(&mut given)?)?;
        Ok(given)
    });
    let given = match given {
        Ok(given) => given,
        Err(message) => {
            complain(&message);
            return ExitCode::from(2);
        }
    };
    if given == ["--mask"] {
        return match mask_stdin() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                complain(&format!("console: --mask: {e}"));
                ExitCode::FAILURE
            }
        };
    }
    let args = match args(&given) {
        Ok(args) => args,
        Err(message) => {
            complain(&message);
            return ExitCode::from(2);
        }
    };
    let stop = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        if let Err(e) = signal_hook::flag::register(signal, Arc::clone(&stop)) {
            complain(&format!("console: cannot listen for signal {signal}: {e}"));
            return ExitCode::FAILURE;
        }
    }
    match run(&args, &stop) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            complain(&format!("console: {}: {e}", args.out));
            ExitCode::FAILURE
        }
    }
}

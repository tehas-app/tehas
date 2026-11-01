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
//!   console --progress <ndjson> ... --now <file>
//!
//! With --now the console also keeps the cycle's step and the driver's last
//! notice in that file, two lines rewritten whenever they change, so the
//! watch loop of tehas work reads them instead of starting processes over the
//! log every tick, see src/now.rs.
//!
//!   console --mask [--private <name>]...
//!
//! The mask alone: standard input to standard output, for libexec/work's own
//! line on the dashboard, so that the mask is written in one place.
//!
//!   console preview run|sweep
//!   console preview choose <repo> [max] <pull requests
//!   console preview network check|create|ensure <network>
//!
//! A preview of every open pull request and which ones get one, see
//! src/preview.rs, and whether the previews' network keeps the host out, see
//! src/edge.rs.
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
//!   console health run <home> [--budget <spec>] | report ...
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
//!   console improve due|run <prompt file>
//!
//! One run of the improvement loop, see src/improve/mod.rs.
//!
//!   console disk <home> [--full] [--json] [--publish]
//!
//! How full this host is and what fills it, for tehas disk and the MCP tool
//! factory_disk, see src/disk.rs.
//!
//!   console energy [--every]
//!
//! One sample of what the model mesh draws, live/energy.json, for tehasbot
//! energy, see src/energy.rs.
//!
//!   console disk free <path> [<seconds>]
//!
//! The free kilobytes a command holds work on, see src/disk.rs.
//!
//!   console size measure | record <issue> <ledger>
//!
//! The measure of an issue body and the ledger of what approved starts
//! measured, see src/size.rs.
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
//!   console state pack|unpack ...
//!
//! A stopped factory's state as one tar, see src/state.rs.
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
//! answer says, for tehas tidy and tehasbot status (each command makes the call
//! to these verbs itself), see src/shipped.rs.
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
//!   console pr-title sync|poll ...
//!
//! A factory pull request's title, kept true by the prefix of its state, see
//! src/pr_title.rs.
//!
//!   console check-product [--sandbox] | table
//!   console init-product <branch> [--dry-run]
//!
//! The product against the contract, and the files it is missing proposed in a
//! pull request, see src/product.rs.
//!
//!   console tehas status|disk|state|limit|pause|analytics|top|api|labels|resume|drain|stop ...
//!   console tehasbot health|top
//!   console check tehas|tehasbot <command> ...
//!   console usage <command>
//!
//! The commands of bin/tehas and bin/tehasbot that are verbs of this program,
//! the check libexec/with-settings makes before it loads the settings, and
//! the text of tehas help, see src/commands.rs.
//!
//! Exit: 0 stopped by a signal or masked, 1 the console file or standard
//! output cannot be written, 2 usage.

mod api;
mod clean;
mod commands;
mod disk;
mod edge;
mod energy;
mod epics;
mod export;
mod flags;
mod forge;
mod frame;
mod health;
mod improve;
mod jq;
mod labels;
mod notice;
mod now;
mod plan;
mod pr_title;
mod preview;
mod product;
mod protolab;
mod render;
mod review;
mod setup;
mod shipped;
mod size;
mod snapshot;
mod state;
mod status;
mod stop;
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
    // The file of the step and the notice, when one was asked for.
    now: Option<String>,
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
        now: given
            .iter()
            .position(|a| *a == "--now")
            .and_then(|i| given.get(i + 1))
            .filter(|v| !v.is_empty())
            .cloned(),
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
    // The last two lines written, so a tick that changed nothing writes the
    // file again not at all.
    let mut written: Option<now::Now> = None;
    write(
        &mut out,
        &Entry::new("tehas", "console attached, waiting for the cycle", None),
    )?;
    loop {
        // Read before the lines are: a stop that arrives during a pass is
        // followed by one more pass, so the last lines are not lost.
        let stopping = stop.load(Ordering::Relaxed);
        out = reopened(out, &args.out)?;
        // The progress lines are read once a pass and given to both readers:
        // the console file and, for --now, the step and the notice. Reading the
        // log again for those two would read the same bytes twice, and the
        // follower is what knows which lines are new.
        let fresh = follower.new_lines(&args.progress);
        for line in &fresh {
            if let Some(key) = render::progress_key(line) {
                if !seen.insert(key) {
                    continue;
                }
            }
            if let Some(entry) = render::progress_line(line) {
                write(&mut out, &entry)?;
            }
        }
        if let Some(path) = &args.now {
            // Written only when this pass moved one of the two lines: with a
            // cycle that says nothing the loop reads the same file each tick.
            // A now file that cannot be written ends the console as an
            // unwritable console file does, and the work script says so.
            let next = written
                .as_ref()
                .unwrap_or(&now::Now::default())
                .later(&now::read(&fresh));
            if written.as_ref() != Some(&next) {
                now::write(path, &next)?;
                written = Some(next);
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
            Some("check-product") => return ExitCode::from(product::check(&given[1..])),
            Some("check") => return ExitCode::from(commands::check(&given[1..])),
            Some("needs") => return ExitCode::from(commands::needs(&given[1..])),
            Some("clean") => return ExitCode::from(clean::main(&given[1..])),
            Some("disk") => return ExitCode::from(disk::main(&given[1..])),
            Some("energy") => return ExitCode::from(energy::main(&given[1..])),
            Some("epics") => return ExitCode::from(epics::main(&given[1..])),
            Some("export") => return ExitCode::from(export::main(&given[1..])),
            Some("flags") => return ExitCode::from(flags::main(&given[1..])),
            Some("forge") => return ExitCode::from(forge::main(&given[1..])),
            Some("init-product") => return ExitCode::from(product::init(&given[1..])),
            Some("review") => return ExitCode::from(review::main(&given[1..])),
            Some("preview") => return ExitCode::from(preview::main(&given[1..])),
            Some("setup") => return ExitCode::from(setup::main(&given[1..])),
            Some("size") => return ExitCode::from(size::main(&given[1..])),
            Some("health") => return ExitCode::from(health::main(&given[1..])),
            Some("improve") => return ExitCode::from(improve::main(&given[1..])),
            Some("state") => return ExitCode::from(state::main(&given[1..])),
            Some("status") => return ExitCode::from(status::main(&given[1..])),
            Some("shipped") => return ExitCode::from(shipped::main(&given[1..])),
            Some("tidy") => return ExitCode::from(tidy::main(&given[1..])),
            Some("plan") => return ExitCode::from(plan::main(&given[1..])),
            Some("pr-title") => return ExitCode::from(pr_title::main(&given[1..])),
            Some("protolab") => return ExitCode::from(protolab::main(&given[1..])),
            Some("tehas") => return ExitCode::from(commands::tehas(&given[1..])),
            Some("tehasbot") => return ExitCode::from(commands::tehasbot(&given[1..])),
            Some("top") => return ExitCode::from(top::main(&given[1..])),
            Some("usage") => return ExitCode::from(commands::usage(&given[1..])),
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

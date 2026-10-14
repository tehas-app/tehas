//! Turn a running /work cycle into one readable, append-only console file
//! for the dashboard.
//!
//!   console --rpc <work-N.jsonl> --runs <ensemble-runs dir>
//!     --out <console.txt> --since <epoch seconds>
//!
//! Follows two sources until it is told to stop (TERM or INT):
//!   - the cycle's Pi RPC stream: driver notices and step boundaries,
//!   - every agent transcript touched since the cycle began: what each agent
//!     said, the commands it ran and the start of what came back.
//!
//! Output is plain text. Tokens, database passwords and private network
//! addresses are masked, see mask.rs.
//!
//!   console --mask
//!
//! The mask alone: standard input to standard output, for libexec/work's own
//! line on the dashboard, so that the mask is written in one place.
//!
//! Exit: 0 stopped by a signal or masked, 1 the console file or standard
//! output cannot be written, 2 usage.

mod mask;
mod render;
mod tail;

use render::Entry;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const POLL: Duration = Duration::from_millis(1500);
// How often a stop is looked for between two polls.
const TICK: Duration = Duration::from_millis(100);

struct Args {
    rpc: String,
    runs: String,
    out: String,
    since: SystemTime,
}

fn args() -> Result<Args, String> {
    let given: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.into_string())
        .collect::<Result<_, _>>()
        .map_err(|_| "console: an argument is not UTF-8")?;
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
        rpc: value("rpc")?,
        runs: value("runs")?,
        out: value("out")?,
        since,
    })
}

fn write(out: &mut File, entry: &Entry) -> io::Result<()> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() % 86400);
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
    let mut workstreams: HashMap<String, String> = HashMap::new();
    write(
        &mut out,
        &Entry::new("tehas", "console attached, waiting for the cycle", None),
    )?;
    loop {
        // Read before the lines are: a stop that arrives during a pass is
        // followed by one more pass, so the last lines are not lost.
        let stopping = stop.load(Ordering::Relaxed);
        out = reopened(out, &args.out)?;
        for line in follower.new_lines(&args.rpc) {
            if let Some(entry) = render::rpc_line(&line) {
                write(&mut out, &entry)?;
            }
        }
        for transcript in follower.transcripts(&args.runs, args.since) {
            // A stop that arrives now goes straight to the last pass.
            if !stopping && stop.load(Ordering::Relaxed) {
                break;
            }
            let role = tail::role(&transcript.path);
            for line in follower.transcript_lines(&transcript) {
                if let Some(workstream) = render::workstream(&line) {
                    workstreams.insert(transcript.path.clone(), workstream);
                }
                // Parallel agents of one role are told apart as the driver
                // names them when it starts them.
                let who = match workstreams.get(&transcript.path) {
                    Some(workstream) => format!("{role}[{workstream}]"),
                    None => role.clone(),
                };
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
    if std::env::args_os().skip(1).eq(["--mask"]) {
        return match mask_stdin() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                complain(&format!("console: --mask: {e}"));
                ExitCode::FAILURE
            }
        };
    }
    let args = match args() {
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

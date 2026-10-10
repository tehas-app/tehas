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
//! Exit: 0 stopped by a signal, 1 the console file cannot be written,
//! 2 usage.

mod mask;
mod render;
mod tail;

use render::Entry;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
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
    let given: Vec<String> = std::env::args().skip(1).collect();
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

fn run(args: &Args, stop: &AtomicBool) -> io::Result<()> {
    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o644)
        .open(&args.out)?;
    let mut follower = tail::Follower::default();
    write(
        &mut out,
        &Entry::new("tehas", "console attached, waiting for the cycle", None),
    )?;
    loop {
        // Read before the lines are: a stop that arrives during a pass is
        // followed by one more pass, so the last lines are not lost.
        let stopping = stop.load(Ordering::Relaxed);
        for line in follower.new_lines(&args.rpc) {
            if let Some(entry) = render::rpc_line(&line) {
                write(&mut out, &entry)?;
            }
        }
        for path in follower.transcripts(&args.runs, args.since) {
            let role = tail::role(&path);
            for line in follower.new_lines(&path) {
                for entry in render::transcript_line(&role, &line) {
                    write(&mut out, &entry)?;
                }
            }
        }
        let skipped = follower.take_skipped();
        if skipped > 0 {
            let said = format!("log lines over 1 MiB left out: {skipped}");
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

fn main() -> ExitCode {
    let args = match args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let stop = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        if let Err(e) = signal_hook::flag::register(signal, Arc::clone(&stop)) {
            eprintln!("console: cannot listen for signal {signal}: {e}");
            return ExitCode::FAILURE;
        }
    }
    match run(&args, &stop) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("console: {}: {e}", args.out);
            ExitCode::FAILURE
        }
    }
}

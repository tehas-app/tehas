//! `tehas-cycle work`: tehas work, one pi-rukas /work cycle run headless in the
//! sandbox image, to its end.
//!
//! The container's command is pi-rukas's one-shot runner, `pi-rukas-headless
//! work <issue> --timeout <minutes> --json`. It writes one JSON result to its
//! standard output and NDJSON progress to its error stream, and exits with a
//! status of its own. Nothing is read from the cycle's tree: the outcome is the
//! result, which this command writes to state/cycles/<issue>.result.json.
//!
//! Before anything is moved or fetched, the image is asked what it supports
//! (`pi-rukas-headless --capabilities`). The cycle starts only when the image
//! speaks the contract versions read here (exit codes 1, result schema 5, stable
//! NDJSON 1), takes the flags passed, lists every PI_ENSEMBLE_* variable the
//! container is given as an input a host may set, and has the features this
//! command relies on: the host handoff channel, the cycle deadline when one is
//! given and, with --restart, a restart that clears needs-human-attention.
//! A refresh, a start whose approval record has how "refresh", passes
//! --continue-pr (pi-rukas#1468): the cycle checks out the issue's open pull
//! request, merges the branch into it and pushes to the same branch. An image
//! that does not take the flag refuses the refresh with 78, before anything is
//! moved; no other start passes it.
//! Anything else is exit 78. So is an entry of TEHAS_RUKAS_WORKAROUNDS whose
//! release the image's pi-rukas has reached: the workaround has expired.
//!
//! Five PI_ENSEMBLE_* values are this command's, whatever the instance says:
//!   PI_ENSEMBLE_HANDOFF_CHANNEL=host  pi-rukas posts no handoff comment and
//!                       applies no label; tehasbot cycle posts the one comment
//!                       from the result's handoff and labels the issue.
//!   PI_ENSEMBLE_HANDOFF_TIMEOUT_MS  the handoff allowance, 480000 (8 minutes,
//!                       pi-rukas's own default): the time the cycle's handoff
//!                       may take. The deadline below is the limit less this,
//!                       so the handoff never lands on the container's stop.
//!   PI_ENSEMBLE_CYCLE_DEADLINE   the time after which the cycle starts no new
//!                       step and parks with cap "deadline", in milliseconds
//!                       since the epoch: the start, plus TEHAS_WORK_TIMEOUT,
//!                       less the handoff allowance: the runner bounds every
//!                       step by the deadline. None when that leaves no time.
//!                       PI_ENSEMBLE_CYCLE_BUDGET_MS is not passed.
//!   PI_ENSEMBLE_VERIFY_CONCURRENCY  runs of the gate at once, from the
//!                       container's share of the CPUs and memory ceiling
//!                       (tehas-cycle sandbox verify).
//!   PI_ENSEMBLE_SPAWN_TIMEOUT_MS  one child's bound in ms from
//!                       TEHAS_CHILD_TIMEOUT: pi-rukas's backstop for a child
//!                       nothing else ended.
//!
//! Usage: tehas work <issue-number> [--restart]
//!
//! ```text
//! Exit status: the runner's own when it ran and wrote its result:
//!               0 merged, 10 handoff, 11 handoff for clarification,
//!              20 aborted, 21 interrupted, 30 crash or its own time limit,
//!              40 refused to start.
//!              2 the cycle was stopped here as stalled or at the time limit
//!                (status "timeout", state/cycles/<issue>.timed),
//!                or the runner ended without a result.
//!              A stall file holds the seconds and, on a second line, the step
//!              in flight as the runner's progress last said it, or "-".
//!              A runner that stopped itself at the time limit, its result
//!              naming no cap and no end, keeps its own exit status; its status
//!              is "timeout" and it leaves state/cycles/<issue>.timed too.
//!              That file holds the limit in seconds, who stopped the cycle
//!              (tehas or runner) and the step it was in.
//!              3 the sandbox cannot push to the repository; no cycle was started.
//!              4 no cycle was started. Where the dispatcher recorded its
//!                approval: the issue's text is not the text it approved, the
//!                approval could not be read, or the issue could not be read for
//!                its text. Under tehasbot cycle (TEHAS_CYCLE_ATTEMPT set): no
//!                approval of that start is on record and it was no start by
//!                hand, the state store could not be read, or the image the
//!                container starts from could not be put on the cycle's
//!                record. Where the instance gives a cycle the issue's
//!                images or files: one could not be fetched, the issue could not
//!                be read for them, or a file is not the one first fetched for
//!                that approval. In both cases the issue is told which, with no
//!                host path in what it is told.
//!              5 a cycle for this issue is running already; nothing was
//!                touched.
//!
//!              It leaves state/cycles/<issue>.runner_exit beside the result:
//!              the runner's own exit status, whatever this command exits with.
//!              tehasbot cycle names that in its comment, not this status. A
//!              container it saw killed leaves state/cycles/<issue>.status with
//!              the word `killed`, which is the only account of the end when the
//!              runner wrote no result at all. A container it watched to its end
//!              that wrote no result and not one line of its stream leaves
//!              state/cycles/<issue>.nostart with the container's own exit
//!              status: one line, plain digits, 1 to 999, no sign and no leading
//!              zero. tehasbot cycle makes that as tehas's own cap
//!              `runner-never-started`. One this command stopped, killed outright
//!              with no result, or ended at the time limit leaves no such note.
//!
//!             64 no bot token or no git identity; no cycle was started.
//!             78 the instance settings are invalid, the state store is not
//!                installed, tehas.slice is not usable on this host, the
//!                approval says driver lean, which this tehas cannot run yet
//!                (state/cycles/<issue>.lean-refused says that was the reason),
//!                the image does not support what the cycle is given, or the
//!                product names no gate in
//!                .pi/verify-cmd and holds neither package.json nor Cargo.toml
//!                at the commit the cycle's clone was made at; no cycle was
//!                started, nothing was pushed, and the workspace made for the
//!                cycle is left where it is.
//!            137 the container was killed while the cycle was running: out of
//!                memory, or stopped from outside. Not by this command's own
//!                stop, and not once the runner wrote its result.
//! ```
//!
//! Instance settings (lib/config.sh; defaults in brackets):
//!   TEHAS_WORK_REPO     the product checkout a cycle is cloned from
//!                       [$TEHAS_HOME/repos/<name>]
//!   TEHAS_WORK_TIMEOUT  seconds a cycle may take [21600]. The runner is given
//!                       them in whole minutes, at least 1, and stops itself;
//!                       this command stops the container TEHAS_WORK_GRACE
//!                       seconds later if it has not.
//!   TEHAS_WORK_STALL    seconds with no line added to any of the cycle's logs
//!                       before it is stopped as stalled [3600]
//!   TEHAS_CHILD_TIMEOUT seconds one child may take, as
//!                       PI_ENSEMBLE_SPAWN_TIMEOUT_MS [7200]
//!   TEHAS_CYCLE_RESERVE the container's memory floor [2g]
//!   TEHAS_RUKAS_WORKAROUNDS  settings that work around a pi-rukas defect, as
//!                       NAME@VERSION: VERSION is the release that fixes it []
//!   TEHAS_WORK_IMAGES   1 gives the cycle the images its issue links [0]
//!   TEHAS_WORK_DESCRIBE 1 has the model describe those images first [0]
//!   TEHAS_WORK_FILES    1 gives the cycle the text files its issue links [0]
//!   TEHAS_WORK_COMMENTS 1 takes those of the issue's comments too [0]
//!   TEHAS_WORK_ZIPS     1 unpacks a zip its issue links into the same directory
//!                       [0]; needs TEHAS_WORK_IMAGES or TEHAS_WORK_FILES
//! From the environment, for tests: TEHAS_PREFLIGHT_WAIT (seconds the push check
//! may take [300]), TEHAS_WORK_GRACE (seconds past the time limit [300]),
//! TEHAS_STOP_WAIT (seconds a docker stop may take before the container is
//! killed [60]), TEHAS_CONSOLE_WAIT (seconds the console may take to stop before
//! it is killed [30]) and TEHAS_TEXT_WAIT (seconds the
//! read of the issue's text may take, and every comment that names why a start
//! stopped [60]).
//!
//! TEHAS_WORK_TIMEOUT bounds the watch loop. Every call to Docker or git
//! outside it is bounded too: a dockerd that stops answering, or a network
//! that swallows the push check, must not hold the issue's lock and its slot
//! for ever.
//!
//! What the container is given is deliberate and pinned by test/work_test.sh:
//! the bot token (pi-rukas pushes and opens the pull request from inside), the
//! PI_ENSEMBLE_* settings, the bot's git identity, the cycle's workspace, and a
//! directory of its own for what it writes: its copy of the code index, its
//! memory and Pi's sessions. Nothing it writes there is what a later cycle of
//! another issue reads. Not the Docker socket, SSH keys or ~/.config/tehas.
//!
//! With TEHAS_WORK_IMAGES=1 it is given two things more: the issue's images,
//! read-only, and PI_ENSEMBLE_EXTRA_IMAGE_DIRS, their path inside the container.
//! The images are fetched here, before anything starts:
//!   which     Links to GitHub's own attachment storage in the issue's text.
//!             Comments only with TEHAS_WORK_COMMENTS=1, and only from the
//!             record: see below. Only when an allowed
//!             account approved the start, or the issue was opened by an owner,
//!             member or collaborator. Where the dispatcher recorded its
//!             approval on the state store, the links are the approval's and
//!             the approver its own; a body whose hash is not the approval's
//!             stops the start. By hand, without one, the links are the body's
//!             and the approver is who applied the last tehas:ready.
//!   types     png, jpg, webp and gif, told by the file's first bytes. Not svg:
//!             it is text and can carry instructions.
//!   how much  At most 8 images and 10 megabytes together.
//!
//! With TEHAS_WORK_FILES=1 it is given the same directory and the same variable,
//! whether or not TEHAS_WORK_IMAGES is 1, and the directory holds the issue's
//! text files too, by the same rule of which links and whose approval:
//!   types     md, txt, csv, json, yaml, yml, log, html, css and js, by the
//!             name's extension, then checked: valid UTF-8 and no NUL byte.
//!             html is text here; rendering it is the Protolab's job.
//!   how much  At most 16 files, 1 megabyte each and 4 megabytes together.
//! They are named file-1.md and so on and listed under "files" in the manifest
//! with their original name. The pi-rukas that tehas pins does not read them
//! yet.
//!
//! With TEHAS_WORK_ZIPS=1 the same directory also holds what a zip the issue
//! links contains, and this adds no mount and no variable more:
//!   which     A link whose first four bytes are those of a zip archive, not one
//!             named .zip that is something else: that is left out and said.
//!             At most 2 archives per issue, each downloaded at most 4 MB.
//!   what      Every file of the archive through the checks above: an image by
//!             its first bytes, a text file by its extension and its encoding.
//!             Anything else, an svg or a nested archive among them, is left
//!             out with its path in the archive and the reason.
//!   how much  The unpacked files count against the limits above, the images'
//!             and the files', as the ones the issue links do. The archive is
//!             unpacked on the room left of the files' 4 MB.
//! They are named on from the linked ones, image-1.png and file-1.md, and each
//! manifest entry says the zip it came from under "zip" and its path inside the
//! archive under "path". The archive itself is not kept, and the directory it
//! was unpacked into is removed whatever it held.
//! They go to $TEHAS_HOME/work/<name>-<issue>.images, beside the workspace and
//! not in it, so a patch cannot commit them, under names made here, with a
//! manifest.json that lists each file, the link it came from, its type, size
//! and sha256, the approval, and what was left out and why. The directory is
//! remade for every cycle. The manifest is kept as
//! state/cycles/<issue>.manifest.json, with images.md and retry-context.md
//! beside it as <issue>.images.md and <issue>.retry-context.md when the cycle
//! has them, for the improvement loop's record, and a later fetch of the same
//! approval whose link gives other bytes stops the start. An image that is left
//! out is said in a comment on the issue, and each one given with its sha256.
//! That comment says whether the cycle was shown them, which is what the sandbox
//! image's own label answers: only when it says its pi-rukas reads the manifest
//! does tehas say a cycle was given images. An image that could not be fetched
//! stops the start: a cycle that works without a picture the issue relies on
//! builds the wrong thing.
//!
//! With TEHAS_WORK_COMMENTS=1 the record's comment_links are fetched too, after
//! the body's links, under the same limits counted together: links of comments
//! an allowed account wrote and nobody edited after the approval, as the
//! dispatcher found them. Each is taken only while its author is still allowed.
//! Every manifest entry has a source, "body", "comment:<id>" or, for a retry,
//! "pr-comment:<id>". By hand there is no record, and only the body is read. A
//! record of a retry tick may carry retry_context, what reviewers wrote on the
//! pull request: it goes to retry-context.md in the same directory, headed as
//! untrusted, every line quoted, cut at 32000 characters. No mount and no
//! variable more.
//!
//! With neither setting a cycle is given nothing: no directory is mounted and
//! no PI_ENSEMBLE_EXTRA_IMAGE_DIRS is passed on, whatever the instance file
//! says. The issue is still read once, for its text, and held to the
//! dispatcher's record (exit 4). That one read is the one the images and the
//! files are fetched from too, so a start asks GitHub for the issue once
//! whatever it is given. By hand, where this host holds no record, the run is
//! its own approval and GitHub is asked for nothing.
//! When the dispatcher's approval record lists attachment links, the gap is said
//! rather than left for an agent to find from a file that is not there: one line
//! naming how many links and which approval it read, on this command's error
//! stream and in $log.err, which is err.txt of the cycle's improvement record, and
//! a manifest kept as state/cycles/<issue>.manifest.json. What the line and the
//! manifest say, and why, is console/attach's (src/gives.rs, left_out).
//!
//! A cycle that ended at a cap, or was aborted, with nothing in $log.err gets that
//! account there: the cap with the reason the progress read for it, or the abort
//! that ended the cycle with no cap, the lines of the cycle's own stream that carry
//! the driver's own warning, kill, failure or cancellation, and the name of the
//! file that holds them all. Never a claim that the record says nothing more
//! without looking at the stream first.
//!
//! With TEHAS_WORK_DESCRIBE=1 the model endpoint describes each image in words
//! first (tehas-cycle describe), into images.md in the same directory, for the steps
//! that are not shown the pixels. It is in the directory already mounted: no
//! mount and no variable more. A description that fails says so in the file and
//! stops nothing.
//!
//! tehas work also bounds one child of a cycle, from TEHAS_CHILD_TIMEOUT, so a
//! fixer still making progress is not stopped at pi-rukas's own default with
//! hours of the cycle's budget left.
//!
//! The container runs in tehas.slice, as every sandbox container does, with the
//! limits tehas-cycle sandbox makes: a memory ceiling of 1/TEHAS_SLICE_CONTAINERS of the
//! slice, no swap, the reserve as its floor, a CPU quota of the host less one CPU,
//! a pids limit that follows its share of that and a CPU weight.
//! It has an init (--init): Pi collects only its own children, and a process
//! that outlives its parent would count against the pids limit until the
//! container ends. A pass of the shell tests leaves several hundred.
//! Without a usable slice nothing is cloned and nothing starts. The check that
//! the sandbox can push is a container of a few seconds and is not in the slice.
//!
//! Which approval is this start's: under tehasbot cycle, TEHAS_CYCLE_ATTEMPT
//! names the attempt the state store gave the start, and only the approval that
//! attempt used counts (the store's approval-view). A start the store
//! has on record as one by hand is a run by hand. Anything else stops the start
//! with exit 4: a store that cannot be read is never taken for a run by hand.
//! Run on its own (tehas work by hand), the approval of the issue's latest start
//! counts, and none is a run by hand.
//!
//! As the container starts, the image's id and the pi-rukas commit of its label
//! tehas.rukas go on the cycle's record on the state store (cycle-image), for
//! the cycle's report. So do the PI_ENSEMBLE_* values the container is given,
//! the TEHAS_* settings that shape a cycle and how many cycles hold their lock,
//! this one with them (console/cycle/src/improve.rs). Only under tehasbot cycle: a run on its
//! own has no attempt to put them on.
//!
//! What the steps before the container work out for each other is in
//! state/cycles/<issue>.prepared, under the lock. It goes before the container
//! starts.
//!
//! Docker writes the container's id to state/cycles/<issue>.cid, which is how
//! the dispatcher tells this cycle's container from the others in the slice.
//! The file is removed at the end. This command counts the processes the kernel
//! killed for memory in the container from its cgroup while it runs, and from
//! the kernel log when it has ended. A count above 0 is printed and left in
//! state/cycles/<issue>.oom for tehasbot cycle. The starts of a process that the
//! pids limit refused are counted the same way, into state/cycles/<issue>.forks.
//!
//! It holds the issue's lock, state/cycles/<issue>.lock, as fd 9 for as long as
//! it runs, so a run by hand and the unit's cycle never share a workspace.
//! libexec/work loads the settings, takes the lock and execs this; under
//! tehasbot cycle the lock comes down from libexec/cycle already held. The lock
//! stays in bash because every program this starts, the docker client among
//! them, must inherit it, and safe Rust opens each file close-on-exec. So a
//! cycle killed outright keeps its place until they are gone too.
//!
//! A TERM, INT or HUP before the container starts ends the start after the
//! command it is in, with 143 and no container. Once the container runs, the
//! watch stops it and the cycle ends interrupted.
//!
//! The workspace is $TEHAS_HOME/work/<name>-<issue>, a fresh clone made here for
//! every cycle. What a stopped cycle of the same issue left there is kept
//! beside it as <name>-<issue>.before-retry. Every cycle has its workspace at the
//! same path inside its container, $TEHAS_HOME/work/<name>: the code index is kept
//! under that path, so one index serves all cycles. The container never sees the
//! product checkout: an agent can write anything into the tree it works in, hooks
//! and git configuration included, and the host runs git in the product checkout.
//! So the host runs git only in a tree no container has touched, and reads nothing
//! from the workspace afterwards.
//!
//! Beside the workspace is <name>-<issue>.state, the cycle's own: cache holds its
//! copy of the code index $TEHAS_HOME/state/index, memory its own store of what
//! its agents learned, sessions Pi's session files. It is made fresh for every
//! cycle and removed with the workspace by tehasbot cycle. The cycle's agents
//! index nothing of another cycle's and their caches start cold.
//!
//! The agents' transcripts go to ~/.pi/agent/ensemble-runs/issue-<issue>, so a
//! cycle sees its own and the console shows no other cycle's agents.
//!
//! A cycle that writes nothing is stopped before its time limit. While the
//! runner's progress, its error stream and the agents' transcripts all stay as
//! they are for TEHAS_WORK_STALL seconds, the cycle is taken as stalled: a step
//! that hangs on a child process, or a gate that never returns, does not hold a
//! slot for hours. The status is then "stalled", and state/cycles/<issue>.stalled
//! holds the seconds and the step in flight for tehasbot cycle, as .timed does.
//! The limit is longer than one run of the product's gate may take, which can
//! print nothing.
//!
//! The console and the dashboard's line are fed from the stable kinds of the
//! runner's NDJSON only. At the end, the per-role usage and why the cycle
//! stopped at its cap go to state/cycles/<issue>.progress.json for the report
//! (console/cycle/src/progress.rs). They come from the issue's entry of the schema 5
//! result, which every image tehas starts a cycle on is checked for before the
//! cycle starts. A runner that wrote no result leaves the usage of the stable
//! dispatch-completed lines and a reason read from lines the runner marks
//! unstable, which may be empty.

use crate::gives::{self, Gap};
use crate::prepare::{self, Stop, STOPPED};
use crate::start;
use crate::watch;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tehas_core::{self, sandbox, slice, time};

/// What libexec/work gives this.
pub struct Given {
    pub issue: u64,
    pub restart: bool,
    /// The console beside the script, which the watch runs.
    pub console: String,
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

/// A wait a test may set, one to four digits and not 0, else the default.
fn short_wait(name: &str, default: u64) -> u64 {
    let v = env(name);
    let shaped =
        (1..=4).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_digit()) && !v.starts_with('0');
    if shaped {
        v.parse().unwrap_or(default)
    } else {
        default
    }
}

fn executable(path: &str) -> bool {
    fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// What the start leaves until the watch: the approval, the issue's answer and
/// the plan of the steps. They go however the start ends.
struct Leftovers(Vec<PathBuf>);

impl Drop for Leftovers {
    fn drop(&mut self) {
        for f in &self.0 {
            let _ = fs::remove_file(f);
        }
    }
}

/// The state store is installed: the program and its database.
fn store_ready(home: &Path) -> bool {
    let bin = home.join("libexec/store");
    let db = home.join("state/tehas.db");
    !home.as_os_str().is_empty()
        && executable(&bin.to_string_lossy())
        && fs::symlink_metadata(&db).is_ok_and(|m| m.is_file())
}

/// The product checkout is there and is TEHAS_REPO's. Docker would make a
/// missing mount source as a root-owned directory. tehas_core::settings holds
/// the rule, with the same line lib/config.sh gives.
fn checkout_ok() -> bool {
    match tehas_core::settings::checkout() {
        Ok(()) => true,
        Err(line) => {
            eprintln!("{line}");
            false
        }
    }
}

/// The branch cycles start from: TEHAS_BRANCH, else the checkout's
/// origin/HEAD, which install.sh sets. Never looked up here.
fn branch() -> Option<String> {
    match tehas_core::settings::branch() {
        Ok(name) => Some(name),
        Err(line) => {
            eprintln!("{line}");
            None
        }
    }
}

/// Why the limits and security options a slice gave are not the documented
/// ones: fourteen words of limits in tehas.slice, and the ten of "What a
/// container may not do". A list that drifted starts nothing.
fn misshaped(limits: &[String], security: &[String]) -> Option<String> {
    let (who, what) = ("tehas work", "a cycle");
    let l = limits;
    if l.len() != 14 || l[1] != "tehas.slice" || l[12] != "--cpu-shares" {
        return Some(format!(
            "{who}: the container limits could not be made, not starting"
        ));
    }
    let s = security;
    let tmpfs = s.get(9).and_then(|w| {
        w.strip_prefix("/tmp:rw,exec,nosuid,nodev,size=")?
            .strip_suffix("m,mode=1777")
    });
    let fixed = [
        "--cap-drop",
        "ALL",
        "--cap-add",
        "CHOWN",
        "--cap-add",
        "FOWNER",
        "--security-opt",
        "no-new-privileges:true",
        "--tmpfs",
    ];
    let shaped = s.len() == 10
        && s[..9] == fixed
        && tmpfs.is_some_and(|m| {
            !m.is_empty() && m.bytes().all(|b| b.is_ascii_digit()) && !m.starts_with('0')
        });
    (!shaped).then(|| {
        format!(
            "{who}: tehas_sandbox_security gave {} words, wants 10: not starting {what}",
            s.len()
        )
    })
}

/// The container's limits and security options, as the slice gives them.
fn limits() -> Option<Vec<String>> {
    let load = slice::Load::from_env();
    let taken = match sandbox::take(&slice::Host::here(), &load, "tehas work", "a cycle") {
        Ok(t) => t,
        Err(line) => {
            eprintln!("{line}");
            return None;
        }
    };
    if let Some(why) = misshaped(&taken.limits, &taken.security) {
        eprintln!("{why}");
        return None;
    }
    let mut words = taken.limits;
    words.extend(taken.security);
    Some(words)
}

/// The app's token for the container, with at least 10 minutes left. Without
/// an app, GH_TOKEN is the instance's.
fn github_token(console: &str) -> i32 {
    if !env("TEHAS_GITHUB_APP_ID").is_empty() {
        let out = Command::new(console)
            .args(["tehasbot", "github-token"])
            .stderr(Stdio::inherit())
            .output();
        match out {
            Ok(o) if o.status.success() => {
                let token = String::from_utf8_lossy(&o.stdout);
                // Before any thread: the process is still single-threaded.
                std::env::set_var("GH_TOKEN", token.trim_end_matches('\n'));
            }
            _ => {
                eprintln!("tehas work: no GitHub token for the app");
                return 1;
            }
        }
    }
    if env("GH_TOKEN").is_empty() {
        eprintln!("tehas work: GH_TOKEN is not set (source ~/tehas/env.sh)");
        return 64;
    }
    0
}

/// The renewer of the app's token while the container reads it, none without
/// an app. The watch ends it with a TERM.
fn renewer(home: &Path) -> Option<Child> {
    if env("TEHAS_GITHUB_APP_ID").is_empty() {
        return None;
    }
    let every = env("TEHAS_GITHUB_RENEW_EVERY");
    let every = if every.is_empty() {
        "300".to_string()
    } else {
        every
    };
    let child = Command::new(home.join("libexec/tehas-cycle"))
        .arg("renew")
        .arg("--cmd")
        .arg(home.join("libexec/github-token"))
        .args(["--every", &every, "--retry", "30"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn();
    child.ok()
}

/// The renewer gone after the watch's TERM, or killed after five seconds.
fn reap(mut child: Child) {
    for _ in 0..100 {
        if !matches!(child.try_wait(), Ok(None)) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// A child to its end, or until a stop came: then it is left to finish on its
/// own, as a step's command is, and None.
fn until_stop(mut child: Child, stop: &Stop) -> Option<bool> {
    loop {
        match child.try_wait() {
            Ok(Some(s)) => return Some(s.success()),
            Ok(None) if stop.flag().load(std::sync::atomic::Ordering::SeqCst) => return None,
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => return Some(false),
        }
    }
}

/// A comment on the issue, bounded. Its failure is said, never a stop.
fn comment(issue: u64, wait: u64, body: &str) {
    let done = Command::new("timeout")
        .args(["-k", "10", &wait.to_string(), "gh", "issue", "comment"])
        .args([
            &issue.to_string(),
            "--repo",
            &env("TEHAS_REPO"),
            "--body",
            body,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if !done.is_ok_and(|s| s.success()) {
        eprintln!("tehas work: the comment on #{issue} failed");
    }
}

/// Whether an allowed account approved the text the start works on: yes,
/// no, or the label events could not be read.
enum Approved {
    Yes,
    No,
    Unread,
}

fn approved(issue: u64, record: &str, console: &str) -> Approved {
    if start::is_record(record) {
        // The dispatcher's record of the start: who approved and which links. A
        // start of the millwright's counts on the person's approval it rests on.
        let by = start::record_value(Path::new(record))
            .and_then(|r| r.get("by").and_then(|b| b.as_str().map(String::from)))
            .unwrap_or_default();
        let ok = gives::allowed(&by)
            || (gives::millwright_is(&by) && gives::millwright_base(record).is_some());
        return if ok { Approved::Yes } else { Approved::No };
    }
    // By hand. The label approved the text, links and all.
    let console = [env("TEHAS_CONSOLE_CMD"), env("TEHAS_CONSOLE")]
        .into_iter()
        .find(|c| !c.is_empty())
        .unwrap_or_else(|| console.to_string());
    let asked = Command::new(console)
        .args(["label", "allowed", &issue.to_string(), "tehas:ready"])
        .stdin(Stdio::null())
        .status();
    match asked.ok().and_then(|s| s.code()) {
        Some(0) => Approved::Yes,
        Some(2) => Approved::Unread,
        _ => Approved::No,
    }
}

/// What a start is given of its issue's files.
struct Files<'a> {
    home: &'a Path,
    issue: u64,
    record: &'a str,
    dir: &'a Path,
    inside: &'a str,
    kept: &'a Path,
    kinds: &'a str,
    text_wait: u64,
}

/// The issue's images and files, fetched before the workspace: a start that
/// fails here must not move aside what the last cycle left. The options that
/// give them to the container, or the exit status.
fn fetch(f: &Files, answer: &str, console: &str, stop: &Stop) -> Result<Vec<String>, i32> {
    let n = f.issue;
    let ok = match approved(n, f.record, console) {
        Approved::Unread => {
            Err("the issue's label events could not be read for its images".to_string())
        }
        how => {
            let word = if matches!(how, Approved::Yes) {
                "approved"
            } else {
                ""
            };
            gives::issue_images(f.home, n, f.dir, word, f.record, answer)
                .and_then(|()| gives::changed(f.home, f.kept, f.dir))
        }
    };
    if let Err(why) = ok {
        let _ = fs::remove_dir_all(f.dir);
        eprintln!("tehas work: {why}, not starting a cycle");
        comment(n, f.text_wait, &format!("Tehas started no cycle: {why}. This instance gives a cycle the {} an issue links, and a cycle that works without one of them may build the wrong thing. To retry, apply `tehas:ready`.", f.kinds));
        return Err(4);
    }
    if env("TEHAS_WORK_DESCRIBE") == "1" {
        // A program of its own, so a stop does not wait on the model.
        // The tehas-cycle beside the script, as a pinned cycle has it.
        let me = Path::new(console).with_file_name("tehas-cycle");
        let described = {
            Command::new(me)
                .arg("describe")
                .arg(f.dir)
                .stdin(Stdio::null())
                .spawn()
        };
        let done = match described {
            Ok(child) => until_stop(child, stop),
            Err(_) => Some(false),
        };
        match done {
            None => {
                stop.came();
                return Err(STOPPED);
            }
            Some(false) => eprintln!("tehas work: the descriptions of the images could not be written, the cycle goes on without them"),
            Some(true) => {}
        }
    }
    let kept_new = f.kept.with_extension("json.new");
    let copied = fs::copy(f.dir.join("manifest.json"), &kept_new).is_ok()
        && fs::rename(&kept_new, f.kept).is_ok();
    if !copied {
        eprintln!(
            "tehas work: could not keep the manifest as {}",
            f.kept.display()
        );
    }
    // Kept beside it for the improvement loop's record: the directory is removed
    // when the cycle ends. Only this cycle's: an earlier one's would mislead.
    let stem = f.home.join(format!("state/cycles/{n}"));
    for name in ["images.md", "retry-context.md"] {
        let beside = PathBuf::from(format!("{}.{name}", stem.display()));
        let _ = fs::remove_file(&beside);
        let from = f.dir.join(name);
        if from.is_file() && fs::copy(&from, &beside).is_err() {
            eprintln!(
                "tehas work: could not keep {name} beside {}",
                f.kept.display()
            );
        }
    }
    let said = gives::said(f.home, f.dir, "cycle", &gives::reads_images());
    if !said.is_empty() {
        comment(n, f.text_wait, &said);
    }
    Ok(vec![
        "-v".to_string(),
        format!("{}:{}:ro", f.dir.display(), f.inside),
        "-e".to_string(),
        format!("PI_ENSEMBLE_EXTRA_IMAGE_DIRS={}", f.inside),
    ])
}

/// The line that says what a cycle given nothing was not given, on the error
/// stream and in $log.err. Said only once the cycle can start: a start that
/// fails at the workspace or the push check leaves no claim about a cycle that
/// never ran.
fn gap(home: &Path, issue: u64, kept: &Path, record: &str, log: &Path) -> String {
    let said = match gives::left_out(home, issue, kept, record) {
        Gap::Said(gap) => format!("tehas work: {gap}"),
        Gap::Unread => format!("tehas work: the approval record of issue {issue}, or the manifest kept for it, could not be read: nothing is said about what this cycle was not given"),
        Gap::Nothing => return String::new(),
    };
    eprintln!("{said}");
    let err = PathBuf::from(format!("{}.err", log.display()));
    let wrote = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&err)
        .and_then(|mut f| writeln!(f, "{said}"));
    if wrote.is_err() {
        eprintln!("tehas work: could not write {}", err.display());
    }
    said
}

fn git_email() -> String {
    let out = Command::new("git")
        .args(["config", "--global", "user.email"])
        .stdin(Stdio::null())
        .output();
    out.map(|o| {
        String::from_utf8_lossy(&o.stdout)
            .trim_end_matches('\n')
            .to_string()
    })
    .unwrap_or_default()
}

/// One cycle of tehas work. Its exit status.
pub fn run(home: &Path, g: &Given) -> i32 {
    let stop = Stop::default();
    for sig in [
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGHUP,
    ] {
        let which = signal_hook::flag::register_usize(sig, stop.which(), sig as usize);
        if signal_hook::flag::register(sig, stop.flag()).is_err() || which.is_err() {
            eprintln!("tehas work: cannot listen for signal {sig}, not starting a cycle");
            return 78;
        }
    }
    let n = g.issue;
    // Every start reads an approval from the store.
    if !store_ready(home) {
        eprintln!("tehas work: ERROR: the state store is not installed (run install.sh)");
        return 78;
    }
    if let Err(line) = sandbox::settings_ok() {
        eprintln!("{line}");
        return 78;
    }
    if !checkout_ok() {
        return 78;
    }
    let Some(branch) = branch() else {
        return 78;
    };
    let cmd = env("TEHAS_CONSOLE_CMD");
    let console = if cmd.is_empty() {
        g.console.clone()
    } else {
        cmd
    };
    if !executable(&console) {
        eprintln!(
            "tehas work: no console beside this command at {}, run install.sh",
            g.console
        );
        return 78;
    }
    // No container starts outside the slice, and none with half its limits.
    let Some(words) = limits() else {
        return 78;
    };
    let work_dir = env("TEHAS_WORK_DIR");
    // The issue's images on the host, and where the container has them.
    let images = PathBuf::from(format!("{work_dir}-{n}.images"));
    let images_inside = format!("{work_dir}.images");
    // The runner's NDJSON progress, which it writes to its error stream. Named as
    // the improvement loop looks for it.
    let log = home.join(format!("logs/work-{n}-{}.jsonl", time::stamp(time::now())));
    let name = format!("tehas-work-{n}");
    let runs = PathBuf::from(format!("{}/.pi/agent/ensemble-runs/issue-{n}", env("HOME")));
    // The manifest of the last fetch, kept.
    let kept = home.join(format!("state/cycles/{n}.manifest.json"));
    let number = |name: &str, default: u64| env(name).parse().unwrap_or(default);
    let caps_wait = short_wait("TEHAS_CAPS_WAIT", 120);
    let text_wait = short_wait("TEHAS_TEXT_WAIT", 60);
    let stop_wait = number("TEHAS_STOP_WAIT", 60);
    let grace = env("TEHAS_WORK_GRACE")
        .parse()
        .ok()
        .filter(|s| *s < 100_000)
        .unwrap_or(300);
    let console_wait = env("TEHAS_CONSOLE_WAIT")
        .parse()
        .ok()
        .filter(|s| (1..10_000).contains(s))
        .unwrap_or(30);
    let seconds = |name: &str| env(name).parse::<u64>().ok();
    let (Some(limit), Some(stall)) = (seconds("TEHAS_WORK_TIMEOUT"), seconds("TEHAS_WORK_STALL"))
    else {
        eprintln!("tehas work: TEHAS_WORK_TIMEOUT and TEHAS_WORK_STALL are not whole seconds, not starting a cycle");
        return 78;
    };
    let rc = github_token(&console);
    if rc != 0 {
        return rc;
    }
    let made = fs::create_dir_all(home.join("logs"))
        .and_then(|()| fs::create_dir_all(home.join("state/cycles")));
    if made.is_err() {
        eprintln!(
            "tehas work: could not make the logs and state of {}, not starting a cycle",
            home.display()
        );
        return 78;
    }

    // What the dispatcher approved for this start, as a private file that goes
    // when this command ends, and why a start with none stops, which is said
    // below with the issue's text. Read before anything of the cycle is written.
    // Driver lean is refused here: it needs trail-openers/pi-rukas#1920.
    let attempt = env("TEHAS_CYCLE_ATTEMPT");
    let prepared = home.join(format!("state/cycles/{n}.prepared"));
    let mut left = Leftovers(vec![prepared.clone()]);
    let (record, record_why) = match start::record(home, n, &attempt) {
        Ok(r) => r,
        Err(_) => return 78,
    };
    let record = record.map_or(String::new(), |r| {
        left.0.push(r.clone());
        r.to_string_lossy().into_owned()
    });
    if stop.came() {
        return STOPPED;
    }

    // The images and the text files share one directory, given to the container
    // when the instance gives either kind.
    let (files_on, images_on) = (
        env("TEHAS_WORK_FILES") == "1",
        env("TEHAS_WORK_IMAGES") == "1",
    );
    let give = files_on || images_on;
    let kinds = match (images_on, files_on) {
        (true, true) => "images and files",
        (false, true) => "files",
        _ => "images",
    };
    // The bot's identity, what the container is given and what the image
    // supports, before anything is moved or fetched. What the next steps need
    // is in the prepared file.
    let rc = prepare::check(&prepare::Check {
        issue: n,
        file: prepared.clone(),
        record: record.clone(),
        restart: g.restart,
        caps_wait,
        console: console.clone(),
        words,
        stop: stop.clone(),
    });
    if rc != 0 {
        return rc;
    }

    // The words a cycle works on are the words somebody approved. This is the
    // last place on the host that can still compare them, and it compares as it
    // starts, so a body or a title edited after the label starts no cycle. A
    // start with an approval reads the issue once, here, for all of it.
    let text = start::Text {
        issue: n,
        repo: env("TEHAS_REPO"),
        record: record.clone(),
        why: record_why.to_string(),
        log: log.clone(),
        wait: text_wait,
        console: console.clone(),
    };
    let answer = match start::text(home, &text) {
        Ok(Some(f)) => {
            left.0.push(f.clone());
            let read = fs::read(&f).unwrap_or_default();
            String::from_utf8_lossy(&read)
                .trim_end_matches('\n')
                .to_string()
        }
        Ok(None) => String::new(),
        Err(rc) => return rc,
    };
    if stop.came() {
        return STOPPED;
    }

    let works = images.parent().unwrap_or(Path::new("/"));
    let marked =
        fs::create_dir_all(works).and_then(|()| fs::write(works.join(".tehas-workspaces"), ""));
    if marked.is_err() {
        eprintln!(
            "tehas work: could not make {}, not starting a cycle",
            works.display()
        );
        return 78;
    }
    let mut image_opts = Vec::new();
    if give {
        let f = Files {
            home,
            issue: n,
            record: &record,
            dir: &images,
            inside: &images_inside,
            kept: &kept,
            kinds,
            text_wait,
        };
        image_opts = match fetch(&f, &answer, &g.console, &stop) {
            Ok(o) => o,
            Err(rc) => return rc,
        };
        if stop.came() {
            return STOPPED;
        }
    }

    // A fresh workspace for this cycle, the gate it runs, the check that it can
    // push, its copy of the code index and its mounts.
    let rc = prepare::tree(
        home,
        &prepare::Tree {
            issue: n,
            file: prepared.clone(),
            branch,
            log: log.clone(),
            push_wait: number("TEHAS_PREFLIGHT_WAIT", 300),
            console: console.clone(),
            stop: stop.clone(),
        },
    );
    if rc != 0 {
        return rc;
    }
    let gap = if give {
        String::new()
    } else {
        gap(home, n, &kept, &record, &log)
    };

    // The cycle's image and settings on its record, the notes of an earlier cycle
    // cleared, and the words of its docker run.
    let docker = match prepare::record(
        home,
        &prepare::Record {
            issue: n,
            file: prepared,
            attempt: attempt.clone(),
            log: log.clone(),
            name: name.clone(),
            stop_wait,
            text_wait,
            gap,
            images: image_opts,
            stop: stop.clone(),
        },
    ) {
        Ok(words) => words,
        Err(rc) => return rc,
    };
    drop(left);
    if stop.came() {
        return STOPPED;
    }

    // The runner is the container's command, so a docker stop reaches it: it
    // ends the cycle as interrupted and still writes its result. The container
    // reads the app's token from a directory the host keeps fresh; the renewer
    // ends with the container, or with the watch.
    let renew = renewer(home);
    let given = watch::Given {
        issue: n,
        attempt,
        log,
        runs,
        name,
        limit,
        stall,
        grace,
        stop_wait,
        console_wait,
        console: PathBuf::from(&g.console),
        models: format!("{}/.pi/agent/models.json", env("HOME")),
        email: git_email(),
        renewer: renew.as_ref().map_or(String::new(), |c| c.id().to_string()),
        docker,
        signal: stop.which(),
    };
    let rc = watch::run(home, &given, stop.flag());
    if let Some(c) = renew {
        reap(c);
    }
    rc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(String::from).collect()
    }

    const FOURTEEN: &str = "--cgroup-parent tehas.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --cpus 3.75 --pids-limit 4096 --cpu-shares 1024";
    const TEN: &str = "--cap-drop ALL --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true --tmpfs /tmp:rw,exec,nosuid,nodev,size=1024m,mode=1777";

    #[test]
    fn the_documented_lists_start_a_cycle() {
        assert_eq!(misshaped(&words(FOURTEEN), &words(TEN)), None);
    }

    #[test]
    fn a_list_of_limits_that_is_not_the_documented_one_starts_no_cycle() {
        for limits in [
            "--cgroup-parent tehas.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --cpus 3.75 --pids-limit 4096",
            "--cgroup-parent other.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --cpus 3.75 --pids-limit 4096 --cpu-shares 1024",
            "--memory 2048m --cgroup-parent tehas.slice --memory-swap 2048m --memory-reservation 2g --cpus 3.75 --pids-limit 4096 --cpu-shares 1024",
            "",
        ] {
            assert_eq!(
                misshaped(&words(limits), &words(TEN)).as_deref(),
                Some("tehas work: the container limits could not be made, not starting"),
                "{limits}"
            );
        }
    }

    #[test]
    fn security_options_that_are_not_the_documented_ten_start_no_cycle() {
        for (opts, n) in [
            ("--cap-drop ALL --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true", 8),
            ("--cap-drop NET_RAW --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true --tmpfs /tmp:rw,exec,nosuid,nodev,size=1024m,mode=1777", 10),
            ("--cap-drop ALL --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true --tmpfs /tmp:rw,exec,nosuid,nodev,size=0m,mode=1777", 10),
            ("--cap-drop ALL --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true --tmpfs /tmp:rw,exec,size=1024m,mode=1777", 10),
        ] {
            assert_eq!(
                misshaped(&words(FOURTEEN), &words(opts)),
                Some(format!("tehas work: tehas_sandbox_security gave {n} words, wants 10: not starting a cycle")),
                "{opts}"
            );
        }
    }

    #[test]
    fn a_short_wait_is_one_to_four_digits() {
        std::env::set_var("TEHAS_TEST_SHORT_WAIT", "0");
        assert_eq!(short_wait("TEHAS_TEST_SHORT_WAIT", 60), 60);
        std::env::set_var("TEHAS_TEST_SHORT_WAIT", "12345");
        assert_eq!(short_wait("TEHAS_TEST_SHORT_WAIT", 60), 60);
        std::env::set_var("TEHAS_TEST_SHORT_WAIT", "7");
        assert_eq!(short_wait("TEHAS_TEST_SHORT_WAIT", 60), 7);
        std::env::remove_var("TEHAS_TEST_SHORT_WAIT");
        assert_eq!(short_wait("TEHAS_TEST_SHORT_WAIT", 60), 60);
    }
}

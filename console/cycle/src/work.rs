//! `tehas-cycle work`: tehas work, one pi-rukas /work cycle run headless in the
//! sandbox image, to its end.
//!
//! The container's command is pi-rukas's one-shot runner, `pi-rukas-headless
//! work <issue> --timeout <minutes> --json`, or `work-lean` in place of `work`
//! when the start's driver is lean: the approval's, or --driver by hand. It writes one JSON result to its
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
//! given and, with --restart, a restart that clears needs-human-attention. On
//! lean the flags are the work-lean verb's, the features leanHandoffHost and
//! leanDeadline, and workLeanResultSchema is 5; an image without them leaves
//! state/cycles/<issue>.lean-refused and never runs classic in lean's place.
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
//! Usage: tehas work <issue-number> [--restart] [--driver lean|classic]
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
//!              that wrote no byte of its result and no line of its stream leaves
//!              state/cycles/<issue>.nostart with the container's own exit
//!              status: one line, plain digits, 1 to 999, no sign and no leading
//!              zero. tehasbot cycle makes that as tehas's own cap
//!              `runner-never-started`. One this command stopped, killed outright
//!              with no result, or ended at the time limit leaves no such note.
//!
//!             64 no bot token or no git identity, or --driver names neither
//!                lean nor classic; no cycle was started.
//!             78 the instance settings are invalid, the state store is not
//!                installed, tehas.slice is not usable on this host, the
//!                approval names a driver this tehas does not know, the image
//!                does not support what the cycle is given (on driver lean
//!                state/cycles/<issue>.lean-refused says so), or the
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
//!   TEHAS_SCRATCH_MAX   what the cycle's scratch may hold [20g]
//!   TEHAS_CYCLE_STOP_FREE  free disk of the scratch below which the cycle with
//!                       the largest scratch stops [5g] (src/scratch.rs)
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
//! The token reads issues only: the container's issue writes go to this
//! command through the relay directory in its state, `relay`, mounted
//! read-write at /run/tehas/relay, which src/relay.rs serves while the
//! container runs. A request still there when the container ends is answered
//! with 1.
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
//! state/cycles/<issue>.manifest.json, with images.md and comments.json
//! beside it as <issue>.images.md and <issue>.comments.json when the cycle
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
//! "pr-comment:<id>". By hand there is no record, and only the body is read.
//!
//! A record whose retry_context holds anything gives the cycle comments: the
//! comments a comments retry recorded at its tick (the issue's and its pull
//! requests', by allowed accounts that may write), the fixer's note of a
//! millwright's start, or a refresh's task. console/attach writes them to
//! comments.json in the same directory, in the format of
//! trail-openers/pi-rukas#2020, and names it under "comments" in the manifest:
//! each item cut at 4000 characters, the blob at 64 KB with the oldest dropped
//! and counted. With TEHAS_WORK_IMAGES and TEHAS_WORK_FILES both 0 the
//! directory is still mounted and named in PI_ENSEMBLE_EXTRA_IMAGE_DIRS, and
//! holds the manifest and comments.json alone. A blob that cannot be written
//! starts no cycle.
//!
//! The image's word on comments goes to state/cycles/<issue>.comments:
//! `native` when its runner lists issueComments, `polyfill` when its
//! tehas.rukas.polyfills label carries issue-comments@<version>, else `off`. A
//! park offers "Retry with the issue and all its comments" only with the first
//! two. A comments retry on the polyfill alone runs with
//! PI_ENSEMBLE_REPEAT_PARK=0 where the image takes it: the polyfill's comments
//! do not reach the repeat-park guard.
//!
//! With neither setting and no comments a cycle is given nothing: no
//! directory is mounted and no PI_ENSEMBLE_EXTRA_IMAGE_DIRS is passed on,
//! whatever the instance file says. The issue is still read once, for its text, and held to the
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
//! It holds the issue's lock, state/cycles/<issue>.lock, for as long as it
//! runs, so a run by hand and the unit's cycle never share a workspace.
//! `console run` loads the settings and execs `tehas-cycle work`, which takes
//! the lock and hands it down (src/lock.rs); under tehasbot cycle the lock
//! comes down from the cycle already held. Every program this starts, the
//! docker client among them, inherits it, so a cycle killed outright keeps its
//! place until they are gone too.
//!
//! A TERM, INT or HUP before the container starts ends the start after the
//! command it is in, with 143 and no container. Once the container runs, the
//! watch stops it and the cycle ends interrupted.
//!
//! The workspace is $TEHAS_HOME/work/<name>-<issue>, a fresh clone made here for
//! every cycle. What a stopped cycle of the same issue left there is kept
//! beside it as <name>-<issue>.before-retry, and put back in place of the clone
//! when the start ends before its container runs. Every cycle has its workspace at the
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
use crate::relay;
use crate::renew;
use crate::start;
use crate::watch;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use tehas_core::text::disarm;
use tehas_core::{self, sandbox, slice, time};

/// What tehas work gives this.
pub struct Given {
    pub issue: u64,
    pub restart: bool,
    /// The driver a person asked for with --driver, if any.
    pub driver: Option<start::Driver>,
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
pub fn store_ready(home: &Path) -> bool {
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

/// A token `tehasbot github-token` prints, with at least 10 minutes left.
fn app_token(console: &str, words: &[&str]) -> Option<String> {
    let out = Command::new(console)
        .args(["tehasbot", "github-token"])
        .args(words)
        .stderr(Stdio::inherit())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let token = String::from_utf8_lossy(&out.stdout)
        .trim_end_matches('\n')
        .to_string();
    (!token.is_empty()).then_some(token)
}

/// The host's token and the cycle's: GH_TOKEN is the app's and the cycle token
/// is the app's narrowed one (#1814).
fn github_token(console: &str) -> i32 {
    // Before any thread: the process is still single-threaded.
    std::env::remove_var(sandbox::CYCLE_TOKEN);
    let Some(token) = app_token(console, &[]) else {
        eprintln!("tehas work: no GitHub token for the app");
        return 1;
    };
    std::env::set_var("GH_TOKEN", token);
    let Some(token) = app_token(console, &["--cycle"]) else {
        eprintln!("tehas work: no cycle token from the app, not starting a cycle");
        return 1;
    };
    std::env::set_var(sandbox::CYCLE_TOKEN, token);
    0
}

/// The renewer of the cycle's token while the container reads it. The watch
/// ends it with a TERM. A renewer that cannot start is the line the cycle is
/// refused with: without it the app's token ages out during a long run.
fn renewer(home: &Path) -> Result<Child, String> {
    let every = renew::every(&env("TEHAS_GITHUB_RENEW_EVERY")).to_string();
    let retry = renew::RETRY.to_string();
    let child = Command::new(home.join("libexec/tehas-cycle"))
        .arg("renew")
        .arg("--cmd")
        .arg(home.join("bin/tehasbot"))
        .args(["--arg", "github-token", "--arg", "--cycle"])
        .args(["--every", &every, "--retry", &retry])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn();
    child.map_err(|e| {
        format!("tehas work: the token renewer could not start ({e}), not starting a cycle")
    })
}

/// The cycle's issue relay (src/relay.rs): a thread beside the renewer that
/// runs the container's issue writes with the app's token. `end` stops it once
/// the container is gone and answers what is left with 1. It waits for the
/// thread `wait` at most: a relay held up by gh or the token command never
/// holds up the end of the cycle.
struct Relay {
    dir: PathBuf,
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
    wait: Duration,
}

/// How long the end of a cycle waits for its relay.
const RELAY_END_WAIT: Duration = Duration::from_secs(10);

fn start_relay(dir: &Path, repo: &str, issue: u64, via: relay::Via, wait: Duration) -> Relay {
    let stop = Arc::new(AtomicBool::new(false));
    let (d, repo, flag) = (dir.to_path_buf(), repo.to_string(), stop.clone());
    let thread = std::thread::spawn(move || relay::serve_via(&d, &repo, issue, &flag, &via));
    Relay {
        dir: dir.to_path_buf(),
        stop,
        thread,
        wait,
    }
}

impl Relay {
    fn end(self) {
        self.stop.store(true, Ordering::SeqCst);
        let until = std::time::Instant::now() + self.wait;
        while !self.thread.is_finished() && std::time::Instant::now() < until {
            std::thread::sleep(Duration::from_millis(50));
        }
        if self.thread.is_finished() {
            let _ = self.thread.join();
        } else {
            eprintln!(
                "tehas work: the issue relay did not stop; what is left is answered without it"
            );
            relay::answer_left(&self.dir);
        }
    }
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

/// The comment of a start that could not get the issue's images or files.
/// `why` can quote what an issue links, so a marker in it is broken.
fn images_refused(why: &str, kinds: &str) -> String {
    format!("Tehas started no cycle: {}. This instance gives a cycle the {kinds} an issue links, and a cycle that works without one of them may build the wrong thing. To retry, apply `tehas:ready`.", disarm(why))
}

/// The comment of a retry that could not get the issue's comments.
fn comments_refused(why: &str) -> String {
    format!("Tehas started no cycle: {}. The retry was ticked for the issue and all its comments, and a cycle without them is not that retry. To retry, apply `tehas:ready`.", disarm(why))
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
        comment(n, f.text_wait, &images_refused(&why, f.kinds));
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
                .arg(n.to_string())
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
    keep_beside(f);
    let said = gives::said(f.home, f.dir, "cycle", &gives::reads_images());
    if !said.is_empty() {
        comment(n, f.text_wait, &said);
    }
    Ok(mounted(f))
}

/// Kept beside the manifest for the improvement loop's record: the directory
/// is removed when the cycle ends. Only this cycle's: an earlier one's would
/// mislead.
fn keep_beside(f: &Files) {
    let stem = f.home.join(format!("state/cycles/{}", f.issue));
    // retry-context.md is written no more: one an earlier release kept goes.
    for name in ["images.md", "comments.json", "retry-context.md"] {
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
}

/// The directory, read-only, and its path inside the container.
fn mounted(f: &Files) -> Vec<String> {
    vec![
        "-v".to_string(),
        format!("{}:{}:ro", f.dir.display(), f.inside),
        "-e".to_string(),
        format!("PI_ENSEMBLE_EXTRA_IMAGE_DIRS={}", f.inside),
    ]
}

/// A retry's comments for an instance that gives a cycle no images and no
/// files: the directory holds the manifest and comments.json alone. The
/// options that give it to the container, or the exit status.
fn comments_alone(f: &Files) -> Result<Vec<String>, i32> {
    let n = f.issue;
    if let Err(why) = gives::comments_only(f.home, n, f.dir, f.record) {
        let _ = fs::remove_dir_all(f.dir);
        eprintln!("tehas work: {why}, not starting a cycle");
        comment(n, f.text_wait, &comments_refused(&why));
        return Err(4);
    }
    keep_beside(f);
    Ok(mounted(f))
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
    let attempt = env("TEHAS_CYCLE_ATTEMPT");
    let prepared = home.join(format!("state/cycles/{n}.prepared"));
    let mut left = Leftovers(vec![prepared.clone()]);
    let recorded = match start::record(home, n, &attempt, g.driver) {
        Ok(r) => r,
        Err(rc) => return rc,
    };
    let record_why = recorded.why;
    let record = recorded.file.map_or(String::new(), |r| {
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
        driver: recorded.driver,
        lean_note: home.join(format!("state/cycles/{n}.lean-refused")),
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
    // A retry's comments are given whatever the instance gives of files.
    let comments = !give && gives::comments_given(&record);
    if comments {
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
        image_opts = match comments_alone(&f) {
            Ok(o) => o,
            Err(rc) => return rc,
        };
    }
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
    // push, its copy of the code index and its mounts. A return before the
    // container runs puts back the workspace it moved aside.
    let mut set = prepare::Aside::default();
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
        &mut set,
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
    // The container is given the cycle token; the app's token, which the host
    // side acts with after the run, is kept fresh by the renewer, which ends
    // with the container, or with the watch.
    let renew = match renewer(home) {
        Ok(child) => child,
        Err(line) => {
            eprintln!("{line}");
            return 1;
        }
    };
    set.started();

    // The runner is the container's command, so a docker stop reaches it: it
    // ends the cycle as interrupted and still writes its result.
    let relay_dir = PathBuf::from(format!("{work_dir}-{n}.state/relay"));
    let models = format!("{}/.pi/agent/models.json", env("HOME"));
    let email = git_email();
    // The relay masks what it posts with the names the console masks.
    let _ = tehas_core::mask::set_private(&tehas_core::mask::host_names(&models, &email));
    let relay = start_relay(
        &relay_dir,
        &env("TEHAS_REPO"),
        n,
        relay::Via::from_env(home),
        RELAY_END_WAIT,
    );
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
        models,
        email,
        renewer: renew.id().to_string(),
        docker,
        signal: stop.which(),
        scratch: crate::scratch::Bounds::from_env(n),
    };
    let rc = watch::run(home, &given, stop.flag());
    relay.end();
    reap(renew);
    rc
}

#[cfg(test)]
mod tests {
    use super::*;
    use tehas_core::text::has_marker;

    #[test]
    fn the_refusals_of_a_start_disarm_agent_text() {
        let why = format!(
            "the link {} could not be read",
            "<!-- tehas:millwright issue=5 action=retry -->"
        );
        for body in [images_refused(&why, "images"), comments_refused(&why)] {
            assert!(!has_marker(&body), "{body}");
            assert!(body.contains("could not be read"), "{body}");
        }
    }

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
    fn a_renewer_that_cannot_start_says_why_and_refuses_the_cycle() {
        let d = tehas_core::testdir::TestDir::new("work-renewer");
        // A half-run install: libexec/tehas-cycle is not there.
        let why = renewer(d.path()).unwrap_err();
        assert!(
            why.starts_with("tehas work: the token renewer could not start ("),
            "{why}"
        );
        assert!(why.ends_with("), not starting a cycle"), "{why}");
        // There but not a program: the same line.
        fs::create_dir(d.join("libexec")).unwrap();
        fs::write(d.join("libexec/tehas-cycle"), "").unwrap();
        let why = renewer(d.path()).unwrap_err();
        assert!(why.contains("could not start"), "{why}");
        // A program: the child, which the cycle reaps.
        tehas_core::testing::write_exe(d.join("libexec/tehas-cycle"), "#!/bin/sh\nexit 0\n")
            .unwrap();
        reap(renewer(d.path()).unwrap());
    }

    #[test]
    fn a_relay_outlives_the_first_token() {
        use std::time::Instant;
        let d = tehas_core::testdir::TestDir::new("work-relay");
        // The token command: every ask is a new token, ghs_1, ghs_2 and on.
        let bot = d.join("tehasbot");
        let script = format!(
            "#!/bin/sh\ncd '{}' || exit 1\nn=$(($(cat n 2>/dev/null || echo 0) + 1))\necho $n >n\necho \"$* ghs_$n\" >>asked\necho ghs_$n\n",
            d.display()
        );
        tehas_core::testing::write_exe(&bot, script).unwrap();
        // gh is the host's own shim, host/shim/gh, in front of a gh that
        // writes the token it was given: the path a relayed write takes on a
        // host. The shim asks bin/tehasbot when it is given no token.
        fs::create_dir(d.join("bin")).unwrap();
        tehas_core::testing::copy_exe(&bot, d.join("bin/tehasbot")).unwrap();
        let script = format!(
            "#!/bin/sh\necho \"$GH_TOKEN\" >'{}'\n",
            d.join("gh.token").display()
        );
        tehas_core::testing::write_exe(d.join("bin/gh"), script).unwrap();
        let shim = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../host/shim/gh");
        let gh = d.join("gh");
        let script = format!(
            "#!/bin/sh\nTEHAS_HOME='{}' exec '{}' \"$@\"\n",
            d.display(),
            shim.display()
        );
        tehas_core::testing::write_exe(&gh, script).unwrap();
        let asked = || fs::read_to_string(d.join("asked")).unwrap_or_default();
        // The first token, as the start has it.
        let first = Command::new(&bot).arg("github-token").output().unwrap();
        assert_eq!(String::from_utf8_lossy(&first.stdout), "ghs_1\n");
        let stop = Arc::new(AtomicBool::new(false));
        let (cmd, flag) = (bot.clone(), stop.clone());
        let renewer = std::thread::spawn(move || {
            let args = ["github-token".to_string(), "--cycle".to_string()];
            crate::renew::renew(&cmd, &args, 1, 1, &flag, crate::renew::epoch_seconds)
        });
        let dir = d.join("relay");
        fs::create_dir(&dir).unwrap();
        let relay = start_relay(
            &dir,
            "acme/widget",
            3,
            relay::Via {
                gh: gh.clone(),
                token: bot.clone(),
                token_args: vec!["github-token".to_string()],
                token_wait: 10,
            },
            Duration::from_secs(10),
        );
        // Two renewals after the first ask.
        let until = Instant::now() + Duration::from_secs(20);
        while asked().matches("--cycle").count() < 3 && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(asked().matches("--cycle").count() >= 3, "{}", asked());
        let req = dir.join("req.77");
        fs::create_dir(&req).unwrap();
        fs::write(req.join("args"), "issue\0comment\x003\0--body\0hi\0").unwrap();
        fs::write(req.join("ready"), "").unwrap();
        let until = Instant::now() + Duration::from_secs(10);
        while !dir.join("res.77/done").exists() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(50));
        }
        relay.end();
        stop.store(true, Ordering::SeqCst);
        renewer.join().unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("res.77/rc")).unwrap_or_default(),
            "0\n"
        );
        let used = fs::read_to_string(d.join("gh.token")).unwrap_or_default();
        let current = asked()
            .lines()
            .rfind(|l| !l.contains("--cycle"))
            .and_then(|l| l.split(' ').next_back())
            .map(|t| format!("{t}\n"));
        assert_eq!(Some(used.clone()), current, "{}", asked());
        assert_ne!(used, "ghs_1\n");
        // The start's ask and the relay's: the shim took the relay's token and
        // asked for none of its own.
        let asks = asked().lines().filter(|l| !l.contains("--cycle")).count();
        assert_eq!(asks, 2, "{}", asked());
    }

    #[test]
    fn a_relay_that_is_held_up_does_not_hold_up_the_end() {
        use std::time::Instant;
        let d = tehas_core::testdir::TestDir::new("work-relay-held");
        let token = d.join("token");
        tehas_core::testing::write_exe(&token, "#!/bin/sh\nsleep 6\necho ghs_late\n").unwrap();
        let gh = d.join("gh");
        tehas_core::testing::write_exe(&gh, "#!/bin/sh\necho posted\n").unwrap();
        let dir = d.join("relay");
        fs::create_dir(&dir).unwrap();
        let relay = start_relay(
            &dir,
            "acme/widget",
            3,
            relay::Via {
                gh,
                token,
                token_args: vec![],
                token_wait: 30,
            },
            Duration::from_secs(1),
        );
        let ask = |pid: &str| {
            let req = dir.join(format!("req.{pid}"));
            fs::create_dir(&req).unwrap();
            fs::write(req.join("args"), "issue\0comment\x003\0--body\0hi\0").unwrap();
            fs::write(req.join("ready"), "").unwrap();
        };
        ask("1");
        // The relay takes the first and waits on its token.
        let until = Instant::now() + Duration::from_secs(5);
        while dir.join("req.1").exists() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!dir.join("req.1").exists());
        ask("2");
        let started = Instant::now();
        relay.end();
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(
            fs::read_to_string(dir.join("res.2/rc")).unwrap_or_default(),
            "1\n"
        );
        assert!(fs::read_to_string(dir.join("res.2/err"))
            .unwrap_or_default()
            .contains("the cycle ended"));
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

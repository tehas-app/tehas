//! The host's side of a planning turn, for start.rs: the keep directory,
//! the workspace around the clone, the runner's words, the container's time
//! and the end of the turn.
//!
//!   workspace   the directory of the workspaces, marked as tehas's, and
//!               <keep>. With a brief it is a first turn: <keep> is made anew.
//!   give        a fresh copy of the images when given and there is a manifest.
//!   place       the clone made ready: the commit noted or put back, the
//!               session carried in, the brief put in.
//!   first, later  the words after the runner's name. A later turn writes
//!               what the runner reads as its answer.
//!   run         starts docker and watches it, then ends the turn.
//!
//! Each failure says why, on standard error. 78 nothing may start, 2 a turn
//! that ran failed, 0 as src/usage/plan.txt says.
//!
//! <keep> is what the session carries from one turn to the next. The
//! dispatcher gives $TEHAS_WORK_DIR-plan-<issue>.keep, beside the turn's
//! workspace and out of the state directory, and writes a later turn's answer
//! there as answer.md. The first turn makes it anew; the caller removes it
//! when the session ends. It holds:
//!   brief.md        the approved text of the issue, the ACCEPTANCE CRITERIA
//!                   block of its pi-spec v1 criteria when it has them
//!                   (`criteria_directive`), then lib/issue-brief.md.
//!                   Every turn's workspace has it as .tehas/brief.md. The
//!                   first turn gives it to the planner whole as the context,
//!                   and its title, as `descriptor` reads it, as what to plan.
//!   commit          the commit the first turn's workspace was at. Every later
//!                   turn's workspace is at the same one, so the planner plans
//!                   against the code it began with.
//!   plan-sessions/  pi-rukas's record of the session, which it keeps in the
//!                   clone's git directory (.git/pi-plan-sessions) while
//!                   PI_ENSEMBLE_PLAN_SESSION_DIR is unset. It is copied here
//!                   when a turn ends and back before the next one starts.
//!                   Only plain files are carried, at most 1 MB each. Nothing
//!                   new is mounted and the container is given nothing new.
//!   exit            the runner's exit status of the last turn.
//!   draft.json      the title and spec of the last drafted plan (exit 12),
//!                   and "block": the `pi-spec v1` block pi-rukas placed at
//!                   the top of the spec, when it placed one.
//!   images/         the issue's images, with TEHAS_WORK_IMAGES=1, and its
//!                   text files, with TEHAS_WORK_FILES=1: fetched on the first
//!                   turn as tehas work fetches a cycle's. Each turn is given
//!                   a copy, $TEHAS_HOME/work/<name>-plan-<issue>.images,
//!                   read-only, with PI_ENSEMBLE_EXTRA_IMAGE_DIRS, and the
//!                   copy is removed with the workspace. With
//!                   TEHAS_WORK_DESCRIBE=1 it holds images.md too, written on
//!                   the first turn. A later turn with --comment adds what the
//!                   reply links, by the same rules and limits, numbered on
//!                   from the files there, source "answer:<id>", and the
//!                   answer sent to the planner names the new files. The
//!                   turn's container is then also told PI_ENSEMBLE_PLAN_ISSUE,
//!                   the issue's number. The comment that says what a session
//!                   was given says whether it was shown them, which is what
//!                   the sandbox image's own label answers.
//!
//! What a turn runs, by the runner's own flags (docs/headless.md in pi-rukas):
//!   first turn                    plan "<what to plan>" --start --context <brief> [--issue <N>]
//!   after a plan was shown (12),
//!     "File as a new issue"       plan --resume <id> --confirm
//!     "Update this issue"         nothing: the turn runs no container
//!   any other answer              plan --resume <id> --answer <file>
//! The runner writes one JSON document to stdout and NDJSON progress to stderr.
//!
//! On exit 0 the --out file holds, as `outcome` writes it,
//!   {"session": "<id>", "text": "<what the turn says>",
//!    "question": null | {"question": "...", "options": [{"label", "description"}]},
//!    "filed": ["<issue address>", ...], "update": {"title", "body"}}
//! for each runner exit as follows:
//!   11 questions      the planner's questions; one is asked as it is, two or
//!                     more as one numbered text. Answered by a reply.
//!   12 shown          the drafted plan, title and spec, after the turn's text;
//!                     its open points with the planner's proposals; the option
//!                     "Update this issue", "File as a new issue" with --kind
//!                     refine; without it, "clear": true when no point is open.
//!   10 duplicate-risk the turn's text; answered by a reply.
//!    0 filed          the issue the confirm filed, under "filed".
//! "filed" holds the issues this session filed; with none there is no such key.
//! The dispatcher posts them on the issue and names them to the next turn.
//! "update" is the drafted plan an answer chose to make the issue's text; the
//! dispatcher writes it there and ends the session with "text", which holds
//! the issue's text before it.
//! On exit 2 it holds {"session": "<id>"} when the session is known, and
//! {"session": "<id>", "lost": true} when the runner says the session is gone
//! (exit 41): the session cannot go on. Alongside them it holds "why", what
//! the turn says went wrong, cut to 500 characters, so the dispatcher can put
//! it on the issue. A turn that failed with no session known writes a document
//! with "why" and no "session", which is no session for the caller.

use super::outcome;
use serde_json::{json, Map, Value};
use signal_hook::consts::{SIGINT, SIGTERM};
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tehas_core::plan::{criteria_directive, WHY_MAX};

/// `find ! -size +1024k`: a carried file is at most this long.
const CARRIED: u64 = tehas_core::file::READ_BOUND;
const POLL: Duration = Duration::from_millis(250);
/// Where pi-rukas keeps a session, inside the clone.
const SESSIONS: &str = ".git/pi-plan-sessions";
const STOP_ON_SIGNAL: u32 = 10;
const STOP_ON_LIMIT: u32 = 20;
const NOTHING_STARTS: u8 = 78;
const FAILED: u8 = 2;

fn say(line: &str) {
    let _ = writeln!(io::stdout(), "{line}");
}

fn warn(line: &str) {
    let _ = writeln!(io::stderr(), "{line}");
}

/// Says why on standard error and gives the status that starts nothing.
fn refuse(why: &str) -> u8 {
    warn(&format!("tehas plan: {why}"));
    NOTHING_STARTS
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `rm -rf`: what is not there is gone, and a symlink goes, not what it names.
fn remove(path: &str) -> io::Result<()> {
    let gone = match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(e) => Err(e),
    };
    match gone {
        Err(e) if !matches!(e.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => Err(e),
        _ => Ok(()),
    }
}

/// `cp`: the source's mode under the umask, which fs::copy would not apply.
fn copy(from: &Path, to: &Path) -> io::Result<()> {
    let mut source = File::open(from)?;
    let mode = source.metadata()?.permissions().mode();
    let mut target = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .open(to)?;
    io::copy(&mut source, &mut target).map(|_| ())
}

/// `$(cat file)`: no NUL and no line end after the text.
fn captured(mut bytes: Vec<u8>) -> Vec<u8> {
    bytes.retain(|b| *b != 0);
    while bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    bytes
}

/// A directory only tehas made may be used, marked before anything is made in
/// it: a turn that stops early must not leave one every later cycle refuses.
fn mark(works: &str) -> io::Result<()> {
    fs::create_dir_all(works)?;
    File::create(format!("{works}/.tehas-workspaces")).map(|_| ())
}

pub(super) fn workspace(works: &str, keep: &str, brief: Option<(&str, &str)>) -> u8 {
    if Path::new(works).exists() && !Path::new(works).join(".tehas-workspaces").is_file() {
        return refuse(&format!(
            "{works} exists and was not made by tehas, not touching it"
        ));
    }
    if mark(works).is_err() {
        return refuse(&format!("could not make {works}, not starting a turn"));
    }
    if brief.is_some() {
        let _ = remove(keep);
    }
    if fs::create_dir_all(keep).is_err() {
        return refuse(&format!("could not make {keep}, not starting a turn"));
    }
    if let Some((brief, rules)) = brief {
        if keep_brief(keep, brief, rules).is_err() {
            return refuse(&format!(
                "could not keep the brief in {keep}, not starting a turn"
            ));
        }
    }
    0
}

/// Every session is told what a good issue is, after the text it plans. The
/// criteria of the request's `pi-spec v1` block go between the two, as the
/// block pi-rukas reads criteria from: it reads none inside a fence.
fn keep_brief(keep: &str, brief: &str, rules: &str) -> io::Result<()> {
    let mut text = fs::read(brief)?;
    if let Some(criteria) = criteria_directive(&String::from_utf8_lossy(&text)) {
        text.extend_from_slice(format!("\n\n{criteria}").as_bytes());
    }
    text.extend_from_slice(b"\n\n---\n\n");
    text.extend(fs::read(rules)?);
    fs::write(format!("{keep}/brief.md"), text)
}

pub(super) fn give(works: &str, images: &str, to: &str, give: bool) -> u8 {
    if mark(works).is_err() || remove(to).is_err() {
        return refuse(&format!("could not make {works}, not starting a turn"));
    }
    // A session that began without them goes on without them.
    if give && Path::new(images).join("manifest.json").is_file() {
        let copied = Command::new("cp").args(["-r", images, to]).status();
        if !copied.is_ok_and(|s| s.success()) {
            return refuse(&format!(
                "could not copy the issue's images to {to}, not starting a turn"
            ));
        }
    }
    0
}

/// The plain files of one session directory into another, under the same
/// relative paths, with what <to> held before dropped. A symlink or a file
/// over 1 MB is left out. The copy is made beside <to> and moved into place,
/// so a failure leaves what was there.
pub fn carry(from: &str, to: &str) -> io::Result<()> {
    let new = format!("{to}.new");
    remove(&new)?;
    let copied = if Path::new(from).is_dir() {
        fs::create_dir_all(&new).and_then(|()| carry_files(Path::new(from), Path::new(&new)))
    } else {
        Ok(())
    };
    if let Err(e) = copied.and_then(|()| remove(to)) {
        let _ = remove(&new);
        return Err(e);
    }
    if Path::new(&new).is_dir() {
        fs::rename(&new, to)?;
    }
    Ok(())
}

fn carry_files(from: &Path, to: &Path) -> io::Result<()> {
    // As find does, a directory that cannot be read is passed over.
    let Ok(entries) = fs::read_dir(from) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        let target = to.join(entry.file_name());
        if meta.is_dir() {
            carry_files(&entry.path(), &target)?;
        } else if meta.is_file() && meta.len() <= CARRIED {
            fs::create_dir_all(to)?;
            copy(&entry.path(), &target)?;
        }
    }
    Ok(())
}

pub(super) fn place(tree: &str, keep: &str, branch: &str, first: bool) -> u8 {
    if first {
        if note_commit(tree, keep).is_err() {
            return refuse(&format!(
                "could not note the commit of {tree}, not starting a turn"
            ));
        }
    } else {
        pin(tree, keep, branch);
        // Without the record the runner says the session is lost, and the
        // session ends: that is what a missing record means.
        if carry(
            &format!("{keep}/plan-sessions"),
            &format!("{tree}/{SESSIONS}"),
        )
        .is_err()
        {
            warn(&format!(
                "tehas plan: could not carry the session into {tree}"
            ));
        }
    }
    // The planner plans from the text that was approved, so it is handed over
    // as a file and not fetched again from the tracker, where it may have
    // changed.
    let brief = format!("{keep}/brief.md");
    if !Path::new(&brief).is_file() {
        say("tehas plan: no brief on record for this session, the turn has none");
    } else if put_brief(tree, &brief).is_err() {
        return refuse(&format!("could not put the brief into {tree}"));
    }
    0
}

fn note_commit(tree: &str, keep: &str) -> io::Result<()> {
    let noted = File::create(format!("{keep}/commit"))?;
    let status = Command::new("git")
        .args(["-C", tree, "rev-parse", "HEAD"])
        .stdout(noted)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(ErrorKind::Other.into())
    }
}

/// Moving on without the commit is safe: the planner then plans against the
/// tip, and a confirm files the plan that was shown, whatever the code.
fn pin(tree: &str, keep: &str, branch: &str) {
    let pin = fs::read_to_string(format!("{keep}/commit")).unwrap_or_default();
    let pin = pin.trim_end_matches('\n');
    if pin.is_empty() || !pin.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        say(&format!(
            "tehas plan: no commit on record for this session, the turn starts at the tip of {branch}"
        ));
        return;
    }
    let moved = Command::new("git")
        .args(["-C", tree, "checkout", "-q", "-B", branch, pin])
        .stderr(Stdio::null())
        .status();
    if !moved.is_ok_and(|s| s.success()) {
        say(&format!(
            "tehas plan: the session's commit {pin} is not in the checkout, the turn starts at the tip of {branch}"
        ));
    }
}

fn put_brief(tree: &str, brief: &str) -> io::Result<()> {
    fs::create_dir_all(format!("{tree}/.tehas"))?;
    fs::create_dir_all(format!("{tree}/.git/info"))?;
    copy(
        Path::new(brief),
        Path::new(&format!("{tree}/.tehas/brief.md")),
    )?;
    let mut exclude = OpenOptions::new()
        .append(true)
        .create(true)
        .open(format!("{tree}/.git/info/exclude"))?;
    exclude.write_all(b"/.tehas/brief.md\n/.tehas/answer.md\n")
}

fn owned(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| w.to_string()).collect()
}

pub(super) fn first(keep: &str, what: &str, issue: &str) -> Vec<String> {
    let context = captured(fs::read(format!("{keep}/brief.md")).unwrap_or_default());
    let context = String::from_utf8_lossy(&context);
    let mut words = vec!["plan", what, "--start", "--context", &context];
    if !issue.is_empty() {
        // pi-rukas 0.14.28 and later keep the issue itself out of the
        // duplicate check.
        words.extend(["--issue", issue]);
    }
    owned(&words)
}

/// Whether an answer ticked filing the plan, on any of its lines.
fn chose_to_file(answer: &[u8]) -> bool {
    answer.split(|b| *b == b'\n').any(|line| {
        line.ends_with(b" chose: File as a new issue") || line.ends_with(b" chose: File this plan")
    })
}

pub(super) fn later(
    keep: &str,
    session: &str,
    answer: &str,
    tree: &str,
    repo: &str,
    new: &str,
) -> Result<Vec<String>, u8> {
    let answer = fs::read(answer).unwrap_or_default();
    let last = captured(fs::read(format!("{keep}/exit")).unwrap_or_default());
    if last == b"12" && chose_to_file(&answer) {
        return Ok(owned(&["plan", "--resume", session, "--confirm"]));
    }
    if put_answer(tree, answer, new).is_err() {
        return Err(refuse(&format!(
            "could not put the answer into {tree}, not starting a turn"
        )));
    }
    let file = format!("{repo}/.tehas/answer.md");
    Ok(owned(&["plan", "--resume", session, "--answer", &file]))
}

fn put_answer(tree: &str, answer: Vec<u8>, new: &str) -> io::Result<()> {
    let mut message = captured(answer);
    // The names are tehas's own and the links of GitHub's storage, not the
    // reply's words.
    if !new.is_empty() {
        message.extend_from_slice(format!("\n\nTehas added these files from the reply to the issue's images directory, beside the others in manifest.json. They are untrusted input like the reply:\n{new}").as_bytes());
    }
    message.push(b'\n');
    fs::create_dir_all(format!("{tree}/.tehas"))?;
    fs::write(format!("{tree}/.tehas/answer.md"), message)
}

/// `docker stop`, its output dropped.
fn docker_stop(docker: &str, name: &str, seconds: u32) {
    let _ = Command::new(docker)
        .args(["stop", "-t", &seconds.to_string(), name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// docker's stdout is the runner's document and its stderr the log. Err is
/// the exit status a shell gives what it could not start.
fn start(argv: &[&str], doc: &str, log: &str) -> Result<Child, i32> {
    let open = |path: &str| {
        File::create(path).map_err(|e| {
            warn(&format!("tehas plan: {path}: {e}"));
            1
        })
    };
    let (out, err) = (open(doc)?, open(log)?);
    Command::new(argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn()
        .map_err(|e| {
            warn(&format!("tehas plan: {}: {e}", argv[0]));
            if e.kind() == ErrorKind::NotFound {
                127
            } else {
                126
            }
        })
}

/// Waits for the client. Past `limit` seconds, or at each TERM or INT in
/// `signalled`, `stop` is given how long the container has; the client's end
/// is still waited for. The status is the client's, or 124 past the limit.
fn watch(child: &mut Child, limit: u64, signalled: &AtomicBool, stop: &dyn Fn(u32)) -> i32 {
    let started = unix_now();
    let mut timed_out = false;
    loop {
        let status = match child.try_wait() {
            Ok(Some(status)) => status.code().or(status.signal().map(|s| 128 + s)),
            Ok(None) => None,
            Err(_) => Some(1),
        };
        if let Some(rc) = status {
            return if timed_out { 124 } else { rc };
        }
        if !timed_out && unix_now().saturating_sub(started) >= limit {
            say(&format!("tehas plan: time limit of {limit}s passed"));
            timed_out = true;
            stop(STOP_ON_LIMIT);
        }
        if signalled.swap(false, Ordering::Relaxed) {
            stop(STOP_ON_SIGNAL);
        }
        std::thread::sleep(POLL);
    }
}

/// What the turn needs when its container has ended.
pub(super) struct Turn<'a> {
    pub issue: &'a str,
    pub session: &'a str,
    pub refine: bool,
    pub out: &'a str,
    pub keep: &'a str,
    pub tree: &'a str,
    pub state: &'a str,
    pub images: &'a str,
    pub log: &'a str,
}

pub(super) fn run(turn: &Turn, limit: u64, name: &str, argv: &[&str]) -> u8 {
    let signalled = Arc::new(AtomicBool::new(false));
    for signal in [SIGTERM, SIGINT] {
        if signal_hook::flag::register(signal, Arc::clone(&signalled)).is_err() {
            warn("tehas plan: could not listen for signals, not starting a turn");
            return FAILED;
        }
    }
    let docker = argv[0];
    // A container under this name outlived the client that started it. The
    // dispatcher runs one turn at a time, so it is no turn's: left, it idles
    // for ever and Docker would refuse the name.
    let _ = Command::new(docker)
        .args(["rm", "-f", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let doc = format!(
        "{}.json",
        turn.log.strip_suffix(".jsonl").unwrap_or(turn.log)
    );
    let child = start(argv, &doc, turn.log);
    let started = unix_now();
    say(&format!(
        "tehas plan: issue {} turn started, log {}",
        turn.issue, turn.log
    ));
    let rc = match child {
        Ok(mut child) => watch(&mut child, limit, &signalled, &|seconds| {
            docker_stop(docker, name, seconds)
        }),
        Err(rc) => rc,
    };
    finish(turn, rc, &doc, started)
}

/// The first object of the runner's output: what `jq -c objects | head -n 1`
/// gave. Output after it, read or not, says nothing.
fn first_object(doc: &[u8]) -> Value {
    let text = String::from_utf8_lossy(doc);
    serde_json::Deserializer::from_str(&text)
        .into_iter::<Value>()
        .map_while(Result::ok)
        .find(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

fn session_ok(id: &str) -> bool {
    (1..=128).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// What the runner said went wrong. An `error` or `failure` that is no object
/// says nothing, as jq could not index it.
fn runner_why(d: &Value) -> String {
    let parts = [
        ("error", "code"),
        ("error", "message"),
        ("failure", "reason"),
        ("failure", "detail"),
    ];
    let holder = |key: &str| d.get(key).filter(|v| !v.is_null());
    if ["error", "failure"]
        .iter()
        .any(|key| holder(key).is_some_and(|v| !v.is_object()))
    {
        return String::new();
    }
    parts
        .iter()
        .filter_map(|(outer, inner)| holder(outer)?.get(inner)?.as_str())
        .collect::<Vec<_>>()
        .join(": ")
}

/// The turn failed. The session, when it is known, and what went wrong are
/// all that is left for the caller; `lost` says the session is gone, so the
/// caller ends it. The reason goes in the output too, so the dispatcher can
/// put it on the issue and a person need not open the log. With no session
/// known the document holds only the reason, which is no session for the
/// caller.
fn give_up(turn: &Turn, sid: &str, what: &str, lost: bool) -> u8 {
    say(&format!(
        "tehas plan: issue={} {what}, see {}",
        turn.issue, turn.log
    ));
    // serde builds the document: the shape three readers parse is one the
    // type system knows, and "nothing to say" is an empty map, not a byte
    // count of a buffer.
    let mut doc = Map::new();
    if !sid.is_empty() {
        doc.insert("session".to_string(), json!(sid));
        if lost {
            doc.insert("lost".to_string(), json!(true));
        }
    }
    let why = tehas_core::jq::first(what, WHY_MAX);
    if !why.is_empty() {
        doc.insert("why".to_string(), json!(why));
    }
    if doc.is_empty() {
        return FAILED;
    }
    let doc = match serde_json::to_string_pretty(&doc) {
        Ok(doc) => format!("{doc}\n"),
        Err(e) => {
            warn(&format!("tehas plan: could not write {}: {e}", turn.out));
            return FAILED;
        }
    };
    if let Err(e) = write_out(turn.out, &doc) {
        warn(&format!("tehas plan: could not write {}: {e}", turn.out));
    }
    FAILED
}

fn write_out(out: &str, doc: &str) -> io::Result<()> {
    let new = format!("{out}.new");
    fs::write(&new, doc)?;
    fs::rename(&new, out)
}

fn finish(turn: &Turn, rc: i32, doc: &str, started: u64) -> u8 {
    // The session, before the workspace goes: it is all a later turn has of
    // this one. A turn whose runner kept none replaces what an earlier one
    // kept.
    let (keep, tree) = (turn.keep, turn.tree);
    if carry(
        &format!("{tree}/{SESSIONS}"),
        &format!("{keep}/plan-sessions"),
    )
    .is_err()
    {
        warn(&format!("tehas plan: could not keep the session of {tree}"));
    }
    if fs::write(format!("{keep}/exit"), format!("{rc}\n")).is_err() {
        warn(&format!(
            "tehas plan: could not keep the exit status in {keep}"
        ));
    }
    let removed = [tree, turn.state, turn.images].map(remove);
    if removed.iter().any(Result::is_err) {
        warn(&format!(
            "tehas plan: could not remove the workspace {tree}"
        ));
    }

    let answer = first_object(&fs::read(doc).unwrap_or_default());
    let sid = answer["session"]["id"]
        .as_str()
        .filter(|id| session_ok(id))
        .unwrap_or(turn.session);
    match rc {
        0 | 10 | 11 | 12 => {}
        41 => {
            let what = format!(
                "the session {} is gone: the runner has no record of it",
                turn.session
            );
            return give_up(turn, sid, &what, true);
        }
        124 => return give_up(turn, sid, "the turn ran out of time", false),
        _ => {
            let why = runner_why(&answer);
            let why = if why.is_empty() {
                why
            } else {
                format!(" ({why})")
            };
            let what = format!("the runner exited {rc}{why}");
            return give_up(turn, sid, &what, false);
        }
    }
    if sid.is_empty() {
        return give_up(turn, sid, "the session id is unknown", false);
    }
    let said = match outcome(
        rc.into(),
        sid,
        Path::new(keep),
        &answer.to_string(),
        turn.refine,
    ) {
        Ok(said) => said,
        Err(why) => {
            let _ = fs::remove_file(format!("{}.new", turn.out));
            return give_up(turn, sid, &why, false);
        }
    };
    if write_out(turn.out, &format!("{said}\n")).is_err() {
        let _ = fs::remove_file(format!("{}.new", turn.out));
        return give_up(turn, sid, &format!("could not write {}", turn.out), false);
    }
    let asked = serde_json::from_str::<Value>(&said).is_ok_and(|v| !v["question"].is_null());
    say(&format!(
        "tehas plan: issue={} session={sid} runner_exit={rc} asked={} seconds={}",
        turn.issue,
        if asked { "yes" } else { "no" },
        unix_now().saturating_sub(started)
    ));
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Scratch(tehas_core::testdir::TestDir);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            Scratch(tehas_core::testdir::TestDir::new(&format!(
                "plan-turn-{name}"
            )))
        }

        fn at(&self, path: &str) -> String {
            self.0.join(path).to_str().unwrap().to_string()
        }

        fn write(&self, path: &str, text: &str) {
            let path = self.0.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }

        fn read(&self, path: &str) -> String {
            fs::read_to_string(self.0.join(path)).unwrap()
        }

        fn has(&self, path: &str) -> bool {
            fs::symlink_metadata(self.0.join(path)).is_ok()
        }
    }

    fn git(dir: &str, args: &[&str]) {
        let ok = Command::new("git")
            .args([
                "-C",
                dir,
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
            ])
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(ok.success(), "git {args:?}");
    }

    #[test]
    fn carry_takes_plain_files_of_a_megabyte_at_most_under_their_paths() {
        let s = Scratch::new("carry");
        s.write("from/a/session.json", "one");
        s.write("from/a/b/deep.md", "two");
        s.write("from/whole.bin", &"x".repeat(1024 * 1024));
        s.write("from/big.bin", &"x".repeat(1024 * 1024 + 1));
        std::os::unix::fs::symlink("/etc/passwd", s.0.join("from/link")).unwrap();
        fs::create_dir(s.0.join("from/empty")).unwrap();
        s.write("to/old", "dropped");
        carry(&s.at("from"), &s.at("to")).unwrap();
        assert_eq!(s.read("to/a/session.json"), "one");
        assert_eq!(s.read("to/a/b/deep.md"), "two");
        assert!(s.has("to/whole.bin"));
        assert!(!s.has("to/big.bin"));
        assert!(!s.has("to/link"));
        assert!(!s.has("to/empty"));
        assert!(!s.has("to/old"));
        assert!(!s.has("to.new"));
    }

    #[test]
    fn carry_from_nothing_leaves_nothing() {
        let s = Scratch::new("carry-none");
        s.write("to/old", "dropped");
        carry(&s.at("from"), &s.at("to")).unwrap();
        assert!(!s.has("to"));
        // A directory with nothing to carry is still the session's.
        fs::create_dir(s.0.join("from")).unwrap();
        carry(&s.at("from"), &s.at("to")).unwrap();
        assert!(s.has("to"));
    }

    #[test]
    fn carry_keeps_what_was_there_on_failure() {
        let s = Scratch::new("carry-fail");
        s.write("from/locked", "secret");
        s.write("to/old", "kept");
        s.write("to.new/stale", "gone");
        let locked = s.0.join("from/locked");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        // A root that reads anything cannot fail this way.
        if File::open(&locked).is_ok() {
            return;
        }
        assert!(carry(&s.at("from"), &s.at("to")).is_err());
        assert_eq!(s.read("to/old"), "kept");
        assert!(!s.has("to.new"));
    }

    #[test]
    fn remove_takes_a_link_and_not_what_it_names_and_nothing_is_no_failure() {
        let s = Scratch::new("remove");
        s.write("dir/kept", "here");
        std::os::unix::fs::symlink(s.0.join("dir"), s.0.join("link")).unwrap();
        remove(&s.at("link")).unwrap();
        assert_eq!(s.read("dir/kept"), "here");
        s.write("file", "x");
        remove(&s.at("file")).unwrap();
        assert!(!s.has("file"));
        remove(&s.at("nothing")).unwrap();
        remove(&s.at("dir/kept/below")).unwrap();
        remove(&s.at("dir")).unwrap();
        assert!(!s.has("dir"));
    }

    #[test]
    fn a_workspace_is_refused_in_a_directory_tehas_did_not_make() {
        let s = Scratch::new("works");
        fs::create_dir(s.0.join("works")).unwrap();
        assert_eq!(workspace(&s.at("works"), &s.at("keep"), None), 78);
        assert!(!s.has("keep"));
    }

    #[test]
    fn a_first_turn_makes_the_keep_anew_with_the_brief_and_a_later_one_keeps_it() {
        let s = Scratch::new("keep");
        s.write("keep/left-over", "old");
        s.write("brief", "# Fast\n");
        s.write("rules", "rules\n");
        let (works, keep) = (s.at("works"), s.at("keep"));
        let briefs = Some((s.at("brief"), s.at("rules")));
        let briefs = briefs.as_ref().map(|(a, b)| (a.as_str(), b.as_str()));
        assert_eq!(workspace(&works, &keep, None), 0);
        assert!(s.has("keep/left-over"));
        assert!(s.has("works/.tehas-workspaces"));
        assert_eq!(workspace(&works, &keep, briefs), 0);
        assert!(!s.has("keep/left-over"));
        assert_eq!(s.read("keep/brief.md"), "# Fast\n\n\n---\n\nrules\n");
        // A request whose criteria are in its spec block has them said again
        // where pi-rukas reads criteria, before the rules.
        let spec =
            "```pi-spec v1\n{\"criteria\": [{\"id\": \"AC1\", \"text\": \"It works.\"}]}\n```\n";
        s.write("brief", &format!("# Fast\n\n{spec}"));
        assert_eq!(workspace(&works, &keep, briefs), 0);
        assert_eq!(
            s.read("keep/brief.md"),
            format!("# Fast\n\n{spec}\n\nACCEPTANCE CRITERIA:\n- AC1: It works.\n\n---\n\nrules\n")
        );
    }

    #[test]
    fn images_are_given_fresh_only_when_on_and_there_is_a_manifest() {
        let s = Scratch::new("give");
        s.write("given/stale", "old");
        s.write("images/image-1.png", "png");
        let (works, images, to) = (s.at("works"), s.at("images"), s.at("given"));
        assert_eq!(give(&works, &images, &to, true), 0);
        assert!(!s.has("given"));
        s.write("images/manifest.json", "{}");
        assert_eq!(give(&works, &images, &to, false), 0);
        assert!(!s.has("given"));
        assert_eq!(give(&works, &images, &to, true), 0);
        assert_eq!(s.read("given/image-1.png"), "png");
    }

    #[test]
    fn a_first_turn_is_placed_by_noting_the_commit_and_a_later_one_goes_back_to_it() {
        let s = Scratch::new("place");
        let (tree, keep) = (s.at("tree"), s.at("keep"));
        fs::create_dir_all(&tree).unwrap();
        fs::create_dir_all(&keep).unwrap();
        git(&tree, &["init", "-q", "-b", "main"]);
        git(&tree, &["commit", "-q", "--allow-empty", "-m", "first"]);
        s.write("keep/brief.md", "# Fast\n");
        assert_eq!(place(&tree, &keep, "main", true), 0);
        assert_eq!(s.read("tree/.tehas/brief.md"), "# Fast\n");
        assert!(s
            .read("tree/.git/info/exclude")
            .ends_with("/.tehas/brief.md\n/.tehas/answer.md\n"));
        let noted = s.read("keep/commit");
        git(&tree, &["commit", "-q", "--allow-empty", "-m", "second"]);
        s.write("keep/plan-sessions/abc/session.json", "{}");
        assert_eq!(place(&tree, &keep, "main", false), 0);
        assert_eq!(s.read(&format!("tree/{SESSIONS}/abc/session.json")), "{}");
        let head = Command::new("git")
            .args(["-C", &tree, "rev-parse", "HEAD"])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&head.stdout), noted);
        // No clone, no commit, nothing starts.
        assert_eq!(place(&s.at("none"), &keep, "main", true), 78);
    }

    #[test]
    fn a_first_turn_starts_with_the_brief_as_context_and_names_its_issue_only_when_asked() {
        let s = Scratch::new("first");
        fs::create_dir_all(s.0.join("keep")).unwrap();
        s.write("keep/brief.md", "# T\n\nBody.\n\n");
        let keep = s.at("keep");
        let context = "# T\n\nBody.";
        assert_eq!(
            first(&keep, "T (issue #7)", ""),
            owned(&["plan", "T (issue #7)", "--start", "--context", context])
        );
        assert_eq!(
            first(&keep, "T (issue #7)", "7"),
            owned(&[
                "plan",
                "T (issue #7)",
                "--start",
                "--context",
                context,
                "--issue",
                "7"
            ])
        );
    }

    #[test]
    fn a_confirm_is_only_a_tick_after_a_drafted_plan() {
        assert!(chose_to_file(b"alice chose: File as a new issue\n"));
        assert!(chose_to_file(
            b"Already filed: x\n\nalice chose: File this plan\n"
        ));
        assert!(!chose_to_file(b"alice chose: File this plan, but later\n"));
        assert!(!chose_to_file(b"alice chose: File this plan\r\n"));
        assert!(!chose_to_file(b"alice chose: Update this issue\n"));

        let s = Scratch::new("later");
        s.write("answer", "alice chose: File this plan\n");
        fs::create_dir_all(s.0.join("keep")).unwrap();
        let (keep, answer, tree) = (s.at("keep"), s.at("answer"), s.at("tree"));
        s.write("keep/exit", "12\n");
        assert_eq!(
            later(&keep, "abc", &answer, &tree, "/w", ""),
            Ok(owned(&["plan", "--resume", "abc", "--confirm"]))
        );
        assert!(!s.has("tree/.tehas/answer.md"));
        s.write("keep/exit", "11\n");
        assert_eq!(
            later(&keep, "abc", &answer, &tree, "/w", ""),
            Ok(owned(&[
                "plan",
                "--resume",
                "abc",
                "--answer",
                "/w/.tehas/answer.md"
            ]))
        );
        assert_eq!(
            s.read("tree/.tehas/answer.md"),
            "alice chose: File this plan\n"
        );
    }

    #[test]
    fn an_answer_ends_in_one_line_end_and_names_the_new_files() {
        let s = Scratch::new("answer");
        put_answer(&s.at("tree"), b"Adopt all\n\n\n".to_vec(), "").unwrap();
        assert_eq!(s.read("tree/.tehas/answer.md"), "Adopt all\n");
        put_answer(&s.at("tree"), b"See\n".to_vec(), "- image-2.jpg").unwrap();
        let said = s.read("tree/.tehas/answer.md");
        assert!(said.starts_with("See\n\nTehas added these files"));
        assert!(said.ends_with("like the reply:\n- image-2.jpg\n"));
    }

    #[test]
    fn an_answer_that_cannot_be_written_starts_nothing() {
        let s = Scratch::new("answer-fail");
        s.write("answer", "yes\n");
        s.write("tree", "a file, no directory");
        let (keep, answer, tree) = (s.at("keep"), s.at("answer"), s.at("tree"));
        assert_eq!(later(&keep, "abc", &answer, &tree, "/w", ""), Err(78));
    }

    #[test]
    fn the_first_object_of_the_output_is_the_answer() {
        assert_eq!(first_object(b"[1] 2 {\"a\":1} {\"b\":2}"), json!({"a": 1}));
        assert_eq!(first_object(b"{\"a\":1} not json"), json!({"a": 1}));
        assert_eq!(first_object(b"not json {\"a\":1}"), json!({}));
        assert_eq!(first_object(b""), json!({}));
        assert_eq!(first_object(b"[1]"), json!({}));
    }

    #[test]
    fn a_session_id_is_letters_digits_and_dashes_up_to_128() {
        assert!(session_ok("abc-123"));
        assert!(session_ok(&"a".repeat(128)));
        assert!(!session_ok(&"a".repeat(129)));
        assert!(!session_ok(""));
        assert!(!session_ok("abc\n"));
        assert!(!session_ok("a b"));
    }

    #[test]
    fn why_joins_what_the_runner_said_and_an_unindexable_field_says_nothing() {
        assert_eq!(
            runner_why(&json!({"error": {"code": "busy", "message": "held"}})),
            "busy: held"
        );
        assert_eq!(
            runner_why(&json!({"error": null, "failure": {"reason": "r", "detail": 3}})),
            "r"
        );
        assert_eq!(runner_why(&json!({})), "");
        assert_eq!(
            runner_why(&json!({"error": "text", "failure": {"reason": "r"}})),
            ""
        );
    }

    fn at_end(paths: &[String; 6]) -> Turn<'_> {
        Turn {
            issue: "7",
            session: "abc-123",
            refine: false,
            out: &paths[0],
            keep: &paths[1],
            tree: &paths[2],
            state: &paths[3],
            images: &paths[4],
            log: &paths[5],
        }
    }

    fn paths(s: &Scratch) -> [String; 6] {
        [
            "out.json",
            "keep",
            "tree",
            "tree.state",
            "tree.images",
            "log.jsonl",
        ]
        .map(|p| s.at(p))
    }

    fn ended(name: &str, rc: i32, doc: &str, session: &str) -> (Scratch, u8) {
        let s = Scratch::new(name);
        s.write("keep/draft", "");
        s.write(&format!("tree/{SESSIONS}/abc-123/session.json"), "{}");
        s.write("tree.state/cache", "");
        s.write("tree.images/manifest.json", "{}");
        s.write("doc.json", doc);
        let p = paths(&s);
        let mut turn = at_end(&p);
        turn.session = session;
        let code = finish(&turn, rc, &s.at("doc.json"), unix_now());
        (s, code)
    }

    #[test]
    fn a_turn_that_asked_writes_its_outcome_and_clears_the_workspace() {
        let doc = r#"{"session":{"id":"abc-123"},"text":"Q.","questions":[{"question":"Which?"}]}"#;
        let (s, code) = ended("asked", 11, doc, "");
        assert_eq!(code, 0);
        assert_eq!(
            s.read("out.json"),
            "{\"session\":\"abc-123\",\"text\":\"Q.\",\"question\":{\"options\":[],\"question\":\"Which?\"}}\n"
        );
        assert_eq!(s.read("keep/exit"), "11\n");
        assert_eq!(s.read("keep/plan-sessions/abc-123/session.json"), "{}");
        assert!(!s.has("tree"));
        assert!(!s.has("tree.state"));
        assert!(!s.has("tree.images"));
        assert!(!s.has("out.json.new"));
    }

    #[test]
    fn a_failed_turn_gives_up_keeping_the_session_as_jq_wrote_it() {
        let doc = r#"{"exitCode":40,"error":{"code":"draft-invalid","message":"the Acceptance criteria section is the fallback placeholder"}}"#;
        let (s, code) = ended("give-up", 40, doc, "abc-123");
        assert_eq!(code, 2);
        assert_eq!(
            s.read("out.json"),
            "{\n  \"session\": \"abc-123\",\n  \"why\": \"the runner exited 40 (draft-invalid: the Acceptance criteria section is the fallback placeholder)\"\n}\n"
        );
        assert_eq!(s.read("keep/exit"), "40\n");
    }

    #[test]
    fn the_why_of_a_failed_turn_is_cut_to_five_hundred_characters() {
        let long = "x".repeat(600);
        let doc = format!(r#"{{"exitCode":40,"failure":{{"reason":"{long}"}}}}"#);
        let (s, code) = ended("why-cut", 40, &doc, "abc-123");
        assert_eq!(code, 2);
        let text = s.read("out.json");
        let said: Value = serde_json::from_str(&text).expect("the outcome is json");
        let why = said["why"].as_str().expect("the reason is a string");
        assert_eq!(why.chars().count(), WHY_MAX);
        assert!(why.starts_with("the runner exited 40 ("));
        assert!(why.ends_with('x'));
        // The same document, byte for byte, as its readers receive it.
        let cut = "x".repeat(WHY_MAX - "the runner exited 40 (".len());
        assert_eq!(
            text,
            format!(
                "{{\n  \"session\": \"abc-123\",\n  \"why\": \"the runner exited 40 ({cut}\"\n}}\n"
            )
        );
    }

    #[test]
    fn a_session_the_runner_lost_says_so() {
        let (s, code) = ended("lost", 41, r#"{"lost":true}"#, "abc-123");
        assert_eq!(code, 2);
        assert_eq!(
            // serde_json's Map here is a BTreeMap, so the keys print sorted.
            s.read("out.json"),
            "{\n  \"lost\": true,\n  \"session\": \"abc-123\",\n  \"why\": \"the session abc-123 is gone: the runner has no record of it\"\n}\n"
        );
    }

    #[test]
    fn with_no_session_known_only_the_reason_is_written() {
        let (s, code) = ended("no-session", 11, r#"{"text":"Hm."}"#, "");
        assert_eq!(code, 2);
        assert_eq!(
            s.read("out.json"),
            "{\n  \"why\": \"the session id is unknown\"\n}\n"
        );
        assert!(!s.read("out.json").contains("\"session\""));
        let (s, code) = ended("too-long", 30, "", "");
        assert_eq!(code, 2);
        assert_eq!(
            s.read("out.json"),
            "{\n  \"why\": \"the runner exited 30\"\n}\n"
        );
    }

    #[test]
    fn a_turn_that_said_nothing_keeps_the_session_and_no_half_written_file() {
        let (s, code) = ended(
            "silent",
            11,
            r#"{"session":{"id":"abc-123"},"text":""}"#,
            "",
        );
        assert_eq!(code, 2);
        assert_eq!(
            s.read("out.json"),
            "{\n  \"session\": \"abc-123\",\n  \"why\": \"the turn said and asked nothing\"\n}\n"
        );
        assert!(!s.has("out.json.new"));
    }

    #[test]
    fn a_refused_turn_that_says_nothing_still_keeps_its_session() {
        // The runner gave no `error` and no `failure`, so its own words are
        // empty and the document holds only what the turn knows: the status
        // it exited with, and the session. A missing `why` means it said naught.
        let (s, code) = ended("no-why", 40, "", "abc-123");
        assert_eq!(code, 2);
        assert_eq!(
            s.read("out.json"),
            "{\n  \"session\": \"abc-123\",\n  \"why\": \"the runner exited 40\"\n}\n"
        );
    }

    #[test]
    fn every_document_a_turn_gives_up_with_is_parsable() {
        // The keys are optional and serde writes them, so the shape a reader
        // gets is a document, in every combination of them.
        for (name, rc, doc, sid, want) in [
            (
                "both",
                41,
                r#"{"lost":true}"#,
                "abc-123",
                r#"{"lost":true,"session":"abc-123","why":"the session abc-123 is gone: the runner has no record of it"}"#,
            ),
            (
                "session",
                40,
                r#"{"error":{"code":"busy"}}"#,
                "abc-123",
                r#"{"session":"abc-123","why":"the runner exited 40 (busy)"}"#,
            ),
            ("reason", 30, "", "", r#"{"why":"the runner exited 30"}"#),
        ] {
            let (s, code) = ended(name, rc, doc, sid);
            assert_eq!(code, 2, "{name}: a turn that gave up exits 2");
            let text = s.read("out.json");
            let said: Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
            let want: Value = serde_json::from_str(want).unwrap();
            assert_eq!(said, want, "{name}");
            // The bytes on disk are exactly that document, pretty printed.
            assert_eq!(
                text,
                format!("{}\n", serde_json::to_string_pretty(&want).unwrap()),
                "{name}"
            );
        }
    }

    fn spawned(script: &str) -> Child {
        Command::new("sh").args(["-c", script]).spawn().unwrap()
    }

    #[test]
    fn a_watched_container_gives_its_status() {
        let quiet = AtomicBool::new(false);
        let stop = |_: u32| panic!("nothing to stop");
        assert_eq!(watch(&mut spawned("exit 11"), 100, &quiet, &stop), 11);
        assert_eq!(watch(&mut spawned("kill -9 $$"), 100, &quiet, &stop), 137);
    }

    #[test]
    fn past_the_limit_the_container_is_stopped_with_twenty_seconds_and_gives_124() {
        let s = Scratch::new("limit");
        let flag = s.at("stopped");
        let mut child = spawned(&format!(
            "while [ ! -e {flag} ]; do sleep 0.02; done; exit 143"
        ));
        let given = std::cell::Cell::new(0);
        let rc = watch(&mut child, 0, &AtomicBool::new(false), &|seconds| {
            given.set(seconds);
            File::create(&flag).unwrap();
        });
        assert_eq!(rc, 124);
        assert_eq!(given.get(), STOP_ON_LIMIT);
    }

    #[test]
    fn a_signal_stops_the_container_with_ten_seconds_and_its_status_stands() {
        let s = Scratch::new("signal");
        let flag = s.at("stopped");
        let mut child = spawned(&format!(
            "while [ ! -e {flag} ]; do sleep 0.02; done; exit 143"
        ));
        let given = std::cell::Cell::new(0);
        let rc = watch(&mut child, 100, &AtomicBool::new(true), &|seconds| {
            given.set(seconds);
            File::create(&flag).unwrap();
        });
        assert_eq!(rc, 143);
        assert_eq!(given.get(), STOP_ON_SIGNAL);
    }

    #[test]
    fn a_client_that_cannot_be_started_gives_a_shells_status() {
        let s = Scratch::new("start");
        let (doc, log) = (s.at("doc"), s.at("log"));
        assert!(matches!(start(&["/no/such/docker"], &doc, &log), Err(127)));
        assert!(s.has("log"));
        assert!(matches!(start(&["true"], &s.at("none/doc"), &log), Err(1)));
    }
}

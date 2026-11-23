//! The issue relay of a cycle. A cycle's token reads issues and cannot write
//! them (#1814), so a label or a `<!-- tehas: -->` marker the bot writes is
//! the host's own. The three issue writes a cycle's agents make, `gh issue
//! create`, `gh issue edit` and `gh issue comment`, go to the host instead:
//! the container's gh (host/sandbox/gh-shim) leaves a request in the cycle's
//! relay directory, and this makes it with the app's token when it is one of
//! them and carries a title, a body and an issue number only.
//!
//! The directory is `<workspace>.state/relay` on the host and
//! `/run/tehas/relay` in the container, which can write anything there, so
//! every request is read as hostile:
//!   req.<pid>/args   the arguments from the verb on, each ended by a NUL
//!   req.<pid>/body   the file of --body-file/-F, copied by the shim
//!   req.<pid>/ready  made last: the request is complete
//!   res.<pid>/out, err, rc, then done   the answer, written here
//! `<pid>` is one to ten digits; any other name is left alone. Every file is
//! opened relative to a directory opened without following a link, with
//! O_NOFOLLOW, and read only when it is a regular file of at most 1 MiB, so a
//! link to a file of the host is never read. The answer directory is made
//! here, after whatever stood at its name is removed, and its files are made
//! with O_EXCL inside it, so nothing is written outside the directory. A
//! request is read into an [`Ask`], tehas's own terms, and made through the
//! blueprints port's write handle the program's wiring gives, so no word of
//! the request reaches the site as an argument. The request is removed before
//! the write, so it is made once.
//!
//! Refused, with the reason in the answer and status 1: any verb but the
//! three, any flag but --title/-t, --body/-b, --body-file/-F and --repo/-R,
//! naming the flag (labels, assignees, milestones, projects, state, --add-*,
//! --remove-*, --delete-last, --edit-last among them); a --repo that is not
//! TEHAS_REPO, and every request when TEHAS_REPO is not `<owner>/<name>`;
//! anything but one issue number in digits for edit and comment; and a title
//! or body with a tehas marker in it. A marker is found after this
//! normalisation: at every `<!--`, the characters that follow for which
//! Unicode says white space are skipped, and the next six are compared with
//! `tehas:` without regard to ASCII case. So `<!--  TEHAS:` is one, wherever
//! it stands: in a code block or a quoted email too.
//!
//! A title and a body that pass go on masked as the console masks a log, with
//! the host's names given by `tehas work`, and with every `@` mention broken
//! by a zero-width space.
//!
//! Which issues: a comment goes only on the cycle's own issue or on one the
//! cycle created through the relay, and an edit only on one it created. An
//! edit made with the app's token counts as the bot's, which the dispatcher
//! trusts, so a cycle may not rewrite an approved text. An issue counts as
//! created when the port answers the open with its number.
//! A cycle has at most 50 writes made.
//!
//! An edit is conditional, as the port's edit at a revision is: the issue is
//! read, then edited at that revision, and the edit is not made when another
//! account changed it in between. Its answer is then status 1 and one line
//! asking the agent to read the issue again.
//!
//! The token is asked of `tehasbot github-token` for every request, bounded
//! to 60 seconds: the one the start had runs out after an hour, and that
//! command keeps a token with at least ten minutes left. The handle's gh is
//! given it as GH_TOKEN and TEHAS_GH_TOKEN, so the host's shim (host/shim/gh)
//! uses this token and still logs the write.

use rustix::fs::{AtFlags, FileType, Mode, OFlags};
use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tehas_core::call::gh_why;
use tehas_core::forge::GhRunner;
use tehas_core::text::has_marker;
use tehas_ports::{
    BlueprintNo, BlueprintWrites, Body, Depth, Lost, MarkerEdit, NewBlueprint, Outcome,
};

/// The most a body file, or the arguments, may hold.
pub const MAX_BODY: u64 = 1 << 20;

/// The most writes one cycle has run.
pub const MAX_WRITES: usize = 50;

pub const MARKER: &str = "gh-shim: the relay refuses text with a tehas marker (<!-- tehas:)";

/// How often the directory is read.
const POLL: Duration = Duration::from_millis(500);

/// How long one request's gh runs may take together. The shim waits 120 s.
const GH_WAIT: u64 = 100;

/// How long the token command may take.
pub const TOKEN_WAIT: u64 = 60;

/// How old an answer is when no one will read it: longer than the gh-shim
/// waits, 120 s. It is removed then.
const STALE: Duration = Duration::from_secs(300);

fn no(why: &str) -> String {
    format!("gh-shim: the relay {why}")
}

/// The flags the relay takes, by their long name.
fn long(flag: &str) -> Option<&'static str> {
    match flag {
        "--title" | "-t" => Some("--title"),
        "--body" | "-b" => Some("--body"),
        "--body-file" | "-F" => Some("--body-file"),
        "--repo" | "-R" => Some("--repo"),
        _ => None,
    }
}

pub fn repo_shaped(repo: &str) -> bool {
    let ok = |w: &str| {
        !w.is_empty()
            && w.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    };
    matches!(repo.split_once('/'), Some((o, n)) if ok(o) && ok(n))
}

/// A cycle's issue write, as the relay planned it: its title and body masked
/// and held to the relay's rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ask {
    /// `gh issue create`.
    Open { title: String, body: String },
    /// `gh issue edit`: a title, a body, or both.
    Edit {
        no: BlueprintNo,
        title: Option<String>,
        body: Option<String>,
    },
    /// `gh issue comment`.
    Comment { no: BlueprintNo, body: String },
}

impl Ask {
    /// gh's word for it, for the log.
    fn verb(&self) -> &'static str {
        match self {
            Ask::Open { .. } => "create",
            Ask::Edit { .. } => "edit",
            Ask::Comment { .. } => "comment",
        }
    }

    /// The issue it writes to; None for a new one.
    fn no(&self) -> Option<BlueprintNo> {
        match self {
            Ask::Open { .. } => None,
            Ask::Edit { no, .. } | Ask::Comment { no, .. } => Some(*no),
        }
    }
}

/// The write a request asks, or why it is refused. `args` start at the verb;
/// `body` is the request's body file.
pub fn plan(args: &[String], body: Option<&str>, repo: &str) -> Result<Ask, String> {
    if !repo_shaped(repo) {
        return Err(no(&format!(
            "has no TEHAS_REPO of the form <owner>/<name> ({repo:?}): it runs nothing"
        )));
    }
    let verb = match args {
        [issue, verb, ..] if issue == "issue" => verb.as_str(),
        _ => "",
    };
    if !matches!(verb, "create" | "edit" | "comment") {
        return Err(no("runs issue create, issue edit and issue comment only"));
    }
    let (mut title, mut text, mut file, mut repo_given, mut number) =
        (None, None, false, None, None);
    let mut rest = args[2..].iter();
    while let Some(a) = rest.next() {
        if a.starts_with('-') && a.len() > 1 {
            let (name, inline) = match a.split_once('=') {
                Some((n, v)) if a.starts_with("--") => (n, Some(v.to_string())),
                _ => (a.as_str(), None),
            };
            let Some(flag) = long(name) else {
                let name = if name.starts_with("--") {
                    name
                } else {
                    &name[..2.min(name.len())]
                };
                return Err(no(&format!(
                    "refuses {name}: an issue write of a cycle takes a title, a body and an issue number only"
                )));
            };
            let Some(value) = inline.or_else(|| rest.next().cloned()) else {
                return Err(no(&format!("needs a value for {flag}")));
            };
            let twice = match flag {
                "--title" => title.replace(value).is_some(),
                "--body" => text.replace(value).is_some() || file,
                "--body-file" => std::mem::replace(&mut file, true) || text.is_some(),
                _ => repo_given.replace(value).is_some(),
            };
            if twice {
                return Err(no("takes one title, one body and one --repo"));
            }
        } else if verb != "create"
            && number.is_none()
            && !a.is_empty()
            && a.len() <= 20
            && a.bytes().all(|b| b.is_ascii_digit())
        {
            number = Some(a.clone());
        } else {
            return Err(no(
                "takes one issue number in digits, and none for issue create",
            ));
        }
    }
    if verb != "create" && number.is_none() {
        return Err(no(
            "takes one issue number in digits, and none for issue create",
        ));
    }
    if let Some(r) = &repo_given {
        if !r.eq_ignore_ascii_case(repo) {
            return Err(no(&format!("writes to {repo} only, not {r}")));
        }
    }
    if file && body.is_none() {
        return Err(no("got no body file for --body-file"));
    }
    let input = if file { body.map(String::from) } else { text };
    let texts = [title.as_deref(), input.as_deref()];
    if texts.into_iter().flatten().any(has_marker) {
        return Err(MARKER.to_string());
    }
    let title = title.as_deref().map(posted);
    let body = input.as_deref().map(posted);
    let issue = number
        .and_then(|n| n.parse().ok())
        .and_then(BlueprintNo::new);
    match (verb, issue, title, body) {
        ("create", _, Some(title), Some(body)) => Ok(Ask::Open { title, body }),
        ("create", ..) => Err(no("needs a title and a body for issue create")),
        (_, None, ..) => Err(no(
            "takes one issue number in digits, and none for issue create",
        )),
        ("comment", Some(no), None, Some(body)) => Ok(Ask::Comment { no, body }),
        ("comment", ..) => Err(no("takes a body and no title for issue comment")),
        (_, Some(_), None, None) => Err(no("needs a title or a body for issue edit")),
        (_, Some(no), title, body) => Ok(Ask::Edit { no, title, body }),
    }
}

/// A title or body as the host posts it: masked as the console masks a log,
/// and with every mention broken, so a cycle's text notifies nobody.
fn posted(text: &str) -> String {
    let masked = tehas_core::mask::mask(text);
    let mut out = String::with_capacity(masked.len());
    let mut chars = masked.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        if c == '@' && chars.peek().is_some_and(char::is_ascii_alphanumeric) {
            out.push('\u{200b}');
        }
    }
    out
}

/// Which issues a cycle's writes may touch, and how many it has run.
#[derive(Debug, Default)]
pub struct Scope {
    /// The cycle's own issue.
    pub own: u64,
    /// The issues it created through the relay.
    pub created: HashSet<u64>,
    /// The writes run so far.
    pub run: usize,
}

impl Scope {
    pub fn new(own: u64) -> Scope {
        Scope {
            own,
            ..Scope::default()
        }
    }

    /// Whether `ask` may be made now, or why not.
    pub fn permits(&self, ask: &Ask) -> Result<(), String> {
        if self.run >= MAX_WRITES {
            return Err(no(&format!(
                "has run {MAX_WRITES} issue writes for this cycle, its limit"
            )));
        }
        let ours = |n: &BlueprintNo| self.created.contains(&n.get());
        match ask {
            Ask::Open { .. } => Ok(()),
            Ask::Comment { no: n, .. } if n.get() == self.own || ours(n) => Ok(()),
            Ask::Comment { .. } => Err(no(&format!(
                "comments only on this cycle's issue #{} and on issues it created",
                self.own
            ))),
            Ask::Edit { no: n, .. } if ours(n) => Ok(()),
            Ask::Edit { .. } => Err(no(
                "edits only issues this cycle created, never its own or another",
            )),
        }
    }
}

/// A blueprints write handle whose requests go through the gh runner given.
/// The program's wiring makes it, so the relay never names the adapter.
pub type Writes = Box<dyn Fn(Box<dyn GhRunner>) -> Box<dyn BlueprintWrites> + Send + Sync>;

/// The program's wiring of [`Writes`]: the handle of repository `repo` over
/// the gh runner given.
pub type WritesOf = fn(repo: &str, gh: Box<dyn GhRunner>) -> Box<dyn BlueprintWrites>;

/// The handles of `repo` that `of` makes.
pub fn writes_of(repo: &str, of: WritesOf) -> Writes {
    let repo = repo.to_string();
    Box::new(move |gh| of(&repo, gh))
}

/// What the relay runs: gh, the command that prints the app's token with the
/// seconds it may take, and the handle its writes go through.
pub struct Via {
    pub gh: PathBuf,
    pub token: PathBuf,
    pub token_args: Vec<String>,
    pub token_wait: u64,
    pub writes: Writes,
}

impl Via {
    /// gh from PATH (TEHAS_GH_BIN in tests) and `<home>/bin/tehasbot
    /// github-token`, the command the renewer asks too.
    pub fn from_env(home: &Path, writes: Writes) -> Via {
        let gh = std::env::var_os("TEHAS_GH_BIN")
            .filter(|v| !v.is_empty())
            .map_or_else(|| PathBuf::from("gh"), PathBuf::from);
        Via {
            gh,
            token: home.join("bin/tehasbot"),
            token_args: vec!["github-token".to_string()],
            token_wait: TOKEN_WAIT,
            writes,
        }
    }
}

/// Serves the relay directory `dir` of the cycle of issue `issue` for `repo`
/// until `stop` is set, then answers what is left with 1.
pub fn serve_via(dir: &Path, repo: &str, issue: u64, stop: &AtomicBool, via: &Via) {
    let mut scope = Scope::new(issue);
    while !stop.load(Ordering::SeqCst) {
        pass(dir, repo, via, &mut scope, stop);
        let until = Instant::now() + POLL;
        while Instant::now() < until && !stop.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    answer_left(dir);
}

fn open_dir(dir: &Path) -> Option<OwnedFd> {
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    rustix::fs::open(dir, flags, Mode::empty()).ok()
}

/// The process ids of the requests in `dir`, in order.
fn requests(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut pids: Vec<String> = entries
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter_map(|n| n.strip_prefix("req.").map(String::from))
        .filter(|p| (1..=10).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()))
        .collect();
    pids.sort();
    pids
}

/// One reading of the directory: every complete request answered, and
/// answers no one read removed. Once `stop` is set no further request is
/// run: the end answers what is left.
fn pass(dir: &Path, repo: &str, via: &Via, scope: &mut Scope, stop: &AtomicBool) {
    let Some(top) = open_dir(dir) else {
        return;
    };
    for pid in requests(dir) {
        if stop.load(Ordering::SeqCst) {
            return;
        }
        if let Some(taken) = take(dir, &top, &pid) {
            write_request(dir, &top, &pid, taken, repo, via, scope);
        }
    }
    remove_stale(dir, &top, STALE);
}

/// The answers in `dir` older than `age`: whoever asked has given up.
fn remove_stale(dir: &Path, top: &OwnedFd, age: Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let Ok(name) = e.file_name().into_string() else {
            continue;
        };
        if !name.starts_with("res.") {
            continue;
        }
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|t| t.elapsed().is_ok_and(|a| a > age));
        if old {
            remove_at(dir, top, &name);
        }
    }
}

/// `name` in `dir` removed: a directory with what is in it, anything else
/// unlinked, a link never followed.
fn remove_at(dir: &Path, top: &OwnedFd, name: &str) -> bool {
    match rustix::fs::statat(top, name, AtFlags::SYMLINK_NOFOLLOW) {
        Err(_) => true,
        // std's remove_dir_all removes a link it meets, never what it names.
        Ok(st) if FileType::from_raw_mode(st.st_mode) == FileType::Directory => {
            std::fs::remove_dir_all(dir.join(name)).is_ok()
        }
        Ok(_) => rustix::fs::unlinkat(top, name, AtFlags::empty()).is_ok(),
    }
}

/// Every complete request answered with 1, unread: the cycle has ended.
/// Safe beside a relay thread still running: a request is answered by
/// whichever of the two takes it first.
pub fn answer_left(dir: &Path) {
    let Some(top) = open_dir(dir) else {
        return;
    };
    for pid in requests(dir) {
        if take(dir, &top, &pid).is_some() {
            answer(
                dir,
                &top,
                &pid,
                "",
                &no("did not run this: the cycle ended"),
                1,
            );
        }
    }
}

/// A request read and removed: its arguments and body, each or why not.
/// None when it is not complete or could not be removed.
struct Taken {
    args: Result<Vec<String>, String>,
    body: Result<Option<String>, String>,
}

fn take(dir: &Path, top: &OwnedFd, pid: &str) -> Option<Taken> {
    let name = format!("req.{pid}");
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let Ok(req) = rustix::fs::openat(top, &name, flags, Mode::empty()) else {
        // A link or a file in place of the directory: never followed, removed.
        let _ = rustix::fs::unlinkat(top, &name, AtFlags::empty());
        return None;
    };
    match regular(&req, "ready", 0) {
        Ok(Some(_)) => {}
        _ => return None,
    }
    let args = regular(&req, "args", MAX_BODY).and_then(|a| {
        let a = a.unwrap_or_default();
        let a = String::from_utf8(a).map_err(|_| no("takes arguments of UTF-8 text"))?;
        let Some(a) = a.strip_suffix('\0').or(a.is_empty().then_some("")) else {
            return Err(no("takes arguments each ended by a NUL"));
        };
        Ok(a.split('\0').map(String::from).collect::<Vec<_>>())
    });
    let body = regular(&req, "body", MAX_BODY).and_then(|b| {
        b.map(|b| String::from_utf8(b).map_err(|_| no("takes a body of UTF-8 text")))
            .transpose()
    });
    // A request that names a kind is no issue write, whatever else it holds.
    let args = match regular(&req, "kind", 0) {
        Ok(None) => args,
        _ => Err(no("takes issue writes only")),
    };
    drop(req);
    if std::fs::remove_dir_all(dir.join(&name)).is_err() {
        // Not run: it could be taken again.
        answer(dir, top, pid, "", &no("could not take the request"), 1);
        return None;
    }
    Some(Taken { args, body })
}

/// An issue write: planned, held to the cycle's scope, run and answered.
fn write_request(
    dir: &Path,
    top: &OwnedFd,
    pid: &str,
    taken: Taken,
    repo: &str,
    via: &Via,
    scope: &mut Scope,
) {
    let Taken { args, body } = taken;
    let ran = args.and_then(|a| answer_request(&a, body?.as_deref(), repo, via, scope));
    match ran {
        Ok((out, err, rc)) => answer(dir, top, pid, &out, &err, rc),
        Err(why) => {
            eprintln!("tehas relay: refused request {pid}: {}", why.escape_debug());
            answer(dir, top, pid, "", &why, 1);
        }
    }
}

/// One request of a cycle, planned, held to its scope and made: the output,
/// error stream and status gh's would have, or why it is refused.
fn answer_request(
    args: &[String],
    body: Option<&str>,
    repo: &str,
    via: &Via,
    scope: &mut Scope,
) -> Result<(String, String, i32), String> {
    let ask = plan(args, body, repo)?;
    scope.permits(&ask)?;
    scope.run += 1;
    let (out, err, rc) = match token(via) {
        Some(token) => {
            let gh = RelayGh {
                gh: via.gh.clone(),
                token,
                until: Instant::now() + Duration::from_secs(GH_WAIT),
            };
            make(&mut *(via.writes)(Box::new(gh)), &ask, scope)
        }
        None => (String::new(), no(NO_TOKEN), 1),
    };
    eprintln!(
        "tehas relay: issue {} {}: {rc}",
        ask.verb(),
        ask.no().map_or(String::new(), |n| n.get().to_string())
    );
    Ok((out, err, rc))
}

/// `ask` made through `writes`: a link on standard output and status 0, as gh
/// prints one, or one line in our words and status 1. A new issue joins the
/// scope's.
fn make(writes: &mut dyn BlueprintWrites, ask: &Ask, scope: &mut Scope) -> (String, String, i32) {
    let made = match ask {
        Ask::Open { title, body } => writes
            .open(&NewBlueprint {
                title: title.clone(),
                body: Body::Text(body.clone()),
                markers: Vec::new(),
            })
            .map(|p| {
                scope.created.insert(p.id.get());
                p.link
            }),
        Ask::Comment { no, body } => writes.comment(*no, body, &[]).map(|p| p.link),
        Ask::Edit { no, title, body } => {
            let read = match writes.blueprint(*no, Depth::Cut { notes: 0 }) {
                Ok(b) => b,
                Err(e) => {
                    return failed(&format!(
                        "could not read #{} before the edit, and did not edit it: {}",
                        no.get(),
                        e.line()
                    ))
                }
            };
            let edited = writes.edit(
                *no,
                title.as_deref(),
                body.as_deref(),
                &MarkerEdit::default(),
                &read.revision,
            );
            if edited == Err(Outcome::Conflict(Lost::Changed)) {
                return failed(&format!(
                    "did not edit #{}: another account changed its title or body after the relay read it. Read it again",
                    no.get()
                ));
            }
            edited
        }
    };
    match made {
        Ok(link) => (format!("{}\n", link.as_str()), String::new(), 0),
        Err(e) => failed(&format!(
            "could not {} {}: {}",
            ask.verb(),
            ask.no()
                .map_or("the issue".to_string(), |n| format!("#{}", n.get())),
            e.line()
        )),
    }
}

/// The answer of a write not made: one line in our words.
fn failed(why: &str) -> (String, String, i32) {
    (String::new(), no(&format!("{why}\n")), 1)
}

/// The regular file `name` in `dir`, at most `max` bytes. None when there is
/// none; Err when it is a link, not a regular file, or too large.
fn regular(dir: &OwnedFd, name: &str, max: u64) -> Result<Option<Vec<u8>>, String> {
    let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
    let fd = match rustix::fs::openat(dir, name, flags, Mode::empty()) {
        Ok(fd) => fd,
        Err(rustix::io::Errno::NOENT) => return Ok(None),
        Err(_) => return Err(no("takes regular files only")),
    };
    let st = rustix::fs::fstat(&fd).map_err(|_| no("takes regular files only"))?;
    if FileType::from_raw_mode(st.st_mode) != FileType::RegularFile {
        return Err(no("takes regular files only"));
    }
    let mut data = Vec::new();
    let read = File::from(fd).take(max + 1).read_to_end(&mut data);
    if read.is_err() {
        return Err(no("could not read the request"));
    }
    if data.len() as u64 > max {
        return Err(no("takes a body and arguments of at most 1 MiB"));
    }
    Ok(Some(data))
}

/// `program args...` under timeout, which signals its whole process group, so
/// a child behind a shim does not outlive it and hold the pipes.
fn bounded(seconds: u64, program: &Path, args: &[String]) -> Command {
    let mut cmd = Command::new("timeout");
    cmd.args(["-k", "5", &seconds.to_string()])
        .arg(program)
        .args(args);
    cmd
}

/// The app's token, asked of the host's token command.
fn token(via: &Via) -> Option<String> {
    bounded(via.token_wait, &via.token, &via.token_args)
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|t| !t.is_empty())
}

const NO_TOKEN: &str = "has no GitHub token from the host";

/// gh asked by the write handle, with the relay's token. The runs of one
/// request share its time, so a request is answered before the shim gives up.
struct RelayGh {
    gh: PathBuf,
    token: String,
    until: Instant,
}

impl RelayGh {
    /// gh's output, or one line in our words: gh's error stream is the
    /// site's text, which goes back into the container, so only the words
    /// of `gh_why`'s fixed list are kept.
    fn call(&self, args: &[&str], input: Option<&str>) -> Result<String, String> {
        let left = self
            .until
            .saturating_duration_since(Instant::now())
            .as_secs();
        if left == 0 {
            return Err(format!("the relay's {GH_WAIT} s are up"));
        }
        let argv: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        match run_gh(&self.gh, &self.token, left, &argv, input) {
            (out, _, 0) => Ok(out),
            (_, err, rc) => Err(format!("gh exited {rc}{}", gh_why(&err))),
        }
    }

    /// As `run`, with `input` on gh's standard input, in the time a write
    /// has.
    fn run_input(&mut self, args: &[&str], input: &str) -> Result<String, String> {
        let argv: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        match run_gh(
            &self.gh,
            &self.token,
            GH_WAIT - 2 * READ_WAIT,
            &argv,
            Some(input),
        ) {
            (out, _, 0) => Ok(out),
            (_, err, rc) => Err(format!("gh exited {rc}{}", gh_why(&err))),
        }
    }
}

impl GhRunner for RelayGh {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.call(args, None)
    }

    fn run_input(&mut self, args: &[&str], input: &str) -> Result<String, String> {
        self.call(args, Some(input))
    }
}

/// gh run with `token` for at most `wait` seconds: its output, its error
/// stream and its status.
fn run_gh(
    gh: &Path,
    token: &str,
    wait: u64,
    argv: &[String],
    input: Option<&str>,
) -> (String, String, i32) {
    // The host's shim takes TEHAS_GH_TOKEN over asking for one of its own.
    let child = bounded(wait, gh, argv)
        .env("GH_TOKEN", token)
        .env("TEHAS_GH_TOKEN", token)
        .env("GH_PROMPT_DISABLED", "1")
        .current_dir("/")
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let Ok(mut child) = child else {
        return (String::new(), no("could not start gh"), 1);
    };
    let writer = child.stdin.take().map(|mut i| {
        let text = input.unwrap_or_default().to_string();
        std::thread::spawn(move || {
            let _ = i.write_all(text.as_bytes());
        })
    });
    let done = child.wait_with_output();
    if let Some(t) = writer {
        let _ = t.join();
    }
    let Ok(done) = done else {
        return (String::new(), no("lost gh"), 1);
    };
    let out = String::from_utf8_lossy(&done.stdout).into_owned();
    let mut err = String::from_utf8_lossy(&done.stderr).into_owned();
    let status = done.status.code().unwrap_or(1);
    if status == 124 {
        err.push_str(&no(&format!("stopped gh after {wait} s\n")));
    }
    (out, err, status)
}

/// The answer to request `pid`: made in a directory made here, so the
/// container cannot have it written anywhere else.
fn answer(dir: &Path, top: &OwnedFd, pid: &str, out: &str, err: &str, rc: i32) {
    let rc = format!("{rc}\n");
    let files = [
        ("out", out),
        ("err", err),
        ("rc", rc.as_str()),
        ("done", ""),
    ];
    let name = format!("res.{pid}");
    if !remove_at(dir, top, &name)
        || rustix::fs::mkdirat(top, &name, Mode::from_raw_mode(0o700)).is_err()
    {
        eprintln!("tehas relay: could not answer request {pid}");
        return;
    }
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let Ok(res) = rustix::fs::openat(top, &name, flags, Mode::empty()) else {
        eprintln!("tehas relay: could not answer request {pid}");
        return;
    };
    for (file, text) in files {
        if !write_new(&res, file, text) {
            eprintln!("tehas relay: could not answer request {pid}");
            return;
        }
    }
}

fn write_new(dir: &OwnedFd, name: &str, text: &str) -> bool {
    let flags = OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    match rustix::fs::openat(dir, name, flags, Mode::from_raw_mode(0o600)) {
        Ok(fd) => File::from(fd).write_all(text.as_bytes()).is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tehas_core::testdir::TestDir;
    use tehas_ports::fake::{FakeSite, World};
    use tehas_ports::{
        Actor, Attachment, AttachmentId, Blueprint, BlueprintDelta, BlueprintHit, Blueprints,
        Bound, Caller, ChangeToken, Cite, Cites, Comment, CommentId, Cursor, Did, Edit, Got,
        Marker, Posted, Report, Reports, Revision, Site, Took, WebLink, Whole, Within, Write,
    };

    const REPO: &str = "acme/widget";

    fn args(line: &[&str]) -> Vec<String> {
        line.iter().map(|s| s.to_string()).collect()
    }

    fn refused(line: &[&str], body: Option<&str>) -> String {
        plan(&args(line), body, REPO).expect_err("refused")
    }

    fn no(n: u64) -> BlueprintNo {
        BlueprintNo::new(n).unwrap()
    }

    fn text(s: &str) -> Option<String> {
        Some(s.to_string())
    }

    #[test]
    fn create_with_title_and_body_is_an_open() {
        assert_eq!(
            plan(
                &args(&[
                    "issue",
                    "create",
                    "--title",
                    "A bug",
                    "--body",
                    "It breaks."
                ]),
                None,
                REPO
            ),
            Ok(Ask::Open {
                title: "A bug".to_string(),
                body: "It breaks.".to_string()
            })
        );
    }

    #[test]
    fn the_three_verbs_and_their_flags_in_every_spelling_are_planned() {
        assert_eq!(
            plan(
                &args(&["issue", "comment", "3", "-b", "hi", "-R", REPO]),
                None,
                REPO
            ),
            Ok(Ask::Comment {
                no: no(3),
                body: "hi".to_string()
            })
        );
        assert_eq!(
            plan(
                &args(&["issue", "edit", "--body-file=/tmp/b.md", "12", "-t", "New"]),
                Some("the text"),
                REPO
            ),
            Ok(Ask::Edit {
                no: no(12),
                title: text("New"),
                body: text("the text")
            })
        );
        assert_eq!(
            plan(
                &args(&[
                    "issue",
                    "create",
                    "-t",
                    "T",
                    "-F",
                    "-",
                    "--repo=acme/widget"
                ]),
                Some(""),
                REPO
            ),
            Ok(Ask::Open {
                title: "T".to_string(),
                body: String::new()
            })
        );
        assert_eq!(
            plan(&args(&["issue", "edit", "3", "-t", "T"]), None, REPO),
            Ok(Ask::Edit {
                no: no(3),
                title: text("T"),
                body: None
            })
        );
    }

    #[test]
    fn a_write_missing_its_text_is_refused() {
        for (line, why) in [
            (&["issue", "create", "-t", "T"][..], "a title and a body"),
            (&["issue", "create", "-b", "B"], "a title and a body"),
            (&["issue", "comment", "3"], "a body and no title"),
            (
                &["issue", "comment", "3", "-t", "T", "-b", "B"],
                "a body and no title",
            ),
            (&["issue", "edit", "3"], "a title or a body"),
            (&["issue", "edit", "0", "-t", "T"], "one issue number"),
            (
                &["issue", "edit", "99999999999999999999", "-t", "T"],
                "one issue number",
            ),
        ] {
            let got = refused(line, None);
            assert!(got.contains(why), "{line:?}: {got}");
        }
    }

    #[test]
    fn a_value_that_looks_like_a_flag_stays_a_value() {
        assert_eq!(
            plan(
                &args(&["issue", "comment", "3", "--body", "--add-label=tehas:ready"]),
                None,
                REPO
            ),
            Ok(Ask::Comment {
                no: no(3),
                body: "--add-label=tehas:ready".to_string()
            })
        );
        assert_eq!(
            plan(
                &args(&["issue", "create", "-t", "--label", "-b", "x"]),
                None,
                REPO
            ),
            Ok(Ask::Open {
                title: "--label".to_string(),
                body: "x".to_string()
            })
        );
    }

    #[test]
    fn a_body_from_a_file_or_a_flag_is_kept_whole() {
        let big = "x".repeat(200 * 1024);
        let ask = plan(
            &args(&["issue", "comment", "3", "--body", &big]),
            None,
            REPO,
        );
        assert_eq!(
            ask,
            Ok(Ask::Comment {
                no: no(3),
                body: big
            })
        );
        let ask = plan(
            &args(&["issue", "edit", "3", "-F", "f"]),
            Some("from file"),
            REPO,
        );
        assert_eq!(
            ask,
            Ok(Ask::Edit {
                no: no(3),
                title: None,
                body: text("from file")
            })
        );
    }

    #[test]
    fn a_title_and_a_body_are_posted_masked_and_notify_nobody() {
        let token = format!("ghp_{}", "A".repeat(36));
        let ask = plan(
            &args(&[
                "issue",
                "create",
                "--title",
                &format!("token {token} for @alice"),
                "--body",
                &format!("key AKIA{} and GH_TOKEN=abc ask @bob", "B".repeat(16)),
            ]),
            None,
            REPO,
        );
        assert_eq!(
            ask,
            Ok(Ask::Open {
                title: "token ghp_*** for @\u{200b}alice".to_string(),
                body: "key AKIA*** and GH_TOKEN=*** ask @\u{200b}bob".to_string()
            })
        );
        let ask = plan(
            &args(&["issue", "comment", "3", "-F", "f"]),
            Some("mail a@b.example, @ alone, xoxb-1234567890ab"),
            REPO,
        );
        assert_eq!(
            ask,
            Ok(Ask::Comment {
                no: no(3),
                body: "mail a@\u{200b}b.example, @ alone, xoxb-***".to_string()
            })
        );
    }

    #[test]
    fn without_a_repository_every_request_is_refused() {
        for repo in ["", "widget", "acme/", "/widget", "acme/widget/x", "a b/c"] {
            let why = plan(&args(&["issue", "comment", "3", "--body", "x"]), None, repo)
                .expect_err("refused");
            assert!(why.contains("TEHAS_REPO"), "{repo:?}: {why}");
        }
    }

    fn planned(line: &[&str]) -> Ask {
        plan(&args(line), None, REPO).unwrap()
    }

    #[test]
    fn a_comment_goes_on_the_cycles_issue_or_one_it_created() {
        let mut scope = Scope::new(3);
        assert_eq!(
            scope.permits(&planned(&["issue", "comment", "3", "-b", "x"])),
            Ok(())
        );
        let why = scope
            .permits(&planned(&["issue", "comment", "4", "-b", "x"]))
            .unwrap_err();
        assert!(why.contains("#3") && why.contains("created"), "{why}");
        scope.created.insert(4);
        assert_eq!(
            scope.permits(&planned(&["issue", "comment", "4", "-b", "x"])),
            Ok(())
        );
    }

    #[test]
    fn an_edit_goes_only_on_an_issue_the_cycle_created() {
        let mut scope = Scope::new(3);
        for n in ["3", "4"] {
            let why = scope
                .permits(&planned(&["issue", "edit", n, "-t", "x"]))
                .unwrap_err();
            assert!(
                why.contains("edits only issues this cycle created"),
                "{n}: {why}"
            );
        }
        assert_eq!(
            scope.permits(&planned(&["issue", "create", "-t", "x", "-b", "y"])),
            Ok(())
        );
        scope.created.insert(4);
        assert_eq!(
            scope.permits(&planned(&["issue", "edit", "4", "-t", "x"])),
            Ok(())
        );
        assert!(scope
            .permits(&planned(&["issue", "edit", "3", "-t", "x"]))
            .is_err());
    }

    #[test]
    fn a_cycle_runs_at_most_fifty_writes() {
        let mut scope = Scope::new(3);
        scope.run = MAX_WRITES - 1;
        assert_eq!(
            scope.permits(&planned(&["issue", "comment", "3", "-b", "x"])),
            Ok(())
        );
        scope.run = MAX_WRITES;
        let why = scope
            .permits(&planned(&["issue", "comment", "3", "-b", "x"]))
            .unwrap_err();
        assert!(why.contains("50"), "{why}");
    }

    #[test]
    fn labels_are_refused() {
        assert!(refused(&["issue", "edit", "3", "--label", "x"], None).contains("--label"));
        assert!(refused(&["issue", "edit", "3", "--add-label", "x"], None).contains("--add-label"));
        assert!(refused(&["issue", "edit", "3", "--add-label=x"], None).contains("--add-label"));
        assert!(
            refused(&["issue", "create", "-t", "a", "-b", "b", "-l", "x"], None).contains("-l")
        );
        assert!(refused(&["issue", "create", "-t", "a", "-b", "b", "-lx"], None).contains("-l"));
    }

    #[test]
    fn every_other_flag_is_refused_by_name() {
        for flag in [
            "--assignee",
            "--add-assignee",
            "--remove-assignee",
            "--milestone",
            "--remove-milestone",
            "--project",
            "--add-project",
            "--remove-label",
            "--state",
            "--delete-last",
            "--edit-last",
            "--web",
            "--editor",
            "--template",
            "--recover",
            "-a",
            "-m",
            "-p",
            "-w",
            "-e",
            "--",
        ] {
            let why = refused(&["issue", "comment", "3", flag, "x"], None);
            assert!(why.contains(flag), "{flag}: {why}");
        }
    }

    #[test]
    fn only_three_verbs_are_relayed() {
        for line in [
            &["issue", "close", "3"][..],
            &["issue", "reopen", "3"],
            &["issue", "delete", "3"],
            &["issue", "transfer", "3", "x/y"],
            &["issue", "lock", "3"],
            &["issue", "pin", "3"],
            &["issue", "develop", "3"],
            &["pr", "comment", "3", "--body", "x"],
            &["label", "create", "x"],
            &["api", "repos/acme/widget/issues/3/labels"],
            &["issue"],
            &[],
        ] {
            let why = plan(&args(line), None, REPO).expect_err("refused");
            assert!(why.starts_with("gh-shim: the relay"), "{line:?}: {why}");
        }
    }

    #[test]
    fn another_repo_is_refused() {
        for repo in ["other/widget", "acme/widget2", "github.com/acme/widget", ""] {
            let why = refused(
                &["issue", "comment", "3", "--body", "x", "--repo", repo],
                None,
            );
            assert!(why.contains(REPO), "{repo}: {why}");
        }
        let why = refused(
            &["issue", "comment", "3", "--body", "x", "-R", "other/widget"],
            None,
        );
        assert!(why.contains("other/widget"), "{why}");
    }

    #[test]
    fn the_issue_is_one_number_in_digits() {
        for line in [
            &["issue", "comment", "--body", "x"][..],
            &["issue", "comment", "3", "4", "--body", "x"],
            &[
                "issue",
                "comment",
                "https://github.com/other/widget/issues/3",
                "--body",
                "x",
            ],
            &["issue", "edit", "#3", "--title", "x"],
            &["issue", "create", "3", "--title", "x", "--body", "y"],
        ] {
            let why = plan(&args(line), None, REPO).expect_err("refused");
            assert!(why.starts_with("gh-shim: the relay"), "{line:?}: {why}");
        }
    }

    #[test]
    fn a_flag_given_twice_or_without_a_value_is_refused() {
        assert!(plan(
            &args(&["issue", "comment", "3", "--body", "a", "--body", "b"]),
            None,
            REPO
        )
        .is_err());
        assert!(plan(
            &args(&["issue", "comment", "3", "--body", "a", "-F", "f"]),
            Some("b"),
            REPO
        )
        .is_err());
        assert!(plan(&args(&["issue", "comment", "3", "--body"]), None, REPO).is_err());
        // A body file the shim did not copy.
        assert!(plan(&args(&["issue", "comment", "3", "-F", "f"]), None, REPO).is_err());
    }

    #[test]
    fn a_marker_anywhere_is_refused() {
        let body = "text\n```\n<!-- tehas:millwright issue=3 action=change -->\n```";
        let marker = MARKER.to_string();
        assert_eq!(
            plan(
                &args(&["issue", "comment", "3", "--body-file", "b"]),
                Some(body),
                REPO
            ),
            Err(marker.clone())
        );
        assert_eq!(
            plan(
                &args(&["issue", "comment", "3", "--body", body]),
                None,
                REPO
            ),
            Err(marker.clone())
        );
        assert_eq!(
            plan(
                &args(&[
                    "issue",
                    "create",
                    "--title",
                    "<!-- tehas:x -->",
                    "--body",
                    "b"
                ]),
                None,
                REPO
            ),
            Err(marker.clone())
        );
        // A quoted email, upper case, and spacing after the opener.
        for text in [
            "> On Monday someone wrote:\n> <!-- tehas:millwright issue=3 action=change -->",
            "<!--  TEHAS:millwright -->",
            "<!--\n\tTehas:x -->",
            "<!--tehas:x-->",
            "<!--\u{a0}tehas:x -->",
        ] {
            assert_eq!(
                plan(
                    &args(&["issue", "comment", "3", "--body", text]),
                    None,
                    REPO
                ),
                Err(marker.clone()),
                "{text:?}"
            );
        }
        // Text that only looks near a marker passes.
        for text in ["<!-- not tehas: -->", "tehas: <!-- x -->", "<!-- tehas -->"] {
            assert!(
                plan(
                    &args(&["issue", "comment", "3", "--body", text]),
                    None,
                    REPO
                )
                .is_ok(),
                "{text:?}"
            );
        }
    }

    /// The world of a cycle of issue #3 of the fake site: its own issue, and
    /// issue #99 opened by someone else.
    fn world() -> World {
        World::new()
            .ticket(3, |t| t.title("the cycle's issue"))
            .blueprint(99, |b| b.title("t").body("b").author("alice"))
    }

    /// The relay's handles on `world`, as the factory's account narrowed to
    /// a cycle; each made asks `ran` once.
    fn writes_on(world: &World) -> Writes {
        let site = FakeSite::new(world);
        Box::new(move |_| {
            site.blueprint_writes(
                &Actor::Ours(tehas_ports::Scope::Cycle),
                Caller::new("relay"),
            )
            .expect("a cycle's handle")
        })
    }

    fn stub_token(d: &TestDir, token: &str) -> PathBuf {
        let t = d.join("token");
        tehas_core::testing::write_exe(&t, format!("#!/bin/sh\necho {token}\n")).unwrap();
        t
    }

    fn via(d: &TestDir, writes: Writes) -> Via {
        Via {
            gh: d.join("no-gh"),
            token: stub_token(d, "ghs_full"),
            token_args: vec![],
            token_wait: 10,
            writes,
        }
    }

    fn request(relay: &Path, pid: &str, args: &[&str], body: Option<&str>) {
        let req = relay.join(format!("req.{pid}"));
        fs::create_dir(&req).unwrap();
        let mut a = Vec::new();
        for w in args {
            a.extend_from_slice(w.as_bytes());
            a.push(0);
        }
        fs::write(req.join("args"), a).unwrap();
        if let Some(b) = body {
            fs::write(req.join("body"), b).unwrap();
        }
        fs::write(req.join("ready"), "").unwrap();
    }

    fn wait_done(relay: &Path, pid: &str) -> bool {
        let until = Instant::now() + Duration::from_secs(10);
        while Instant::now() < until {
            if relay.join(format!("res.{pid}/done")).exists() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    fn served(
        d: &TestDir,
        world: &World,
    ) -> (PathBuf, Arc<AtomicBool>, std::thread::JoinHandle<()>) {
        let relay = d.join("relay");
        fs::create_dir(&relay).unwrap();
        let via = via(d, writes_on(world));
        let stop = Arc::new(AtomicBool::new(false));
        let (r, s) = (relay.clone(), stop.clone());
        let t = std::thread::spawn(move || serve_via(&r, REPO, 3, &s, &via));
        (relay, stop, t)
    }

    fn read(p: PathBuf) -> String {
        fs::read_to_string(p).unwrap_or_default()
    }

    /// What the world was asked to write, in order.
    fn wrote(world: &World) -> Vec<Write> {
        world.wrote().into_iter().map(|w| w.what).collect()
    }

    #[test]
    fn serve_answers_a_request_with_the_link_of_what_it_wrote() {
        let d = TestDir::new("relay-serve");
        let w = world();
        let (relay, stop, t) = served(&d, &w);
        request(
            &relay,
            "42",
            &["issue", "comment", "3", "--body", "hi"],
            None,
        );
        assert!(wait_done(&relay, "42"));
        stop.store(true, Ordering::SeqCst);
        t.join().unwrap();
        assert_eq!(read(relay.join("res.42/rc")), "0\n");
        let out = read(relay.join("res.42/out"));
        assert!(
            out.starts_with("https://fake.test/") && out.ends_with('\n'),
            "{out}"
        );
        assert!(!relay.join("req.42").exists());
        assert_eq!(
            wrote(&w),
            vec![Write::Comment {
                on: no(3),
                body: Body::Text("hi".to_string()),
                markers: vec![]
            }]
        );
    }

    #[test]
    fn a_request_that_names_a_kind_is_refused_and_runs_nothing() {
        let d = TestDir::new("relay-kind");
        let w = world();
        let (relay, stop, t) = served(&d, &w);
        let args = ["issue", "comment", "3", "--body", "hi"];
        for (pid, kind) in [("6", "vipune\n"), ("7", "")] {
            let req = relay.join(format!("req.{pid}"));
            fs::create_dir(&req).unwrap();
            fs::write(req.join("args"), args.join("\0") + "\0").unwrap();
            fs::write(req.join("kind"), kind).unwrap();
            fs::write(req.join("ready"), "").unwrap();
            assert!(wait_done(&relay, pid));
            assert_eq!(read(relay.join(format!("res.{pid}/rc"))), "1\n");
            assert!(
                read(relay.join(format!("res.{pid}/err"))).contains("takes issue writes only"),
                "{pid}"
            );
        }
        stop.store(true, Ordering::SeqCst);
        t.join().unwrap();
        assert_eq!(wrote(&w), vec![]);
    }

    #[test]
    fn serve_takes_a_body_file() {
        let d = TestDir::new("relay-body");
        let w = world();
        let (relay, stop, t) = served(&d, &w);
        request(
            &relay,
            "7",
            &["issue", "comment", "3", "--body-file", "/x/b.md"],
            Some("line one\nline two\n"),
        );
        assert!(wait_done(&relay, "7"));
        stop.store(true, Ordering::SeqCst);
        t.join().unwrap();
        assert_eq!(read(relay.join("res.7/rc")), "0\n");
        assert_eq!(
            wrote(&w),
            vec![Write::Comment {
                on: no(3),
                body: Body::Text("line one\nline two\n".to_string()),
                markers: vec![]
            }]
        );
    }

    #[test]
    fn serve_refuses_without_writing() {
        let d = TestDir::new("relay-refuse");
        let w = world();
        let (relay, stop, t) = served(&d, &w);
        request(
            &relay,
            "8",
            &["issue", "edit", "3", "--add-label", "tehas:ready"],
            None,
        );
        assert!(wait_done(&relay, "8"));
        stop.store(true, Ordering::SeqCst);
        t.join().unwrap();
        assert_eq!(read(relay.join("res.8/rc")), "1\n");
        assert!(read(relay.join("res.8/err")).contains("--add-label"));
        assert_eq!(wrote(&w), vec![]);
    }

    #[test]
    fn a_symlink_or_an_oversized_body_is_refused() {
        let d = TestDir::new("relay-hostile");
        let secret = d.join("secret");
        fs::write(&secret, "the key").unwrap();
        let w = world();
        let (relay, stop, t) = served(&d, &w);
        // A body that names a file of the host.
        let req = relay.join("req.9");
        fs::create_dir(&req).unwrap();
        fs::write(req.join("args"), "issue\0comment\x003\0-F\0b\0").unwrap();
        std::os::unix::fs::symlink(&secret, req.join("body")).unwrap();
        fs::write(req.join("ready"), "").unwrap();
        // A body over the bound.
        request(
            &relay,
            "10",
            &["issue", "comment", "3", "-F", "b"],
            Some(&"x".repeat(MAX_BODY as usize + 1)),
        );
        // A request directory that is a link to another directory.
        let elsewhere = d.join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        fs::write(elsewhere.join("args"), "issue\0comment\x003\0--body\0x\0").unwrap();
        fs::write(elsewhere.join("ready"), "").unwrap();
        std::os::unix::fs::symlink(&elsewhere, relay.join("req.11")).unwrap();
        // An answer directory the container made in advance, as a link.
        let trap = d.join("trap");
        fs::create_dir(&trap).unwrap();
        std::os::unix::fs::symlink(&trap, relay.join("res.12")).unwrap();
        request(
            &relay,
            "12",
            &["issue", "comment", "3", "--body", "x"],
            None,
        );
        // A name that is no process id.
        request(
            &relay,
            "x1",
            &["issue", "comment", "3", "--body", "x"],
            None,
        );
        for pid in ["9", "10", "12"] {
            assert!(wait_done(&relay, pid), "{pid}");
        }
        std::thread::sleep(Duration::from_millis(800));
        stop.store(true, Ordering::SeqCst);
        t.join().unwrap();
        assert_eq!(read(relay.join("res.9/rc")), "1\n");
        assert_eq!(read(relay.join("res.10/rc")), "1\n");
        assert!(read(relay.join("res.10/err")).contains("1 MiB"));
        assert_eq!(read(relay.join("res.12/rc")), "0\n");
        assert!(fs::symlink_metadata(relay.join("res.12")).unwrap().is_dir());
        assert_eq!(fs::read_dir(&trap).unwrap().count(), 0);
        assert!(!relay.join("res.11").exists());
        assert!(elsewhere.join("args").exists());
        assert!(!relay.join("res.x1").exists());
        assert!(relay.join("req.x1").exists());
        // Request 12 alone was written, and nothing of the host's file.
        assert_eq!(
            wrote(&w),
            vec![Write::Comment {
                on: no(3),
                body: Body::Text("x".to_string()),
                markers: vec![]
            }]
        );
    }

    #[test]
    fn serve_lets_a_cycle_edit_the_issue_it_created_and_no_other() {
        let d = TestDir::new("relay-scope");
        let w = world();
        let (relay, stop, t) = served(&d, &w);
        let ask = |pid: &str, line: &[&str]| {
            request(&relay, pid, line, None);
            assert!(wait_done(&relay, pid), "{pid}");
        };
        ask("1", &["issue", "edit", "3", "--title", "x"]);
        ask("2", &["issue", "edit", "99", "--title", "x"]);
        ask("3", &["issue", "comment", "99", "--body", "x"]);
        ask("4", &["issue", "create", "--title", "t", "--body", "b"]);
        let made = read(relay.join("res.4/out"));
        let n = made.trim().rsplit('/').next().unwrap().to_string();
        ask("5", &["issue", "edit", &n, "--title", "x"]);
        ask("6", &["issue", "comment", &n, "--body", "x"]);
        stop.store(true, Ordering::SeqCst);
        t.join().unwrap();
        let rc = |p: &str| read(relay.join(format!("res.{p}/rc")));
        assert_eq!(rc("1"), "1\n");
        assert!(read(relay.join("res.1/err")).contains("edits only issues this cycle created"));
        assert_eq!(rc("2"), "1\n");
        assert_eq!(rc("3"), "1\n");
        assert!(read(relay.join("res.3/err")).contains("#3"));
        assert_eq!(rc("4"), "0\n");
        assert!(made.starts_with("https://fake.test/"), "{made}");
        assert_eq!(rc("5"), "0\n");
        assert_eq!(rc("6"), "0\n");
        let n = no(n.parse().unwrap());
        let kinds: Vec<String> = wrote(&w)
            .iter()
            .map(|w| match w {
                Write::OpenBlueprint(_) => "open".to_string(),
                Write::EditBlueprint { no, .. } => format!("edit {}", no.get()),
                Write::Comment { on, .. } => format!("comment {}", on.get()),
                other => format!("{other:?}"),
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "open".to_string(),
                format!("edit {}", n.get()),
                format!("comment {}", n.get())
            ]
        );
    }

    /// A handle that reads through `inner`, then has another account edit
    /// the blueprint read, once: the edit that follows is at a revision
    /// already gone.
    struct EditedAfterRead {
        inner: Box<dyn BlueprintWrites>,
        world: World,
        edits: u32,
    }

    impl Reports for EditedAfterRead {
        fn take_reports(&mut self) -> Vec<Report> {
            self.inner.take_reports()
        }
    }

    impl Cites for EditedAfterRead {
        fn cite(&self, what: &Cite) -> String {
            self.inner.cite(what)
        }
    }

    impl Blueprints for EditedAfterRead {
        fn blueprint(&mut self, no: BlueprintNo, depth: Depth) -> Got<Blueprint> {
            let read = self.inner.blueprint(no, depth);
            if self.edits > 0 {
                self.edits -= 1;
                let key = tehas_ports::TicketKey::Number(no.get().try_into().unwrap());
                self.world.edit_elsewhere(&key, "edited", "alice");
            }
            read
        }
        fn several(
            &mut self,
            nos: &[BlueprintNo],
            depth: Depth,
        ) -> Vec<(BlueprintNo, Got<Blueprint>)> {
            self.inner.several(nos, depth)
        }
        fn comments(&mut self, no: BlueprintNo, bound: Bound) -> Got<Whole<Comment>> {
            self.inner.comments(no, bound)
        }
        fn comments_by_id(&mut self, ids: &[CommentId]) -> Vec<(CommentId, Got<Comment>)> {
            self.inner.comments_by_id(ids)
        }
        fn body_edits(&mut self, no: BlueprintNo, bound: Bound) -> Got<Whole<Edit>> {
            self.inner.body_edits(no, bound)
        }
        fn comment_edits(&mut self, id: &CommentId, bound: Bound) -> Got<Whole<Edit>> {
            self.inner.comment_edits(id, bound)
        }
        fn search(
            &mut self,
            words: &str,
            within: Within,
            open: Option<bool>,
            first: std::num::NonZeroU32,
        ) -> Got<Took<BlueprintHit>> {
            self.inner.search(words, within, open, first)
        }
        fn attachments(&mut self, no: BlueprintNo) -> Got<Whole<Attachment>> {
            self.inner.attachments(no)
        }
        fn fetch(&mut self, a: &AttachmentId, limit: u64, into: &mut dyn std::io::Write) -> Did {
            self.inner.fetch(a, limit, into)
        }
        fn changed(&mut self, since: Option<&ChangeToken>) -> Got<(bool, ChangeToken)> {
            self.inner.changed(since)
        }
        fn delta(&mut self, from: Option<&Cursor>, bound: Bound) -> Got<BlueprintDelta> {
            self.inner.delta(from, bound)
        }
    }

    impl BlueprintWrites for EditedAfterRead {
        fn open(&mut self, new: &NewBlueprint) -> Got<Posted<BlueprintNo>> {
            self.inner.open(new)
        }
        fn edit(
            &mut self,
            no: BlueprintNo,
            title: Option<&str>,
            body: Option<&str>,
            markers: &MarkerEdit,
            since: &Revision,
        ) -> Got<WebLink> {
            self.inner.edit(no, title, body, markers, since)
        }
        fn comment(
            &mut self,
            no: BlueprintNo,
            body: &str,
            markers: &[Marker],
        ) -> Got<Posted<CommentId>> {
            self.inner.comment(no, body, markers)
        }
        fn edit_comment(
            &mut self,
            id: &CommentId,
            body: &str,
            markers: &MarkerEdit,
            since: &Revision,
        ) -> Got<WebLink> {
            self.inner.edit_comment(id, body, markers, since)
        }
        fn close(&mut self, no: BlueprintNo, done: bool) -> Did {
            self.inner.close(no, done)
        }
    }

    fn cycle_handle(world: &World) -> Box<dyn BlueprintWrites> {
        FakeSite::new(world)
            .blueprint_writes(
                &Actor::Ours(tehas_ports::Scope::Cycle),
                Caller::new("relay"),
            )
            .unwrap()
    }

    #[test]
    fn an_edit_of_an_issue_another_account_changed_after_the_read_is_not_made() {
        let w = world();
        let mut scope = Scope::new(3);
        let mut writes = cycle_handle(&w);
        let open = Ask::Open {
            title: "t".to_string(),
            body: "b".to_string(),
        };
        assert_eq!(make(&mut *writes, &open, &mut scope).2, 0);
        let n = *scope.created.iter().next().unwrap();
        let edit = Ask::Edit {
            no: no(n),
            title: text("x"),
            body: None,
        };
        let mut raced = EditedAfterRead {
            inner: cycle_handle(&w),
            world: w.clone(),
            edits: 1,
        };
        assert_eq!(
            make(&mut raced, &edit, &mut scope),
            (
                String::new(),
                format!("gh-shim: the relay did not edit #{n}: another account changed its title or body after the relay read it. Read it again\n"),
                1
            )
        );
        // Read again, unchanged since: the retry is made.
        assert_eq!(make(&mut raced, &edit, &mut scope).2, 0);
        let edits = wrote(&w)
            .into_iter()
            .filter(|w| matches!(w, Write::EditBlueprint { .. }))
            .count();
        assert_eq!(edits, 1);
    }

    #[test]
    fn an_edit_whose_read_fails_is_not_made() {
        use tehas_ports::fake::{BlueprintsOp, Fault, Faulty, Op};
        let w = world();
        let mut scope = Scope::new(3);
        scope.created.insert(99);
        let site = Faulty::new(FakeSite::new(&w))
            .on(Op::Blueprints(BlueprintsOp::Blueprint), Fault::Unreachable);
        let mut writes = site
            .blueprint_writes(
                &Actor::Ours(tehas_ports::Scope::Cycle),
                Caller::new("relay"),
            )
            .unwrap();
        let edit = Ask::Edit {
            no: no(99),
            title: text("x"),
            body: None,
        };
        let (out, err, rc) = make(&mut *writes, &edit, &mut scope);
        assert_eq!((out.as_str(), rc), ("", 1));
        assert_eq!(
            err,
            format!(
                "gh-shim: the relay could not read #99 before the edit, and did not edit it: {}\n",
                Fault::Unreachable.outcome().line()
            )
        );
        assert_eq!(wrote(&w), vec![]);
    }

    #[test]
    fn a_write_the_site_refuses_is_answered_in_our_words() {
        let w = world();
        let mut scope = Scope::new(3);
        let ask = Ask::Comment {
            no: no(404),
            body: "x".to_string(),
        };
        let (out, err, rc) = make(&mut *cycle_handle(&w), &ask, &mut scope);
        assert_eq!((out.as_str(), rc), ("", 1));
        assert_eq!(
            err,
            format!(
                "gh-shim: the relay could not comment #404: {}\n",
                Outcome::NotFound.line()
            )
        );
    }

    #[test]
    fn an_issue_opened_joins_the_cycles_scope_by_the_number_the_site_answers() {
        let w = world();
        let mut scope = Scope::new(3);
        let open = Ask::Open {
            title: "t".to_string(),
            body: "b".to_string(),
        };
        let (out, _, rc) = make(&mut *cycle_handle(&w), &open, &mut scope);
        assert_eq!(rc, 0);
        let n = *scope.created.iter().next().unwrap();
        assert!(out.trim_end().ends_with(&format!("/{n}")), "{out}");
        assert_eq!(scope.created.len(), 1);
    }

    #[test]
    fn the_relays_gh_takes_an_input_on_standard_input_with_its_token() {
        let d = TestDir::new("relay-gh-input");
        let mut gh = RelayGh {
            gh: stub_gh(&d),
            token: "ghs_full".to_string(),
            until: Instant::now() + Duration::from_secs(10),
        };
        let body = "b".repeat(200 * 1024);
        let got = gh.run_input(&["issue", "comment", "3", "--body-file=-"], &body);
        assert_eq!(got, Ok("posted\n".to_string()));
        assert_eq!(
            read(d.join("gh.log")),
            format!("issue|comment|3|--body-file=-|token=ghs_full,ghs_full\n{body}")
        );
    }

    #[test]
    fn the_relays_gh_runs_nothing_once_its_time_is_up() {
        let d = TestDir::new("relay-gh-late");
        let mut gh = RelayGh {
            gh: stub_gh(&d),
            token: "ghs_full".to_string(),
            until: Instant::now(),
        };
        assert_eq!(
            gh.run_input(&["api", "x"], "b"),
            Err(format!("the relay's {GH_WAIT} s are up"))
        );
        assert!(!d.join("gh.log").exists());
    }

    #[test]
    fn the_relays_gh_takes_an_input_on_standard_input_with_its_token() {
        let d = TestDir::new("relay-gh-input");
        let mut gh = RelayGh {
            gh: stub_gh(&d),
            token: "ghs_full".to_string(),
        };
        let body = "b".repeat(200 * 1024);
        let got = gh.run_input(&["issue", "comment", "3", "--body-file=-"], &body);
        assert_eq!(got, Ok("posted\n".to_string()));
        assert_eq!(
            read(d.join("gh.log")),
            format!("issue|comment|3|--body-file=-|token=ghs_full,ghs_full\n{body}")
        );
    }

    #[test]
    fn a_token_command_that_hangs_is_given_up_on() {
        let d = TestDir::new("relay-token-hangs");
        let relay = d.join("relay");
        fs::create_dir(&relay).unwrap();
        let token = d.join("token");
        tehas_core::testing::write_exe(&token, "#!/bin/sh\nsleep 30\necho late\n").unwrap();
        let w = world();
        let via = Via {
            gh: d.join("no-gh"),
            token,
            token_args: vec![],
            token_wait: 1,
            writes: writes_on(&w),
        };
        let stop = Arc::new(AtomicBool::new(false));
        let (r, s) = (relay.clone(), stop.clone());
        let t = std::thread::spawn(move || serve_via(&r, REPO, 3, &s, &via));
        let started = Instant::now();
        request(
            &relay,
            "9",
            &["issue", "comment", "3", "--body", "hi"],
            None,
        );
        assert!(wait_done(&relay, "9"));
        assert!(
            started.elapsed() < Duration::from_secs(9),
            "{:?}",
            started.elapsed()
        );
        stop.store(true, Ordering::SeqCst);
        t.join().unwrap();
        assert_eq!(read(relay.join("res.9/rc")), "1\n");
        assert!(read(relay.join("res.9/err")).contains("no GitHub token"));
        assert_eq!(wrote(&w), vec![]);
    }

    #[test]
    fn a_request_left_at_the_end_is_answered_1() {
        let d = TestDir::new("relay-end");
        let relay = d.join("relay");
        fs::create_dir(&relay).unwrap();
        request(
            &relay,
            "5",
            &["issue", "comment", "3", "--body", "hi"],
            None,
        );
        let w = world();
        serve_via(
            &relay,
            REPO,
            3,
            &AtomicBool::new(true),
            &via(&d, writes_on(&w)),
        );
        assert_eq!(read(relay.join("res.5/rc")), "1\n");
        assert!(relay.join("res.5/done").exists());
        assert_eq!(wrote(&w), vec![]);
    }

    #[test]
    fn answer_request_refuses_what_the_scope_does_not_permit_and_runs_nothing() {
        // A token command that leaves a file: a write that ran asked it.
        let d = TestDir::new("relay-refused-runs-nothing");
        let ran = d.join("ran");
        let token = d.join("token");
        tehas_core::testing::write_exe(&token, format!("#!/bin/sh\n: >'{}'\n", ran.display()))
            .unwrap();
        let w = world();
        let via = Via {
            gh: d.join("no-gh"),
            token,
            token_args: Vec::new(),
            token_wait: 5,
            writes: writes_on(&w),
        };
        let mut scope = Scope::new(3);
        let args: Vec<String> = ["issue", "comment", "4", "--body", "x"]
            .map(String::from)
            .to_vec();
        let why = answer_request(&args, None, REPO, &via, &mut scope).unwrap_err();
        assert!(
            why.contains("comments only on this cycle's issue #3"),
            "{why}"
        );
        assert_eq!(scope.run, 0);
        assert!(!ran.exists());
        assert_eq!(wrote(&w), vec![]);
    }

    /// A gh that writes what it was called with, its token and its standard
    /// input, and answers 0.
    fn stub_gh(d: &TestDir) -> PathBuf {
        let gh = d.join("gh");
        let log = d.join("gh.log");
        tehas_core::testing::write_exe(
            &gh,
            format!(
                "#!/bin/sh\nprintf '%s|' \"$@\" >>'{l}'\nprintf 'token=%s,%s\\n' \"$GH_TOKEN\" \"$TEHAS_GH_TOKEN\" >>'{l}'\ncat >>'{l}'\necho posted\n",
                l = log.display(),
            ),
        )
        .unwrap();
        gh
    }

    #[test]
    fn the_handle_is_given_a_gh_with_the_token_asked_for_this_request() {
        let d = TestDir::new("relay-gh-token");
        let gh = stub_gh(&d);
        let w = world();
        let site = FakeSite::new(&w);
        let via = Via {
            gh,
            token: stub_token(&d, "ghs_now"),
            token_args: vec![],
            token_wait: 10,
            writes: Box::new(move |mut gh| {
                assert_eq!(gh.run(&["probe"]), Ok("posted\n".to_string()));
                site.blueprint_writes(
                    &Actor::Ours(tehas_ports::Scope::Cycle),
                    Caller::new("relay"),
                )
                .unwrap()
            }),
        };
        let mut scope = Scope::new(3);
        let args: Vec<String> = ["issue", "comment", "3", "--body", "x"]
            .map(String::from)
            .to_vec();
        assert_eq!(
            answer_request(&args, None, REPO, &via, &mut scope).map(|r| r.2),
            Ok(0)
        );
        assert_eq!(read(d.join("gh.log")), "probe|token=ghs_now,ghs_now\n");
    }

    #[test]
    fn an_answer_no_one_read_is_removed_once_stale() {
        let d = TestDir::new("relay-stale");
        let relay = d.join("relay");
        fs::create_dir(&relay).unwrap();
        let outside = d.join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("keep"), "x").unwrap();
        for name in ["res.1", "res.2"] {
            fs::create_dir(relay.join(name)).unwrap();
            fs::write(relay.join(name).join("done"), "").unwrap();
        }
        std::os::unix::fs::symlink(&outside, relay.join("res.3")).unwrap();
        fs::create_dir(relay.join("req.4")).unwrap();
        let hour_ago = std::time::SystemTime::now() - Duration::from_secs(3600);
        for name in ["res.1", "req.4"] {
            fs::File::open(relay.join(name))
                .unwrap()
                .set_modified(hour_ago)
                .unwrap();
        }
        let top = open_dir(&relay).unwrap();
        remove_stale(&relay, &top, Duration::from_secs(60));
        assert!(!relay.join("res.1").exists());
        assert!(relay.join("res.2/done").exists());
        assert!(relay.join("req.4").exists());
        // The link is fresh by its own time; at age zero it goes, never followed.
        remove_stale(&relay, &top, Duration::ZERO);
        assert!(fs::symlink_metadata(relay.join("res.3")).is_err());
        assert!(outside.join("keep").exists());
    }
}

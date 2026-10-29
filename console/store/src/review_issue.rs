//! The review issue of one run: rendered by the rule of `tehas_core::review`,
//! masked on the way out, and filed. The filing decisions of `tehasbot review`
//! live here, beside the renderer they belong to (#508 put that glue in bash;
//! #952 took it out again), so the answers to "what does the issue say, is
//! there anything to file, and has this range been filed already" are Rust
//! functions with Rust tests beside the source.
//!
//!   review-issue <path> <branch> <commit>
//!     The issue's title and body, from the document of one review run, the
//!     body masked as an agent's words are masked, and the count of the
//!     findings the body lists. The rule of what is listed and how much of it
//!     is shown is tehas_core::review, where it is tested; this verb is the
//!     way the shell reaches it and the way the values arrive as one line the
//!     store's caller can read. The words of a finding come from the model, so
//!     both values are escaped by the line builder and the answer stays one
//!     line whatever a finding says.
//!
//!   review-file --repo <owner/name> --doc <path> --branch <b> --commit <sha>
//!               [--ask <line>] [--private <name>]...
//!     The filing itself: the render above, then the questions a filing has to
//!     answer - whether there is anything to file at all, whether the range is
//!     filed already as an open issue of `tehas:improvement` that a later
//!     run's findings go into as a comment, and what to title the issue that
//!     is filed instead. The two or three calls of GitHub are made here and
//!     nowhere else. `--ask` is the command's own last line of the body, put on
//!     after the masking so no finding can become it; `--private` names what
//!     belongs to the host and nobody else (its aliases, its model endpoints,
//!     the bot's email) for the same masking the console's log gets.
//!
//!     The body gh is asked for is written to `state/review.body` (0600)
//!     while the calls are made and taken away as the verb ends: gh is a
//!     program of the host, and a body of a model's words on a command line is
//!     readable by the host's other accounts. The answer says what happened:
//!     `findings` the count filed (0 when the run left none), `action` one of
//!     `issue`, `comment` or `nothing`, `number` the issue commented on,
//!     `said` the line the command prints when the filing itself is what to
//!     say, `why` the reason nothing was filed when a filing was asked for and
//!     GitHub refused, and `cost` how many programs ran - 0 when GitHub was
//!     asked nothing, 1 for the list and one call of the filing, 2 when the
//!     list refused as well as the filing. A filing GitHub refused is an
//!     answered filing, exit 0: the status of the run does not turn on it.
//!
//! Exit: 0 the document was read and the issue rendered, or the filing
//! answered; 64 the document is not a plain file, is over 1 MiB, is no JSON,
//! or is no object, or an argument is outside its grammar. A `<branch>` or
//! `<commit>` outside its grammar is 64 too, and is refused by `parse` before
//! the file is read; 78 the body of a filing could not be written down.

use crate::verbs;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// A review's document is one object with the findings of the six lenses. This
/// is far above it, and is the bound the verb agrees to read: the same bound
/// the import of the older records reads a file at.
const MAX_BYTES: u64 = 1 << 20;

/// What the findings of a filing may reach GitHub as. GitHub refuses a body of
/// more than 65 536 characters, and the line that asks for `tehas:refine` is
/// put on after them, so a body that runs past this bound asks for a filing
/// that cannot be posted and is tried again next week for nothing. The
/// renderer keeps each listed finding to a line of `MAX_CHARS` characters
/// (tehas_core::review), so this is the bound of a run of many findings, and
/// the same ceiling the questions of the factory are cut at.
const MAX_BODY: usize = 60_000;

/// The label of the issues of the review, the one label a filing gives. Never
/// `tehas:ready`: a finding starts no cycle until a person says so.
const LABEL: &str = "tehas:improvement";

/// What the title of a review issue starts with, which is how a filing
/// recognises its own among the open issues of the label. One string with the
/// renderer's title: `the_title_prefix_is_the_renderers` keeps the two so.
pub const TITLE_PREFIX: &str = "Six-lens review:";

/// The body file of a filing: beside the store's own file, in the directory of
/// 0700 the store keeps its database in, which no container is given
/// (AGENTS.md rule 4). `backup` copies that directory and no verb of the store
/// reads a file of it that it did not write.
const BODY_FILE: &str = "state/review.body";

/// The bound of one call of gh. A filing is at most three, made after the run
/// is over and its record written; a gh that will not answer is cut off rather
/// than left to hold the review's lock, as every bounded call of the factory
/// is. TEHAS_REVIEW_GH_WAIT (lib/config.sh) is the name of the wait; the
/// store's own bound of a call is the same shape and not read from a setting,
/// which the store never does.
const GH_WAIT: Duration = Duration::from_secs(30);

/// What the filing was given: the repository the settings named, the document
/// of the run, the two names of the title, the command's own line, the host's
/// names for the masking, and the program that speaks to GitHub (`gh` on a
/// host, where it is the shim of host/shim/gh that gives the call the App's
/// token; a test names a stub of its own, so the cases of the filing need no
/// GitHub and no token).
#[derive(Clone)]
pub struct Ask {
    pub repo: String,
    /// The program that speaks to GitHub. The command names it as `gh`, which
    /// is the factory's own shim on the path of a host (host/shim/gh), the one
    /// that gives the call the App's token; a test names one of its own.
    pub doc: PathBuf,
    pub branch: String,
    /// The full head of the range; the title and the lines name seven of it.
    pub commit: String,
    pub ask: String,
    /// The names of the host's own words, for the mask. The command's entry
    /// gives them to the mask once, before the verb runs (main.rs's
    /// ReviewFile), and they are kept here for the cases, which build an Ask
    /// of their own and set them from it.
    pub private: Vec<String>,
    pub gh: PathBuf,
}

/// The line of a refusal about the document of a run. `main` prints it with
/// the `store: ` the store's failures carry, so the word of the verb here is
/// what makes the line read `store: review-issue: ...`.
fn refuse(verb: &str, path: &Path, why: &str) -> (u8, String) {
    // The word names the verb that refused: the store's reader reads it and
    // holds the caller to the answer of the verb it asked (lib/store.sh).
    (64, format!("{verb}: {} {why}", path.display()))
}

/// The document of a run, read by the rule every host file the store reads
/// goes by, and parsed as far as the renderer asks. Both verbs share this, and
/// a filing reads the document exactly as the render of it does.
///
/// A document of no bytes at all is the document of a run whose container
/// wrote nothing: the console makes the file before the container fills it, so
/// emptiness is what a silent run leaves. It renders as a document that holds
/// no findings - the verb answers the line of a filing of nothing, and GitHub
/// is asked nothing - rather than a refusal, which is what the command has
/// always said about such a run (`the_empty_document_of_a_run_that_wrote_
/// nothing`, and the case of the run whose container wrote no document in
/// test/review_test.sh).
fn document(verb: &str, path: &Path) -> Result<serde_json::Value, (u8, String)> {
    let (bytes, _) = verbs::read_file(path, MAX_BYTES).map_err(|e| {
        refuse(
            verb,
            path,
            match e {
                verbs::FileErr::TooLarge => "is over 1 MiB",
                verbs::FileErr::Refused => "is not a plain file",
            },
        )
    })?;
    // No bytes at all is a run whose container wrote nothing: the console made
    // the file before the container filled it, and an empty one is what a run
    // that said nothing leaves. It is rendered as a document that holds no
    // findings, so the verb answers the line of a filing of nothing and GitHub
    // is asked nothing, which is what the command says and does. Any other
    // bytes that are no JSON are a document that is malformed, and refused.
    if bytes.is_empty() {
        return Ok(serde_json::Value::Object(serde_json::Map::new()));
    }
    // Bytes that are no UTF-8 are no JSON either, so they answer as a document
    // that does not parse, and one reader covers both.
    let text = String::from_utf8_lossy(&bytes);
    let document: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| refuse(verb, path, &format!("is not JSON: {e}")))?;
    // A document of the run is the object the runner answers with. An array,
    // a string or a number is no document, whatever it holds inside it.
    if !document.is_object() {
        return Err(refuse(verb, path, "is not a review document"));
    }
    Ok(document)
}

/// The words of the model made safe for a public issue: the patterns of the
/// mask over them, then the rules of `q_clean`, then the host's own names among
/// them. The console masks the document and the log of the run with the same
/// two functions before it writes either file (console/src/review.rs's
/// mask_file), so a run's words are made safe one way, in tehas_core, and a
/// filing does not invent a second rule for what goes to GitHub. The mask is
/// what takes a token, a password or a private address out; the rules of
/// q_clean are what escape a comment marker and a task box and disarm an
/// account name, which a mask does not touch and a public issue does.
pub fn mask(text: &str) -> String {
    tehas_core::question::q_clean(&tehas_core::mask::mask(text), usize::MAX)
}

/// The first seven characters of a commit, which is what the title names and
/// the command says. A `<commit>` is hex of 7 to 40 characters, so the cut
/// never falls in the middle of one.
fn short(commit: &str) -> &str {
    &commit[..commit.len().min(7)]
}

/// The findings the body lists: the lines the renderer prints one to a
/// finding. A body of nothing, which is what a document of no findings
/// renders, counts none, and a filing of none asks GitHub nothing.
fn count(body: &str) -> usize {
    body.lines().filter(|l| l.starts_with("- [")).count()
}

/// `review-issue`: the title, the masked body and the count, as one line.
pub fn run(path: &Path, branch: &str, commit: &str) -> Result<String, (u8, String)> {
    let document = document("review-issue", path)?;
    let issue = tehas_core::review::review_issue(&document, branch, short(commit));
    let body = mask(&issue.body);
    let findings = count(&body);
    Ok(verbs::line(
        "review-issue",
        &[
            ("title", verbs::j(issue.title)),
            ("body", verbs::j(body)),
            ("findings", verbs::j(findings)),
        ],
    ))
}

/// What one filing made of the run.
struct Filed {
    repo: String,
    /// The findings that were filed; 0 when the run left none.
    count: usize,
    /// `issue`, `comment` or `nothing`.
    action: &'static str,
    /// The issue commented on, when the findings went into one.
    number: Option<i64>,
    /// The line the command prints, when the filing is what to say.
    said: String,
    /// Why nothing was filed, when a filing was asked for and was refused.
    why: Option<String>,
    /// How many programs of the host ran.
    cost: usize,
}

impl Filed {
    fn line(&self) -> String {
        verbs::line(
            "review-file",
            &[
                ("repo", verbs::j(self.repo.clone())),
                ("findings", verbs::j(self.count)),
                ("action", verbs::j(self.action)),
                ("number", verbs::j(self.number)),
                ("said", verbs::j(self.said.clone())),
                ("why", verbs::j(self.why.clone())),
                ("cost", verbs::j(self.cost)),
            ],
        )
    }
}

/// What one program of the host gave: whether it said it succeeded, and what
/// it printed.
struct Ran {
    ok: bool,
    out: String,
}

/// Run one program with its arguments, its standard input closed and its
/// standard error dropped, under a bound: the two calls of a filing. Nothing
/// else of the host is run, and the arguments are built from what the verb was
/// given and never from the document.
fn run_bounded(program: &Path, args: &[String]) -> Ran {
    let made = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = made else {
        return Ran {
            ok: false,
            out: String::new(),
        };
    };
    // As timeout -k: the bound is a wall the call is held to, and one that
    // will not answer at it is not waited past. The factory's other bounds are
    // the same shape (console/src/review.rs's bounded, the dispatcher's run of
    // gh); nothing here holds a lock a second process wants, so a program that
    // outlives its bound is killed and reaped and no more is said of it.
    let began = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Ran {
                    ok: status.success(),
                    out: read(&mut child),
                }
            }
            Ok(None) if began.elapsed() < GH_WAIT => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Ran {
                    ok: false,
                    out: String::new(),
                };
            }
        }
    }
}

/// What a program printed, which is the list of the open issues and nothing
/// else: the two calls of the filing itself answer nothing a filing reads.
fn read(child: &mut std::process::Child) -> String {
    let mut out = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        let _ = pipe.read_to_string(&mut out);
    }
    out
}

/// `review-file`: the whole of one filing, whose answer is the store's line.
pub fn file(ask: &Ask, home: &Path) -> Result<String, (u8, String)> {
    let document = document("review-file", &ask.doc)?;
    let head = short(&ask.commit);
    let issue = tehas_core::review::review_issue(&document, &ask.branch, head);
    let found = count(&issue.body);
    // A run that found nothing files nothing, and GitHub is asked nothing at
    // all: no list, no issue, no comment, as the command has always had it.
    if found == 0 {
        return Ok(Filed {
            repo: ask.repo.clone(),
            count: 0,
            action: "nothing",
            number: None,
            said: format!("nothing to file: {head} left no findings"),
            why: None,
            cost: 0,
        }
        .line());
    }
    // The tehas:refine line is the command's own, so it goes on after the
    // masking and after the cut, and no finding can become it.
    let masked: String = mask(&issue.body).chars().take(MAX_BODY).collect();
    let body = format!("{masked}\n{}", ask.ask);
    let text = write_body(home, &body).map_err(|why| (78, format!("review-file: {why}")))?;
    let filed = file_with(ask, &text, found, &issue.title);
    // The body is the run's words, not the host's: it goes as the verb ends,
    // and with a gh that could not be run it never left the instance's state.
    let _ = std::fs::remove_file(&text);
    Ok(filed.line())
}

/// The body in a file of the instance's own, mode 0600, which is what gh is
/// given with `--body-file`. A body held in a shell variable has to reach gh on
/// a command line, where the host's other accounts read it; a file the store
/// wrote, in the 0700 directory the store's database already lives in, does
/// not.
fn write_body(home: &Path, body: &str) -> Result<PathBuf, String> {
    let state = home.join("state");
    std::fs::create_dir_all(&state).map_err(|e| format!("{}: {e}", state.display()))?;
    let file = home.join(BODY_FILE);
    let made = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&file);
    let wrote = match made {
        Ok(mut f) => f
            .write_all(body.as_bytes())
            .and_then(|()| f.sync_all())
            .map_err(|e| format!("{}: {e}", file.display())),
        Err(e) => Err(format!("{}: {e}", file.display())),
    };
    wrote.map(|()| file)
}

/// The names of the host as the filing was given them. The command's entry hands
/// them to the mask once, before the verb runs, because the mask holds one list
/// for the process and one review files one issue; the field is kept so that the
/// cases, which build an Ask of their own, can hand them the same way.
impl Ask {
    pub fn private_names(&self) -> &[String] {
        &self.private
    }
}

/// The list, and the one call of the filing. Split out of `file` so that the
/// cases of the filing name a program of their own and need no GitHub: the verb
/// is asked for `gh`, the shim of a host, and a case names a stub. The private
/// names are not read here: the command's entry gives them to the mask once,
/// before the verb runs (main.rs's ReviewFile), because the mask holds one list
/// for the process and a review files one issue.
fn file_with(ask: &Ask, body: &Path, found: usize, title: &str) -> Filed {
    // The list is always asked, and one call files the findings: a filing of a
    // run that has something to file costs the host two calls of it.
    let open = open_issue(ask);
    if let Some(number) = open {
        let args = gh_comment(ask, number, body);
        if run_bounded(&ask.gh, &args).ok {
            return Filed {
                repo: ask.repo.clone(),
                count: found,
                action: "comment",
                number: Some(number),
                said: format!(
                    "{found} findings are a comment on #{number} of {}",
                    ask.repo
                ),
                why: None,
                cost: 2,
            };
        }
        return Filed {
            repo: ask.repo.clone(),
            count: found,
            action: "nothing",
            number: Some(number),
            said: String::new(),
            why: Some(format!(
                "GitHub refused the comment on #{number} of {}",
                ask.repo
            )),
            cost: 2,
        };
    }
    // One issue per range, and the open one is the range's own. A gh that
    // would not answer the list is asked nothing more than the list: the
    // findings are filed as new, because a duplicate is read by a person and a
    // lost finding is not.
    let args = gh_create(ask, title, body);
    if run_bounded(&ask.gh, &args).ok {
        return Filed {
            repo: ask.repo.clone(),
            count: found,
            action: "issue",
            number: None,
            said: format!("{found} findings are filed in {}", ask.repo),
            why: None,
            cost: 2,
        };
    }
    // The list and the issue, both refused: the list first, then the issue that
    // was filed as new because the list would not say. A filing that got as far
    // as asking GitHub at all cost the host two calls of it, whatever answered.
    Filed {
        repo: ask.repo.clone(),
        count: found,
        action: "nothing",
        number: None,
        said: String::new(),
        why: Some(format!("GitHub refused the issue in {}", ask.repo)),
        cost: 2,
    }
}

/// The open issue of the review: the number of the first open issue of the
/// label whose title is the renderer's, or nothing when there is none and when
/// the list would not answer, which the filing reads as none.
fn open_issue(ask: &Ask) -> Option<i64> {
    let r = run_bounded(&ask.gh, &gh_list(ask));
    if !r.ok {
        return None;
    }
    let rows: Vec<serde_json::Value> = serde_json::from_str(&r.out).ok()?;
    rows.iter().find_map(|row| {
        let number = row.get("number").and_then(serde_json::Value::as_i64)?;
        let title = row.get("title").and_then(serde_json::Value::as_str)?;
        title.starts_with(TITLE_PREFIX).then_some(number)
    })
}

/// The list of the open issues of the review's label, by the same names the
/// command asked GitHub for, so GitHub's answer is the shape it was.
fn gh_list(ask: &Ask) -> Vec<String> {
    vec![
        "issue".into(),
        "list".into(),
        "--repo".into(),
        ask.repo.clone(),
        "--state".into(),
        "open".into(),
        "--label".into(),
        LABEL.into(),
        "--limit".into(),
        "100".into(),
        "--json".into(),
        "number,title".into(),
    ]
}

fn gh_comment(ask: &Ask, number: i64, body: &Path) -> Vec<String> {
    vec![
        "issue".into(),
        "comment".into(),
        number.to_string(),
        "--repo".into(),
        ask.repo.clone(),
        "--body-file".into(),
        body.display().to_string(),
    ]
}

fn gh_create(ask: &Ask, title: &str, body: &Path) -> Vec<String> {
    vec![
        "issue".into(),
        "create".into(),
        "--repo".into(),
        ask.repo.clone(),
        "--title".into(),
        title.to_string(),
        "--body-file".into(),
        body.display().to_string(),
        "--label".into(),
        LABEL.into(),
    ]
}

/// The file of the body, for the cases that ask what a filing wrote while its
/// calls were made; `file` takes it away again as the verb ends.
#[cfg(test)]
pub fn body_file(home: &Path) -> PathBuf {
    home.join(BODY_FILE)
}

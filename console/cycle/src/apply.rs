//! A cycle's report applied: the writes of `report`, in its order, with the
//! tries and bounds the bash report had. Every write to GitHub goes through the
//! action journal, and each try is a row of its own. Nothing decides a report
//! again and tehas:running left on an issue parks it for good, so a write the
//! journal cannot record is made with no row. Then the end on the cycle's
//! record, 5 tries, and what follows it. Run by `supervise` after tehas work,
//! with the cycle's lock held; a signal then is a flag nobody reads.

use crate::after::{self, log};
use crate::end_failed::Failure;
use crate::facts::Facts;
use crate::host::{self, Start};
use crate::last_attempt;
use crate::report::{report, Action, Ask, Park, Report};
use serde_json::Value;
use std::fs;
use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::Duration;
use tehas_core::call::{run_full, store_try, GH_BOUND, GRACE};
use tehas_core::end_fault;
use tehas_core::home::Home;
use tehas_core::journal::{settle as rules, Found, Journal, Journaled, RowId};
use tehas_core::question::is_handoff_marker;
use tehas_ports::{
    At, Blueprints, Bound, ChangeNo, CodeHost, Depth, Mark, NoBlobs, Outcome, Receipt,
    Sent as Passed, State, TicketKey, Tickets, Window, Write, Writes,
};

/// The comment a park posts when its question's label could not be added.
pub const BOXES: &str = "A reply to the choices above does nothing: the label `tehas:question` could not be added (run `tehas labels` on the host). To retry, apply `tehas:ready`.";

/// The read of a develop timeout's issue and the write of its note.
const NOTE_BOUND: Duration = Duration::from_secs(30);

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

/// The bounds of the report's calls to GitHub. After a stop every call is one
/// bounded try, and KILL: the report ignores TERM, and gh inherits that.
#[derive(Clone, Copy)]
struct Gh {
    tries: u32,
    retry_wait: Duration,
    bound: Duration,
    grace: Duration,
}

impl Gh {
    fn of(signalled: bool) -> Gh {
        let wait = env("TEHAS_CYCLE_RETRY_WAIT").parse().unwrap_or(5);
        if !signalled {
            return Gh {
                tries: 3,
                retry_wait: Duration::from_secs(wait),
                bound: GH_BOUND,
                grace: GRACE,
            };
        }
        Gh {
            tries: 1,
            retry_wait: Duration::ZERO,
            bound: Duration::from_secs(stop_wait()),
            grace: Duration::ZERO,
        }
    }
}

/// TEHAS_CYCLE_STOP_GH_WAIT, 20 when it is no number of one to four digits.
fn stop_wait() -> u64 {
    let v = env("TEHAS_CYCLE_STOP_GH_WAIT");
    let digits =
        (1..=4).contains(&v.len()) && !v.starts_with('0') && v.bytes().all(|b| b.is_ascii_digit());
    if digits {
        v.parse().unwrap_or(20)
    } else {
        20
    }
}

fn strs(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}

/// GitHub asked with gh, with the report's tries.
struct Tried {
    repo: String,
    issue: u64,
    gh: Gh,
    /// Whose pull requests the cycle's are: the app's login, as the
    /// dispatcher names it.
    author: String,
}

impl Tried {
    fn call(&self, args: &[String], bound: Duration, grace: Duration) -> Option<Value> {
        let r = run_full("gh", args, None, bound, grace, &[]);
        r.ok().then(|| serde_json::from_str(&r.out).ok()).flatten()
    }

    fn tried(&self, args: &[String], fit: fn(&Value) -> bool) -> Option<Value> {
        for t in 1..=self.gh.tries {
            let got = self.call(args, self.gh.bound, self.gh.grace).filter(fit);
            if got.is_some() {
                return got;
            }
            if t < self.gh.tries {
                std::thread::sleep(self.gh.retry_wait);
            }
        }
        None
    }

    fn issue_args(&self) -> Vec<String> {
        vec![
            "api".into(),
            format!("repos/{}/issues/{}", self.repo, self.issue),
        ]
    }
}

/// What the post of a park question came to.
enum Posted {
    Id(String),
    /// Posted, with an id the store cannot keep: a number under 2^53 is
    /// what it takes.
    Unnumbered,
    /// GitHub may have taken it (#1754).
    Maybe,
    No,
}

/// Whether the store keeps `id` as a comment's: digits, no leading zero,
/// under 2^53, as its comment_id reads them.
fn numbered(id: &str) -> bool {
    (1..=16).contains(&id.len())
        && !id.starts_with('0')
        && id.bytes().all(|b| b.is_ascii_digit())
        && id.parse::<u64>().is_ok_and(|n| n < 1 << 53)
}

/// A write GitHub may have taken and no read could settle: not sent again.
fn maybe_posted(n: u64, why: &str) {
    log(&format!(
        "WARN: a write on #{n} may have been posted{why}; no read could tell, so it is not sent again, and the next poll reads it"
    ));
}

/// Whether `body` carries the handoff of pi-rukas for `issue`: its first line
/// is the marker, and the marker of this issue alone. Every body tehas posts
/// with the marker puts it on the first line (`marker_line`, `q_render`), so a
/// real handoff counts; a body that only quotes a marker, or names another
/// issue, does not.
fn carries_handoff(body: &str, issue: u64) -> bool {
    body.lines()
        .next()
        .is_some_and(|l| is_handoff_marker(l, issue))
}

/// The author a cycle's pull requests are listed by: the bot's login, or the
/// token's own account while the login is not known.
fn author_of(bot: &str) -> String {
    if bot.trim().is_empty() {
        "@me".to_string()
    } else {
        bot.trim().to_string()
    }
}

fn pr_list(repo: &str, branch: &str, author: &str) -> Vec<String> {
    [
        "pr",
        "list",
        "--repo",
        repo,
        "--head",
        branch,
        "--author",
        author,
        "--state",
        "all",
        "--json",
        "url,state,body,headRefName,isDraft,createdAt",
        "--limit",
        "20",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

impl Ask for Tried {
    fn prs(&mut self, branch: &str) -> Option<Vec<Value>> {
        let args = pr_list(&self.repo, branch, &self.author);
        Some(self.tried(&args, Value::is_array)?.as_array()?.clone())
    }

    fn attention(&mut self) -> Option<bool> {
        let v = self.tried(&self.issue_args(), Value::is_object)?;
        Some(v.get("labels").and_then(Value::as_array).is_some_and(|l| {
            l.iter()
                .any(|x| x.get("name").and_then(Value::as_str) == Some("needs-human-attention"))
        }))
    }

    fn issue(&mut self) -> Option<Value> {
        self.call(&self.issue_args(), NOTE_BOUND, GRACE)
    }
}

/// The ports a report's writes go through, and the reads that settle a
/// write the site may have taken. The program's wiring makes them.
pub struct Handles {
    pub writes: Box<dyn Writes>,
    pub tickets: Box<dyn Tickets>,
    /// Where a ticket is its own blueprint, its notes are the blueprint's
    /// comments.
    pub blueprints: Box<dyn Blueprints>,
    pub code_host: Box<dyn CodeHost>,
}

/// The wiring's handles of repository `repo`, each call bounded by `bound`
/// with `grace` after TERM.
pub type HandlesOf = fn(repo: &str, bound: Duration, grace: Duration) -> Handles;

/// How a write of the ports ended.
enum Sent {
    Done(Receipt),
    Failed(Outcome),
    /// The site may have taken it; its row, unless it went with none.
    Unknown(Outcome, Option<RowId>),
}

/// The site may have taken a write that did not answer as made.
fn maybe(e: &Outcome) -> bool {
    matches!(
        e,
        Outcome::Unavailable {
            sent: Passed::Maybe,
            ..
        } | Outcome::Unread(_)
    )
}

/// Marks as the log says them.
fn said(remove: &[Mark], add: &[Mark]) -> String {
    let words = |ms: &[Mark]| {
        ms.iter()
            .map(|m| format!("{m:?}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    match (remove.is_empty(), add.is_empty()) {
        (false, false) => format!("off {}, on {}", words(remove), words(add)),
        (false, true) => format!("off {}", words(remove)),
        _ => format!("on {}", words(add)),
    }
}

/// The writes of one report.
struct Writer<'a> {
    /// The ports the writes go through.
    ports: Handles,
    home: &'a Path,
    repo: String,
    issue: u64,
    attempt: u64,
    gh: Gh,
    before: i64,
    poll_at: i64,
    no_row: bool,
    /// The journal failed a write. The rest go with no row: a hung store
    /// must not spend the time a stop leaves.
    unjournaled: bool,
    /// The label stayed: the next comment says so.
    stuck: bool,
    /// A comment that carries the handoff of pi-rukas was posted.
    handed_off: bool,
    /// What gh said of the last write that failed all its tries.
    last_failed: Failure,
    /// What gh said of the comment with the handoff that was not posted.
    handoff_failed: Option<Failure>,
    /// The comment with the handoff may have been posted: no read could tell.
    handoff_maybe: bool,
}

impl Writer<'_> {
    /// One write of the ports through the journal, or with no row once the
    /// journal failed: a hung store must not spend the time a stop leaves.
    fn send(&mut self, write: &Write) -> Sent {
        let home = self.home;
        let mut store =
            |a: &[&str]| store_try(3, Duration::from_secs(1), || host::store_value(home, a));
        if !self.unjournaled {
            let mut journal = Journal {
                repo: &self.repo,
                gh: "gh",
                before: self.before,
                bound: self.gh.bound,
                grace: self.gh.grace,
                token: None,
                poll_at: &mut self.poll_at,
                row_not_begun: &mut self.no_row,
                cycle: Some((self.issue, self.attempt)),
            };
            match journal.send(&mut store, write, &mut *self.ports.writes, &NoBlobs) {
                Journaled::Answered(Ok(r)) => return Sent::Done(r),
                Journaled::Answered(Err(e)) => return Sent::Failed(e),
                Journaled::Unknown { answer, row } => return Sent::Unknown(answer, Some(row)),
                Journaled::Unrecorded => {}
            }
            self.unjournaled = true;
            log(&format!(
                "WARN: the journal will not record the report of #{}: its writes go to GitHub with no row",
                self.issue
            ));
        }
        match self.ports.writes.send(write, &NoBlobs) {
            Ok(r) => Sent::Done(r),
            Err(e) if maybe(&e) => Sent::Unknown(e, None),
            Err(e) => Sent::Failed(e),
        }
    }

    /// Whether a write the site may have taken is there, read after the
    /// wait between tries by the rule of its kind in core. None: no read
    /// could tell.
    fn found(&mut self, write: &Write, since: i64) -> Option<Found> {
        std::thread::sleep(self.gh.retry_wait);
        let since = At::new(since);
        let bound = Bound::new(100).expect("100 is a bound");
        match write {
            Write::Mark { ticket, .. } | Write::Unmark { ticket, .. } => {
                let (_, digest) = self
                    .ports
                    .tickets
                    .several(std::slice::from_ref(ticket))
                    .pop()?;
                rules::by_marks(write, &digest.ok()?.marks)
            }
            Write::TicketNote { ticket, .. } => {
                let notes = match ticket.same_item() {
                    Some(no) => self
                        .ports
                        .blueprints
                        .comments(no, bound)
                        .ok()?
                        .items()
                        .to_vec(),
                    None => {
                        let last = std::num::NonZeroU32::new(100).expect("100 is no zero");
                        self.ports
                            .tickets
                            .notes(ticket, Window::Last(last))
                            .ok()?
                            .items
                    }
                };
                rules::by_comments(write, &notes, since)
            }
            Write::MarkChange { no, .. } | Write::UnmarkChange { no, .. } => {
                // Its digest alone: the whole change answers Cut past its
                // bound of files or comments, and its marks would never
                // settle.
                let filter = tehas_ports::ChangeFilter {
                    states: tehas_ports::ChangeStates::All,
                    numbers: vec![*no],
                    ..Default::default()
                };
                let changes = self.ports.code_host.changes(&filter, bound).ok()?;
                let digest = changes.items().iter().find(|d| d.no == *no)?;
                rules::by_marks(write, &digest.marks)
            }
            Write::CommentChange { no, .. } => {
                let change = self.ports.code_host.change(*no, bound).ok()?;
                rules::by_comments(write, &change.comments, since)
            }
            _ => None,
        }
    }

    /// A write of the ports tried as the bash tried it, `warn` logged when
    /// all tries fail. One the site may have taken is read before it is sent
    /// again, and its row settled by what the read found: Some(true) when it
    /// is there, None when no read could tell (#1754). `n` is the item it
    /// writes to, for the log.
    fn tried_send(&mut self, warn: &str, n: u64, write: &Write) -> (Option<bool>, Option<Receipt>) {
        let home = self.home;
        for t in 1..=self.gh.tries {
            let since = tehas_core::time::now();
            match self.send(write) {
                Sent::Done(r) => return (Some(true), Some(r)),
                Sent::Failed(why) => self.last_failed = Failure::Answered(why),
                Sent::Unknown(why, row) => {
                    let said = format!(": {}", why.line());
                    self.last_failed = Failure::Answered(why);
                    let Some(found) = self.found(write, since) else {
                        maybe_posted(n, &said);
                        return (None, None);
                    };
                    if let Some(row) = row {
                        let mut store = |a: &[&str]| {
                            store_try(3, Duration::from_secs(1), || host::store_value(home, a))
                        };
                        let _ = Journal::settle(&mut store, row, found);
                    }
                    if found == Found::Landed {
                        return (Some(true), None);
                    }
                    continue;
                }
            }
            if t < self.gh.tries {
                std::thread::sleep(self.gh.retry_wait);
            }
        }
        log(warn);
        (Some(false), None)
    }

    fn ticket(&self) -> TicketKey {
        TicketKey::Number(std::num::NonZeroU64::new(self.issue).expect("an issue is no zero"))
    }

    /// The issue's marks, each taken off then each put on, a row each. One
    /// that fails all its tries stops the rest.
    fn relabel(&mut self, remove: &[Mark], add: &[Mark]) -> bool {
        let warn = format!(
            "WARN: label change on #{} failed: {}",
            self.issue,
            said(remove, add)
        );
        let ticket = self.ticket();
        let writes = remove
            .iter()
            .map(|m| Write::Unmark {
                ticket: ticket.clone(),
                mark: m.clone(),
            })
            .chain(add.iter().map(|m| Write::Mark {
                ticket: ticket.clone(),
                mark: m.clone(),
            }));
        for w in writes.collect::<Vec<_>>() {
            if self.tried_send(&warn, self.issue, &w).0 != Some(true) {
                return false;
            }
        }
        true
    }

    fn comment(&mut self, body: &str) {
        self.note(body, Vec::new());
    }

    /// A note on the issue, its markers beside its body.
    fn note(&mut self, body: &str, markers: Vec<tehas_ports::Marker>) {
        let warn = format!("WARN: comment on #{} failed", self.issue);
        let note = Write::TicketNote {
            ticket: self.ticket(),
            body: tehas_ports::Body::Text(body.to_string()),
            markers,
        };
        let (posted, _) = self.tried_send(&warn, self.issue, &note);
        self.handed(posted, body);
    }

    /// A body posted with pi-rukas's handoff marker carries its handoff. One
    /// that may have been posted is no failure of the host's.
    fn handed(&mut self, posted: Option<bool>, body: &str) {
        if !carries_handoff(body, self.issue) {
            return;
        }
        match posted {
            Some(true) => self.handed_off = true,
            Some(false) => self.handoff_failed = Some(self.last_failed.clone()),
            None => self.handoff_maybe = true,
        }
    }

    fn park(&mut self, p: &mut Park, f: &Facts) {
        let n = self.issue;
        let running = [Mark::State(State::Running)];
        let attention = [Mark::State(State::NeedsPerson)];
        if self.relabel(&running, &attention) {
            self.stuck = false;
        }
        // On record before the issue is told: a question whose park is not on
        // record would offer boxes that start nothing.
        let mut park = vec!["park"];
        park.extend(strs(&p.args));
        let on_record = match host::store_try(self.home, 3, &park) {
            Ok(_) => true,
            Err(e) => {
                log(&format!(
                    "WARN: could not record the park of #{n} ({}: {}), so it asks no question",
                    e.word, e.line
                ));
                false
            }
        };
        self.last_attempt(p, f);
        // A park with nothing for a person to decide (no retry to offer)
        // says so in a comment, and asks nothing.
        if on_record && p.asks() {
            if let Some((full, text, markers)) = p.question_parts(n, self.stuck) {
                match self.post_question(&text, markers) {
                    Posted::Id(id) => {
                        let offers = tehas_core::question::park_offers(!p.no_retry, p.comments)
                            .iter()
                            .map(|c| c.word())
                            .collect::<Vec<_>>()
                            .join(",");
                        self.asked(&id, &full, &offers);
                        return;
                    }
                    // Posted, but the store keeps no question of an id that is
                    // not its number: nothing more is said, and no mark makes
                    // the poll wait on a record that is not there.
                    Posted::Unnumbered => {
                        log(&format!(
                            "WARN: the park question of #{n} got a comment id the store cannot keep, so it is not on record and no reply to it counts"
                        ));
                        return;
                    }
                    // The question may be there: a plain comment would say the
                    // park twice.
                    Posted::Maybe => return,
                    Posted::No => {}
                }
            }
        }
        let (said, markers) = p.comment_parts(self.stuck, on_record);
        self.note(&said, markers);
    }

    /// A develop timeout leaves its note on the issue, as a marker beside the
    /// text, at the revision read; a second one on the same text offers no
    /// retry. The park is on record before the issue is read. An issue that
    /// cannot be read changes nothing.
    fn last_attempt(&mut self, p: &mut Park, f: &Facts) {
        if !p.develop {
            return;
        }
        let n = self.issue;
        let key = self.ticket();
        // The ticket's revision moves with a comment or a mark too: an edit
        // that lost to one reads again and tries once more.
        for _ in 0..2 {
            // Its whole text: a cut one would give the note a wrong base.
            let one = std::num::NonZeroU32::new(1).expect("1 is no zero");
            let Ok(t) = self
                .ports
                .tickets
                .ticket(&key, Depth::Last { notes: one }, None)
            else {
                return;
            };
            let result = f.file("result.json").text();
            let note = match last_attempt::of_ticket(
                n,
                &t.text,
                &t.markers,
                result,
                f.work_state.as_ref(),
            ) {
                Some(last_attempt::Parts::Repeat) => {
                    p.no_retry = true;
                    p.say = last_attempt::REPEAT.to_string();
                    return;
                }
                Some(last_attempt::Parts::Note(note)) => note,
                _ => return,
            };
            let edit = Write::EditTicket {
                ticket: key.clone(),
                title: None,
                text: None,
                markers: tehas_ports::MarkerEdit {
                    set: vec![tehas_ports::Marker::LastAttempt(note)],
                    drop: vec![],
                },
                since: t.digest.revision,
            };
            match self.send(&edit) {
                Sent::Done(_) => return,
                Sent::Failed(Outcome::Conflict(tehas_ports::Lost::Changed)) => continue,
                _ => break,
            }
        }
        log(&format!(
            "WARN: could not leave the last attempt's note in the body of #{n}"
        ));
    }

    /// The question posted as a note, its markers beside its text: its id,
    /// or whether it may have been posted. One the site may have taken is
    /// looked for in the issue's notes by its rule, and its row settled.
    fn post_question(&mut self, text: &str, markers: Vec<tehas_ports::Marker>) -> Posted {
        let n = self.issue;
        let note = Write::TicketNote {
            ticket: self.ticket(),
            body: tehas_ports::Body::Text(text.to_string()),
            markers,
        };
        let since = tehas_core::time::now();
        let id = match self.send(&note) {
            Sent::Done(Receipt::Comment(p)) => p.id.as_str().to_string(),
            Sent::Done(_) => String::new(),
            Sent::Failed(why) => {
                self.last_failed = Failure::Answered(why);
                return Posted::No;
            }
            Sent::Unknown(why, row) => {
                std::thread::sleep(self.gh.retry_wait);
                let found = self.landed_note(&note, since);
                if let (Some(row), Some(f)) = (row, &found) {
                    let home = self.home;
                    let mut store = |a: &[&str]| {
                        store_try(3, Duration::from_secs(1), || host::store_value(home, a))
                    };
                    let landed = if f.is_some() {
                        Found::Landed
                    } else {
                        Found::NotSent
                    };
                    let _ = Journal::settle(&mut store, row, landed);
                }
                match found {
                    Some(Some(id)) => id,
                    // Not there: the plain comment, as a refused post gets.
                    Some(None) => return Posted::No,
                    None => {
                        maybe_posted(n, &format!(": {}", why.line()));
                        self.last_failed = Failure::Answered(why);
                        self.handed(None, text);
                        return Posted::Maybe;
                    }
                }
            }
        };
        self.handed(Some(true), text);
        if numbered(&id) {
            Posted::Id(id)
        } else {
            Posted::Unnumbered
        }
    }

    /// The id of the note `note` asks for, as its rule finds it in the
    /// issue's notes since `since`: Some(None) when it is not there, None
    /// when no read could tell.
    fn landed_note(&mut self, note: &Write, since: i64) -> Option<Option<String>> {
        let Write::TicketNote { ticket, .. } = note else {
            return None;
        };
        let notes = self.notes_of(ticket)?;
        let at = At::new(since);
        Some(
            notes
                .iter()
                .find(|c| {
                    rules::by_comments(note, std::slice::from_ref(c), at) == Some(Found::Landed)
                })
                .map(|c| c.id.as_str().to_string()),
        )
    }

    /// The notes of a ticket: where a ticket is its own blueprint, its
    /// blueprint's comments.
    fn notes_of(&mut self, ticket: &TicketKey) -> Option<Vec<tehas_ports::Comment>> {
        let bound = Bound::new(100).expect("100 is a bound");
        Some(match ticket.same_item() {
            Some(no) => self
                .ports
                .blueprints
                .comments(no, bound)
                .ok()?
                .items()
                .to_vec(),
            None => {
                let last = std::num::NonZeroU32::new(100).expect("100 is no zero");
                self.ports
                    .tickets
                    .notes(ticket, Window::Last(last))
                    .ok()?
                    .items
            }
        })
    }

    /// On record before the label shows it: a tick counts only in the
    /// question on record. The poll reads questions only on issues with the
    /// label, so without it the boxes do nothing: say so.
    fn asked(&mut self, id: &str, body: &str, offers: &str) {
        let (home, n) = (self.home, self.issue.to_string());
        let sum = crate::last_attempt::sha(body);
        let asked = [
            "question-asked",
            &n,
            "--id",
            id,
            "--kind",
            "park",
            "--body-sha",
            &sum,
            "--offers",
            offers,
        ];
        if let Err(e) = host::store_try(home, 3, &asked) {
            log(&format!(
                "WARN: could not record the park question of #{n}, a reply to it will not count ({})",
                e.word
            ));
        }
        if !self.relabel(&[], &[Mark::State(State::Question)]) {
            let withdraw = ["question-answered", &n, "--id", id, "--withdraw"];
            if let Err(e) = host::store_try(home, 3, &withdraw) {
                if e.word != "unknown" {
                    log(&format!(
                        "WARN: could not take the park question of #{n} off the record ({})",
                        e.word
                    ));
                }
            }
            self.comment(BOXES);
        }
    }

    fn pr_comment(&mut self, pr: &str, body: &str) {
        let Some(no) = change_no(pr) else {
            log(&format!(
                "WARN: comment on {pr} failed: it names no pull request"
            ));
            return;
        };
        let comment = Write::CommentChange {
            no,
            body: tehas_ports::Body::Text(body.to_string()),
            markers: vec![],
        };
        let warn = format!("WARN: comment on {pr} failed");
        let (posted, _) = self.tried_send(&warn, no.get(), &comment);
        self.handed(posted, body);
    }

    /// The refreshing mark off the pull request a refresh marked.
    fn refresh_off(&mut self, pr: &str) {
        let Some(no) = change_no(pr) else {
            log(&format!(
                "WARN: label change on pull request #{pr} failed: it names no pull request"
            ));
            return;
        };
        let off = Write::UnmarkChange {
            no,
            mark: Mark::State(State::Refreshing),
        };
        let warn =
            format!("WARN: label change on pull request #{pr} failed: off State(Refreshing)");
        self.tried_send(&warn, no.get(), &off);
    }
}

/// The pull request a report names: its number, or the last part of its
/// address.
fn change_no(pr: &str) -> Option<ChangeNo> {
    let n = pr.trim_end_matches('/').rsplit('/').next()?;
    ChangeNo::new(n.parse().ok()?)
}

/// The driver word of an approval row: the store writes a row with its inputs
/// under `input` and the word at `input.driver` (console/store/src/approval.rs,
/// `approval_json`), and `approval-view` writes one with the word on its own.
fn driver_of(a: &Value) -> Option<&str> {
    a.get("driver")
        .and_then(Value::as_str)
        .or_else(|| a["input"].get("driver").and_then(Value::as_str))
}

/// Whether the cycle's approval bound the lean driver: the store keeps the
/// driver with the approval's inputs, and writes it only for lean, so a record
/// with none anywhere is classic. The copy of the driver's record is kept for a
/// lean run alone.
///
/// The facts hold the raw answer of `approval-current`, which names the two
/// rows of the issue's latest start: `approval`, an unused one, and `current`,
/// the one a run took. Either may be the row that bound the driver, so both are
/// read. The `view` shape is that of `approval-view`, `{at, how, driver}`, which
/// a reader of the published approval holds instead. An answer that is none of
/// these - null, or one that holds the store's error - is classic: the cycle
/// keeps no record rather than guessing at one.
pub fn approved_lean(facts: &Value) -> bool {
    let approval = facts.get("approval");
    [
        approval,
        approval.and_then(|a| a.get("approval")),
        approval.and_then(|a| a.get("current")),
        approval.and_then(|a| a.get("view")),
    ]
    .into_iter()
    .flatten()
    .any(|a| driver_of(a) == Some("lean"))
}

/// The driver's own record of a lean run, kept with the cycle's other notes:
/// the copy of `.pi/lean-state/<issue>.json` of the workspace, taken before the
/// workspace is cleaned and read by `analytics run` for the steps the classic
/// history holds no account of. The name of the note, said once.
pub const LEAN_STATE: &str = "lean-state.json";

/// The driver's record of a lean run, copied out of its workspace beside the
/// cycle's other notes, before anything of the workspace is removed. The copy is
/// made by the one rule the protocol's files have: the file is a plain file of
/// the workspace and no link on the way, and of a sane size. The driver writes a
/// new file each attempt, so what is there now is this run's. None, with a
/// warning, when there is nothing to keep or it cannot be kept: a run is
/// recorded with no lean figures rather than not at all.
fn keep_lean_state(home: &Path, workdir: &str, issue: u64, lean: bool) {
    // The note exists only to feed the analytics, so on a host that has it off
    // nothing is written.
    if !lean || !crate::after::analytics_on(home) {
        return;
    }
    let to = Home::new(home).cycle_note(issue, LEAN_STATE);
    let _ = fs::remove_file(&to);
    // The read of the workspace's own file, which refuses a link anywhere on the
    // way and a file of no sane size; the copy is of what that read answered.
    let Some(kept) = crate::last_attempt::lean(Path::new(workdir), issue) else {
        return;
    };
    if !write_private(&to, &kept.to_string()) {
        log(&format!(
            "WARN: the lean record of #{issue} is not with its run"
        ));
    }
}

/// A file of `text`, made for its owner and no one else, and removed again when
/// the write did not finish. The directory is the cycle's notes, which its start
/// makes, so a write into one that is not there is a host out of order and no
/// directory is made here.
fn write_private(to: &Path, text: &str) -> bool {
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    opts.mode(0o600);
    let Ok(mut f) = opts.open(to) else {
        return false;
    };
    if f.write_all(text.as_bytes()).is_err() {
        let _ = fs::remove_file(to);
        return false;
    }
    f.flush().is_ok()
}

/// `rm -rf`: gone or never there.
fn remove_all(p: &str) -> bool {
    match fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => fs::remove_dir_all(p).is_ok(),
        Ok(_) => fs::remove_file(p).is_ok(),
        Err(_) => true,
    }
}

/// The cycle's images and its own state: only the container read them. The
/// workspace stays for a person; what its agents wrote beside it does not.
fn clear_workspace(workdir: &str) {
    let marked = Path::new(workdir)
        .parent()
        .is_some_and(|p| p.join(".tehas-workspaces").is_file());
    if !marked {
        return;
    }
    for (ext, what) in [(".images", "the images"), (".state", "the cycle's state")] {
        let p = format!("{workdir}{ext}");
        if !remove_all(&p) {
            log(&format!("WARN: could not remove {what} {p}"));
        }
    }
}

/// The notes of the protocol, read once: the next cycle of the issue must not
/// take them for its own. The driver's kept record of a lean run is not one of
/// them: the run of the analytics reads it, and the next cycle takes what is
/// left as it starts (src/supervise.rs), which is the one point reached
/// however the cycle before it ended.
fn clear_notes(home: &Path, issue: u64) {
    for name in tehas_core::cycle_notes::ENDED {
        let _ = fs::remove_file(Home::new(home).cycle_note(issue, name));
    }
}

/// Why tehas work refused the start, as the cycle's last notice: `cycle-end`
/// keeps it, and the factory's line says it.
fn refused_notice(home: &Path, f: &Facts, why: &str) {
    let (issue, attempt) = (f.issue.to_string(), f.attempt.to_string());
    let args = [
        "cycle-heartbeat",
        &issue,
        "--attempt",
        &attempt,
        "--ended",
        "--status",
        "refused",
        "--text",
        why,
    ];
    if let Err(e) = host::store_try(home, 3, &args) {
        log(&format!(
            "WARN: why #{issue} was refused is not on record ({}: {})",
            e.word, e.line
        ));
    }
}

/// `store cycle-end`, with the paths of the result and the progress in their
/// place. False when the end is not on record.
fn end(home: &Path, r: &Report, f: &Facts) -> bool {
    let issue = f.issue;
    let cycles = Home::new(home).cycles_dir();
    let mut args: Vec<String> = r.end[..r.files_at].to_vec();
    for k in &r.files {
        args.push(format!("--{k}"));
        args.push(
            cycles
                .join(format!("{issue}.{k}.json"))
                .to_string_lossy()
                .into_owned(),
        );
    }
    args.extend_from_slice(&r.end[r.files_at..]);
    let mut call = vec!["cycle-end"];
    call.extend(strs(&args));
    match host::store_try(home, 5, &call) {
        Ok(v) => {
            for k in ["result", "progress"] {
                let kept = v.get(k).and_then(Value::as_str).unwrap_or("");
                if ["refused", "too-large", "unreadable", "stale"].contains(&kept) {
                    log(&format!(
                        "WARN: the {k} of #{issue} is not on record: {kept}"
                    ));
                }
            }
            true
        }
        Err(e) => {
            log(&format!(
                "ERROR: the end of #{issue} is not on record ({}: {}): cycle-end {}",
                e.word,
                e.line,
                args.join(" ")
            ));
            false
        }
    }
}

/// The record of a failed GitHub step of this end (#1726), or none: an end
/// with no failed step removes the one an earlier cycle of the issue left.
/// False when the store of records is not as this end left it: a record that
/// is not written is never tried again nor shown, and one that is not removed
/// is.
fn keep_end_fault(
    home: &Path,
    f: &Facts,
    a: &after::After,
    handoff: Option<&Failure>,
    now: i64,
) -> bool {
    let issue = f.issue;
    let handoff = handoff.filter(|_| a.ended.handoff_delivered == Some(false));
    let left = crate::end_failed::Left {
        issue,
        attempt: a.attempt,
        result: f.file("result.json").text(),
        progress: f.file("progress.json").text(),
        pr: !a.ended.pr.is_empty(),
        handoff_failed: handoff,
    };
    let kept = match crate::end_failed::of(&left, now) {
        Some(r) => {
            log(&format!("#{issue}: {}: on record", r.failure()));
            end_fault::write(home, &r)
                .map_err(|why| format!("the failed end of #{issue} is not on record: {why}"))
        }
        None => end_fault::remove(home, issue).map_err(|why| {
            format!("the failed end of an earlier cycle of #{issue} is still on record: {why}")
        }),
    };
    match kept {
        Ok(()) => true,
        Err(e) => {
            log(&format!("ERROR: {e}"));
            false
        }
    }
}

/// Report the cycle `s`. The exit status of the cycle: 0, or 70 when its end
/// or the record of its failed end is not on record.
/// How a report's writes go: to which issue and attempt, with what tries
/// and bounds, and the gh of the writes that do not go through the ports yet.
pub struct How {
    pub repo: String,
    pub issue: u64,
    pub attempt: u64,
    /// When the report began: the read its writes rest on.
    pub now: i64,
    pub tries: u32,
    pub retry_wait: Duration,
    pub bound: Duration,
    pub grace: Duration,
    /// The workspace a tidy removes.
    pub workdir: String,
}

/// What a report's writes left for its end.
pub struct Written {
    /// A comment that carries the handoff of pi-rukas was posted.
    pub handed_off: bool,
    /// What the site said of the comment with the handoff that was not
    /// posted.
    pub handoff_failed: Option<Failure>,
    /// The comment with the handoff may have been posted: no read could tell.
    pub handoff_maybe: bool,
}

/// The writes of report `r`, in its order, through `ports`, each a row of
/// the journal of the store under `home`.
pub fn write(home: &Path, how: &How, r: &mut Report, facts: &Facts, ports: Handles) -> Written {
    let mut w = Writer {
        ports,
        home,
        repo: how.repo.clone(),
        issue: how.issue,
        attempt: how.attempt,
        gh: Gh {
            tries: how.tries,
            retry_wait: how.retry_wait,
            bound: how.bound,
            grace: how.grace,
        },
        before: how.now,
        poll_at: 0,
        no_row: false,
        unjournaled: false,
        stuck: true,
        handed_off: false,
        last_failed: Failure::Said(String::new()),
        handoff_failed: None,
        handoff_maybe: false,
    };
    for a in &mut r.actions {
        match a {
            Action::Relabel { remove, add } => {
                if w.relabel(remove, add) {
                    w.stuck = false;
                }
            }
            Action::Comment { ok, stuck } => {
                let said = if w.stuck { stuck.clone() } else { ok.clone() };
                w.comment(&said);
            }
            Action::Park(p) => w.park(p, facts),
            Action::PrComment { pr, body } => w.pr_comment(pr, body),
            Action::Tidy { remove, warn } => {
                if !warn.is_empty() {
                    log(warn);
                } else {
                    // Every path is tried, whichever of them fails.
                    let mut ok = true;
                    for p in remove {
                        ok &= remove_all(p);
                    }
                    if !ok {
                        log(&format!(
                            "WARN: could not remove the workspace {}",
                            how.workdir
                        ));
                    }
                }
            }
            Action::RefreshOff { pr } => w.refresh_off(pr),
        }
    }
    Written {
        handed_off: w.handed_off,
        handoff_failed: w.handoff_failed,
        handoff_maybe: w.handoff_maybe,
    }
}

pub fn run(home: &Path, s: &Start, handles: HandlesOf) -> i32 {
    reported(home, s, handles).rc
}

/// What a report leaves for the hooks that follow it.
pub struct Reported {
    /// The exit status of the cycle, as [`run`] gives it.
    pub rc: i32,
    /// The pull request the report names, open or merged.
    pub pr: Option<u64>,
    /// tehas work refused the start before its runner ran.
    pub refused: bool,
}

/// The number of a pull request's URL, `.../pull/<n>`.
fn pull_number(url: &str) -> Option<u64> {
    let (rest, n) = url.trim_end_matches('/').rsplit_once('/')?;
    if !rest.ends_with("/pull") || n.is_empty() || n.starts_with('0') || n.len() > 10 {
        return None;
    }
    n.parse().ok()
}

/// [`run`], and what the hooks need of the report.
pub fn reported(home: &Path, s: &Start, handles: HandlesOf) -> Reported {
    let issue = s.issue;
    let workdir = host::workdir(s);
    let now = s.now.unwrap_or_else(tehas_core::time::now);
    let facts_json = host::facts(home, s);
    // The driver's record goes with the run before the workspace is cleared of
    // what only its container wrote: the record is the only account of the
    // driver's own steps, and nothing else reads it later.
    keep_lean_state(home, &workdir, issue, approved_lean(&facts_json));
    clear_workspace(&workdir);
    let facts = match Facts::from_json(&facts_json) {
        Ok(f) => f,
        Err(e) => {
            log(&format!("ERROR: the end of #{issue} cannot be read: {e}"));
            return Reported {
                rc: 70,
                pr: None,
                refused: false,
            };
        }
    };
    let gh = Gh::of(s.signalled);
    let mut ask = Tried {
        repo: facts.repo.clone(),
        issue,
        gh,
        author: author_of(&std::env::var("TEHAS_BOT_LOGIN").unwrap_or_default()),
    };
    let mut r = report(&facts, &mut ask);
    clear_notes(home, issue);
    for line in &r.log {
        log(line);
    }
    let how = How {
        repo: facts.repo.clone(),
        issue,
        attempt: s.attempt,
        now,
        tries: gh.tries,
        retry_wait: gh.retry_wait,
        bound: gh.bound,
        grace: gh.grace,
        workdir: workdir.clone(),
    };
    let ports = handles(&facts.repo, gh.bound, gh.grace);
    let w = write(home, &how, &mut r, &facts, ports);
    log(&r.done);
    let approved = after::approved(home, issue, s.attempt);
    if !r.refused_why.is_empty() {
        refused_notice(home, &facts, &r.refused_why);
    }
    let on_record = end(home, &r, &facts);
    let mut a = r.after.clone();
    let delivered = &mut a.ended.handoff_delivered;
    *delivered = delivered.map(|d| d || w.handed_off);
    if *delivered == Some(false) && w.handoff_maybe && w.handoff_failed.is_none() {
        log(&format!(
            "WARN: the handoff of #{issue} may not have reached a comment: the next poll reads it"
        ));
    } else if *delivered == Some(false) {
        log(&format!(
            "WARN: the handoff of #{issue} reached no comment: it is only in the run log"
        ));
    }
    let fault_kept = keep_end_fault(home, &facts, &a, w.handoff_failed.as_ref(), now);
    a.approved = approved;
    a.console = s.console.clone();
    a.status_cmd = s.status_cmd.clone();
    a.status_args = s.status_args.clone();
    a.workdir = workdir;
    after::after(home, &a);
    Reported {
        rc: if on_record && fault_kept { 0 } else { 70 },
        pr: pull_number(&r.after.ended.pr),
        refused: !r.refused_why.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::{approved_lean, author_of, carries_handoff, pr_list, pull_number};
    use serde_json::json;

    #[test]
    fn the_number_of_a_pull_request_is_read_off_its_url() {
        assert_eq!(pull_number("https://github.com/a/b/pull/12"), Some(12));
        assert_eq!(pull_number("https://github.com/a/b/pull/12/"), Some(12));
        assert_eq!(pull_number("https://github.com/a/b/issues/12"), None);
        assert_eq!(pull_number("https://github.com/a/b/pull/012"), None);
        assert_eq!(pull_number(""), None);
    }

    #[test]
    fn a_handoff_counts_only_by_its_marker_on_the_first_line() {
        let marker = "<!-- pi-rukas:handoff issue=7 cap=verify-failed:develop -->";
        assert!(carries_handoff(
            &format!("{marker}\nTehas cycle handed off."),
            7
        ));
        // Quoted below the first line, the marker is text the agent wrote.
        assert!(!carries_handoff(
            "Tehas cycle handed off.\n\nIt wrote:\n{marker}",
            7
        ));
        // Another issue's marker is no handoff of this one.
        assert!(!carries_handoff(
            &format!("{marker}\nTehas cycle handed off."),
            8
        ));
        assert!(!carries_handoff("", 7));
    }

    #[test]
    fn the_cycles_pull_requests_are_listed_by_the_bots_login() {
        let args = pr_list(
            "acme/widget",
            "feature/issue-7-a",
            &author_of("tehas-bot[bot]"),
        );
        let at = args.iter().position(|a| a == "--author").unwrap();
        assert_eq!(args[at + 1], "tehas-bot[bot]");
        assert_eq!(author_of(""), "@me");
        // createdAt is what keeps an earlier attempt's pull request out of
        // this cycle's report (#1747), so the call has to ask for it.
        let json = args.iter().position(|a| a == "--json").unwrap();
        assert_eq!(
            args[json + 1],
            "url,state,body,headRefName,isDraft,createdAt"
        );
    }

    #[test]
    fn the_approval_of_the_start_says_whether_the_run_went_lean() {
        // The raw answer of `approval-current`: the row of the latest start
        // under `current`, its inputs under `input`, the driver only for lean.
        let current = |driver: Option<&str>| {
            let input = match driver {
                Some(d) => json!({"driver": d}),
                None => json!({}),
            };
            json!({"approval": {
                "api": 1, "verb": "approval-current", "ok": true,
                "current": {"at": "900", "how": "approved", "input": input}
            }})
        };
        assert!(approved_lean(&current(Some("lean"))));
        assert!(!approved_lean(&current(Some("classic"))));
        assert!(!approved_lean(&current(None)));
        // An approval taken by a run: the same row under `approval`, which is
        // what the facts hold when the start's row is still unused.
        let unused = |driver: &str| {
            json!({"approval": {
                "api": 1, "verb": "approval-current", "ok": true,
                "approval": {"at": "900", "how": "approved", "input": {"driver": driver}}
            }})
        };
        assert!(approved_lean(&unused("lean")));
        assert!(!approved_lean(&unused("classic")));
        // The approval-view's shape, which holds the word on its own.
        assert!(approved_lean(
            &json!({"approval": {"view": {"at": "9", "how": "approved", "driver": "lean"}}})
        ));
        assert!(!approved_lean(
            &json!({"approval": {"view": {"at": "9", "how": "approved"}}})
        ));
        // No answer at all, or one of the store's errors, is classic: no record
        // is kept on a guess.
        assert!(!approved_lean(&json!({})));
        assert!(!approved_lean(&json!({"approval": null})));
        assert!(!approved_lean(&json!({"approval": {"error": "store"}})));
        // A word of another type, or of another shape, is no driver.
        assert!(!approved_lean(
            &json!({"approval": {"input": {"driver": true}}})
        ));
    }
}

/// The report's writes of the ports, on the ports' fake: each a row of its
/// own in the journal of a stub store, tried as the bash tried them, and a
/// write the site may have taken read back by its kind's rule before it is
/// sent again.
#[cfg(test)]
mod ports_tests {
    use super::*;
    use crate::report::report;
    use std::path::PathBuf;
    use tehas_core::testdir::TestDir;
    use tehas_ports::fake::{
        BlueprintsOp, Clock, CodeHostOp, FakeSite, Fault, Faulty, Op, TicketsOp, World,
    };
    use tehas_ports::{Actor, Blobs, Caller, Got, Refusal, Scope, Site};

    /// A store that logs its calls and answers each as the real one does.
    const STORE: &str = r#"#!/usr/bin/env bash
d=$(dirname "$0")
printf 'store %s\n' "$*" >>"$d/../calls.log"
if [ -f "$d/../store-fail" ] && [[ "$*" == $(cat "$d/../store-fail")* ]]; then
  echo "store: refused #7: not now" >&2
  exit 1
fi
case $1 in
  poll-begin) echo '{"api":1,"verb":"poll-begin","ok":true,"at":100}' ;;
  journal-record) n=$(($(cat "$d/../n" 2>/dev/null || echo 0) + 1)); echo "$n" >"$d/../n"
    echo "{\"api\":1,\"verb\":\"journal-record\",\"ok\":true,\"action\":$n}" ;;
  *) echo "{\"api\":1,\"verb\":\"$1\",\"ok\":true}" ;;
esac
"#;

    /// A port write, said beside the store's calls so their order shows. A
    /// comment it posts gets a number from 4001 on, as GitHub's ids are,
    /// unless `numbered` is false.
    struct Logged {
        inner: Box<dyn Writes>,
        log: PathBuf,
        numbered: bool,
        next: u64,
        /// The first edit of a ticket loses, as one another change beat.
        conflict_once: bool,
    }

    fn said_write(w: &Write) -> String {
        match w {
            Write::Mark { mark, .. } => format!("mark {mark:?}"),
            Write::Unmark { mark, .. } => format!("unmark {mark:?}"),
            Write::TicketNote { body, markers, .. } if markers.is_empty() => {
                format!("note {body:?}")
            }
            Write::TicketNote { body, markers, .. } => {
                let kinds: Vec<_> = markers.iter().map(|m| m.kind()).collect();
                format!("note {body:?} markers {kinds:?}")
            }
            Write::CommentChange { no, body, .. } => {
                format!("change-comment {} {body:?}", no.get())
            }
            Write::UnmarkChange { no, mark } => format!("change-unmark {} {mark:?}", no.get()),
            other => format!("{:?}", other.kind()),
        }
    }

    impl Writes for Logged {
        fn send(&mut self, w: &Write, files: &dyn Blobs) -> Got<Receipt> {
            let mut f = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.log)
                .unwrap();
            writeln!(f, "write {}", said_write(w)).unwrap();
            if matches!(w, Write::EditTicket { .. }) && std::mem::take(&mut self.conflict_once) {
                return Err(Outcome::Conflict(tehas_ports::Lost::Changed));
            }
            let mut got = self.inner.send(w, files);
            if let (true, Ok(Receipt::Comment(p))) = (self.numbered, &mut got) {
                self.next += 1;
                p.id = tehas_ports::CommentId::new((4000 + self.next).to_string());
            }
            got
        }
        fn read_only(&self) -> bool {
            false
        }
    }

    struct Host {
        dir: TestDir,
    }

    impl Host {
        fn new(name: &str) -> Host {
            let dir = TestDir::new(&format!("apply-ports-{name}"));
            fs::create_dir_all(dir.join("libexec")).unwrap();
            tehas_core::testing::write_exe(dir.join("libexec/store"), STORE).unwrap();
            Host { dir }
        }

        fn calls(&self) -> Vec<String> {
            fs::read_to_string(self.dir.join("calls.log"))
                .unwrap_or_default()
                .lines()
                .map(str::to_string)
                .collect()
        }

        fn count(&self, what: &str) -> usize {
            self.calls().iter().filter(|c| c.starts_with(what)).count()
        }

        /// The handles of `site`, the factory's own, every write logged and
        /// every comment numbered.
        fn handles(&self, site: &impl Site) -> Handles {
            self.handles_numbered(site, true)
        }

        fn handles_numbered(&self, site: &impl Site, numbered: bool) -> Handles {
            let (by, c) = (Actor::Ours(Scope::Full), Caller::new("report"));
            let writes = tehas_ports::Writer {
                tickets: site.ticket_writes(&by, c).unwrap(),
                blueprints: site.blueprint_writes(&by, c).unwrap(),
                code_host: site.code_host_writes(&by, c).unwrap(),
            };
            Handles {
                writes: Box::new(Logged {
                    inner: Box::new(writes),
                    log: self.dir.join("calls.log"),
                    numbered,
                    next: 0,
                    conflict_once: self.dir.join("conflict-once").exists(),
                }),
                tickets: site.tickets(&by, c).unwrap(),
                blueprints: site.blueprints(&by, c).unwrap(),
                code_host: site.code_host(&by, c).unwrap(),
            }
        }

        /// The report's writer of issue 7 on `site`, three tries with no
        /// wait between them.
        fn writer(&self, site: &impl Site) -> Writer<'_> {
            Writer {
                ports: self.handles(site),
                home: self.dir.path(),
                repo: "a/b".to_string(),
                issue: 7,
                attempt: 1,
                gh: Gh {
                    tries: 3,
                    retry_wait: Duration::ZERO,
                    bound: Duration::from_secs(10),
                    grace: Duration::ZERO,
                },
                before: 5,
                poll_at: 0,
                no_row: false,
                unjournaled: false,
                stuck: true,
                handed_off: false,
                last_failed: Failure::Said(String::new()),
                handoff_failed: None,
                handoff_maybe: false,
            }
        }
    }

    fn running() -> Mark {
        Mark::State(State::Running)
    }

    fn attention() -> Mark {
        Mark::State(State::NeedsPerson)
    }

    /// Issue 7, running, and pull request 9 of its branch, refreshing. The
    /// site's clock reads the host's, as a row's time is the host's.
    fn world() -> World {
        let now = Clock::new(At::new(tehas_core::time::now()));
        World::new()
            .on_clock(now)
            .ticket(7, |t| t.title("the issue").mark(running()))
            .change(9, "feature/issue-7-a", |c| {
                c.mark(Mark::State(State::Refreshing))
            })
    }

    fn marks_of_7(w: &World) -> Vec<Mark> {
        let site = FakeSite::new(w);
        let mut t = site.tickets(&Actor::Operator, Caller::new("test")).unwrap();
        let key = TicketKey::Number(std::num::NonZeroU64::new(7).unwrap());
        t.several(&[key]).pop().unwrap().1.unwrap().marks
    }

    fn notes_of_7(w: &World) -> usize {
        let site = FakeSite::new(w);
        let mut b = site
            .blueprints(&Actor::Operator, Caller::new("test"))
            .unwrap();
        b.comments(
            tehas_ports::BlueprintNo::new(7).unwrap(),
            Bound::new(50).unwrap(),
        )
        .unwrap()
        .items()
        .len()
    }

    #[test]
    fn a_change_of_marks_goes_mark_by_mark_each_a_row_named_by_its_cycle() {
        let h = Host::new("marks");
        let w = world();
        let mut writer = h.writer(&FakeSite::new(&w));
        assert!(writer.relabel(&[running()], &[attention()]));
        assert_eq!(marks_of_7(&w), vec![attention()]);
        let order: Vec<String> = h
            .calls()
            .into_iter()
            .filter(|c| c.starts_with("write") || c.starts_with("store journal-record"))
            .map(|c| c.split(" --").next().unwrap().to_string())
            .collect();
        assert_eq!(
            order,
            [
                "store journal-record",
                "write unmark State(Running)",
                "store journal-record",
                "write mark State(NeedsPerson)",
            ]
        );
        let record = h
            .calls()
            .into_iter()
            .find(|c| c.starts_with("store journal-record"))
            .unwrap();
        assert!(
            record.contains(r#""cycle":{"attempt":1,"issue":7}"#),
            "{record}"
        );
        assert!(record.contains("--write"), "{record}");
    }

    #[test]
    fn a_mark_the_site_refuses_is_tried_three_times_each_a_row_and_stops_the_rest() {
        let h = Host::new("marks-refused");
        let w = world();
        let site = Faulty::new(FakeSite::new(&w)).on(
            Op::Tickets(TicketsOp::Unmark),
            Fault::Refused(Refusal::Invalid),
        );
        let mut writer = h.writer(&site);
        assert!(!writer.relabel(&[running()], &[attention()]));
        assert_eq!(h.count("write unmark"), 3);
        assert_eq!(h.count("store journal-record"), 3);
        assert_eq!(h.count("write mark"), 0);
        assert_eq!(
            writer.last_failed,
            Failure::Answered(Outcome::Refused(Refusal::Invalid))
        );
        assert_eq!(marks_of_7(&w), vec![running()]);
    }

    #[test]
    fn a_note_the_site_took_without_answering_is_read_back_and_not_sent_again() {
        let h = Host::new("note-took");
        let w = world();
        let site = Faulty::new(FakeSite::new(&w))
            .on(
                Op::Tickets(TicketsOp::Note),
                Fault::TimeoutMaybe { applied: true },
            )
            .times(1);
        let mut writer = h.writer(&site);
        writer.comment("Tehas cycle stopped.");
        assert_eq!(h.count("write note"), 1);
        assert_eq!(notes_of_7(&w), 1);
        let settles: Vec<String> = h
            .calls()
            .into_iter()
            .filter(|c| c.starts_with("store journal-settle"))
            .collect();
        assert_eq!(
            settles,
            [
                "store journal-settle --action 1 --result unknown",
                "store journal-settle --action 1 --result ok",
            ]
        );
    }

    #[test]
    fn a_note_the_site_did_not_take_is_closed_as_not_sent_and_sent_again() {
        let h = Host::new("note-lost");
        let w = world();
        let site = Faulty::new(FakeSite::new(&w))
            .on(
                Op::Tickets(TicketsOp::Note),
                Fault::TimeoutMaybe { applied: false },
            )
            .times(1);
        let mut writer = h.writer(&site);
        writer.comment("Tehas cycle stopped.");
        assert_eq!(h.count("write note"), 2);
        assert_eq!(notes_of_7(&w), 1);
        assert!(h
            .calls()
            .contains(&"store journal-settle --action 1 --abandoned".to_string()));
        assert!(h
            .calls()
            .contains(&"store journal-settle --action 2 --result ok".to_string()));
    }

    #[test]
    fn a_note_no_read_can_settle_is_left_unknown_and_not_sent_again() {
        let h = Host::new("note-unread");
        let w = world();
        let site = Faulty::new(FakeSite::new(&w))
            .on(
                Op::Tickets(TicketsOp::Note),
                Fault::TimeoutMaybe { applied: true },
            )
            .on(Op::Blueprints(BlueprintsOp::Comments), Fault::Unreachable);
        let mut writer = h.writer(&site);
        writer.comment("<!-- pi-rukas:handoff issue=7 cap=x -->\nHanded off.");
        assert_eq!(h.count("write note"), 1);
        assert_eq!(h.count("store journal-settle"), 1);
        assert!(writer.handoff_maybe);
    }

    #[test]
    fn a_write_the_journal_cannot_record_is_made_with_no_row() {
        let h = Host::new("unrecorded");
        fs::write(h.dir.join("store-fail"), "journal-record").unwrap();
        let w = world();
        let mut writer = h.writer(&FakeSite::new(&w));
        writer.comment("Tehas cycle stopped.");
        assert!(writer.unjournaled);
        assert_eq!(h.count("write note"), 1);
        assert_eq!(notes_of_7(&w), 1);
        assert_eq!(h.count("store journal-settle"), 0);
    }

    #[test]
    fn a_comment_on_the_pull_request_and_its_refreshing_mark_go_to_the_change() {
        let h = Host::new("change");
        let w = world();
        let mut writer = h.writer(&FakeSite::new(&w));
        writer.pr_comment("https://github.com/a/b/pull/9", "Refreshed.");
        writer.refresh_off("9");
        assert_eq!(h.count("write change-comment 9"), 1);
        assert_eq!(h.count("write change-unmark 9 State(Refreshing)"), 1);
        let site = FakeSite::new(&w);
        let mut c = site
            .code_host(&Actor::Operator, Caller::new("test"))
            .unwrap();
        let change = c
            .change(ChangeNo::new(9).unwrap(), Bound::new(50).unwrap())
            .unwrap();
        assert!(change.digest.marks.is_empty());
        assert_eq!(change.comments.len(), 1);
    }

    #[test]
    fn a_refreshing_mark_the_site_may_have_taken_off_is_read_on_the_change() {
        let h = Host::new("change-unread");
        let w = world();
        let site = Faulty::new(FakeSite::new(&w))
            .on(
                Op::CodeHost(CodeHostOp::UnmarkChange),
                Fault::Server { applied: true },
            )
            .times(1);
        let mut writer = h.writer(&site);
        writer.refresh_off("9");
        assert_eq!(h.count("write change-unmark"), 1);
        assert!(h
            .calls()
            .contains(&"store journal-settle --action 1 --result ok".to_string()));
    }

    #[test]
    fn a_pull_request_no_number_names_is_said_and_nothing_is_sent() {
        let h = Host::new("change-none");
        let w = world();
        let mut writer = h.writer(&FakeSite::new(&w));
        writer.refresh_off("x");
        writer.pr_comment("not a link", "Refreshed.");
        assert_eq!(h.count("write"), 0);
        assert_eq!(h.count("store"), 0);
    }

    /// Park case 5 of the fixtures, a step failed for an ordinary reason,
    /// written on `site` with `tries` tries.
    fn park(h: &Host, site: &impl Site, tries: u32) -> Written {
        case(h, site, "park", 5, tries)
    }

    /// Case `n` of fixture suite `suite`, its report written on `site`.
    fn case(h: &Host, site: &impl Site, suite: &str, n: usize, tries: u32) -> Written {
        case_with(h, site, suite, n, tries, true)
    }

    /// [`case`], its comments numbered or not.
    fn case_with(
        h: &Host,
        site: &impl Site,
        suite: &str,
        n: usize,
        tries: u32,
        numbered: bool,
    ) -> Written {
        let path = format!("{}/tests/fixtures/{suite}.json", env!("CARGO_MANIFEST_DIR"));
        let cases: Vec<Value> = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        let c = &cases[n];
        let facts = Facts::from_json(&c["facts"]).unwrap();
        let mut ask = crate::report::Given(c["facts"]["github"].clone());
        let mut r = report(&facts, &mut ask);
        let how = How {
            repo: facts.repo.clone(),
            issue: 7,
            attempt: 1,
            now: 5,
            tries,
            retry_wait: Duration::ZERO,
            bound: Duration::from_secs(10),
            grace: Duration::ZERO,
            workdir: h.dir.join("work").to_string_lossy().into_owned(),
        };
        write(
            h.dir.path(),
            &how,
            &mut r,
            &facts,
            h.handles_numbered(site, numbered),
        )
    }

    /// The calls that start with one of `of`, cut at the first ` --` or ` <<<`.
    fn order(h: &Host, of: &[&str]) -> Vec<String> {
        h.calls()
            .into_iter()
            .filter(|c| of.iter().any(|o| c.starts_with(o)))
            .map(|c| {
                let marked = c.ends_with("markers [Park, Question]");
                let c = c.split(" <<<").next().unwrap();
                let c = c.split(" Text(").next().unwrap();
                let c = c.split(" --").next().unwrap();
                if marked {
                    format!("{c} question")
                } else {
                    c.to_string()
                }
            })
            .collect()
    }

    #[test]
    fn a_park_goes_marks_record_question_asked_mark() {
        let h = Host::new("park");
        let w = world();
        park(&h, &FakeSite::new(&w), 3);
        assert_eq!(
            order(&h, &["write", "store park", "store question"]),
            [
                "write unmark State(Running)",
                "write mark State(NeedsPerson)",
                "store park 7",
                "write note question",
                "store question-asked 7",
                "write mark State(Question)",
            ]
        );
        assert_eq!(
            marks_of_7(&w),
            vec![attention(), Mark::State(State::Question)]
        );
    }

    #[test]
    fn a_handoff_that_waits_for_a_merge_marks_the_issue_and_says_so_on_both() {
        let h = Host::new("handoff");
        let w = world();
        case(&h, &FakeSite::new(&w), "handoff", 6, 3);
        assert_eq!(
            order(&h, &["write"]),
            [
                "write unmark State(Running)",
                "write mark State(AwaitingMerge)",
                "write note",
                "write change-comment 9",
            ]
        );
        assert!(h
            .calls()
            .iter()
            .any(|c| c.starts_with("write note Text(\"Tehas cycle finished:")));
        assert_eq!(marks_of_7(&w), vec![Mark::State(State::AwaitingMerge)]);
    }

    #[test]
    fn a_journal_that_fails_at_any_step_is_asked_no_more_and_said_once() {
        for verb in ["poll-begin", "journal-record", "journal-attempt"] {
            let h = Host::new(&format!("journal-{verb}"));
            fs::write(h.dir.join("store-fail"), verb).unwrap();
            let w = world();
            let mut writer = h.writer(&FakeSite::new(&w));
            assert!(writer.relabel(&[running()], &[attention()]));
            writer.comment("Tehas cycle stopped.");
            assert!(writer.unjournaled, "{verb}");
            assert_eq!(
                h.count(&format!("store {verb}")),
                1,
                "{verb}: {:#?}",
                h.calls()
            );
            assert_eq!(h.count("write"), 3, "{verb}");
            assert_eq!(marks_of_7(&w), vec![attention()], "{verb}");
        }
    }

    #[test]
    fn the_question_goes_with_its_markers_beside_it_and_its_record_keeps_its_ids_and_sum() {
        let h = Host::new("park-question");
        let w = world();
        park(&h, &FakeSite::new(&w), 3);
        let calls = h.calls();
        let q = calls
            .iter()
            .find(|c| c.ends_with("markers [Park, Question]"))
            .unwrap();
        assert!(
            !q.contains("tehas:park") && !q.contains("tehas:question"),
            "{q}"
        );
        let asked = calls
            .iter()
            .find(|c| c.starts_with("store question-asked"))
            .unwrap();
        assert!(asked.contains("--id 4001"), "{asked}");
        assert!(asked.contains("--body-sha "), "{asked}");
    }

    #[test]
    fn a_question_whose_id_the_store_cannot_keep_is_not_recorded_and_nothing_more_is_said() {
        let h = Host::new("park-unnumbered");
        let w = world();
        // The fake's own ids, "comment-3", are no number.
        case_with(&h, &FakeSite::new(&w), "park", 5, 3, false);
        assert_eq!(h.count("store question-asked"), 0);
        assert_eq!(h.count("write mark State(Question)"), 0);
        assert_eq!(h.count("write note"), 1, "{:#?}", h.calls());
        assert_eq!(marks_of_7(&w), vec![attention()]);
    }

    #[test]
    fn a_develop_timeout_leaves_its_note_beside_the_text_at_the_revision_read() {
        let h = Host::new("park-last-attempt");
        let w = world();
        case(&h, &FakeSite::new(&w), "park", 70, 3);
        let edit = h
            .calls()
            .into_iter()
            .find(|c| c.starts_with("write EditTicket"))
            .expect("the note is written");
        assert!(!edit.is_empty());
        let site = FakeSite::new(&w);
        let mut t = site.tickets(&Actor::Operator, Caller::new("test")).unwrap();
        let key = TicketKey::Number(std::num::NonZeroU64::new(7).unwrap());
        let one = std::num::NonZeroU32::new(1).unwrap();
        let read = t.ticket(&key, Depth::Last { notes: one }, None).unwrap();
        let note = read
            .markers
            .iter()
            .find_map(|m| match m {
                tehas_ports::Marker::LastAttempt(l) => Some(l.clone()),
                _ => None,
            })
            .expect("the ticket carries the note");
        assert!(
            note.note
                .starts_with("### The last attempt ran out of time"),
            "{}",
            note.note
        );
        assert_eq!(
            note.base.as_str(),
            crate::last_attempt::sha(read.text.trim())
        );
        // The text itself is not touched.
        assert!(!read.text.contains("last attempt"), "{}", read.text);
    }

    #[test]
    fn the_note_of_a_long_body_takes_the_sum_of_all_of_it() {
        let h = Host::new("park-last-attempt-long");
        // A ticket of a long body: the note's base is the sum of all of it.
        let long = "word ".repeat(200);
        let w = World::new()
            .on_clock(Clock::new(At::new(tehas_core::time::now())))
            .ticket(7, |t| t.title("the issue").text(&long).mark(running()));
        case(&h, &FakeSite::new(&w), "park", 70, 3);
        let site = FakeSite::new(&w);
        let mut t = site.tickets(&Actor::Operator, Caller::new("test")).unwrap();
        let key = TicketKey::Number(std::num::NonZeroU64::new(7).unwrap());
        let one = std::num::NonZeroU32::new(1).unwrap();
        let read = t.ticket(&key, Depth::Last { notes: one }, None).unwrap();
        let base = read
            .markers
            .iter()
            .find_map(|m| match m {
                tehas_ports::Marker::LastAttempt(l) => Some(l.base.as_str().to_string()),
                _ => None,
            })
            .expect("the note is on");
        assert_eq!(base, crate::last_attempt::sha(long.trim()));
    }

    #[test]
    fn a_note_that_lost_to_another_change_of_the_issue_is_read_again_and_tried_once_more() {
        let h = Host::new("park-last-attempt-conflict");
        fs::write(h.dir.join("conflict-once"), "").unwrap();
        let w = world();
        case(&h, &FakeSite::new(&w), "park", 70, 3);
        assert_eq!(h.count("write EditTicket"), 2, "{:#?}", h.calls());
        let site = FakeSite::new(&w);
        let mut t = site.tickets(&Actor::Operator, Caller::new("test")).unwrap();
        let key = TicketKey::Number(std::num::NonZeroU64::new(7).unwrap());
        let one = std::num::NonZeroU32::new(1).unwrap();
        let read = t.ticket(&key, Depth::Last { notes: one }, None).unwrap();
        assert!(read
            .markers
            .iter()
            .any(|m| matches!(m, tehas_ports::Marker::LastAttempt(_))));
    }

    #[test]
    fn a_second_develop_timeout_on_the_same_text_offers_no_retry() {
        let h = Host::new("park-last-attempt-repeat");
        let w = world();
        case(&h, &FakeSite::new(&w), "park", 70, 3);
        let h2 = Host::new("park-last-attempt-repeat-2");
        case(&h2, &FakeSite::new(&w), "park", 70, 3);
        assert_eq!(h2.count("write EditTicket"), 0, "{:#?}", h2.calls());
        let said = h2
            .calls()
            .into_iter()
            .find(|c| c.starts_with("write note"))
            .unwrap();
        assert!(said.contains("twice on the same text"), "{said}");
        assert!(!said.ends_with("markers [Park, Question]"), "{said}");
    }

    #[test]
    fn a_mark_that_stayed_is_said_in_the_question() {
        let h = Host::new("park-stuck");
        let w = world();
        let site = Faulty::new(FakeSite::new(&w)).on(
            Op::Tickets(TicketsOp::Unmark),
            Fault::Refused(Refusal::Invalid),
        );
        park(&h, &site, 3);
        assert_eq!(h.count("write unmark"), 3);
        let q = h
            .calls()
            .into_iter()
            .find(|c| c.ends_with("markers [Park, Question]"))
            .unwrap();
        assert!(q.contains("could not be changed"), "{q}");
    }

    #[test]
    fn a_question_whose_mark_fails_is_taken_off_the_record_and_says_so() {
        let h = Host::new("park-withdraw");
        let w = world();
        // The park's own mark goes on; the question's does not.
        let site = Faulty::new(FakeSite::new(&w))
            .on(
                Op::Tickets(TicketsOp::Mark),
                Fault::Refused(Refusal::Invalid),
            )
            .after(1);
        park(&h, &site, 3);
        let calls = order(&h, &["store question", "write note"]);
        assert_eq!(
            calls,
            [
                "write note question",
                "store question-asked 7",
                "store question-answered 7",
                "write note",
            ]
        );
        let boxes = format!("write note Text({BOXES:?})");
        assert!(h.calls().contains(&boxes), "{:#?}", h.calls());
    }

    #[test]
    fn a_park_the_store_will_not_take_asks_no_question() {
        let h = Host::new("park-unrecorded");
        fs::write(h.dir.join("store-fail"), "park 7").unwrap();
        let w = world();
        park(&h, &FakeSite::new(&w), 3);
        assert!(!h
            .calls()
            .iter()
            .any(|c| c.ends_with("markers [Park, Question]")));
        let said = h
            .calls()
            .into_iter()
            .find(|c| c.starts_with("write note"))
            .unwrap();
        assert!(said.contains("To retry, apply `tehas:ready`."), "{said}");
        // The park's marker goes beside the text, not in it.
        assert!(said.ends_with("markers [Park]"), "{said}");
        assert!(!said.contains("tehas:park"), "{said}");
    }

    #[test]
    fn after_a_stop_each_write_is_one_try() {
        let h = Host::new("park-stop");
        let w = world();
        let site =
            Faulty::new(FakeSite::new(&w)).on(Op::Tickets(TicketsOp::Unmark), Fault::Timeout);
        park(&h, &site, 1);
        assert_eq!(h.count("write unmark"), 1);
    }

    #[test]
    fn a_handoff_the_site_would_not_take_keeps_its_answer_for_the_end() {
        let h = Host::new("handoff-failed");
        let w = world();
        let failed = Outcome::Unavailable {
            why: tehas_ports::Passing::Server,
            sent: Passed::No,
        };
        let site = Faulty::new(FakeSite::new(&w)).on(
            Op::Tickets(TicketsOp::Note),
            Fault::Server { applied: false },
        );
        let mut writer = h.writer(&site);
        writer.comment("<!-- pi-rukas:handoff issue=7 cap=x -->\nHanded off.");
        assert!(!writer.handed_off);
        assert!(
            matches!(
                writer.handoff_failed,
                Some(Failure::Answered(Outcome::Unavailable { .. }))
            ),
            "{:?} (not {failed:?})",
            writer.handoff_failed
        );
    }

    #[test]
    fn a_write_with_no_row_keeps_its_tries() {
        let h = Host::new("no-row-tries");
        fs::write(h.dir.join("store-fail"), "journal-record").unwrap();
        let w = world();
        let site = Faulty::new(FakeSite::new(&w)).on(
            Op::Tickets(TicketsOp::Note),
            Fault::Refused(Refusal::Invalid),
        );
        let mut writer = h.writer(&site);
        writer.comment("Tehas cycle stopped.");
        assert_eq!(h.count("write note"), 3);
        assert_eq!(h.count("store journal-settle"), 0);
    }
}

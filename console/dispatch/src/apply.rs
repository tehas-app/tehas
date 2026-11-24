//! apply: what a decision's actions do, in their order, as the bash dispatcher
//! does them. Every write to GitHub goes through the action journal of the
//! state store (section 5 of its spec): intended, attempted, then settled
//! with its typed result. A write the journal could not record is not made:
//! the next poll finds the same state and decides it again.
//!
//! This part applies the writes that stand alone: the label changes with
//! the comment, log line and answer that follow one that worked, the
//! unticks, the refreshing mark taken off a pull request, the told and said
//! records, the files of the hold, the gate and the branch check, and the
//! containers left behind. And the starts (start.rs), which end the poll.
//! And the stops (stop.rs): a quiet cycle, one whose work is done, and the
//! sweep. And the planning turns: their end (plan.rs) and their start
//! (turn.rs), with the spends of a park's retry.

mod gate;
mod plan;
mod settle;
pub mod start;
mod stop;
mod turn;

use crate::read::run::{run, Calls};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Duration;
use tehas_core::call::{self, GRACE};
use tehas_core::home::{Home, State};
use tehas_core::journal::{Journal, Journaled, Wrote};
use tehas_ports::{Body, NoBlobs, Receipt, TicketKey, Write};

/// The calls of one poll's writes.
pub struct Writer {
    pub repo: String,
    pub home: PathBuf,
    pub gh: String,
    pub docker: String,
    /// The store's calls, journal and records.
    pub calls: Calls,
    /// The poll's row, from poll-begin: what each action names. 0 until the
    /// first write begins it.
    pub poll_at: i64,
    /// The store was asked for the poll's row and gave no time for it: the
    /// poll says so once, in the line its whole owes, where apply() ends.
    pub row_not_begun: bool,
    /// The read the actions rest on.
    pub before: i64,
    pub bound: Duration,
    /// How long `tehas_store_try` waits before it asks a busy store again.
    /// Production: 1 s, as the bash command sleeps.
    pub store_retry: Duration,
    pub host: start::Host,
    /// Set by an action that ends the poll.
    pub ended: Option<i32>,
    /// A refine's park, spent by the turn that follows once its brief is
    /// written.
    pub spend: Option<u64>,
    /// Set by a turn: the status command runs after it, as after a start.
    pub status_due: bool,
    /// The poll changed the queue, so its end refreshes the status and takes
    /// a sample: the poll wrote to GitHub, started or stopped something, or
    /// ended on an action that could not be read. Marked where the change is
    /// made - write(), the two stops here - or where the poll ends inside a
    /// step. An idle poll marks nothing, so it runs neither.
    pub changed: bool,
    /// The gh calls the writes made, for the poll's cost.
    pub gh_writes: u32,
    /// The gh calls the ports' handles made, for the poll's cost: the
    /// wiring's runner counts them (`forge::Counted`).
    pub port_calls: Arc<AtomicU64>,
    /// The issues marked running this poll, for the poll's cost.
    pub starts: u32,
    /// The checkout was refreshed this poll by the index's gate, so the
    /// starts do not refresh it again.
    pub fetched: bool,
    /// What this poll's settle read of the writes left open, by the write
    /// (`looked_key`): one found done is not sent again, and one no read
    /// could settle is not sent at all (#1754).
    pub looked: HashMap<String, Looked>,
    /// Writes a line of the poll's log at once. A start's lines go through it
    /// before its unit runs, so they come before the lines of the cycle itself,
    /// which writes to the same log. None keeps them with the rest.
    pub say_now: Option<fn(&str)>,
    /// The planning sessions this poll ended that leave their issue as it
    /// was: the demand counts what they held out (`decide::demand_after`). A
    /// session that replaced or closed its issue is not one.
    pub plans_ended: Vec<u64>,
    /// A session ended this poll and asked for the poll after it.
    pub asked_again: bool,
    /// The events the writes raise (src/events.rs).
    pub events: tehas_ports::events::Dispatcher,
    /// The ports the poll writes through and settles a row of the ports
    /// with, from the program's wiring. None when the wiring could not make
    /// them: a row of the ports then stays open for the next poll.
    pub ports: Option<Handles>,
}

pub use tehas_core::journal::Handles;

/// The wiring's handles of repository `repo` for the factory of `home`: the
/// program `gh` given the poll's token, when it took one, each call bounded
/// by `bound`. The library names no adapter (R8): the program's main.rs
/// makes them.
/// Each call of gh the handles make is counted into `calls`, for the
/// poll's cost.
pub type HandlesOf = fn(
    home: &Path,
    repo: &str,
    gh: &str,
    token: Option<&str>,
    bound: Duration,
    calls: Arc<AtomicU64>,
) -> tehas_ports::Got<Handles>;

/// What the settle's read found of an open write that this poll may decide
/// to make again.
#[derive(Debug, Clone, PartialEq)]
pub enum Looked {
    /// It is on GitHub: what gh would have printed (a comment's id).
    Done(String),
    /// The read failed: the write stays open and is not sent.
    Unread(String),
}

/// One write as the journal keeps it: its issue, its kind, the gh call and
/// the body by its sum. Two writes with the same key are the same write.
pub fn looked_key(issue: u64, kind: &str, gh: &Value, input: &Value) -> String {
    format!("{issue} {kind} {gh} {input}")
}

/// What applying a decision did: the lines the poll logs, in order, and the
/// actions this part does not apply.
#[derive(Debug, Default)]
pub struct Applied {
    pub log: Vec<String>,
    pub left: Vec<Value>,
    /// What the status command printed after the starts.
    pub printed: Vec<String>,
    /// The poll's exit status when its starts ended it: no action after
    /// them is applied (A2: a retry's untick follows only a start that
    /// worked).
    pub exit: Option<i32>,
}

fn word(v: &Value) -> String {
    v.get("word")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// Issue `n` as the tickets port names it; None for 0.
fn ticket(n: u64) -> Option<TicketKey> {
    std::num::NonZeroU64::new(n).map(TicketKey::Number)
}

/// The longest key kept in <n>.said: the writer here and the reader in
/// read/build.rs both cut the file at this length.
pub const SAID_MAX: usize = 200;

/// What a re-ask carries (#2782), read off its action.
struct Reask<'a> {
    kind: &'a str,
    old: &'a str,
    old_body: &'a str,
    body: Option<&'a str>,
    offers: &'a str,
    unlabel: Option<&'a str>,
}

/// state/refresh/<n>.said: what says that an issue's hold was told.
pub fn said_file(home: &Path, n: u64) -> PathBuf {
    Home::new(home).refresh_said(n)
}

fn text<'a>(a: &'a Value, k: &str) -> &'a str {
    a.get(k).and_then(Value::as_str).unwrap_or("")
}

fn strings(a: &Value, k: &str) -> Vec<String> {
    a.get(k)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

impl Writer {
    pub fn apply(&mut self, actions: &[Value]) -> Applied {
        let mut out = Applied::default();
        let mut i = 0;
        while i < actions.len() {
            let run = actions[i..]
                .iter()
                .take_while(|a| a.get("start").is_some())
                .count();
            if run > 0 {
                let starts = &actions[i..i + run];
                i += run;
                if let Some(rc) = self.starts(starts, &mut out.log, &mut out.printed) {
                    out.exit = Some(rc);
                    return out;
                }
                continue;
            }
            if !self.one(&actions[i], &mut out.log) {
                out.left.push(actions[i].clone());
            }
            i += 1;
            if self.status_due {
                self.status_due = false;
                self.status(&mut out.printed, &mut out.log);
            }
            // A step that ended the poll changed the queue: the units it
            // would have started, or the poll that could not read GitHub
            // again. A step that changed something and lives on says so
            // itself where it does it: write(), quiet()'s stop, stop_done().
            if self.ended.is_some() {
                self.changed = true;
            }
            if let Some(rc) = self.ended.take() {
                out.exit = Some(rc);
                return out;
            }
        }
        // The store was asked for the poll's row and gave no time for it, so
        // no write of it was made: the poll says that once, and the lines the
        // refused writes left name the journal's verb, which is not what
        // happened. They go, and this one stands.
        if self.row_not_begun {
            let refused = "the journal could not record a write on #";
            out.log.retain(|l| !l.contains(refused));
            out.log
                .push("WARN: the state store began no poll row, so no write was made".to_string());
        }
        out
    }

    /// One action; false for one this part leaves.
    fn one(&mut self, a: &Value, log: &mut Vec<String>) -> bool {
        let n = a.get("issue").and_then(Value::as_u64);
        if let (Some(n), Some(_)) = (n, a.get("remove")) {
            self.label(n, a, log);
        } else if let (Some(n), Some(key)) = (n, a.get("refresh_said").and_then(Value::as_str)) {
            if !key.is_empty() && self.say(n, text(a, "say"), log) {
                self.record_said(n, key, log);
            }
        } else if let (Some(n), Some(_)) = (n, a.get("park_told")) {
            if self.say(n, text(a, "say"), log) {
                self.record_told(n, log);
            }
        } else if let (Some(n), Some(say)) = (n, a.get("say").and_then(Value::as_str)) {
            // Our markers the action carries beside its text (`Action::Say`).
            let markers = a
                .get("markers")
                .and_then(|m| serde_json::from_value(m.clone()).ok())
                .unwrap_or_default();
            self.say_marked(n, say, markers, log);
        } else if let Some(n) = a.get("answer_drop").and_then(Value::as_u64) {
            let id = match a.get("id") {
                Some(Value::String(s)) => s.clone(),
                Some(Value::Number(c)) => c.to_string(),
                _ => String::new(),
            };
            let say = a.get("say").and_then(Value::as_str);
            self.answer_drop(n, &id, say, text(a, "fail"), log);
        } else if let Some(n) = a.get("reask").and_then(Value::as_u64) {
            let r = Reask {
                kind: text(a, "kind"),
                old: text(a, "old"),
                old_body: text(a, "old_body"),
                body: a.get("body").and_then(Value::as_str),
                offers: text(a, "offers"),
                unlabel: a.get("unlabel").and_then(Value::as_str),
            };
            self.reask(n, &r, log);
        } else if let Some(pr) = a.get("pr").and_then(Value::as_u64) {
            self.unmark(pr, a, log);
        } else if let Some(n) = a.get("plan_note").and_then(Value::as_u64) {
            self.plan_note(n, a, log);
        } else if let Some(n) = a.get("refresh_drop").and_then(Value::as_u64) {
            self.refresh_drop(n, log);
        } else if let Some(held) = a.get("refresh_held").and_then(Value::as_str) {
            let path = Home::new(&self.home).state(State::RefreshHeld);
            if held.is_empty() {
                let _ = std::fs::remove_file(&path);
            } else if std::fs::write(&path, held).is_err() {
                log.push(format!("WARN: could not write {}", path.display()));
            }
        } else if let Some(note) = a.get("branch_note").and_then(Value::as_str) {
            let path = Home::new(&self.home).state(State::BranchCheck);
            if std::fs::write(&path, format!("{note}\n")).is_err() {
                log.push(format!("WARN: could not write {}", path.display()));
            }
        } else if let Some(n) = a.get("lost_cycle").and_then(Value::as_u64) {
            self.lost(
                n,
                a.get("attempt").and_then(Value::as_u64).unwrap_or(0),
                log,
            );
        } else if let Some(name) = a.get("reap").and_then(Value::as_str) {
            self.reap(name, text(a, "why"), log);
        } else if let Some(n) = a.get("quiet").and_then(Value::as_u64) {
            if let stop::Then::End = self.quiet(n, a, log) {
                self.ended = Some(78);
            }
        } else if let Some(n) = a.get("stop_done").and_then(Value::as_u64) {
            self.stop_done(n, a, log);
        } else if let Some(n) = a.get("sweep").and_then(Value::as_u64) {
            self.sweep(n, a, log);
        } else if let Some(n) = a.get("plan_collect").and_then(Value::as_u64) {
            self.plan_collect_action(n, log);
        } else if let Some(n) = a.get("plan_post").and_then(Value::as_u64) {
            // A turn that ends on a keep that would not land is not a left
            // action: nothing of it was journaled, on either refusal route
            // (the clear route keeps before it says), so the next poll repeats
            // it whole, which is what the keep waiting for it is for.
            self.plan_post(n, log);
        } else if let Some(n) = a.get("plan_close").and_then(Value::as_u64) {
            if self.plan_close(n, log) {
                self.plans_ended.push(n);
            }
        } else if let Some(n) = a.get("plan_start_drop").and_then(Value::as_u64) {
            self.plan_start_drop(n);
        } else if let Some(n) = a.get("units").and_then(Value::as_u64) {
            if !self.units(n, log) {
                self.ended = Some(1);
            }
        } else if a.get("index_gate").is_some() {
            if !self.index_gate(log) {
                self.ended = Some(0);
            }
        } else if let Some(n) = a.get("plan_turn").and_then(Value::as_u64) {
            if let turn::Then::End(rc) = self.plan_turn(n, a, log) {
                self.ended = Some(rc);
            }
        } else if let Some(n) = a.get("park_retry").and_then(Value::as_u64) {
            if !self.park_retry(n, a, log) {
                return false;
            }
        } else if let Some(n) = a.get("plan_end").and_then(Value::as_u64) {
            self.plan_end(n, text(a, "say"), false, false, log);
        } else {
            return false;
        }
        true
    }

    /// A write to GitHub, journaled. `input` goes to gh on standard input.
    /// `done_if`: words in a failed call's answer that mean the change is
    /// there already. The poll says once that no row stands: apply().
    fn write(
        &mut self,
        issue: u64,
        kind: &str,
        args: Vec<String>,
        input: Option<&str>,
        done_if: Option<&str>,
    ) -> Wrote {
        // The settle read this write's earlier try: done is not sent again,
        // unread is not sent until a read settles it.
        if let Some(wrote) = self.looked_1x(issue, kind, &args, input) {
            return wrote;
        }
        let token = self.calls.gh_token.clone();
        let mut journal = Journal {
            repo: &self.repo,
            gh: &self.gh,
            before: self.before,
            bound: self.bound,
            grace: GRACE,
            token: token.as_deref(),
            poll_at: &mut self.poll_at,
            row_not_begun: &mut self.row_not_begun,
            cycle: None,
        };
        let calls = &mut self.calls;
        let wrote = journal.write(
            &mut |a: &[&str]| calls.store(a),
            issue,
            kind,
            args,
            input,
            done_if,
        );
        // The queue changes here: the poll's end owes a status and a sample.
        if !matches!(wrote, Wrote::Unrecorded) {
            self.changed = true;
            self.gh_writes += 1;
        }
        wrote
    }

    /// What this poll's settle read of a 1.x row of this write, the call of
    /// gh a poll before T4.7 made for it: done is not sent again, unread is
    /// not sent until a read settles it. A write of the ports asks it too,
    /// so a row in flight at the upgrade is never sent twice (the ports
    /// spec, "Rows in flight at upgrade"). None: no such row was read.
    fn looked_1x(
        &mut self,
        issue: u64,
        kind: &str,
        args: &[String],
        input: Option<&str>,
    ) -> Option<Wrote> {
        let input_sum = tehas_core::journal::summed(args, input).unwrap_or(Value::Null);
        let key = looked_key(issue, kind, &json!(args), &input_sum);
        match self.looked.get(&key)? {
            Looked::Done(out) => {
                let out = out.clone();
                self.looked.remove(&key);
                Some(Wrote::Done(out))
            }
            Looked::Unread(why) => Some(Wrote::Unknown(format!(": {why}"))),
        }
    }

    /// A write of the ports, journaled, as `write` journals a call of gh:
    /// one this poll's settle found done is not sent again, one it could not
    /// read is not sent at all (`looked`, by the write as its row keeps it).
    /// `Done` holds the id a comment was given, else nothing.
    fn send(&mut self, n: u64, write: &Write) -> Wrote {
        let Ok(kept) = tehas_core::journal::kept(write) else {
            return Wrote::Failed(": its body could not be summed".into());
        };
        let key = looked_key(n, "", &json!(kept), &Value::Null);
        match self.looked.get(&key) {
            Some(Looked::Done(out)) => {
                let out = out.clone();
                self.looked.remove(&key);
                return Wrote::Done(out);
            }
            Some(Looked::Unread(why)) => return Wrote::Unknown(format!(": {why}")),
            None => {}
        }
        let Some(ports) = self.ports.as_mut() else {
            return Wrote::Failed(": the poll has no handles of the code host".into());
        };
        let token = self.calls.gh_token.clone();
        let mut journal = Journal {
            repo: &self.repo,
            gh: &self.gh,
            before: self.before,
            bound: self.bound,
            grace: GRACE,
            token: token.as_deref(),
            poll_at: &mut self.poll_at,
            row_not_begun: &mut self.row_not_begun,
            cycle: None,
        };
        let calls = &mut self.calls;
        let sent = journal.send(
            &mut |a: &[&str]| calls.store(a),
            write,
            &mut *ports.writes,
            &NoBlobs,
        );
        let wrote = match sent {
            Journaled::Answered(Ok(Receipt::Comment(posted))) => {
                Wrote::Done(posted.id.as_str().to_string())
            }
            Journaled::Answered(Ok(_)) => Wrote::Done(String::new()),
            Journaled::Answered(Err(o)) => Wrote::Failed(format!(": {}", o.line())),
            Journaled::Unknown { answer, .. } => Wrote::Unknown(format!(": {}", answer.line())),
            Journaled::Unrecorded => Wrote::Unrecorded,
        };
        // The queue changes here: the poll's end owes a status and a sample.
        // Its calls of gh are counted by the handles' runner.
        if !matches!(wrote, Wrote::Unrecorded) {
            self.changed = true;
        }
        wrote
    }

    /// reap: a container no cycle holds, removed; said only when docker
    /// named one it removed.
    fn reap(&mut self, name: &str, why: &str, log: &mut Vec<String>) {
        let r = run(
            &self.docker,
            &["rm".into(), "-f".into(), name.into()],
            Duration::from_secs(60),
        );
        if r.ok() && !r.out.trim().is_empty() {
            log.push(format!("removed the container {name}: {why}"));
        }
    }

    /// tehas_store_try: asked again, a second apart, only when the store
    /// answered 1 busy or timeout.
    pub fn store_try(&mut self, tries: u32, args: &[&str]) -> Result<Value, Value> {
        let wait = self.store_retry;
        call::store_try(tries, wait, || self.calls.store(args))
    }

    /// A write GitHub may have taken: it is read before it is sent again.
    fn unknown(n: u64, what: &str, why: &str) -> String {
        format!("WARN: the {what} on #{n} may have been posted{why}; the next poll reads it before it is sent again")
    }

    fn unrecorded(n: u64) -> String {
        format!("WARN: the journal could not record a write on #{n}, so it was not made")
    }

    /// The refresh request of issue `n`, off this host.
    fn refresh_drop(&self, n: u64, log: &mut Vec<String>) {
        let path = Home::new(&self.home).refresh_request(n);
        if let Err(e) = std::fs::remove_file(&path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                log.push(format!("WARN: could not remove {}", path.display()));
            }
        }
    }

    /// say: one comment, a warning when it fails, never the poll's end.
    /// True when GitHub took it. Only the records of having said something
    /// (record_said, record_told) read the answer; the rest of the comments
    /// of a poll are said again by their own rule, so they drop it.
    fn say(&mut self, n: u64, body: &str, log: &mut Vec<String>) -> bool {
        self.say_marked(n, body, Vec::new(), log)
    }

    /// `say`, with our markers beside the text: a note of the tickets port.
    fn say_marked(
        &mut self,
        n: u64,
        body: &str,
        markers: Vec<tehas_ports::Marker>,
        log: &mut Vec<String>,
    ) -> bool {
        let Some(ticket) = ticket(n) else {
            log.push(format!("WARN: comment on #{n} failed"));
            return false;
        };
        let note = Write::TicketNote {
            ticket,
            body: Body::Text(body.to_string()),
            markers,
        };
        let args: Vec<String> = ["issue", "comment", &n.to_string(), "--repo", &self.repo]
            .into_iter()
            .chain(["--body-file", "-"])
            .map(str::to_string)
            .collect();
        // A text that still holds our marker inside it goes as 1.x wrote
        // it, through gh, until its site gives the marker typed beside the
        // text (the finish's at T4.7's step 5): a write of the ports
        // refuses it.
        let wrote = if tehas_core::text::has_marker(body) {
            self.write(n, "comment-post", args, Some(body), None)
        } else {
            match self.looked_1x(n, "comment-post", &args, Some(body)) {
                Some(wrote) => wrote,
                None => self.send(n, &note),
            }
        };
        match wrote {
            Wrote::Done(_) => true,
            Wrote::Failed(_) => {
                log.push(format!("WARN: comment on #{n} failed"));
                false
            }
            Wrote::Unknown(why) => {
                log.push(Self::unknown(n, "comment", &why));
                false
            }
            Wrote::Unrecorded => {
                log.push(Self::unrecorded(n));
                false
            }
        }
    }

    /// A label change, and what follows one that worked (A6): its line, its
    /// comment, the answer of its question.
    fn label(&mut self, n: u64, a: &Value, log: &mut Vec<String>) {
        // A change that rests on a label read again now: gone, the change is
        // not made; a read that fails ends the poll.
        if let Some(want) = a.get("reread").and_then(Value::as_str) {
            let repo = self.repo.clone();
            let path = format!("repos/{repo}/issues/{n}");
            let labels = match self.calls.gh(&["api", &path]) {
                Ok(v) => v
                    .get("labels")
                    .and_then(Value::as_array)
                    .cloned()
                    .filter(|_| v.is_object()),
                Err(_) => {
                    log.push(format!("ERROR: gh api {path} failed"));
                    self.ended = Some(1);
                    return;
                }
            };
            let Some(labels) = labels else {
                log.push(format!("ERROR: GitHub's answer for #{n} could not be read"));
                self.ended = Some(1);
                return;
            };
            if !labels
                .iter()
                .any(|l| l.get("name").and_then(Value::as_str) == Some(want))
            {
                log.push(format!(
                    "skip #{n}: {want} is gone from it since the list was read"
                ));
                return;
            }
        }
        if let Some(line) = a.get("first").and_then(Value::as_str) {
            log.push(line.to_string());
        }
        let remove = strings(a, "remove");
        let add = strings(a, "add");
        let mut args: Vec<String> = vec![
            "issue".into(),
            "edit".into(),
            n.to_string(),
            "--repo".into(),
            self.repo.clone(),
        ];
        // As the bash sites give them: what comes off, then what goes on.
        for l in &remove {
            args.extend(["--remove-label".into(), l.clone()]);
        }
        for l in &add {
            args.extend(["--add-label".into(), l.clone()]);
        }
        let kind = if add.is_empty() {
            "label-remove"
        } else {
            "label-add"
        };
        match self.write(n, kind, args, None, None) {
            Wrote::Done(_) => {}
            Wrote::Failed(why) => {
                log.push(format!("{}{why}", text(a, "fail")));
                return;
            }
            Wrote::Unknown(why) => {
                log.push(Self::unknown(n, "label change", &why));
                return;
            }
            Wrote::Unrecorded => {
                log.push(Self::unrecorded(n));
                return;
            }
        }
        if let Some(line) = a.get("log").and_then(Value::as_str) {
            log.push(line.to_string());
        }
        if let Some(say) = a.get("say").and_then(Value::as_str) {
            self.say(n, say, log);
        }
        if let Some(q) = a.get("answered") {
            let id = q.get("id").map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            });
            let by = text(q, "by").to_string();
            let last = self.store_try(
                3,
                &[
                    "question-answered",
                    &n.to_string(),
                    "--id",
                    id.as_deref().unwrap_or(""),
                    "--by",
                    &by,
                ],
            );
            if let Err(f) = last {
                log.push(format!(
                    "WARN: could not record the answer to the question of #{n} ({})",
                    word(&f)
                ));
            }
        }
    }

    /// lost_cycle: the open record of a cycle that died without a word, ended
    /// as lost with its line of history. The store takes it only with the
    /// lock free and the cycle quiet: a cycle that took the lock since is
    /// running, and one heard from just now is not lost yet; nothing is said
    /// of either.
    fn lost(&mut self, n: u64, attempt: u64, log: &mut Vec<String>) {
        let (issue, attempt) = (n.to_string(), attempt.to_string());
        match self.store_try(3, &["cycle-end", &issue, "--attempt", &attempt, "--lost"]) {
            Ok(v) => {
                log.push(format!(
                    "lost #{n}: attempt {attempt} died without a word and no cycle holds its lock; ended as lost"
                ));
                self.raise_lost(n, attempt.parse().unwrap_or(0), &v.to_string(), log);
            }
            Err(f) if matches!(word(&f).as_str(), "running" | "attempt" | "heard") => {}
            Err(f) => log.push(format!(
                "WARN: the lost cycle of #{n} could not be ended ({})",
                word(&f)
            )),
        }
    }

    /// answer_drop: the answer `tehas answer` gave, taken away as a box is
    /// unticked, then its note.
    fn answer_drop(
        &mut self,
        n: u64,
        id: &str,
        say: Option<&str>,
        fail: &str,
        log: &mut Vec<String>,
    ) {
        match self.store_try(3, &["answer-drop", &n.to_string(), "--id", id]) {
            Ok(_) => {
                if let Some(say) = say {
                    self.say(n, say, log);
                }
            }
            Err(f) => log.push(format!("{fail} ({})", word(&f))),
        }
    }

    /// The REST path of `rest` under the repository's issues.
    fn issues_path(&self, rest: &str) -> String {
        format!("repos/{}/issues/{rest}", self.repo)
    }

    /// A question's comment posted on issue `n`: its id, or None when GitHub
    /// did not take it or gave no id.
    pub(crate) fn question_post(&mut self, n: u64, body: &str) -> Option<u64> {
        let args = vec![
            "api".into(),
            "-X".into(),
            "POST".into(),
            self.issues_path(&format!("{n}/comments")),
            "--input".into(),
            "-".into(),
            "--jq".into(),
            ".id".into(),
        ];
        let input = json!({"body": body}).to_string();
        let Wrote::Done(said) = self.write(n, "comment-post", args, Some(&input), None) else {
            return None;
        };
        // gh prints the id alone, as --jq .id has it.
        said.trim().parse::<u64>().ok()
    }

    /// A question of before 2.0 asked again in 2.0's words (#2782): the new
    /// comment posted and recorded, the old one taken off the record, then
    /// told once where it went (a stop closed with nothing to ask also loses
    /// `tehas:question`). A step before the withdrawal that fails stops there,
    /// and the next poll asks again; one after it is said in the log and not
    /// repeated. An old comment that already ends in the line is not edited.
    fn reask(&mut self, n: u64, r: &Reask, log: &mut Vec<String>) {
        let mut new = None;
        if let Some(body) = r.body {
            let Some(id) = self.question_post(n, body) else {
                log.push(format!("WARN: the question of #{n} could not be asked again; the next poll tries again"));
                return;
            };
            let (id, issue) = (id.to_string(), n.to_string());
            let mut args = vec!["question-asked", &issue, "--id", &id, "--kind", r.kind];
            if !r.offers.is_empty() {
                args.extend(["--offers", r.offers]);
            }
            match self.store_try(3, &args) {
                Ok(v) => self.raise_asked(n, &v.to_string(), log),
                Err(f) => {
                    log.push(format!(
                        "WARN: the question of #{n} was asked again as comment {id}, but it could not be recorded ({})",
                        word(&f)
                    ));
                    return;
                }
            }
            new = id.parse().ok();
        }
        // Off the record first: a step after it that fails is not repeated
        // by the next poll, which no longer finds the old question marked.
        let issue = n.to_string();
        if let Err(f) = self.store_try(
            3,
            &["question-answered", &issue, "--id", r.old, "--withdraw"],
        ) {
            log.push(format!(
                "WARN: the old question of #{n} could not be taken off the record ({})",
                word(&f)
            ));
            return;
        }
        // A stop closed with nothing to ask no longer waits on a question.
        if let Some(label) = r.unlabel {
            if !self.edit_labels(n, &[("--remove-label", label)]) {
                log.push(format!(
                    "WARN: the question's label could not be taken off #{n}"
                ));
            }
        }
        let line = tehas_core::question::superseded(new);
        if !r.old_body.trim_end().ends_with(&line) {
            let edited = format!("{}\n\n{line}", r.old_body.trim_end());
            let args = vec![
                "api".into(),
                "-X".into(),
                "PATCH".into(),
                self.issues_path(&format!("comments/{}", r.old)),
                "--input".into(),
                "-".into(),
            ];
            let input = json!({"body": edited}).to_string();
            if !matches!(
                self.write(n, "comment-edit", args, Some(&input), None),
                Wrote::Done(_)
            ) {
                log.push(format!(
                    "WARN: the old question of #{n} could not be told it is superseded"
                ));
            }
        }
        log.push(format!(
            "asked #{n}'s question again in the words of 2.0 (comment {} superseded)",
            r.old
        ));
    }

    /// refresh_unmark: tehas:refreshing off a pull request; one already
    /// without it is done.
    fn unmark(&mut self, pr: u64, a: &Value, log: &mut Vec<String>) {
        let args = vec![
            "api".into(),
            "-X".into(),
            "DELETE".into(),
            format!("repos/{}/issues/{pr}/labels/tehas:refreshing", self.repo),
        ];
        let worked = match self.write(pr, "change-unmark", args, None, Some("Not Found")) {
            Wrote::Done(_) => true,
            Wrote::Failed(_) => false,
            Wrote::Unknown(why) => {
                log.push(Self::unknown(pr, "label change", &why));
                return;
            }
            Wrote::Unrecorded => {
                log.push(Self::unrecorded(pr));
                return;
            }
        };
        if worked {
            if let Some(line) = a.get("log").and_then(Value::as_str) {
                log.push(line.to_string());
            }
        } else {
            log.push(format!(
                "WARN: label change on pull request #{pr} failed: --remove-label tehas:refreshing"
            ));
        }
    }

    /// The record that this hold of a refresh was said, made only after a
    /// comment that worked: a hold nobody was told about has not told.
    fn record_said(&mut self, n: u64, key: &str, log: &mut Vec<String>) {
        let path = said_file(&self.home, n);
        // The reader compares the head of the file, so a key longer than that
        // would never be recognised and the hold would be said again forever.
        let key = &key[..key.len().min(SAID_MAX)];
        if std::fs::write(&path, key).is_err() {
            log.push(format!("WARN: could not write {}", path.display()));
        }
    }

    /// The park's tell, made only after a comment that worked.
    fn record_told(&mut self, n: u64, log: &mut Vec<String>) {
        if let Err(f) = self.calls.store(&["park-told", &n.to_string()]) {
            log.push(format!(
                "WARN: could not record the hold on the park of #{n} ({})",
                word(&f)
            ));
        }
    }

    /// A note of a planning session, said once: the store answers whether
    /// this is the first time.
    fn plan_note(&mut self, n: u64, a: &Value, log: &mut Vec<String>) {
        let id = match a.get("id") {
            Some(Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => String::new(),
        };
        match self.calls.store(&[
            "plan-note",
            &n.to_string(),
            "--kind",
            text(a, "kind"),
            "--id",
            &id,
        ]) {
            Ok(v) => {
                if v.get("said") == Some(&json!(true)) {
                    // Deliberate: the store's record comes first here, so a
                    // comment this poll failed to post is never said again.
                    self.say(n, text(a, "say"), log);
                }
            }
            Err(f) => log.push(format!(
                "WARN: the record of #{n} could not be updated ({})",
                word(&f)
            )),
        }
    }
}

#[cfg(test)]
#[path = "apply_tests.rs"]
mod tests;

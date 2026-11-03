//! The programs a snapshot asks: gh, the state store and the host's own
//! tools, each under a time bound. Only reads leave here: a gh call that could
//! change anything is refused before it runs. How a program is run is
//! tehas_core::call.

use super::retry::{retry, Budget, Outcome, Stopped};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tehas_core::call::{self, store_executable, store_failed, store_failure, word_of, GRACE};
pub use tehas_core::call::{
    gh_env, gh_why, run, run_grace, run_grace_env, run_input, run_input_bounded, run_input_env,
    run_quiet, wait, Ran,
};

#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// The reading of the host's own clock, which is what `poll_now` gives when no
/// case has set `CASE_NOW`. No second of the real clock reaches it. Test-only,
/// like the seam that reads it.
#[cfg(test)]
const HOST_CLOCK: u64 = u64::MAX;

/// The case clock a test of a retry holds, as the cases of `retry.rs` do: a
/// reading of the clock the rule measures its waits against, so the case sleeps
/// none of them and no case of the workspace waits on a retry. A case never names
/// it: `measured` holds it, and nothing outside this module's tests can.
#[cfg(test)]
const TEST_CLOCK: u64 = 1_000;

/// The clock of `poll_now`: the host's own clock while `CASE_NOW` holds
/// `HOST_CLOCK`, and the seconds a case has set, so that the case sleeps none of
/// the waits of the rule and reads none of the host's time. Test-only, like the
/// `Case` that writes it: a poll never reads a value but the host's own, and the
/// lib target ships no clock a caller can set.
#[cfg(test)]
static CASE_NOW: AtomicU64 = AtomicU64::new(HOST_CLOCK);

/// The cases that write the clock of `poll_now`, and so must not run at once:
/// a reading of the clock belongs to the process, not to the case that set it.
#[cfg(test)]
static CASE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Whether a case of a retry holds the clock, and the reading it set: nothing
/// when the host's own clock stands, which is what `HOST_CLOCK` means. The one
/// place that asks the question, so `poll_now` and `sleep_out` cannot disagree
/// about it. Test-only: with no `Case` in the lib there is nothing to hold.
#[cfg(test)]
fn held() -> Option<u64> {
    match CASE_NOW.load(Ordering::Relaxed) {
        HOST_CLOCK => None,
        at => Some(at),
    }
}

/// The clock the rule of `retry` measures its waits against, and the clock the
/// rule itself reads. Whole seconds, as the rule's `WAITS` are whole seconds; the
/// host's own whenever no case holds one.
fn poll_now() -> Option<u64> {
    #[cfg(test)]
    if let Some(at) = held() {
        return Some(at);
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// The thread that made a call sleeps out the waits the rule named, unless a
/// case has set `CASE_NOW` and so waits for nothing: the rule of `retry.rs` runs
/// no process and no thread and no case of it sleeps, and no case of the
/// workspace either starts a process or waits on a retry it did not ask for.
fn sleep_out(waited: u64) {
    // A case that holds the clock waits for none of them; a poll sleeps all of
    // them on the thread that made the call.
    #[cfg(test)]
    if held().is_some() {
        return;
    }
    std::thread::sleep(Duration::from_secs(waited));
}

/// The rule of `retry` applied to one gh call, and the waits slept on the thread
/// that made it. `call` runs the program of one attempt; no attempt ever runs on
/// a thread of its own. Only `RetryPolicy::read` asks for it, so the budget stays
/// inside the policy that owns it.
fn tried(call: &mut dyn FnMut() -> Ran, budget: &mut Budget) -> Outcome {
    let outcome = retry(call, budget, poll_now);
    for waited in &outcome.waits {
        sleep_out(*waited);
    }
    outcome
}

/// What the retry of one failed read costs the poll: the words the failed
/// attempt printed, and the bound or the clock that stopped the retry. The
/// retry rule is asked here and not by the caller, so the words are the rule's
/// `Outcome` and nothing else. Made only by `RetryPolicy::read`, which is the
/// only thing that logs, so no line is ever kept without an `Outcome` behind it.
struct Note {
    /// What the answer the caller keeps did not say: the words of the attempt
    /// the retry replaced, which `gh_why` puts in the failed line.
    first_err: Option<String>,
    /// How many times the read was made, which a line says only when the retry
    /// was more than nothing.
    attempts: u32,
    /// A retry the rule asked for and no bound served; nothing when the answer
    /// the rule kept stood. Kept as `retry`'s variant and not as its words, so
    /// a new `Stopped` is one edit in `stopped_words` and the line stays in step
    /// with the enum by the compiler rather than by a match written twice.
    stopped: Option<Stopped>,
}

impl Note {
    /// What the retry of a read did: the rule's `Outcome` in the parts a log
    /// line is made of. `ran` is the answer the caller keeps, whose error stream
    /// is what `gh_why` reads.
    fn of(outcome: &Outcome, ran: &Ran) -> Note {
        Note {
            first_err: gh_why_err(&outcome.first_err, ran),
            attempts: outcome.attempts,
            stopped: (outcome.stopped != Stopped::Answered).then_some(outcome.stopped),
        }
    }
}

/// The words of a bound that refused a retry the rule asked for. The one place
/// `Stopped` is put into the poll's log.
fn stopped_words(stopped: Stopped) -> &'static str {
    match stopped {
        Stopped::Answered => "",
        Stopped::AtCap => "at the bound of one call",
        Stopped::NoClock => "with no clock to measure the wait",
        Stopped::NoBudget => "with the poll's retries spent",
    }
}

/// The words of a failed answer that the line quotes: `gh_why`'s match of the
/// fixed list, without the `": "` `gh_why` puts before it. `line` owns that
/// joiner, so nothing here has to undo what `gh_why` added and another site
/// re-add it.
fn why_words(err: &str) -> Option<String> {
    let why = gh_why(err);
    (!why.is_empty()).then(|| why.replacen(": ", "", 1))
}

/// What the failed line quotes: gh's words of the attempt the retry replaced
/// when there was one, and of the answer the caller keeps otherwise. So a read
/// that failed twice says what the first attempt said, which the answer now
/// held no longer carries, and a read that never retried says what it said.
fn gh_why_err(first_err: &Option<String>, ran: &Ran) -> Option<String> {
    match first_err {
        Some(e) if !e.is_empty() => why_words(e),
        _ => why_words(&ran.err),
    }
}

/// The line one failed read leaves in the poll's log: `word` of gh's verb and
/// its path, the words `gh_why` kept, and the fact that the read was tried
/// again when the rule took it more than once. Nothing of a token is ever in
/// it: only the words of the fixed list reach the line, and `tehas_core::call`
/// cuts the line to the same 300 characters and control characters the store's
/// lines are cut to, so nothing a caller puts in `word` can put a second line
/// or an escape sequence in the poll's log.
fn line(word: &str, note: &Note) -> String {
    // `word` names the call and comes from the poll's own arguments; the words of
    // gh come after it, so `first_err` never has to be escaped for the log.
    let mut out = match &note.first_err {
        Some(why) => format!("{word}: {why}"),
        None => word.to_string(),
    };
    // A failure the rule kept was a failure it owed no retry to, so on this path
    // `stopped` stands whenever an attempt was refused: the two halves of the
    // line cannot disagree, and one test pins them together.
    if note.attempts > 1 {
        out.push_str(&format!(" (tried {} times", note.attempts));
        if let Some(stopped) = note.stopped {
            out.push_str(&format!(", stopped {}", stopped_words(stopped)));
        }
        out.push(')');
    }
    tehas_core::call::trim(&out)
}

/// The reads of a poll that no single `Calls` made: the budget of retries the
/// poll pays for, and the lines of the reads it could not make.
///
/// A poll asks its reads with several `Calls` - the snapshot of `body`, the
/// writes of `Writer`, the reads of `refresh` - built and dropped apart from one
/// another, so what belongs to the poll as a whole cannot sit in only one of
/// them. The parts are behind an `Arc` and the value is cloned: every `Calls` of
/// one poll holds the same budget and the same lines, and the budget is spent
/// once for the poll rather than once a `Calls`. Two polls, or two cases of a
/// test that run side by side, keep their own.
///
/// One poll holds one value, named at the top of `run_poll`, and gives it to
/// each `Calls` it makes through `writer` and `body`: the sharing is an argument
/// a caller must pass, not a field assignment one can leave out. A `Calls` of
/// `Calls::idle` owns its own policy, which is what a program that is not a poll
/// wants - `refresh` and the printed snapshot are such programs.
#[derive(Clone)]
pub struct RetryPolicy {
    /// The retries the poll may still pay for, of the whole `RETRIES_PER_CALLS`
    /// one snapshot is given. Private: the budget is the policy's to spend and no
    /// caller swaps one `Arc` for another and splits a poll's retries.
    budget: Arc<Mutex<Budget>>,
    /// One line for each gh read the poll could not make, with gh's own words for
    /// it and, when the rule tried it again, how many attempts it got and what
    /// stopped it. `poll.rs` prints them, and a case of a failed read reads them
    /// with `taken`. Private for the same reason as `budget`: a line is said once
    /// because only `taken` takes them, and a line exists only because `read`
    /// failed. Bounded by `NOTES_MAX`, where the queue that grows is.
    notes: Arc<Mutex<Notes>>,
}

/// The ceiling of one poll's queue of failed-read lines. A poll's gh reads are
/// bounded by its own batch sizes, but the ceiling lives with the queue that
/// grows rather than with every call site, and a poll whose reads all fail in a
/// bad minute writes the log a fixed number of lines and one line of the rest.
const NOTES_MAX: usize = 64;

/// The poll's lines of failed reads and the count of those it could not keep.
struct Notes {
    lines: Vec<String>,
    /// Reads that failed past `NOTES_MAX`, said as one line at the end.
    more: usize,
}

impl Notes {
    /// Keep one line, or count the read as one the queue would not hold. The
    /// oldest lines are the ones a poll keeps: they name what failed first.
    fn push(&mut self, line: String) {
        if self.lines.len() < NOTES_MAX {
            self.lines.push(line);
        } else {
            self.more += 1;
        }
    }
}

impl Default for RetryPolicy {
    fn default() -> RetryPolicy {
        RetryPolicy {
            budget: Arc::new(Mutex::new(Budget::new())),
            notes: Arc::new(Mutex::new(Notes {
                lines: Vec::new(),
                more: 0,
            })),
        }
    }
}

impl RetryPolicy {
    /// One read of gh under the rule: `call` gives the answer of an attempt, and
    /// the line of a failure is kept for the poll whether or not the retry
    /// answered. The `Outcome` is the caller's to read, so it can tell a bad
    /// minute from a read GitHub refused.
    pub fn read(&self, word: &str, call: &mut dyn FnMut() -> Ran) -> Outcome {
        // The lock is taken and given back by `tried` alone, and never held
        // across an attempt: no thread of the poll can wait on it for the length
        // of a bound gh call.
        let mut budget = self.budget.lock().unwrap_or_else(|e| e.into_inner());
        let outcome = tried(call, &mut budget);
        drop(budget);
        if !outcome.ran.ok() {
            self.log(word, &Note::of(&outcome, &outcome.ran));
        }
        outcome
    }

    /// One line for a read the poll could not make, kept in the order the poll
    /// asked for them. Only `read` calls it, so no line is kept without an
    /// `Outcome` behind it, and `Notes::push` is what bounds the queue.
    fn log(&self, word: &str, note: &Note) {
        self.notes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(line(word, note));
    }

    /// The lines of the reads that failed, oldest first, and the lines gone from
    /// the poll: what `poll.rs` prints at its end, so a line is said once however
    /// the poll ended. Past the ceiling of the queue one line says how many reads
    /// the poll did not name.
    pub fn taken(&self) -> Vec<String> {
        let mut held = self.notes.lock().unwrap_or_else(|e| e.into_inner());
        let lines = std::mem::take(&mut held.lines);
        let more = std::mem::take(&mut held.more);
        if more == 0 {
            return lines;
        }
        let mut said = lines;
        said.push(format!("ERROR: and {more} more failed reads of this poll"));
        said
    }
}

/// The clock a case of a retry holds, and the only way a test takes it: through
/// `measured`, which holds the case clock. A case cannot name a reading of its
/// own, and so cannot pass the host's sentinel and start sleeping on the real
/// clock, nor the reading another case of the workspace already uses. The whole
/// seam is test-only: the lib target ships no clock a caller can set and no lock
/// a case serialises on.
#[cfg(test)]
pub struct Case {
    /// The lock of the cases that write `CASE_NOW`, held for one case. Nothing
    /// reads it: it is here so that `Drop::drop` runs while the lock still stands
    /// and is given up after it, which is what keeps a second case from setting
    /// the clock while this one is still putting the host's back.
    #[allow(dead_code)]
    held: std::sync::MutexGuard<'static, ()>,
}

#[cfg(test)]
impl Case {
    /// Take the clock for one case: the reading of `CASE_NOW` the rule measures
    /// its waits against, held until the case ends. Private to the module: a
    /// case of a retry gets the clock through `measured`, which holds `TEST_CLOCK`
    /// and nothing else.
    fn hold(at: u64) -> Case {
        // The same recovery as every other lock of this module: a case that
        // panics while it holds the clock leaves the guard poisoned but the
        // clock itself restored by `Drop`, so the next case takes it rather than
        // aborting for a panic it did not cause.
        let held = CASE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        CASE_NOW.store(at, Ordering::Relaxed);
        Case { held }
    }
}

#[cfg(test)]
impl Drop for Case {
    fn drop(&mut self) {
        // The order is the point, and it is Rust's: the clock goes back to the
        // host in `drop` and the field's guard is given back only afterwards, so
        // no second case can set the clock while this one is still putting the
        // host's back.
        CASE_NOW.store(HOST_CLOCK, Ordering::Relaxed);
    }
}

/// Whether a gh call only reads: `api graphql` with a query, `api` of a path
/// with no method or field, or a list or view.
pub fn gh_reads(args: &[String]) -> bool {
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    match a.as_slice() {
        // Only variables and the query, each a flag and its value: no method,
        // no input file, nothing joined to a flag.
        ["api", "graphql", rest @ ..] => {
            rest.len() % 2 == 0
                && rest
                    .chunks(2)
                    .all(|p| matches!(p[0], "-f" | "-F") && p[1].contains('='))
                && rest
                    .chunks(2)
                    .find_map(|p| p[1].strip_prefix("query="))
                    .is_some_and(|q| q.trim_start().starts_with("query") && !q.contains("mutation"))
        }
        // A path, and at most --paginate.
        ["api", path, rest @ ..] => {
            !path.starts_with('-') && rest.iter().all(|x| *x == "--paginate")
        }
        ["issue" | "pr", "list" | "view", rest @ ..] => {
            !rest.iter().any(|x| x.starts_with("--web") || *x == "-w")
        }
        _ => false,
    }
}

/// gh's output as JSON. --paginate prints one array a page, one after
/// another: they are one list, as `jq -s add` makes them.
fn parse(out: &str) -> Value {
    if let Ok(v) = serde_json::from_str(out) {
        return v;
    }
    let pages: Result<Vec<Value>, _> = serde_json::Deserializer::from_str(out)
        .into_iter::<Value>()
        .collect();
    match pages {
        Ok(p) if !p.is_empty() && p.iter().all(Value::is_array) => Value::Array(
            p.into_iter()
                .flat_map(|v| match v {
                    Value::Array(a) => a,
                    _ => Vec::new(),
                })
                .collect(),
        ),
        _ => Value::String(out.to_string()),
    }
}

/// The calls of one snapshot, and how many went to GitHub.
pub struct Calls {
    /// The instance's home, which `TEHAS_HOME` gives every store call: the
    /// store names its own database from it. Every caller passes it explicitly;
    /// a `Calls` built by a test that means a store of its own sets it, and one
    /// that names no home has no store to ask.
    pub home: Option<String>,
    pub bound: Duration,
    /// The bound of one store call, as tehas_core::call has it.
    pub store_bound: Duration,
    pub gh: String,
    pub store: String,
    pub gh_count: u32,
    /// The GitHub App's token, taken once at the poll's first gh call and kept
    /// here. Every gh process of the poll gets it as `TEHAS_GH_TOKEN`, which
    /// host/shim/gh hands to gh as `GH_TOKEN` instead of asking
    /// libexec/github-token for one of its own. The poll holds it in memory
    /// only: it is never logged, written, or put in the poll's record, and a
    /// poll with no app named has nothing here.
    pub gh_token: Option<String>,
    /// What the retry of this poll's reads costs and leaves behind: the budget of
    /// retries and the lines of the reads it could not make. It belongs to the
    /// poll and not to one `Calls`, because a poll asks its reads with several of
    /// them, and the poll gives it through `Calls::for_poll`; a `Calls` of
    /// `Calls::idle` owns its own, which is what a test or a program that is not a
    /// poll wants. The fields of the policy are private, so a caller cannot spend
    /// another poll's retries or take its lines; only the fixture that names its
    /// `Calls` field by field gives it one of its own.
    pub retry: RetryPolicy,
}

impl Calls {
    /// The calls of a case: every field named by the case that needs one, and
    /// `retry` defaulted. A fixture that gives its `Calls` a policy of its own
    /// writes `retry: RetryPolicy::default()` beside the rest, which is what
    /// `apply_tests.rs` and `read/build.rs` do; a fixture that means to be one
    /// poll's part takes the poll's policy through `Calls::for_poll`.
    pub fn idle(store_bound: Duration, store: String) -> Calls {
        Calls {
            home: std::env::var("TEHAS_HOME").ok().filter(|h| !h.is_empty()),
            bound: Duration::from_secs(30),
            store_bound,
            gh: "gh".into(),
            store,
            gh_count: 0,
            gh_token: None,
            retry: RetryPolicy::default(),
        }
    }

    /// The calls of a snapshot read by one poll, or of the `Writer` that writes
    /// for one poll: the same bound and the same paths as `idle`, and the poll's
    /// own `RetryPolicy` rather than a fresh one, so the reads of this `Calls`
    /// spend the retries the poll pays for and leave their lines where the poll
    /// prints them. A poll builds its `Calls` this way and never by assigning
    /// `retry` afterwards: a `Calls` of the poll that forgot the argument would
    /// quietly own its own budget and its own lines, and nothing would say so.
    pub fn for_poll(store_bound: Duration, store: String, retry: &RetryPolicy) -> Calls {
        Calls {
            home: std::env::var("TEHAS_HOME").ok().filter(|h| !h.is_empty()),
            bound: Duration::from_secs(30),
            store_bound,
            gh: "gh".into(),
            store,
            gh_count: 0,
            gh_token: None,
            retry: retry.clone(),
        }
    }
}

/// The request `store batch` reads: the same 1 MiB the store bounds its own
/// standard input with (src/batch.rs). The store bounds what it is given and
/// not what it prints, and one `plan-show` of a queue may answer a brief of the
/// store's own bound, so the answer of a batch is the poll's own risk to bound:
/// what lies past the size of a legal request is not an answer anyone meant,
/// and is not read.
const BATCH_BOUND: u64 = 1 << 20;

/// The token of the host's GitHub App, asked for once by the poll that first
/// needs it: `$TEHAS_HOME/libexec/github-token` with the poll's own environment,
/// as the bash runs it, printing the token and nothing else. Only when
/// `TEHAS_GITHUB_APP_ID` names an app. When the program could not be run or
/// answered nothing, this gives nothing and the poll's gh calls go on as they
/// did before, each asking for a token of its own.
pub fn read_gh_token(home: &Path, bound: Duration) -> Option<String> {
    if std::env::var("TEHAS_GITHUB_APP_ID")
        .unwrap_or_default()
        .is_empty()
    {
        return None;
    }
    let program = home
        .join("libexec/github-token")
        .to_string_lossy()
        .into_owned();
    let r = run(&program, &[], bound);
    let token = r.out.lines().next().unwrap_or("").trim();
    if r.ok() && !token.is_empty() {
        Some(token.to_string())
    } else {
        None
    }
}

/// A gh call that failed: what the bash's gh_json says for it.
///
/// `ran` is the answer behind the words when there was one. The rule of `retry`
/// is asked of that answer and never of the line, so the chunk of `gh_all` and
/// the single call of `gh` class the same bytes the same way, and a body that
/// echoes a status can forge a line and not a decision. A `GhFail` with no answer
/// was never a call the rule could class - a write turned away, or the answer of a
/// worker that never came back - and so owes no retry.
pub struct GhFail {
    /// What the poll logs for the failed read.
    pub line: String,
    /// The answer the words came from, kept for the rule of `retry`. Read by
    /// reference where the retry decision is made, so nothing needs to copy it;
    /// `Ran` holds no `Clone` and `core/src/call.rs` is out of scope here.
    pub ran: Option<Ran>,
}

impl GhFail {
    /// A failure the poll made before a process ran, so there is nothing for the
    /// rule to class and nothing to retry.
    fn words(line: String) -> GhFail {
        GhFail { line, ran: None }
    }

    /// The line of a chunk's failure: the call's `word` and gh's own words of the
    /// answer, in the one shape the poll logs. The serial path's line is made by
    /// `line` too, so both say the same thing about a failed read. The answer is
    /// taken by value because the chunk is done with it and the failure owns it.
    fn chunk(word: &str, r: Ran) -> GhFail {
        let note = Note {
            first_err: why_words(&r.err),
            attempts: 1,
            stopped: None,
        };
        GhFail {
            line: line(word, &note),
            ran: Some(r),
        }
    }

    /// Whether the rule of `retry` owes this read another process: the answer
    /// said a 5xx, or a bare 403, or was cut off at its bound. An answerless
    /// failure was never a call the rule could class.
    fn owed(&self) -> bool {
        self.ran.as_ref().is_some_and(super::retry::owed_a_retry)
    }
}

impl Calls {
    /// Several reads of gh side by side, each answer as `gh` gives it.
    pub fn gh_all(&mut self, calls: &[Vec<String>]) -> Vec<Result<Value, GhFail>> {
        let bound = self.bound;
        let gh = self.gh.clone();
        // The token, owned, so each spawned call holds its own view of it and
        // the borrow of self ends here.
        let token = self.gh_token.clone();
        let mut out: Vec<Result<Value, GhFail>> = Vec::new();
        for chunk in calls.chunks(8) {
            let answers: Vec<Result<Value, GhFail>> = std::thread::scope(|s| {
                let handles: Vec<_> = chunk
                    .iter()
                    .map(|args| {
                        let gh = gh.clone();
                        let token = token.clone();
                        s.spawn(move || {
                            let word = format!("ERROR: gh {} {} failed", args[0], args[1]);
                            if !gh_reads(args) {
                                // No process ran, so the rule has no answer to
                                // class and the read owes no retry.
                                return Err(GhFail::words(format!(
                                    "{word}: refused, it is no read"
                                )));
                            }
                            let extra = gh_env(token.as_deref());
                            // The chunk runs the read once, as it always did, on
                            // this thread: a read the chunk could not answer is
                            // tried again serially on the main thread below, so
                            // no retry of a gh read ever runs on a thread.
                            let r = run_input_env(&gh, args, None, bound, &extra);
                            if r.ok() {
                                Ok(parse(&r.out))
                            } else {
                                Err(GhFail::chunk(&word, r))
                            }
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    // A worker that died leaves no answer for the rule to class,
                    // so the slot says the words it always said and is not tried
                    // again: nothing was read, and no retry is owed to a thread
                    // that did not return.
                    .map(|h| {
                        h.join()
                            .unwrap_or_else(|_| Err(GhFail::words("ERROR: gh failed".into())))
                    })
                    .collect()
            });
            self.gh_count += chunk.len() as u32;
            out.extend(answers);
        }
        // What the chunk could not answer is asked again one at a time, on this
        // thread, while the rule of `retry` and the poll's budget allow - of the
        // answer the chunk got, exactly as `Calls::gh` asks it of its own. A read
        // that answers nothing the second time keeps the answer the chunk had,
        // which is the words the poll had before the retry existed. A read that
        // was refused outright is not asked again at all: the rule owes a retry to
        // a bad minute, not to a refusal, and a second process of the same refusal
        // would cost the poll one call for nothing.
        let policy = self.retry.clone();
        for (args, slot) in calls.iter().zip(out.iter_mut()) {
            let owed = match slot {
                Ok(_) => false,
                Err(f) => f.owed(),
            };
            let word = format!("ERROR: gh {} {} failed", args[0], args[1]);
            if !owed || !gh_reads(args) {
                continue;
            }
            // The chunk's own call is already counted, so only the retries after
            // it are new processes of the poll, and a read GitHub refused outright
            // never reaches this line at all.
            let extra = gh_env(self.gh_token.as_deref());
            let outcome = policy.read(&word, &mut || {
                run_input_env(&self.gh, args, None, bound, &extra)
            });
            self.gh_count += outcome.attempts.saturating_sub(1);
            if outcome.ran.ok() {
                // A bad minute the retry cleared leaves no line: the poll reads
                // the answer and nothing of the outage.
                *slot = Ok(parse(&outcome.ran.out));
            }
        }
        out
    }

    /// gh's answer as JSON, Err with the line the poll logs.
    pub fn gh(&mut self, args: &[&str]) -> Result<Value, GhFail> {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let word = format!("ERROR: gh {} {} failed", args[0], args[1]);
        if !gh_reads(&args) {
            return Err(GhFail::words(format!("{word}: refused, it is no read")));
        }
        let extra = gh_env(self.gh_token.as_deref());
        let outcome = self.retry.read(&word, &mut || {
            run_input_env(&self.gh, &args, None, self.bound, &extra)
        });
        self.gh_count += outcome.attempts;
        if !outcome.ran.ok() {
            let line = line(&word, &Note::of(&outcome, &outcome.ran));
            return Err(GhFail {
                line,
                ran: Some(outcome.ran),
            });
        }
        Ok(parse(&outcome.ran.out))
    }

    /// gh's output as JSON, whatever its exit status: the bash reads some
    /// answers so, keeping what came back of a partial one.
    pub fn gh_any(&mut self, args: &[&str]) -> Option<Value> {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        if !gh_reads(&args) {
            return None;
        }
        self.gh_count += 1;
        let extra = gh_env(self.gh_token.as_deref());
        let r = run_input_env(&self.gh, &args, None, self.bound, &extra);
        serde_json::from_str(&r.out).ok()
    }

    /// The answers of several store reads, asked in one run of the store
    /// (`store batch`, #941). Each element is the arguments of one call, given
    /// back in the same order as `store` would have answered it, so a caller
    /// reads an answer exactly as it reads the answer of the single call. The
    /// failure of the batch as a whole is the failure of any one of its calls:
    /// one process stood for all of them, and a poll that could not ask them
    /// could not have asked one of them either.
    ///
    /// The answer is bounded as the request is: the store bounds what it reads
    /// and not what it prints, and one call of `plan-show` may print a brief of
    /// its own bound, so the whole of a queue's briefs is a legal answer of one
    /// batch. Nothing past the bound of the store's own request is read, and an
    /// answer that goes past it is the garbled answer it is.
    pub fn store_batch(&mut self, calls: &[Vec<String>]) -> Result<Vec<Value>, Value> {
        if calls.is_empty() {
            return Ok(Vec::new());
        }
        // A `Calls` that names no home has no store to ask: the failure says so
        // rather than handing the store an empty `TEHAS_HOME` and reading its
        // refusal of the environment as the absence of the program.
        let Some(home) = self.home.clone() else {
            return Err(store_failed(
                78,
                "absent",
                "store: absent: the poll has no home to name the store's file",
            ));
        };
        let request = serde_json::Value::Array(
            calls
                .iter()
                .map(|c| serde_json::Value::Array(c.iter().map(|a| json!(a)).collect()))
                .collect(),
        )
        .to_string();
        // The program is tested before it runs, and the home is given to the
        // child, both as tehas_core::call does for every verb: one that is not an
        // executable file is `absent`, which is the answer a poll whose store was
        // never installed gives, and the only one it can give.
        if !store_executable(&self.store) {
            return Err(store_failed(
                78,
                "absent",
                &format!("store: absent: {} is not an executable file", self.store),
            ));
        }
        let extra = [("TEHAS_HOME", home.as_str())];
        let r = run_input_bounded(
            &self.store,
            &["batch".into()],
            Some(&request),
            self.store_bound,
            &extra,
            BATCH_BOUND,
        );
        // The exit ladder is the bash's, shared with the single call. An answer
        // past the ceiling is no answer either: the store bounds its request and
        // not what it prints, and the poll keeps neither the memory nor the
        // seconds of a queue's worth of briefs.
        if r.truncated {
            return Err(store_failed(
                78,
                "garbled",
                &format!(
                    "store: garbled: batch answered over the {BATCH_BOUND} bytes of one request"
                ),
            ));
        }
        if let Some((rc, word, line)) = store_failure("batch", &r, self.store_bound, &self.store) {
            return Err(store_failed(rc, &word, &line));
        }
        // The bash reads the answer with stderr merged, so anything there makes
        // it no answer, and the answer is one line of the shape `store` gives.
        let said: Value = if r.err.is_empty() {
            serde_json::from_str(r.out.trim()).unwrap_or(Value::Null)
        } else {
            Value::Null
        };
        let answers = said
            .get("answers")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if said.get("api") != Some(&json!(1))
            || said.get("ok") != Some(&json!(true))
            || said.get("verb").and_then(Value::as_str) != Some("batch")
            || answers.len() != calls.len()
        {
            return Err(store_failed(
                78,
                "garbled",
                "store: garbled: batch answered 0 without its answer",
            ));
        }
        Ok(answers
            .into_iter()
            .map(|a| {
                // One call's answer, read as the answer of that call: the exit
                // it had, and what it printed, which is the one line the verb
                // writes and so the one JSON object the single call gives. An
                // element whose shape is not that is no answer of the store, and
                // is read as one: the garbled 78 the single call gives for the
                // same bytes, not a refusal of a word of nothing.
                let Some(rc) = a.get("rc").and_then(Value::as_i64) else {
                    return store_failed(
                        78,
                        "garbled",
                        "store: garbled: an answer of the batch holds no exit",
                    );
                };
                let Some(text) = a.get("out").and_then(Value::as_str) else {
                    return store_failed(
                        78,
                        "garbled",
                        "store: garbled: an answer of the batch holds no line",
                    );
                };
                if rc == 0 {
                    match serde_json::from_str(text) {
                        Ok(v) => v,
                        Err(_) => store_failed(
                            78,
                            "garbled",
                            &format!("store: garbled: a read answered no JSON: {text}"),
                        ),
                    }
                } else {
                    match word_of(text) {
                        // The word the verb's own line says, as the single call
                        // reads it, and the same garbled answer for a refusal
                        // that does not name one.
                        Some(word) => store_failed(rc as i32, word, text),
                        None if rc == 1 => store_failed(
                            78,
                            "garbled",
                            &format!("store: garbled: a read refused without a word: {text}"),
                        ),
                        None => store_failed(rc as i32, "", text),
                    }
                }
            })
            .collect())
    }

    /// The store's answer to a read verb, or its failure as tehas_core::call
    /// keeps it: {failed, word, rc, line}.
    pub fn store(&mut self, args: &[&str]) -> Result<Value, Value> {
        call::store(&self.store, args, self.store_bound, GRACE, &[])
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use tehas_core::testdir::TestDir;

    /// The cases of the poll's token read and write one environment variable,
    /// so one of them runs at a time.
    static TOKEN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn a(x: &[&str]) -> Vec<String> {
        x.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn only_reads_pass() {
        assert!(gh_reads(&a(&[
            "api",
            "graphql",
            "-F",
            "owner=a",
            "-f",
            "query=query { viewer { login } }"
        ])));
        assert!(!gh_reads(&a(&[
            "api",
            "graphql",
            "-f",
            "query=mutation { x }"
        ])));
        assert!(gh_reads(&a(&["api", "user"])));
        assert!(gh_reads(&a(&[
            "api",
            "repos/a/b/issues/7/events?per_page=100",
            "--paginate"
        ])));
        assert!(!gh_reads(&a(&[
            "api",
            "-X",
            "PATCH",
            "repos/a/b/issues/comments/1"
        ])));
        assert!(!gh_reads(&a(&[
            "api",
            "repos/a/b/issues/7/labels",
            "-f",
            "labels[]=x"
        ])));
        assert!(gh_reads(&a(&["pr", "list", "--repo", "a/b"])));
        assert!(!gh_reads(&a(&["issue", "edit", "7"])));
        assert!(!gh_reads(&a(&["issue", "comment", "7"])));
        assert!(!gh_reads(&a(&[
            "api",
            "-XPOST",
            "repos/a/b/issues/7/labels"
        ])));
        assert!(!gh_reads(&a(&[
            "api",
            "repos/a/b/issues/7/labels",
            "-fa=b"
        ])));
        assert!(!gh_reads(&a(&[
            "api",
            "repos/a/b/issues/7/labels",
            "--field=a=b"
        ])));
        assert!(!gh_reads(&a(&[
            "api",
            "graphql",
            "--input=x",
            "-f",
            "query=query { a }"
        ])));
        assert!(!gh_reads(&a(&[
            "api",
            "graphql",
            "-XPOST",
            "-f",
            "query=query { a }"
        ])));
    }

    #[test]
    fn pages_are_one_list() {
        assert_eq!(parse("[1,2][3]"), serde_json::json!([1, 2, 3]));
        assert_eq!(parse("{\"a\":1}"), serde_json::json!({"a": 1}));
        assert_eq!(parse("not json"), serde_json::json!("not json"));
    }

    /// A host of one's own for a case of the token: `libexec/github-token` of
    /// the home counts its runs and answers a token of its own, and the stub gh
    /// beside it answers a read with the token it was given. `counter` asks for
    /// The program of a case that only fails it does not need.
    /// The directory is named by the case, so two cases never share a counter.
    fn token_home(name: &str, counter: bool) -> Option<TestDir> {
        let dir = TestDir::new(name);
        if std::fs::create_dir_all(dir.join("libexec")).is_err() {
            return None;
        }
        use std::os::unix::fs::PermissionsExt;
        let token = dir.join("libexec/github-token");
        // The program of a case that fails at making one says its reason on the
        // error stream and ends non-zero, as libexec/github-token does.
        let body = if counter {
            "#!/bin/sh\n[ \"$1\" = settled ] && exit 0\ncount=\"$0.count\"\n\
             n=$(cat \"$count\" 2>/dev/null || echo 0)\n\
             n=$((n + 1)); echo \"$n\" >\"$count\"\nprintf 'ghs_%s\\n' \"$n\"\n"
        } else {
            "#!/bin/sh\n[ \"$1\" = settled ] && exit 0\n\
             printf 'tehas: no GitHub App is set\\n' >&2\nexit 1\n"
        };
        std::fs::write(&token, body).ok()?;
        std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o755)).ok()?;
        let gh = dir.join("gh");
        std::fs::write(
            &gh,
            "#!/bin/sh\n[ \"$1\" = settled ] && exit 0\n\
             printf '%s\\n' \"${TEHAS_GH_TOKEN:-untouched}\"\n",
        )
        .ok()?;
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).ok()?;
        settled(&token);
        settled(&gh);
        Some(dir)
    }

    /// Wait until a script just written can run. A parallel test's fork holds
    /// it open for writing until that child's exec, and a run in that moment
    /// fails with ETXTBSY.
    pub(in crate::read) fn settled(script: &Path) {
        let until = std::time::Instant::now() + Duration::from_secs(5);
        while std::process::Command::new(script)
            .arg("settled")
            .status()
            .is_err()
            && std::time::Instant::now() < until
        {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// The clock of a case of a retry: the case clock of `Case`, held so the case
    /// sleeps none of the waits of the rule and reads none of the host's time.
    /// `CASE_LOCK` guards the reading, which belongs to the process and not to
    /// the case, and `HOST_CLOCK` stands again when the `Case` is dropped. This is
    /// the only path to a clock in the tests: `Case::hold` is private, so no case
    /// can name a reading of its own and sleep or wait on the host's clock.
    fn measured() -> Case {
        Case::hold(TEST_CLOCK)
    }

    /// A `gh` of one case: its first `times` calls fail as GitHub does on a bad
    /// minute and every call after them answers the read with one issue. One
    /// `/bin/sh` script with no sleep in it, counting its own calls in a file the
    /// case reads to say how many processes a poll started. `settled` is the
    /// argument `settled` below runs it with to wait out a parallel test's fork.
    fn failing_gh(name: &str, times: u32) -> Option<TestDir> {
        let dir = TestDir::new(name);
        use std::os::unix::fs::PermissionsExt;
        let gh = dir.join("gh");
        let count = dir.join("gh.count").to_string_lossy().into_owned();
        // One `/bin/sh` script written as it will run, with `format!` only for the
        // count's file and the number of calls it fails: no sleep in it, and
        // `settled` answered so the wait below can see it run without meeting the
        // case's own failure path.
        let body = format!(
            r#"#!/bin/sh
[ "$1" = settled ] && exit 0
c={count}
n=$(cat "$c" 2>/dev/null || echo 0)
n=$((n + 1)); echo "$n" >"$c"
[ "$n" -le {times} ] && {{ printf 'HTTP 502: Bad Gateway\n' >&2; exit 1; }}
printf '[{{"number":1}}]\n'
"#
        );
        std::fs::write(&gh, body).ok()?;
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).ok()?;
        settled(&gh);
        Some(dir)
    }

    /// How many calls the stub of a retry case has answered. Its own exit names
    /// the reason when the file is not there or holds no number: a stub that
    /// answered nothing is the failure this case is about, and reading it as a
    /// count of 0 would let a poll that started no process pass an assertion of
    /// the count.
    fn stub_calls(home: &Path) -> u32 {
        let said = std::fs::read_to_string(home.join("gh.count"))
            .unwrap_or_else(|e| panic!("the stub counted its calls: {e}"));
        said.trim()
            .parse()
            .unwrap_or_else(|e| panic!("the stub's count {:?} is no number: {e}", said.trim()))
    }

    /// The calls of a snapshot that asks its reads of the stub gh of `home`.
    fn calls_of(home: &Path) -> Calls {
        let mut calls = Calls::idle(
            Duration::from_secs(5),
            home.join("store").to_string_lossy().into_owned(),
        );
        calls.gh = home.join("gh").to_string_lossy().into_owned();
        calls
    }

    #[test]
    fn a_stub_that_fails_once_answers_on_the_second_try() {
        // `Calls::gh` asks the rule of `retry` now: a read that meets a bad
        // minute is made again and the answer of the second attempt is the one
        // the poll reads. One `/bin/sh` stub and exactly two calls of it for the
        // one read, so a retry starts no process beyond the retry itself and
        // waits for nothing but the rule's own bound.
        // The clock of the rule is a reading of the whole process, so the case
        // holds `CASE_LOCK` while it keeps one, and `TOKEN_LOCK` besides, which
        // the cases that name an app hold. Its budget and its lines are the
        // `Calls`' own.
        let _measured = measured();
        let _tokens = TOKEN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = match failing_gh("retry-once", 1) {
            Some(d) => d,
            None => return,
        };
        let mut calls = calls_of(&home);
        let got = calls
            .gh(&["api", "user"])
            .unwrap_or_else(|f| panic!("the second attempt answers, not {}", f.line));
        assert_eq!(got, serde_json::json!([{"number": 1}]));
        assert_eq!(stub_calls(&home), 2, "one read, tried once more");
        assert_eq!(calls.gh_count, 2, "both calls are the poll's cost");
        assert_eq!(
            calls.gh_count,
            stub_calls(&home),
            "the poll counted every gh call it made"
        );
        assert!(
            calls.retry.taken().is_empty(),
            "a read the retry cleared leaves no line: {:?}",
            calls.retry.taken()
        );
    }

    #[test]
    fn a_failed_read_leaves_its_line_for_the_poll() {
        // A read no answer clears leaves one line for the poll, with gh's own
        // words and that the rule tried it again: before this only the queue's
        // failure ever reached the log, and every other failed read became a
        // bare {"failed": true}. The stub never answers, so the call stops at the
        // bound of one attempt.
        let _measured = measured();
        let _tokens = TOKEN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = match failing_gh("retry-note", 9) {
            Some(d) => d,
            None => return,
        };
        let mut calls = calls_of(&home);
        let err = calls
            .gh(&["api", "user"])
            .expect_err("a gh that always fails answers nothing");
        assert_eq!(stub_calls(&home), 2, "made twice, never three times");
        // The poll's own counter and the stub's agree, as the case of `build.rs`
        // that counts a batch's calls pins them: neither can drift from the other
        // unnoticed, and a retry the rule took without the poll paying for it is
        // the disagreement this catches.
        assert_eq!(
            calls.gh_count,
            stub_calls(&home),
            "the poll counted every gh call it made"
        );
        let lines = calls.retry.taken();
        assert_eq!(lines.len(), 1, "one line for one failed read: {lines:?}");
        assert_eq!(err.line, lines[0], "the poll logs what the caller is given");
        assert!(
            err.line.starts_with("ERROR: gh api user failed: HTTP 502"),
            "gh's words of the list, not its body: {}",
            err.line
        );
        assert!(
            err.line.contains("tried 2 times"),
            "the line says the read was tried again: {}",
            err.line
        );
        assert!(
            err.line.contains("stopped at the bound of one call"),
            "and which bound stopped it: {}",
            err.line
        );
    }

    /// One read of gh, answered with the token the stub was given.
    fn given(calls: &mut Calls) -> String {
        match calls.gh(&["api", "user"]) {
            // gh's own answer keeps the newline printf gave it, as any command's
            // does; the token is what lies before it.
            Ok(v) => v.as_str().unwrap_or_default().trim_end().to_string(),
            Err(f) => panic!("the stub gh answers, not {}", f.line),
        }
    }

    #[test]
    fn a_poll_takes_the_token_once_however_many_calls_it_makes() {
        // The poll's order: the token is taken before its first call, and the
        // calls of the poll go after it. The poll takes it with `read_gh_token`
        // and gives what it printed to every `Calls` of the poll, which is what
        // the poll of `poll.rs` does for its reads and its writes.
        let _guard = TOKEN_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        // With an app named, one run of the token program covers the whole
        // poll, and every gh call of it sees the same token.
        std::env::set_var("TEHAS_GITHUB_APP_ID", "5452597");
        let home = match token_home("token-once", true) {
            Some(d) => d,
            None => return,
        };
        let taken = read_gh_token(&home, Duration::from_secs(5));
        assert_eq!(taken.as_deref(), Some("ghs_1"));
        let mut calls = calls_of(&home);
        calls.gh_token = taken.clone();
        for _ in 0..5 {
            assert_eq!(given(&mut calls), "ghs_1");
        }
        assert_eq!(calls.gh_token, taken);
        assert_eq!(calls.gh_count, 5);
        // Nothing between the take above and these five calls ran the token
        // program: the poll's gh calls were served by the token it kept. A
        // second take asks the program again, which is why the poll keeps what
        // it took instead of asking for every call.
        assert_eq!(
            read_gh_token(&home, Duration::from_secs(5)).as_deref(),
            Some("ghs_2")
        );
        let n = std::fs::read_to_string(home.join("libexec/github-token.count"))
            .unwrap_or_default()
            .trim()
            .to_string();
        assert_eq!(n, "2", "the poll ran the token program once for its calls");

        // A write of the poll is given the token in the same way.
        let home = match token_home("token-write", true) {
            Some(d) => d,
            None => return,
        };
        let mut calls = calls_of(&home);
        calls.gh_token = read_gh_token(&home, Duration::from_secs(5));
        let gh = calls.gh.clone();
        let extra = gh_env(calls.gh_token.as_deref());
        let r = run_input_env(
            &gh,
            &a(&["issue", "comment", "7"]),
            Some("a body"),
            Duration::from_secs(5),
            &extra,
        );
        assert!(r.ok());
        assert_eq!(r.out.trim(), "ghs_1");

        // With no app named the poll takes nothing and gh is given nothing:
        // each call pays for a token of its own, as it did before.
        std::env::remove_var("TEHAS_GITHUB_APP_ID");
        let home = match token_home("token-none", true) {
            Some(d) => d,
            None => return,
        };
        let mut calls = calls_of(&home);
        calls.gh_token = read_gh_token(&home, Duration::from_secs(5));
        for _ in 0..3 {
            assert_eq!(given(&mut calls), "untouched");
        }
        assert_eq!(calls.gh_token, None);
        assert_eq!(calls.gh_count, 3);
        assert!(
            !home.join("libexec/github-token.count").exists(),
            "no token program runs when the instance names no app"
        );
    }

    #[test]
    fn a_token_that_could_not_be_had_leaves_the_calls_as_they_were() {
        let _guard = TOKEN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = match token_home("token-failed", false) {
            Some(d) => d,
            None => return,
        };
        use std::os::unix::fs::PermissionsExt;
        let program = home.join("libexec/github-token");
        std::env::set_var("TEHAS_GITHUB_APP_ID", "5452597");
        let mut calls = calls_of(&home);

        // The program is not there at all: nothing is kept, and the calls go
        // as they always did, each asking for a token of its own.
        std::fs::remove_file(&program).ok();
        calls.gh_token = read_gh_token(&home, Duration::from_secs(5));
        assert_eq!(calls.gh_token, None);
        assert_eq!(given(&mut calls), "untouched");

        // One that answers nothing is no token; one that prints a word of a
        // token and still fails is no token; and the one of this home, which
        // says its reason and ends 1, is no token either.
        for body in [
            "#!/bin/sh\nexit 0\n",
            "#!/bin/sh\nprintf 'ghs_x\\n'\nexit 1\n",
            "#!/bin/sh\nprintf 'tehas: no GitHub App is set\\n' >&2\nexit 1\n",
        ] {
            std::fs::write(&program, body).expect("the token program is written");
            let _ = std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755));
            calls.gh_token = read_gh_token(&home, Duration::from_secs(5));
            assert_eq!(calls.gh_token, None, "a failed run of {body}");
            assert_eq!(given(&mut calls), "untouched");
        }
        std::env::remove_var("TEHAS_GITHUB_APP_ID");
    }
}

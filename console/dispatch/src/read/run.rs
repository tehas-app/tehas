//! The programs a snapshot asks: gh, the state store and the host's own
//! tools, each under a time bound. Only reads leave here: a gh call that could
//! change anything is refused before it runs. How a program is run is
//! tehas_core::call.

use super::retry::{self, retry, Budget, Outcome, Stopped};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tehas_core::call::{self, store_executable, store_failed, store_failure, word_of, GRACE};
pub use tehas_core::call::{
    gh_env, gh_why, run, run_grace, run_grace_env, run_input, run_input_bounded, run_input_env,
    run_quiet, wait, Ran,
};

/// The reading of the host's own clock, which is what `poll_now` gives when no
/// case has set `CASE_NOW`. No second of the real clock reaches it.
const HOST_CLOCK: u64 = u64::MAX;

/// The case clock a test of a retry holds, as the cases of `retry.rs` do: a
/// reading of the clock the rule measures its waits against, so the case sleeps
/// none of them and no case of the workspace waits on a retry.
pub const TEST_CLOCK: u64 = 1_000;

/// The clock of `poll_now`: the host's own clock while `CASE_NOW` holds
/// `HOST_CLOCK`, and the seconds a case has set, so that the case sleeps none of
/// the waits of the rule and reads none of the host's time.
static CASE_NOW: AtomicU64 = AtomicU64::new(HOST_CLOCK);

/// The cases that write the clock of `poll_now`, and so must not run at once:
/// a reading of the clock belongs to the process, not to the case that set it.
static CASE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The clock the rule of `retry` measures its waits against, and the clock the
/// rule itself reads. Whole seconds, as the rule's `WAITS` are whole seconds.
pub fn poll_now() -> Option<u64> {
    match CASE_NOW.load(Ordering::Relaxed) {
        HOST_CLOCK => SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs()),
        at => Some(at),
    }
}

/// The thread that made a call sleeps out the waits the rule named, unless a
/// case has set `CASE_NOW` and so waits for nothing: the rule of `retry.rs` runs
/// no process and no thread and no case of it sleeps, and no case of the
/// workspace either starts a process or waits on a retry it did not ask for.
fn sleep_out(waited: u64) {
    if CASE_NOW.load(Ordering::Relaxed) == HOST_CLOCK {
        std::thread::sleep(Duration::from_secs(waited));
    }
}

/// The rule of `retry` applied to one gh call, and the waits slept on the thread
/// that made it. `call` runs the program of one attempt; no attempt ever runs on
/// a thread of its own.
pub fn tried(call: &mut dyn FnMut() -> Ran, budget: &mut Budget) -> Outcome {
    let outcome = retry(call, budget, poll_now);
    for waited in &outcome.waits {
        sleep_out(*waited);
    }
    outcome
}

/// What the retry of one failed read costs the poll: the words the failed
/// attempt printed, and the bound or the clock that stopped the retry. The
/// retry rule is asked here and not by the caller, so the words are the rule's
/// `Outcome` and nothing else.
pub struct Note {
    /// What the answer the caller keeps did not say: the words of the attempt
    /// the retry replaced, which `gh_why` puts in the failed line.
    pub first_err: Option<String>,
    /// How many times the read was made, which a line says only when the retry
    /// was more than nothing.
    pub attempts: u32,
    /// A retry the rule asked for and no bound served, named; nothing when the
    /// answer the rule kept stood.
    pub stopped: Option<&'static str>,
}

impl Note {
    /// What the retry of a read did: the rule's `Outcome` in the words a log
    /// line is made of. `ran` is the answer the caller keeps, whose error stream
    /// is what `gh_why` reads.
    pub fn of(outcome: &Outcome, ran: &Ran) -> Note {
        Note {
            first_err: gh_why_err(&outcome.first_err, ran),
            attempts: outcome.attempts,
            stopped: match outcome.stopped {
                Stopped::Answered => None,
                Stopped::AtCap => Some("at the bound of one call"),
                Stopped::NoClock => Some("with no clock to measure the wait"),
                Stopped::NoBudget => Some("with the poll's retries spent"),
            },
        }
    }
}

/// What the failed line quotes: gh's words of the attempt the retry replaced
/// when there was one, and of the answer the caller keeps otherwise. So a read
/// that failed twice says what the first attempt said, which the answer now
/// held no longer carries, and a read that never retried says what it said.
fn gh_why_err(first_err: &Option<String>, ran: &Ran) -> Option<String> {
    let said = match first_err {
        Some(e) if !e.is_empty() => e.clone(),
        _ => ran.err.clone(),
    };
    let why = gh_why(&said);
    (!why.is_empty()).then(|| why.replacen(": ", "", 1))
}

/// The line one failed read leaves in the poll's log: `word` of gh's verb and
/// its path, the words `gh_why` kept, and the fact that the read was tried
/// again when the rule took it more than once. Nothing of a token is ever in
/// it: only the words of the fixed list reach the line.
pub fn line(word: &str, note: &Note) -> String {
    let mut out = match &note.first_err {
        Some(why) => format!("{word}: {why}"),
        None => word.to_string(),
    };
    if note.attempts > 1 || note.stopped.is_some() {
        out.push_str(&format!(" (tried {} times", note.attempts));
        if let Some(why) = note.stopped {
            out.push_str(&format!(", stopped {why}"));
        }
        out.push(')');
    }
    out
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
#[derive(Clone)]
pub struct RetryPolicy {
    /// The retries the poll may still pay for, of the whole `RETRIES_PER_CALLS`
    /// one snapshot is given.
    pub budget: Arc<Mutex<Budget>>,
    /// One line for each gh read the poll could not make, with gh's own words for
    /// it and, when the rule tried it again, how many attempts it got and what
    /// stopped it. `poll.rs` prints them, and a case of a failed read reads them.
    pub notes: Arc<Mutex<Vec<String>>>,
}

impl Default for RetryPolicy {
    fn default() -> RetryPolicy {
        RetryPolicy {
            budget: Arc::new(Mutex::new(Budget::new())),
            notes: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl RetryPolicy {
    /// One read of gh under the rule: `call` gives the answer of an attempt, and
    /// the line of a failure is kept for the poll whether or not the retry
    /// answered. The `Outcome` is the caller's to read, so it can tell a bad
    /// minute from a read GitHub refused.
    pub fn read(&self, word: &str, call: &mut dyn FnMut() -> Ran) -> Outcome {
        let outcome = tried(
            call,
            &mut self.budget.lock().unwrap_or_else(|e| e.into_inner()),
        );
        if !outcome.ran.ok() {
            self.log(word, &Note::of(&outcome, &outcome.ran));
        }
        outcome
    }

    /// One line for a read the poll could not make, kept in the order the poll
    /// asked for them.
    pub fn log(&self, word: &str, note: &Note) {
        self.notes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(line(word, note));
    }

    /// The lines of the reads that failed, oldest first, and the lines gone from
    /// the poll: what `poll.rs` prints at its end, so a line is said once however
    /// the poll ended.
    pub fn taken(&self) -> Vec<String> {
        let mut held = self.notes.lock().unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *held)
    }
}

/// The clock a case of a retry holds. `CASE_NOW` is a reading of the whole
/// process, so the cases that set it take `CASE_LOCK` one at a time, and the
/// `Case` restores the host's clock before its lock is given back.
pub struct Case {
    /// The lock of the cases that write `CASE_NOW`, held for one case. It is
    /// given up last: a field drops after `Drop::drop`, so no second case can
    /// set the clock while this one is still putting the host's back.
    held: std::sync::MutexGuard<'static, ()>,
}

impl Case {
    /// Take the clock for one case: the reading of `CASE_NOW` the rule measures
    /// its waits against, held until the case ends.
    pub fn hold(at: u64) -> Case {
        let held = CASE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        CASE_NOW.store(at, Ordering::Relaxed);
        Case { held }
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        // The order is the point: the clock goes back to the host while the
        // case still holds `CASE_LOCK`, read here for that step alone.
        let _held = &self.held;
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
    /// store names its own database from it. lib/store.sh passes it explicitly;
    /// a `Calls` built by a test that means a store of its own sets it, and one
    /// that names no home has no store to ask.
    pub home: Option<String>,
    pub bound: Duration,
    /// lib/store.sh's tehas_store_bound.
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
    /// them; a test that means a `Calls` of its own gets a policy of its own too.
    pub retry: RetryPolicy,
}

impl Calls {
    /// The calls of a poll's reads, with the store's bound and its path.
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
#[derive(Clone)]
pub struct GhFail(pub String);

/// The status of gh's that a chunk's line carries, if it carries one: the words
/// of `gh_why` stand at the end of the line, and they are the only part the rule
/// of `retry` is asked of. The verb and the path of the call come before them
/// and hold the issue's own number, so a number that happens to read like a
/// status of gh's is not mistaken for the server's own fault.
fn why_of(words: &str) -> Option<&str> {
    let at = words.rfind(": HTTP ")?;
    Some(&words[at + 2..])
}

/// What the rule of `retry` reads of a read the chunk could not answer. The
/// words of a `GhFail` are those of the fixed list `gh_why` keeps, and the rule
/// decides on the status inside them: a 5xx or a bare 403 is the bad minute it
/// owes a process to, and anything else is the answer the poll already has.
fn owed(slot: &Result<Value, GhFail>) -> bool {
    let words = match slot {
        Ok(_) => return false,
        Err(f) => f.0.as_str(),
    };
    let said = match why_of(words) {
        Some(s) => s,
        None => return false,
    };
    let ran = Ran {
        status: Some(1),
        out: String::new(),
        err: said.to_string(),
        truncated: false,
    };
    retry::owed_a_retry(&ran)
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
                                return Err(GhFail(format!("{word}: refused, it is no read")));
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
                                // The chunk's answer for a failure is the `word` line and `gh_why`'s
                                // words joined by a colon and a space, and it is the only place the two
                                // are joined: so the words of gh are what follows the last of them, and
                                // the path of the call, which holds the issue's own number and can name
                                // any digit at all, is not read by the rule.
                                Err(GhFail(format!("{word}: {}", gh_why(&r.err))))
                            }
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|h| {
                        h.join()
                            .unwrap_or_else(|_| Err(GhFail("ERROR: gh failed".into())))
                    })
                    .collect()
            });
            self.gh_count += chunk.len() as u32;
            out.extend(answers);
        }
        // What the chunk could not answer is asked again one at a time, on this
        // thread, while the rule of `retry` and the poll's budget allow. A read
        // that answers nothing the second time keeps the answer the chunk had,
        // which is the words the poll had before the retry existed. A read that
        // was refused outright is not asked again at all: the rule owes a retry to
        // a bad minute, not to a refusal, and a second process of the same refusal
        // would cost the poll one call for nothing.
        let policy = self.retry.clone();
        for (args, slot) in calls.iter().zip(out.iter_mut()) {
            if slot.is_ok() {
                continue;
            }
            let word = format!("ERROR: gh {} {} failed", args[0], args[1]);
            if !gh_reads(args) || !owed(slot) {
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
            return Err(GhFail(format!("{word}: refused, it is no read")));
        }
        let extra = gh_env(self.gh_token.as_deref());
        let outcome = self.retry.read(&word, &mut || {
            run_input_env(&self.gh, &args, None, self.bound, &extra)
        });
        self.gh_count += outcome.attempts;
        if !outcome.ran.ok() {
            return Err(GhFail(line(&word, &Note::of(&outcome, &outcome.ran))));
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
        // child, both as lib/store.sh does for every verb: one that is not an
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

    /// The store's answer to a read verb, or its failure as lib/store.sh
    /// keeps it: {failed, word, rc, line}.
    pub fn store(&mut self, args: &[&str]) -> Result<Value, Value> {
        call::store(&self.store, args, self.store_bound, GRACE, &[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cases of the poll's token read and write one environment variable,
    /// so one of them runs at a time.
    static TOKEN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// The next free name of a case's home under the temporary directory. The
    /// name is a directory of its own, made and removed by the case that made it
    /// and never nested under a root another case wipes: a stub program read
    /// while some other case is clearing a directory above it is no program at
    /// all, and the poll of the case reading it fails for the wrong reason.
    fn a_case_dir(name: &str) -> String {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        format!("tehas-{name}-{}-{n}", std::process::id())
    }

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
    fn token_home(name: &str, counter: bool) -> Option<std::path::PathBuf> {
        let dir = match std::fs::canonicalize(std::env::temp_dir()) {
            Ok(d) => d.join(a_case_dir(name)),
            Err(_) => return None,
        };
        let _ = std::fs::remove_dir_all(&dir);
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
    fn settled(script: &Path) {
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
    /// the case, and `HOST_CLOCK` stands again when the `Case` is dropped.
    type Measured = Case;

    fn measured() -> Measured {
        Case::hold(TEST_CLOCK)
    }

    /// A `gh` of one case: its first `times` calls fail as GitHub does on a bad
    /// minute and every call after them answers the read with one issue. One
    /// `/bin/sh` script with no sleep in it, counting its own calls in a file the
    /// case reads to say how many processes a poll started.
    fn failing_gh(name: &str, times: u32) -> Option<std::path::PathBuf> {
        let dir = match std::fs::canonicalize(std::env::temp_dir()) {
            Ok(d) => d.join(a_case_dir(name)),
            Err(_) => return None,
        };
        let _ = std::fs::remove_dir_all(&dir);
        if std::fs::create_dir_all(&dir).is_err() {
            return None;
        }
        use std::os::unix::fs::PermissionsExt;
        let gh = dir.join("gh");
        // Written line by line: a shell script of one `/bin/sh`, no sleep, and
        // the count of its own calls in a file beside it.
        let count = dir.join("gh.count").to_string_lossy().into_owned();
        let body = [
            "#!/bin/sh",
            "[ \"$1\" = settled ] && exit 0",
            &format!("c={count}"),
            "n=$(cat \"$c\" 2>/dev/null || echo 0)",
            "n=$((n + 1)); echo \"$n\" >\"$c\"",
            &format!(
                "[ \"$n\" -le {times} ] && {{ printf 'HTTP 502: Bad Gateway\\n' >&2; exit 1; }}"
            ),
            "printf '[{\"number\":1}]\\n'",
        ]
        .join("\n")
            + "\n";
        std::fs::write(&gh, body).ok()?;
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).ok()?;
        settled(&gh);
        Some(dir)
    }

    /// How many calls the stub of a retry case has answered.
    fn stub_calls(home: &Path) -> u32 {
        std::fs::read_to_string(home.join("gh.count"))
            .unwrap_or_default()
            .trim()
            .parse()
            .unwrap_or(0)
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
            .unwrap_or_else(|f| panic!("the second attempt answers, not {}", f.0));
        assert_eq!(got, serde_json::json!([{"number": 1}]));
        assert_eq!(stub_calls(&home), 2, "one read, tried once more");
        assert_eq!(calls.gh_count, 2, "both calls are the poll's cost");
        assert!(
            calls.retry.taken().is_empty(),
            "a read the retry cleared leaves no line: {:?}",
            calls.retry.taken()
        );
        let _ = std::fs::remove_dir_all(&home);
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
        let lines = calls.retry.taken();
        assert_eq!(lines.len(), 1, "one line for one failed read: {lines:?}");
        assert_eq!(err.0, lines[0], "the poll logs what the caller is given");
        assert!(
            err.0.starts_with("ERROR: gh api user failed: HTTP 502"),
            "gh's words of the list, not its body: {}",
            err.0
        );
        assert!(
            err.0.contains("tried 2 times"),
            "the line says the read was tried again: {}",
            err.0
        );
        assert!(
            err.0.contains("stopped at the bound of one call"),
            "and which bound stopped it: {}",
            err.0
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    /// One read of gh, answered with the token the stub was given.
    fn given(calls: &mut Calls) -> String {
        match calls.gh(&["api", "user"]) {
            // gh's own answer keeps the newline printf gave it, as any command's
            // does; the token is what lies before it.
            Ok(v) => v.as_str().unwrap_or_default().trim_end().to_string(),
            Err(f) => panic!("the stub gh answers, not {}", f.0),
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
        let _ = std::fs::remove_dir_all(&home);

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
        let _ = std::fs::remove_dir_all(&home);

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
        let _ = std::fs::remove_dir_all(&home);
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
        let _ = std::fs::remove_dir_all(&home);
    }
}

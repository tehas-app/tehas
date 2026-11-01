//! Which GitHub read is tried again, how long to wait, and when to stop: one
//! pure rule over the `Ran` a call gave, asked of a closure and measured
//! against a clock it is handed, so no case of it runs a process or sleeps.
//!
//! Nothing calls this yet: `Calls::gh` and `gh_all` in `run.rs` still try
//! once, and #943 wires the rule into them.
//!
//! The clock is read for one thing today: a retry is made only while it
//! answers, because a caller that cannot measure its wait must not begin one.
//! The seconds it gives belong to the caller to sleep, and no case of this
//! module sleeps. #943 gives the clock its second use, the deadline a poll
//! holds while it retries; the bounds here compose with the per-call bound of
//! `Calls::bound` rather than replacing it, so one call costs at most
//! `ATTEMPTS_PER_CALL` bounded processes and one poll at most
//! `RETRIES_PER_CALLS` of them, and the caller owns the overall deadline. The
//! reads of a `gh_all` chunk share a `Calls` and so share the budget: under a
//! correlated failure the reads that ask first are served first, which is the
//! ordering #943 can change and this module cannot see.
//!
//! The four policy numbers - the two bounds and the two waits - are constants
//! holding today's values. The issue that gives an instance a setting for
//! them is #943, which builds `Calls` and is where a `Policy` would sit
//! beside the `bound` it already holds.

use super::run::Ran;

/// The clock a call is measured against: seconds on a host, and a reading of
/// the case's own in its tests, so no case of the rule sleeps. The shape
/// `console/cycle/src/clock.rs`'s `Now` has; #943, which builds the `Calls`
/// that hold a clock, is the issue that gathers the workspace's clock types
/// into one module, and a third copy of the fn-pointer shape is what that
/// issue must not add.
pub type Now = fn() -> Option<u64>;

/// The words `run_input_env` puts on the error stream for a program it could
/// not spawn, of the form `"{program} could not be run"`: the one answer of a
/// no-exit call that a retry cannot fix.
const NOT_RUN: &str = "could not be run";

/// How many times one call is made: itself and the one retry it is owed. A
/// caller that must state the bound it hit reads this rather than its own
/// copy: `Outcome::at_cap` is the same question asked of an answer.
pub const ATTEMPTS_PER_CALL: u32 = 2;

/// How many retries one `Calls` pays for, across all the reads of a snapshot:
/// the seventh of a poll is not made, and the bound stops it rather than
/// GitHub.
pub const RETRIES_PER_CALLS: u32 = 6;

/// The whole seconds a caller sleeps before the retry of attempt 1 and of
/// attempt 2, and nothing beyond them. `host/sandbox/gh-shim` waits
/// `TEHAS_GH_RETRY_WAIT` (60) of its own: a different wait for a different
/// class, since the shim sleeps out a limiter that resets on the minute and
/// this waits out a transient 5xx. The two are not meant to be equal, and an
/// operator who tunes the shim tunes it alone until #943 gives this a setting.
const WAITS: [u64; 2] = [1, 2];

/// Where a lower-case line names an HTTP status: `"http "` and the three
/// digits after it, with nothing glued around them. A byte that is a further
/// digit (`"http 5022"`) or a letter (`"0http 502"`) is no status: only a bare
/// three-digit run of gh's own says what a call got, so a body that merely
/// carries `"502"` is no server failure and a malformed run is not read as
/// one. Whether the number is a status at all is `class_of`'s question, not
/// this one. The scan is the one `console/core/src/forge/gh.rs`'s `find_http`
/// makes and `read/run.rs`'s `gh_why` reports, and
/// `status_agrees_with_what_the_poll_logs` pins the two reads against each
/// other so the rule cannot drift from what the poll logs. Promoting the one
/// reader into `tehas_core` is #943's, and the glued-token cases below are what
/// it carries with it.
fn http_of(line: &str) -> Option<u32> {
    let b = line.as_bytes();
    let head = b"http ";
    let mut i = 0;
    while i + head.len() + 3 <= b.len() {
        let glued_before = i > 0 && b[i - 1].is_ascii_alphanumeric();
        let glued_after = i + head.len() + 3 < b.len() && b[i + 8].is_ascii_digit();
        if b[i..i + head.len()] == head[..]
            && b[i + 5..i + 8].iter().all(u8::is_ascii_digit)
            && !glued_before
            && !glued_after
        {
            let d = &b[i + 5..i + 8];
            return Some(
                u32::from(d[0] - b'0') * 100 + u32::from(d[1] - b'0') * 10 + u32::from(d[2] - b'0'),
            );
        }
        i += 1;
    }
    None
}

/// What a status says the answer is. The classes are the rule's, not the
/// numbers': a 5xx is a class, and a number outside the band of a real status
/// is no class at all. The order is the weight the rule gives them, highest
/// first, and is the answer to "which line wins when one answer names two":
/// see `owed_a_retry`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Class {
    /// A number no status is: `"http 052"`, `"http 999"`. It explains nothing
    /// and decides nothing, as no status does.
    Nothing,
    /// A 403 that says no limit: the one 4xx that names neither a state of the
    /// object nor a fault, and the one a retry is owed.
    Unknown403,
    /// A server's own failure: a 5xx.
    Server,
    /// A state of the object asked for: a 404, a 422.
    State,
    /// A limit, plain or secondary: gh's own words, or a bare 429 without them.
    Limit,
}

/// The class of one status. The order of `Class` is the weight the rule gives
/// each, so which class wins when one answer names two is that list read from
/// the top: a limit above a state, a state above a server failure, and a bare
/// 403 above nothing but a number no status is. A limit outranks everything
/// because an answer that says "rate limit" has already explained why it
/// failed; a state outranks a server failure because an answer that knows the
/// object is no fault, whichever line carried which. The 403 is the one 4xx
/// that names no state, since a refusal for a policy can be transient; it does
/// say a limit when the answer says the words, and is then a limit as a 429 is,
/// as `host/sandbox/gh-shim` reads it.
fn class_of(code: u32, limited: bool) -> Class {
    if code == 429 || (limited && code == 403) {
        Class::Limit
    } else if code == 403 {
        Class::Unknown403
    } else if (400..=499).contains(&code) {
        Class::State
    } else if (500..=599).contains(&code) {
        Class::Server
    } else {
        Class::Nothing
    }
}

/// What one reading of an error stream said.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Said {
    /// The class of the answer: the highest its lines named. This is what the
    /// rule decides on.
    class: Class,
    /// The first status the answer named; None when no line named one, and none
    /// when no line named a number a status is. A word for the log and for a
    /// case that wants to see the number was read, never a second decision:
    /// only `owed_a_retry` names a number, the 403.
    code: Option<u32>,
}

impl Said {
    /// Whether the answer explains itself and says the same thing twice: a
    /// limit, or a 4xx that names a state of the object rather than a fault.
    /// The 403 is the one 4xx that explains nothing.
    fn explained(&self) -> bool {
        self.class == Class::Limit || self.class == Class::State
    }

    /// Whether the answer names a server's own failure.
    fn failed_by_server(&self) -> bool {
        self.class == Class::Server
    }
}

/// What an error stream says of a call, read from one lower-cased copy of it
/// and in one pass, so the words that decide a class and the number that
/// decides it come off the same reading. `host/sandbox/gh-shim` reads the same
/// stream whole: its `case` is `*http\ 429*` or `*http\ 403*rate\ limit*` over
/// the lower-cased answer, so the limit words and the number of a 403 are read
/// together there and here, whichever line each came on.
///
/// The class of the answer is the highest its lines name, not the first of
/// them: a 404 on any line holds a 5xx on another, and a limit anywhere holds
/// both.
fn read_of(err: &str) -> Said {
    let low = err.to_ascii_lowercase();
    // The words gh prints for a limit need no status line of their own: an
    // answer that says them is a limit whether or not it named a number, which
    // is the whole-stream `case` of the shim read the same way.
    let limited = low.contains("rate limit");
    let mut class = if limited {
        Class::Limit
    } else {
        Class::Nothing
    };
    let mut code = None;
    for line in low.lines() {
        let Some(s) = http_of(line) else {
            continue;
        };
        let said = class_of(s, limited);
        // A number no status is names nothing and carries no word: an answer
        // that says "http 052" has said nothing about itself.
        if said == Class::Nothing {
            continue;
        }
        // The first status of gh's own is the number kept, whatever class its
        // line named; a later line can raise the class of the answer and never
        // replace it. So the order gh printed its lines in decides nothing.
        code = code.or(Some(s));
        class = class.max(said);
    }
    Said { class, code }
}

/// Whether a call that gave no exit was cut off at our own bound: the answer
/// of `wait` for a program that outlived it (`None`, or 137 once it had to be
/// killed) that is not the answer of a program which never started. The words
/// `run_input_env` prints for a program it could not spawn are the one answer
/// a retry cannot fix, and they are the only words excluded here: a bounded
/// call that said something before the bound took it is still the bound's own
/// answer and is still owed the retry, because `run_input_env` drains both
/// readers before `wait` returns, so what a killed `gh` said is real. A 128
/// plus a signal number is the host's doing and not the clock's, and is no
/// bound's answer at all.
fn cut_off(r: &Ran) -> bool {
    match r.status {
        None | Some(137) => !r.err.contains(NOT_RUN),
        _ => false,
    }
}

/// Whether a call is owed another process. Three classes are, and each is
/// pinned by a case of its own; everything the answer explains is not. The
/// classes are `Class`, and `class_of` says which of them the lines of an
/// answer name and which of those wins.
/// - A server's own failure: a 5xx, when no limit and no state share it.
/// - A 403 that names no limit: `host/sandbox/gh-shim` does not call it a
///   limit unless the sentence says so, and a refusal for a transient policy
///   does clear. One 4xx, one class, one arm.
/// - A call we stopped at its bound, unless it carries the words that say it
///   never started.
///
/// What a retry is not owed: a 404, a 422 and a rate limit say the same thing
/// twice, and an exit of its own with no status of gh's is the program's
/// answer, not the transport's. The transport words `gh_why` and
/// `forge/gh.rs`'s `why` hold (`connection reset`, `no such host`, `timed
/// out`) buy no retry of their own either: such an answer is kept as the
/// answer the poll has, on purpose - a host that cannot resolve the API at all
/// is not cleared by a second process inside the same poll, and the poll ends
/// with the reason gh gave. The rule reads the error stream only, where a gh
/// read puts its refusal, and not the answer's standard output: `store_failure`
/// of `run.rs` does fall back to stdout, because the store gives the two
/// streams as one, and the gh reads have no such reading to answer - a gh
/// refusal whose error stream is empty is an exit with no words, and an exit
/// of its own is the program's answer. It does not
/// read `Ran::truncated`: no gh read today is given a ceiling, so a caller that
/// passes one must treat a cut answer as the store does, as garbled, and not
/// as something to try again.
pub fn owed_a_retry(r: &Ran) -> bool {
    if cut_off(r) {
        return true;
    }
    if r.ok() {
        return false;
    }
    let said = read_of(&r.err);
    if said.explained() {
        return false;
    }
    // The two classes that remain: a server's own failure, and the 403 that
    // named nothing.
    said.failed_by_server() || said.class == Class::Unknown403
}

/// How long to wait before the retry of attempt `attempts`: the whole seconds
/// a caller sleeps for, or None when no retry follows it, or when the clock
/// cannot be read and so no wait can be measured. `retry` waits for no one: the
/// caller that sleeps is #943's, and reads the `waits` of the `Outcome` it gets
/// back, which holds one entry per retry actually made.
fn wait_of(attempts: u32, now: Now) -> Option<u64> {
    let index = usize::try_from(attempts).ok()?.checked_sub(1)?;
    let secs = *WAITS.get(index)?;
    now()?;
    Some(secs)
}

/// Why a call stopped: the answer it kept, or the bound or the clock that
/// refused a retry the rule asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stopped {
    /// The answer the rule kept: nothing more was owed the call.
    Answered,
    /// A retry was asked for and the call had already made its
    /// `ATTEMPTS_PER_CALL` attempts.
    AtCap,
    /// A retry was asked for and no clock could measure the wait for it.
    NoClock,
    /// A retry was asked for and the `Budget` of the poll was spent.
    NoBudget,
}

/// The retries one `Calls` pays for, whatever the order of its reads. The
/// poll of one snapshot holds one and gives it to every call it makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    left: u32,
}

impl Default for Budget {
    fn default() -> Budget {
        Budget::new()
    }
}

impl Budget {
    /// The whole budget of one `Calls`.
    pub fn new() -> Budget {
        Budget {
            left: RETRIES_PER_CALLS,
        }
    }

    /// The retries this budget has left, which is what a test reads and a log
    /// line may want.
    pub fn left(&self) -> u32 {
        self.left
    }

    /// Spend one retry, giving whether the budget had it to give. A call told
    /// no retry keeps the answer of the attempt already made.
    pub fn spend(&mut self) -> bool {
        if self.left == 0 {
            return false;
        }
        self.left -= 1;
        true
    }
}

/// What one call did: the last answer it gave, how many times it ran, why it
/// stopped, the waits it was given, and what the attempts before the last one
/// said. `Ran` holds no `Debug` and `run.rs` is out of scope for this issue, so
/// the whole value is not derived: #943, which gives `Ran` its derives with the
/// rest of `Calls`, is where `Outcome` follows.
pub struct Outcome {
    /// The last answer of the call, the only one its caller reads.
    pub ran: Ran,
    /// How many times the call ran: one when the first answer was kept.
    pub attempts: u32,
    /// Why the call stopped, which names the bound that refused a retry the
    /// rule asked for. A poll log that says only "the read failed" loses the
    /// difference between GitHub, a spent budget, and a clock that cannot be
    /// read at all.
    pub stopped: Stopped,
    /// The whole seconds the caller slept before each attempt after the first,
    /// in order: one entry per retry actually made. The caller of a retry it
    /// has not yet made reads the length of this as the attempt it is on.
    pub waits: Vec<u64>,
    /// What the error stream of the first attempt said, when the call made
    /// more than one: the words the answer the caller now holds no longer
    /// carries, and which a retry that answered something else would hide.
    /// #943 appends them to the `GhFail` line when `attempts` is above one.
    pub first_err: Option<String>,
}

impl Outcome {
    /// Whether the call stopped having made all the attempts the rule allows,
    /// which is the question a caller would otherwise ask of a private bound.
    pub fn at_cap(&self) -> bool {
        self.stopped == Stopped::AtCap
    }

    /// Whether a retry the rule asked for went unserved: one of the bounds
    /// refused it, or a wait no clock could measure. A call whose answer the
    /// rule accepted is never said to have given up, however it ended.
    pub fn gave_up(&self) -> bool {
        self.stopped != Stopped::Answered
    }
}

/// One call, tried again while the rule asks and the bounds allow: `call`
/// gives the `Ran` of an attempt, `budget` is the poll's own, `now` the clock
/// the wait is measured against. The rule runs no program and waits for no
/// one: a caller that sleeps before the next attempt is #943's, and reads
/// `Outcome::waits` to know how long.
///
/// A call is made at most `ATTEMPTS_PER_CALL` times, and a `Calls` pays at
/// most `RETRIES_PER_CALLS` retries for all of its reads together.
pub fn retry(call: &mut dyn FnMut() -> Ran, budget: &mut Budget, now: Now) -> Outcome {
    let mut ran = call();
    let mut attempts = 1;
    let mut waits = Vec::new();
    let mut first_err = None;
    loop {
        if !owed_a_retry(&ran) {
            // An answer, or a failure the rule says to keep: the call is done
            // and nothing was owed it.
            return Outcome {
                ran,
                attempts,
                stopped: Stopped::Answered,
                waits,
                first_err,
            };
        }
        // A retry the rule asked for: the wait first, then the two bounds, so
        // the budget of the `Calls` is spent only on a call actually made.
        let waited = match wait_of(attempts, now) {
            Some(secs) => secs,
            None => {
                return Outcome {
                    ran,
                    attempts,
                    stopped: Stopped::NoClock,
                    waits,
                    first_err,
                }
            }
        };
        if attempts >= ATTEMPTS_PER_CALL {
            return Outcome {
                ran,
                attempts,
                stopped: Stopped::AtCap,
                waits,
                first_err,
            };
        }
        if !budget.spend() {
            return Outcome {
                ran,
                attempts,
                stopped: Stopped::NoBudget,
                waits,
                first_err,
            };
        }
        // The attempt is made: the wait it was owed is one the caller sleeps,
        // and the answer it replaces is kept for the poll's log.
        if first_err.is_none() {
            first_err = Some(ran.err.clone());
        }
        waits.push(waited);
        ran = call();
        attempts += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The clock every case is measured against: a fixed second, so none of
    /// them waits for anything.
    fn clock() -> Option<u64> {
        Some(1_000)
    }

    /// The clock of the case that has no reading: no wait can be measured, so
    /// no retry is made.
    fn no_clock() -> Option<u64> {
        None
    }

    /// What a call that failed printed: its exit and gh's own line.
    fn failed(status: i32, err: &str) -> Ran {
        Ran {
            status: Some(status),
            out: String::new(),
            err: err.to_string(),
            truncated: false,
        }
    }

    /// What a call that read its object printed.
    fn answered() -> Ran {
        Ran {
            status: Some(0),
            out: "[{\"number\":1}]".to_string(),
            err: String::new(),
            truncated: false,
        }
    }

    /// What a call that gave no exit at all printed: the answer of `wait` for
    /// a program past its bound, with nothing on either stream.
    fn stopped() -> Ran {
        Ran {
            status: None,
            out: String::new(),
            err: String::new(),
            truncated: false,
        }
    }

    /// What `run_input_env` gives when the program could not be spawned at
    /// all: no exit, and the words that say so.
    fn unspawnable(program: &str) -> Ran {
        Ran {
            status: None,
            out: String::new(),
            err: format!("{program} could not be run"),
            truncated: false,
        }
    }

    /// The rule over a queue of canned answers and a poll's whole budget: what
    /// the call returned, and how many times it was actually made. The count
    /// is what a case pins, so a rule that asked for more calls than it
    /// reported could not pass.
    fn call_of(answers: Vec<Ran>) -> (Outcome, u32) {
        let mut queue = answers.into_iter();
        let mut made = 0;
        let mut budget = Budget::new();
        let outcome = retry(
            &mut || {
                made += 1;
                queue
                    .next()
                    .expect("the rule asked for a call beyond the canned answers")
            },
            &mut budget,
            clock,
        );
        assert_eq!(made, outcome.attempts, "the attempts the rule reported");
        assert_eq!(
            made - 1,
            outcome.waits.len() as u32,
            "one wait per retry actually made"
        );
        (outcome, made)
    }

    /// How many times the rule made the call.
    fn tries(answers: Vec<Ran>) -> u32 {
        call_of(answers).1
    }

    #[test]
    fn a_502_then_an_answer_is_tried_once_more() {
        let (got, made) = call_of(vec![failed(1, "HTTP 502: Bad Gateway"), answered()]);
        assert_eq!(made, 2, "one retry, and no more");
        assert!(got.ran.ok(), "the answer of the second attempt is kept");
        assert!(!got.gave_up(), "an answer needs no retry");
        assert_eq!(got.stopped, Stopped::Answered);
        // The waits the rule names, in whole seconds; the case waits for none
        // of them.
        assert_eq!(got.waits, vec![WAITS[0]], "the caller slept this");
        assert_eq!(
            got.first_err.as_deref(),
            Some("HTTP 502: Bad Gateway"),
            "the answer the retry replaced is still known to the poll"
        );
        assert_eq!(wait_of(1, clock), Some(1));
        assert_eq!(wait_of(2, clock), Some(2));
        assert_eq!(wait_of(3, clock), None);
    }

    #[test]
    fn three_502s_stop_at_the_bound() {
        let bad = || failed(1, "HTTP 502: Bad Gateway");
        let (got, made) = call_of(vec![bad(), bad(), bad()]);
        // The rule keeps the third answer the original loop was given, so the
        // queue holds it though no process ran for it.
        assert_eq!(made, 2, "a call is made twice, never three times");
        assert_eq!(got.attempts, ATTEMPTS_PER_CALL);
        assert_eq!(got.ran.err, "HTTP 502: Bad Gateway");
        assert!(got.gave_up(), "the bound of a call kept the third try");
        // Which bound refused the third attempt, named rather than inferred
        // from the count: the caller's log line reads this and never has to
        // hold its own copy of a bound of this module.
        assert_eq!(got.stopped, Stopped::AtCap);
        assert!(got.at_cap(), "the cap it stopped at, named by the crate");
        assert_eq!(got.waits, vec![1], "the one retry that was made");
        assert_eq!(got.first_err.as_deref(), Some("HTTP 502: Bad Gateway"));
    }

    #[test]
    fn a_404_is_not_tried_again() {
        let (got, made) = call_of(vec![failed(1, "HTTP 404: Not Found"), answered()]);
        assert_eq!(made, 1);
        assert!(!got.gave_up(), "a 404 is an answer, not a failure to retry");
        assert_eq!(got.stopped, Stopped::Answered);
        assert_eq!(got.ran.status, Some(1));
        assert_eq!(got.ran.err, "HTTP 404: Not Found");
        assert_eq!(got.first_err, None, "nothing was replaced");
        assert!(got.waits.is_empty());
        assert_eq!(tries(vec![failed(1, "HTTP 422: Validation Failed")]), 1);
    }

    #[test]
    fn a_rate_limit_is_not_tried_again() {
        // The verdict is read of the `Ran`: its exit and the words gh prints.
        // `gh_why` is never called and no words of it are consulted.
        for err in [
            "HTTP 403: API rate limit exceeded for user",
            "error: API Secondary Rate Limit exceeded",
            "HTTP 429: Too Many Requests",
            "HTTP 200: rate limit remaining 0",
            // The 403 whose limit words sit apart from its status line: the
            // shim's `case` over the whole stream calls that a limit, and so
            // does this, so a host with the shim in PATH and one without it
            // keep the same answer.
            "HTTP 403: Forbidden\nAPI rate limit exceeded for user",
            "API rate limit exceeded for user\nHTTP 403: Forbidden",
            // A 429 that says nothing else: a limit is never worth a process.
            "gh: HTTP 429",
        ] {
            let (got, made) = call_of(vec![failed(1, err), answered()]);
            assert_eq!(made, 1, "a rate limit that says {err} is kept");
            assert!(!got.gave_up());
            assert_eq!(got.ran.err, err);
        }
        // The 403 that names no limit is not one: the shim's rule reads the
        // sentence with the number, and this reads the same answer, so the
        // read does not keep a failure the rest of the factory tries again.
        assert!(owed_a_retry(&failed(1, "HTTP 403: Forbidden")));
        assert_eq!(tries(vec![failed(1, "HTTP 403: Forbidden"), answered()]), 2);
    }

    #[test]
    fn an_answer_that_names_two_classes_is_decided_by_what_it_said() {
        // The precedence is the rule's and not the order of gh's lines, so a
        // refactor of the scan cannot change it in silence. Each ordering is
        // pinned both ways.
        for (err, owed) in [
            // A 4xx on any line holds a 5xx on another: the state wins.
            ("HTTP 404: Not Found\nHTTP 502: Bad Gateway", false),
            ("HTTP 502: Bad Gateway\nHTTP 404: Not Found", false),
            ("HTTP 404: Not Found\nHTTP 503: Service Unavailable", false),
            ("HTTP 503: Service Unavailable\nHTTP 404: Not Found", false),
            ("HTTP 502: Bad Gateway\nHTTP 422: Validation Failed", false),
            // A limit anywhere beats a server failure wherever it sits, and
            // beats it in both orders, as the shim's whole-stream `case` does.
            ("HTTP 502: Bad Gateway\nAPI rate limit exceeded", false),
            ("API rate limit exceeded\nHTTP 502: Bad Gateway", false),
            ("HTTP 502: Bad Gateway\nHTTP 429: Too Many Requests", false),
            ("HTTP 429: Too Many Requests\nHTTP 502: Bad Gateway", false),
            ("HTTP 429: Too Many Requests\nHTTP 404: Not Found", false),
            // Two server failures are one server failure.
            ("HTTP 502: Bad Gateway\nHTTP 503: Service Unavailable", true),
            // A 403 names neither state nor limit. Alone it is retried; beside
            // a state it is the state that answers, and beside a server
            // failure it is the failure that is retried.
            ("HTTP 403: Forbidden\nHTTP 502: Bad Gateway", true),
            ("HTTP 502: Bad Gateway\nHTTP 403: Forbidden", true),
            ("HTTP 403: Forbidden\nHTTP 404: Not Found", false),
            ("HTTP 404: Not Found\nHTTP 403: Forbidden", false),
            // The words gh prints for a transport failure are no class of
            // their own: kept, as the answer the poll has. `gh_why` gives them
            // a word of its own and the rule does not, so they are pinned
            // against the log rather than with the statuses.
            ("connection reset by peer", false),
            ("dial tcp: no such host", false),
            ("unexpected EOF while reading", false),
            ("net/http: request timed out", false),
            // Out-of-band digits are no class at all, so nothing explains them
            // and the exit of the program stands.
            ("http 999", false),
            ("http 000", false),
            ("http 052", false),
            ("HTTP/1.1 502 Bad Gateway", false),
        ] {
            let ran = failed(1, err);
            assert_eq!(owed_a_retry(&ran), owed, "the rule over {err:?} of exit 1");
        }
        // The bound's own answer is the bound's whatever a line of it says: a
        // KILL whose stderr happens to carry a 5xx is classed by its exit.
        assert!(owed_a_retry(&failed(
            137,
            "HTTP 500: Internal Server Error"
        )));
        // A signalled child is the host's doing however it reads.
        assert!(!owed_a_retry(&failed(143, "")));
    }

    #[test]
    fn a_call_cut_off_at_its_bound_is_tried_again() {
        // No exit at all: the answer of `wait` for a program past its bound.
        assert!(owed_a_retry(&stopped()));
        let (got, made) = call_of(vec![stopped(), answered()]);
        assert_eq!(made, 2);
        assert!(got.ran.ok());

        // 137 with nothing on the error stream: killed by the bound's KILL.
        let killed = Ran {
            status: Some(137),
            out: String::new(),
            err: String::new(),
            truncated: false,
        };
        assert!(owed_a_retry(&killed));
        assert_eq!(tries(vec![killed, answered()]), 2);

        // A bounded gh that said something before the bound took it is still
        // the bound's answer and is still owed the retry: the hang a degraded
        // GitHub produces prints a line and then stops answering, and the
        // bound is there for exactly that read.
        assert!(owed_a_retry(&failed(137, "connection reset by peer")));
        assert!(owed_a_retry(&Ran {
            status: None,
            out: String::new(),
            err: "context deadline exceeded".to_string(),
            truncated: false,
        }));

        // A child ended by a signal, or a call that ran out of time on its
        // own, is the host's doing and not the clock's: no process is owed.
        assert!(!owed_a_retry(&failed(143, "")));
        assert!(!owed_a_retry(&failed(124, "the call that ran out of time")));

        // A program that could not be spawned is the same kind of host fault:
        // the words that say so are kept, on no exit or on a 137, and the poll
        // pays no second process for a `gh` that is not there. The arm reads
        // those words, not the absence of words.
        let missing = unspawnable("gh");
        assert!(!owed_a_retry(&missing));
        let (got, made) = call_of(vec![missing, answered()]);
        assert_eq!(made, 1, "a program that could not be run is not retried");
        assert!(!got.gave_up(), "an answer the rule keeps is no retry owed");
        assert_eq!(got.ran.err, "gh could not be run");
        assert!(!owed_a_retry(&failed(137, "gh could not be run")));
    }

    #[test]
    fn a_default_budget_is_the_whole_budget() {
        assert_eq!(Budget::default(), Budget::new());
        assert_eq!(Budget::default().left(), RETRIES_PER_CALLS);
    }

    // Spawning a path that is not there starts no process. This holds NOT_RUN
    // to the words the real producer writes, which a literal in a case cannot.
    #[test]
    fn the_words_for_a_program_not_spawned_are_the_producers() {
        let ran = crate::read::run::run(
            "/nonexistent/tehas-retry-case",
            &[],
            std::time::Duration::from_secs(1),
        );
        assert!(ran.status.is_none());
        assert!(ran.err.contains(NOT_RUN), "{}", ran.err);
        assert!(!owed_a_retry(&ran));
    }

    #[test]
    fn the_seventh_retry_of_a_poll_is_not_made() {
        // Six reads of one poll each spend their retry; the seventh is
        // refused: it runs once and keeps that answer.
        let mut budget = Budget::new();
        for _ in 0..6 {
            let mut queue = [failed(1, "HTTP 502: Bad Gateway"), answered()].into_iter();
            let mut made = 0;
            let got = retry(
                &mut || {
                    made += 1;
                    queue.next().unwrap()
                },
                &mut budget,
                clock,
            );
            assert_eq!(made, 2, "the retry of one call is made");
            assert_eq!(got.attempts, ATTEMPTS_PER_CALL);
            // The retry served the failure, so the call did not give up.
            assert!(!got.gave_up());
        }
        assert_eq!(budget.left(), 0, "six retries were the whole budget");

        let mut queue = [failed(1, "HTTP 502: Bad Gateway"), answered()].into_iter();
        let mut made = 0;
        let got = retry(
            &mut || {
                made += 1;
                queue.next().unwrap()
            },
            &mut budget,
            clock,
        );
        assert_eq!(made, 1, "the seventh retry of a poll is not made");
        assert_eq!(got.attempts, 1);
        // The poll's budget, not the call's own cap, is what stopped it.
        assert_eq!(got.stopped, Stopped::NoBudget);
        assert!(got.gave_up());
        assert!(got.waits.is_empty(), "a refused retry slept nothing");
        assert_eq!(budget.left(), 0);
    }

    #[test]
    fn a_wait_that_cannot_be_measured_buys_no_retry() {
        let mut queue = [failed(1, "HTTP 502: Bad Gateway"), answered()].into_iter();
        let mut budget = Budget::new();
        let got = retry(&mut || queue.next().unwrap(), &mut budget, no_clock);
        assert_eq!(got.attempts, 1, "a wait no clock can name is not made");
        // Distinguishable from a spent budget and from a call at its cap,
        // which is what the poll's log has to say about it.
        assert_eq!(got.stopped, Stopped::NoClock, "and the log says so");
        assert_ne!(got.stopped, Stopped::NoBudget);
        assert_ne!(got.stopped, Stopped::AtCap);
        assert!(got.gave_up());
        assert!(got.waits.is_empty(), "nothing was slept");
        // Nothing was spent for a retry that was never begun: the budget of
        // the poll is there for the calls that can be made.
        assert_eq!(budget.left(), RETRIES_PER_CALLS);
    }

    #[test]
    fn only_the_digits_of_an_http_line_are_a_status() {
        // The digits are read where "http " says they are, so a gateway
        // failure whose body carries other numbers is no 404, and a line that
        // holds only the word is no status at all. `http_of` is the scan alone:
        // it gives the number, and `class_of` is what says whether one is a
        // status at all.
        assert_eq!(http_of("prefix http 503: service unavailable"), Some(503));
        assert_eq!(http_of("http 404: not found"), Some(404));
        assert_eq!(http_of("http 500"), Some(500));
        assert_eq!(http_of("502 of the 404 requests failed"), None);
        assert_eq!(http_of("http: no digits here"), None);
        assert_eq!(http_of(""), None);
        // A status is a whole token of gh's own: a longer run of digits, a
        // digit glued to it or a letter before the word is no answer of a
        // call, so an issue body echoed into stderr cannot forge one.
        assert_eq!(http_of("http 5022"), None);
        assert_eq!(http_of("0http 502"), None);
        assert_eq!(http_of("http%20502"), None);
        assert_eq!(http_of("http/1.1 502 bad gateway"), None);
        // Three digits are read as a number wherever `"http "` puts them, and a
        // number outside the band of a status is no class: it explains nothing
        // and decides nothing, and so carries no number either.
        assert_eq!(
            class_of(http_of("http 052").unwrap(), false),
            Class::Nothing
        );
        assert_eq!(
            class_of(http_of("http 999").unwrap(), false),
            Class::Nothing
        );
        assert_eq!(
            class_of(http_of("http 000").unwrap(), false),
            Class::Nothing
        );
        assert_eq!(class_of(http_of("http 452").unwrap(), false), Class::State);
        assert_eq!(class_of(http_of("http 502").unwrap(), false), Class::Server);
        assert_eq!(
            class_of(http_of("http 403").unwrap(), false),
            Class::Unknown403
        );
        assert_eq!(class_of(http_of("http 403").unwrap(), true), Class::Limit);
        assert_eq!(class_of(http_of("http 429").unwrap(), false), Class::Limit);
        // Nothing read, nothing classed. A number no status is carries no word
        // either: an answer that says it has said nothing about itself.
        assert_eq!(read_of("502 of the 404 requests failed").code, None);
        assert_eq!(
            read_of("http 052"),
            Said {
                class: Class::Nothing,
                code: None
            }
        );
        assert_eq!(read_of("http 999").code, None);
        assert_eq!(read_of("http 502").code, Some(502));
        assert_eq!(read_of("http 403").code, Some(403));
        assert_eq!(tries(vec![failed(1, "502 of the 404 requests failed")]), 1);
    }

    #[test]
    fn one_reading_gives_the_whole_verdict() {
        // The state, the server failure, the limit and nothing: one pass of
        // the answer gives all four, so the arm that decides and the words the
        // poll logs come off the same reading of the same string.
        assert_eq!(
            read_of("HTTP 429"),
            Said {
                class: Class::Limit,
                code: Some(429)
            }
        );
        assert_eq!(
            read_of("HTTP 403: Forbidden"),
            Said {
                class: Class::Unknown403,
                code: Some(403)
            }
        );
        assert_eq!(
            read_of("HTTP 502: Bad Gateway"),
            Said {
                class: Class::Server,
                code: Some(502)
            }
        );
        assert_eq!(
            read_of("no words of gh at all"),
            Said {
                class: Class::Nothing,
                code: None
            }
        );
        // The 403 is decided once, here: with the words it is a limit, without
        // them it is the class of its own, and no second place spells the
        // exception.
        assert_eq!(
            read_of("HTTP 403: Forbidden\nrate limit").class,
            Class::Limit
        );
        assert!(!read_of("HTTP 403: Forbidden").explained());
        assert!(read_of("HTTP 404: Not Found").explained());
        // The class of an answer is what decides, and the number is only a
        // word for the log: the first status the answer gave is the one kept,
        // whichever class its own line named, and the class is the highest of
        // them. So the 5xx gives way to the state and keeps its own number.
        assert_eq!(
            read_of("HTTP 502: Bad Gateway\nHTTP 404: Not Found"),
            Said {
                class: Class::State,
                code: Some(502)
            }
        );
        assert_eq!(
            read_of("HTTP 404: Not Found\nHTTP 502: Bad Gateway"),
            Said {
                class: Class::State,
                code: Some(404)
            }
        );
        assert!(read_of("HTTP 502: Bad Gateway\nHTTP 404: Not Found").explained());
        assert!(!read_of("HTTP 404: Not Found").failed_by_server());
        assert!(read_of("HTTP 502: Bad Gateway").failed_by_server());
    }

    #[test]
    fn status_agrees_with_what_the_poll_logs() {
        // The rule's read of a status and `gh_why`'s word for the same answer
        // are the same read: `forge/gh.rs`'s `find_http` locates the digits
        // that `why` reports, and `gh_why` is the poll's copy of it. If one
        // copy is corrected and the other is not, the poll retries a class of
        // answer it says it did not, so the two are pinned together here.
        // #943 promotes the one reader into `tehas_core`, and this case is what
        // keeps the two halves of the workspace honest until it does.
        for err in [
            "HTTP 502: Bad Gateway",
            "prefix HTTP 503: Service Unavailable",
            "HTTP 404: Not Found",
            "HTTP 429: Too Many Requests",
            "HTTP 403: Forbidden",
            "502 of the 404 requests failed",
            "HTTP: no digits here",
            // Out of band of a real status: neither read classes them, and
            // `gh_why` says nothing of them either, so an echoed body cannot
            // make the rule and the log disagree.
            "http 999",
            "http 000",
            // The glued forms: `gh_why`'s regex is not anchored the way this
            // read is, and says "http 502" of them. Neither classifies them, so
            // neither retries nor keeps the answer on their account - the one
            // place the two reads differ, held where a future shared reader
            // (#943) will have to settle it.
            "http 5022",
            "0http 502",
            "http%20502",
        ] {
            let said = read_of(err).class;
            let logged = crate::read::run::gh_why(err);
            if said == Class::Nothing {
                // A number no status is buys nothing from either read: the log
                // may name it as gh's word, but the rule gives the answer no
                // class and the exit decides, as it does for a line of no
                // status at all.
                assert!(!owed_a_retry(&failed(1, err)), "{err:?} was classed");
                continue;
            }
            let word = format!(": HTTP {}", read_of(err).code.unwrap());
            assert_eq!(
                logged.rfind(": ").map(|i| &logged[i..]),
                Some(word.as_str()),
                "the rule and the log disagree over {err:?}: {logged:?} against {said:?}"
            );
        }
    }
}

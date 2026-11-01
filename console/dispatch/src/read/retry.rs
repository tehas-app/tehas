//! Which GitHub read is tried again, how long to wait, and when to stop: one
//! pure rule over the `Ran` a call gave, asked of a closure and measured
//! against a clock it is handed, so no case of it runs a process or sleeps.
//!
//! Nothing calls this yet: `Calls::gh` and `gh_all` in `run.rs` still try
//! once, and #943 wires the rule into them.

use super::run::Ran;

/// The clock a call is measured against: seconds on a host, and a reading of
/// the case's own in its tests, so no case of the rule sleeps. The shape
/// `console/cycle/src/clock.rs`'s `Now` has.
pub type Now = fn() -> Option<u64>;

/// How many times one call is made: itself and the one retry it is owed.
const PER_CALL: u32 = 2;

/// How many retries one `Calls` pays for, across all the reads of a snapshot:
/// the seventh of a poll is not made, and the bound stops it rather than
/// GitHub.
const PER_CALLS: u32 = 6;

/// Where a lower-case line names an HTTP status: `"http "` and the three
/// digits after it. The digits are checked so that a line which only holds
/// the word is no status, and a body that merely carries "502" is no server
/// failure. The same read `console/core/src/forge/gh.rs`'s `find_http` makes
/// and `read/run.rs`'s `gh_why` reports, so the rule agrees with what the poll
/// logs.
fn http_of(line: &str) -> Option<u32> {
    let b = line.as_bytes();
    let head = b"http ";
    let mut i = 0;
    while i + head.len() + 3 <= b.len() {
        if b[i..i + head.len()] == head[..] && b[i + 5..i + 8].iter().all(u8::is_ascii_digit) {
            let d = &b[i + 5..i + 8];
            return Some(
                u32::from(d[0] - b'0') * 100 + u32::from(d[1] - b'0') * 10 + u32::from(d[2] - b'0'),
            );
        }
        i += 1;
    }
    None
}

/// The status GitHub answered with: the first HTTP status of what the call
/// printed on its error stream, where gh puts its own line. Nothing when no
/// line names one.
fn status_of(err: &str) -> Option<u32> {
    err.lines().find_map(|l| http_of(&l.to_ascii_lowercase()))
}

/// Whether the answer says a rate limit, plain or secondary: the words gh
/// prints, or the bare 429 that says it without them. `host/sandbox/gh-shim`
/// keeps the same rule - `*http\ 429*` or `*http\ 403*rate\ limit*` - so the
/// read and the container's shim call the same answer a limit: gh's own line
/// for a capacity or secondary limit names no limit at all (the case
/// `test/shim_writes_test.sh` pins reads "HTTP 429: Too Many Requests"), and a
/// 429 is never worth a second process. Deliberately left out of that rule are
/// the other words `gh_why` and `forge/gh.rs`'s `why` hold (`connection
/// reset`, `no such host`, `unexpected EOF`, `timed out`): they are no limit,
/// and the rule reads them as nothing - a transport failure is retried only as
/// a 5xx or as a call cut off at its bound.
fn rate_limited(err: &str) -> bool {
    let low = err.to_ascii_lowercase();
    if low.contains("rate limit") {
        return true;
    }
    matches!(status_of(&low), Some(429))
}

/// Whether the answer says it will say the same thing if asked again: a limit,
/// or a 4xx that names a state rather than a fault (a 404, a 422). A 403 with
/// no limit words is not one of them: `host/sandbox/gh-shim` reads it as no
/// limit unless the sentence says so, and an answer GitHub refused for a
/// transient policy is one a retry can clear.
fn explained(err: &str) -> bool {
    if rate_limited(err) {
        return true;
    }
    matches!(status_of(err), Some(s) if (400..=499).contains(&s) && s != 403)
}

/// Whether a call that gave no exit was cut off at our own bound: the answer
/// of `wait` for a program that outlived it (`None` with nothing on either
/// stream, or 137 once it had to be killed). The error stream is what tells
/// that apart from the two answers a retry cannot fix: 128 plus a signal
/// number is the host's doing, not the clock's, and `run_input_env` reports a
/// program it could not spawn as no exit plus the words "could not be run" -
/// as `store_failure` in `run.rs` keeps that apart from its `timeout` word.
/// A host with no `gh` says the same for every read of the poll, so a poll
/// told either is owed no further process.
fn cut_off(r: &Ran) -> bool {
    match r.status {
        None => r.err.is_empty(),
        Some(137) => r.err.is_empty(),
        _ => false,
    }
}

/// Whether a call is owed another process: a server's own failure (a 5xx), a
/// 403 that names no limit, or a call we stopped at its bound. Anything the
/// answer itself explains is not: a 404, a 422 and a rate limit say the same
/// thing twice. A 403 that names no limit is owed the retry because
/// `host/sandbox/gh-shim` does not call it a limit either, and a refusal for a
/// transient policy does clear. The decision is made on the `Ran` alone.
pub fn retries(r: &Ran) -> bool {
    if cut_off(r) {
        return true;
    }
    if r.ok() || explained(&r.err) {
        return false;
    }
    match status_of(&r.err) {
        Some(s) if (500..=599).contains(&s) => true,
        // Kept whole, as `says_not_found` of `forge/gh.rs` keeps a 404: an
        // exit of its own with no status of gh's is the program's answer, not
        // the transport's.
        Some(403) => true,
        _ => false,
    }
}

/// What the wait before the retry of attempt `attempts` costs: whole seconds,
/// one for the retry of a first attempt and two for the retry of a second.
/// Nothing when no retry follows it.
fn wait_seconds(attempts: u32) -> Option<u64> {
    match attempts {
        1 => Some(1),
        2 => Some(2),
        _ => None,
    }
}

/// How long to wait before the retry of attempt `attempts`: the whole seconds
/// a caller sleeps for, or None when no retry follows it, or when the clock
/// cannot be read and so no wait can be measured. `retry` waits for no one:
/// the caller that sleeps is #943's, and reads this to know how long.
fn wait_of(attempts: u32, now: Now) -> Option<u64> {
    let secs = wait_seconds(attempts)?;
    now()?;
    Some(secs)
}

/// The retries one `Calls` pays for, whatever the order of its reads. The
/// poll of one snapshot holds one and gives it to every call it makes.
pub struct Budget {
    left: u32,
}

impl Budget {
    /// The whole budget of one `Calls`.
    pub fn new() -> Budget {
        Budget { left: PER_CALLS }
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

impl Default for Budget {
    fn default() -> Budget {
        Budget::new()
    }
}

/// What one call did: the last answer it gave, how many times it ran, and
/// whether a retry it was owed went unserved.
pub struct Outcome {
    /// The last answer of the call, the only one its caller reads.
    pub ran: Ran,
    /// How many times the call ran: one when the first answer was kept.
    pub attempts: u32,
    /// The rule asked for a retry and none was served: one of the bounds
    /// refused it, or a wait no clock could measure. A call whose answer the
    /// rule accepted is never said to have given up, however it ended.
    pub gave_up: bool,
}

/// One call, tried again while the rule asks and the bounds allow: `call`
/// gives the `Ran` of an attempt, `budget` is the poll's own, `now` the clock
/// the wait is measured against. The rule runs no program and waits for no
/// one: a caller that sleeps before the next attempt is #943's, and reads
/// `wait_of` to know how long.
///
/// A call is made at most `PER_CALL` times, and a `Calls` pays at most
/// `PER_CALLS` retries for all of its reads together.
pub fn retry(call: &mut dyn FnMut() -> Ran, budget: &mut Budget, now: Now) -> Outcome {
    let mut ran = call();
    let mut attempts = 1;
    loop {
        if !retries(&ran) {
            // An answer, or a failure the rule says to keep: the call is done
            // and nothing was owed it.
            return Outcome {
                ran,
                attempts,
                gave_up: false,
            };
        }
        // A retry the rule asked for: the wait first, then the two bounds, so
        // the budget of the `Calls` is spent only on a call actually made.
        let waited = wait_of(attempts, now);
        if attempts >= PER_CALL || waited.is_none() || !budget.spend() {
            return Outcome {
                ran,
                attempts,
                gave_up: true,
            };
        }
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
    fn ask(answers: Vec<Ran>) -> (Outcome, u32) {
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
        (outcome, made)
    }

    /// How many times the rule made the call.
    fn tries(answers: Vec<Ran>) -> u32 {
        ask(answers).1
    }

    #[test]
    fn a_502_then_an_answer_is_tried_once_more() {
        let (got, made) = ask(vec![failed(1, "HTTP 502: Bad Gateway"), answered()]);
        assert_eq!(made, 2, "one retry, and no more");
        assert!(got.ran.ok(), "the answer of the second attempt is kept");
        assert!(!got.gave_up, "an answer needs no retry");
        // The waits the rule names, in whole seconds; the case waits for none
        // of them.
        assert_eq!(wait_of(1, clock), Some(1));
        assert_eq!(wait_of(2, clock), Some(2));
        assert_eq!(wait_of(3, clock), None);
    }

    #[test]
    fn three_502s_stop_at_the_bound() {
        let bad = || failed(1, "HTTP 502: Bad Gateway");
        let (got, made) = ask(vec![bad(), bad(), bad()]);
        assert_eq!(made, 2, "a call is made twice, never three times");
        assert_eq!(got.attempts, 2);
        assert_eq!(got.ran.err, "HTTP 502: Bad Gateway");
        assert!(got.gave_up, "the bound of a call kept the third try");
    }

    #[test]
    fn a_404_is_not_tried_again() {
        let (got, made) = ask(vec![failed(1, "HTTP 404: Not Found"), answered()]);
        assert_eq!(made, 1);
        assert!(!got.gave_up, "a 404 is an answer, not a failure to retry");
        assert_eq!(got.ran.status, Some(1));
        assert_eq!(got.ran.err, "HTTP 404: Not Found");
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
        ] {
            let (got, made) = ask(vec![failed(1, err), answered()]);
            assert_eq!(made, 1, "a rate limit that says {err} is kept");
            assert!(!got.gave_up);
            assert_eq!(got.ran.err, err);
        }
        // A limit named on a line of its own keeps the answer too, whatever
        // the status of another line says: it is the answer the poll has.
        assert!(!retries(&failed(
            1,
            "HTTP 502: Bad Gateway\nerror: API rate limit exceeded"
        )));
        // The 403 that names no limit is not one: the shim's rule reads the
        // sentence with the number, and this reads the same answer, so the
        // read does not keep a failure the rest of the factory tries again.
        assert!(retries(&failed(1, "HTTP 403: Forbidden")));
        assert_eq!(tries(vec![failed(1, "HTTP 403: Forbidden"), answered()]), 2);
    }

    #[test]
    fn a_call_cut_off_at_its_bound_is_tried_again() {
        // No exit at all: the answer of `wait` for a program past its bound.
        assert!(retries(&stopped()));
        let (got, made) = ask(vec![stopped(), answered()]);
        assert_eq!(made, 2);
        assert!(got.ran.ok());

        // 137 with nothing on the error stream: killed by the bound's KILL.
        let killed = Ran {
            status: Some(137),
            out: String::new(),
            err: String::new(),
            truncated: false,
        };
        assert!(retries(&killed));
        assert_eq!(tries(vec![killed, answered()]), 2);

        // A 137 that says why, or a signalled child (128 plus the signal), is
        // the host's doing and not the clock's: no process is owed for it.
        assert!(!retries(&failed(137, "signal: killed")));
        assert!(!retries(&failed(143, "")));
        assert!(!retries(&failed(124, "the call that ran out of time")));

        // A program that could not be spawned is the same kind of host fault:
        // no exit and the words that say so are kept, and the poll pays no
        // second process for a `gh` that is not there.
        let missing = unspawnable("gh");
        assert!(!retries(&missing));
        let (got, made) = ask(vec![missing, answered()]);
        assert_eq!(made, 1, "a program that could not be run is not retried");
        assert!(!got.gave_up, "an answer the rule keeps is no retry owed");
        assert_eq!(got.ran.err, "gh could not be run");
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
            assert_eq!(got.attempts, 2);
            // The retry served the failure, so the call did not give up.
            assert!(!got.gave_up);
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
        assert!(got.gave_up);
        assert_eq!(budget.left(), 0);
    }

    #[test]
    fn a_wait_that_cannot_be_measured_buys_no_retry() {
        let mut queue = [failed(1, "HTTP 502: Bad Gateway"), answered()].into_iter();
        let mut budget = Budget::new();
        let got = retry(&mut || queue.next().unwrap(), &mut budget, || None);
        assert_eq!(got.attempts, 1, "a wait no clock can name is not made");
        assert!(got.gave_up);
        // Nothing was spent for a retry that was never begun: the budget of
        // the poll is there for the calls that can be made.
        assert_eq!(budget.left(), PER_CALLS);
    }

    #[test]
    fn only_the_digits_of_an_http_line_are_a_status() {
        // The digits are read where "http " says they are, so a gateway
        // failure whose body carries other numbers is no 404, and a line that
        // holds only the word is no status at all.
        assert_eq!(status_of("prefix HTTP 503: Service Unavailable"), Some(503));
        assert_eq!(status_of("HTTP 404: Not Found"), Some(404));
        assert_eq!(status_of("http 500"), Some(500));
        assert_eq!(status_of("502 of the 404 requests failed"), None);
        assert_eq!(status_of("HTTP: no digits here"), None);
        assert_eq!(status_of(""), None);
        assert_eq!(tries(vec![failed(1, "502 of the 404 requests failed")]), 1);
    }
}

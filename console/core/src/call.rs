//! How a command of the workspace runs a program: under a bound, with what it
//! printed and how it ended, and what a failed call of gh or of the state store
//! says. The poll's calls and a cycle's go through here, so the two read one
//! failure the same way.

use regex::Regex;
use serde_json::{json, Value};
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Between the TERM and the KILL of a bounded call: a program that reads a
/// signal gets this long.
pub const GRACE: Duration = Duration::from_secs(10);

/// What a call gave: its exit status and what it printed, or None when it
/// could not be run or ran out of time.
pub struct Ran {
    pub status: Option<i32>,
    pub out: String,
    pub err: String,
    /// Set when a bounded read gave up on the answer at its ceiling, which is
    /// the answer of a program that printed more than the caller would hold.
    pub truncated: bool,
}

impl Ran {
    pub fn ok(&self) -> bool {
        self.status == Some(0)
    }
}

/// Run a program with a bound, its standard input closed.
pub fn run(program: &str, args: &[String], bound: Duration) -> Ran {
    run_input(program, args, None, bound)
}

/// Run a program with a bound, given `input` on its standard input.
pub fn run_input(program: &str, args: &[String], input: Option<&str>, bound: Duration) -> Ran {
    run_input_env(program, args, input, bound, &[])
}

/// run_input with extra variables in the child's environment, each a name and
/// its value. A poll gives every gh call it makes the token it took once.
pub fn run_input_env(
    program: &str,
    args: &[String],
    input: Option<&str>,
    bound: Duration,
    extra: &[(&str, &str)],
) -> Ran {
    run_full(program, args, input, bound, GRACE, extra)
}

/// run_input_env with the grace its caller gives. A program that ignores TERM,
/// as gh does in a report that ignores it, is killed when the grace ends.
pub fn run_full(
    program: &str,
    args: &[String],
    input: Option<&str>,
    bound: Duration,
    grace: Duration,
    extra: &[(&str, &str)],
) -> Ran {
    let mut cmd = Command::new(program);
    cmd.args(args);
    for (k, v) in extra {
        cmd.env(k, v);
    }
    let child = cmd
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let Ok(mut child) = child else {
        return Ran {
            status: None,
            out: String::new(),
            err: format!("{program} could not be run"),
            truncated: false,
        };
    };
    if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
        let text = text.to_string();
        std::thread::spawn(move || {
            use std::io::Write;
            let _ = stdin.write_all(text.as_bytes());
            // The pipe is closed when the words are written: a reader that
            // waits for the end of the request would wait for this otherwise.
            let _ = stdin.flush();
            drop(stdin);
        });
    }
    let mut out = child.stdout.take();
    let mut err = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut o = String::new();
        if let Some(s) = out.as_mut() {
            let _ = s.read_to_string(&mut o);
        }
        o
    });
    let ereader = std::thread::spawn(move || {
        let mut e = String::new();
        if let Some(s) = err.as_mut() {
            let _ = s.read_to_string(&mut e);
        }
        e
    });
    let status = wait(&mut child, bound, grace, false);
    Ran {
        status,
        out: reader.join().unwrap_or_default(),
        err: ereader.join().unwrap_or_default(),
        truncated: false,
    }
}

/// run_input_env with a ceiling on what the program prints. Its standard output
/// is read only up to `ceiling` bytes plus one: an answer that goes past the
/// ceiling is of no use to the poll, and the poll does not hold it. A program
/// that prints no end of bytes is killed by the bound, as any other is.
/// `store batch` is the program this exists for: it bounds its request and not
/// its answer, and one legal read of the store can print the whole of a brief.
pub fn run_input_bounded(
    program: &str,
    args: &[String],
    input: Option<&str>,
    bound: Duration,
    extra: &[(&str, &str)],
    ceiling: u64,
) -> Ran {
    let mut r = run_input_env(program, args, input, bound, extra);
    if r.out.len() as u64 > ceiling {
        // The bytes past the ceiling were never meant for the poll; keeping a
        // line of them for the log is enough.
        r.out.truncate(ceiling as usize);
        r.truncated = true;
    }
    r
}

/// Run a program with a bound and no pipe: nothing it leaves running can hold
/// the poll. Its output goes to `out`, or nowhere.
pub fn run_quiet(
    program: &str,
    args: &[String],
    out: Option<std::fs::File>,
    bound: Duration,
) -> Option<i32> {
    // As the bash runs its host commands: timeout -k 5.
    run_grace(program, args, out, bound, Duration::from_secs(5))
}

/// run_quiet with the grace between TERM and KILL its caller gives, as
/// `timeout -k <grace>`. The program runs in a group of its own, and the
/// signals go to the group, as timeout sends them: what it started goes too.
pub fn run_grace(
    program: &str,
    args: &[String],
    out: Option<std::fs::File>,
    bound: Duration,
    grace: Duration,
) -> Option<i32> {
    run_grace_env(program, args, out, bound, grace, &[])
}

/// run_grace with extra variables in the child's environment, each a name and
/// its value, as `run_input_env` gives them: the poll gives the commands it
/// starts at its end the token it took once, so their gh calls run no token
/// program of their own.
pub fn run_grace_env(
    program: &str,
    args: &[String],
    out: Option<std::fs::File>,
    bound: Duration,
    grace: Duration,
    extra: &[(&str, &str)],
) -> Option<i32> {
    use std::os::unix::process::CommandExt;
    let mut cmd = Command::new(program);
    cmd.args(args);
    for (k, v) in extra {
        cmd.env(k, v);
    }
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(out.map_or_else(Stdio::null, Stdio::from))
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .ok()?;
    wait(&mut child, bound, grace, true)
}

/// The child's exit status, 128 and the signal for one a signal ended, as
/// a shell gives it; None when it outlived its bound and was stopped. With
/// `group`, the child leads a group of its own and the group is signalled.
pub fn wait(
    child: &mut std::process::Child,
    bound: Duration,
    grace: Duration,
    group: bool,
) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    let began = Instant::now();
    let target = if group {
        format!("-{}", child.id())
    } else {
        child.id().to_string()
    };
    loop {
        match child.try_wait() {
            Ok(Some(s)) => break s.code().or_else(|| s.signal().map(|g| 128 + g)),
            Ok(None) if began.elapsed() < bound => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                // As timeout -k: TERM first, so a docker client stops its
                // container, and KILL when it is still there after the grace.
                let _ = Command::new("kill")
                    .args(["-TERM", "--", &target])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                let gone = Instant::now();
                while gone.elapsed() < grace {
                    if let Ok(Some(_)) = child.try_wait() {
                        // Out of time and ended by TERM: timeout's 124.
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                if let Ok(Some(_)) = child.try_wait() {
                    break None;
                }
                if group {
                    let _ = Command::new("kill")
                        .args(["-KILL", "--", &target])
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status();
                }
                let _ = child.kill();
                let _ = child.wait();
                // One that had to be killed: timeout -k answers 137.
                break Some(137);
            }
        }
    }
}

/// What a failed gh call's line says of its error: ": " and the first words
/// of a fixed list in what gh printed, or nothing. Only these words, as an
/// agent can write the rest; the bash's gh_json uses the same list.
pub fn gh_why(err: &str) -> String {
    static WORDS: OnceLock<Regex> = OnceLock::new();
    let words = WORDS.get_or_init(|| {
        Regex::new(r"(?i)HTTP [0-9]{3}|secondary rate limit|rate limit|timed? ?out|connection (reset|refused)|no such host|unexpected EOF")
            .expect("the list is a regex")
    });
    err.lines()
        .find_map(|l| words.find(l))
        .map(|m| format!(": {}", m.as_str()))
        .unwrap_or_default()
}

/// The environment a process of the poll is given beyond its own: the poll's
/// token as `TEHAS_GH_TOKEN`, which host/shim/gh hands to gh as `GH_TOKEN`. A
/// poll with no app named, or no token it could take, gives nothing and each
/// gh of it pays for a token of its own.
pub fn gh_env(token: Option<&str>) -> Vec<(&str, &str)> {
    match token {
        Some(t) => vec![("TEHAS_GH_TOKEN", t)],
        None => Vec::new(),
    }
}

/// What a failed store call's line says, cut as the bash cuts it: the control
/// characters of anything a container wrote are no part of a log line, and a
/// line over 300 of them is no part of one either.
pub fn trim(line: &str) -> String {
    line.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .take(300)
        .collect()
}

/// The first word of the store's line, as lib/store.sh's one regex takes it:
/// the text after `store: `, of lowercase letters, digits of the alphabet, `_`
/// and `-`. Nothing when the line is not the store's own, which is the case a
/// refusal of rc 1 turns into `garbled` rather than a word of nothing.
pub fn word_of(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("store: ")?;
    let end = rest
        .find(|c: char| !(c.is_ascii_lowercase() || c == '_' || c == '-'))
        .unwrap_or(rest.len());
    if end == 0 {
        return None;
    }
    Some(&rest[..end])
}

/// What one run of the store gave: the exit and the word and line to report,
/// for the answer of a single call and of a batch alike. lib/store.sh's case
/// statement is the contract -- its 124 and 137 are the timeout, its 126 and
/// 127 the program that is not there, and a refusal of 1 always names its word
/// -- and this is the one place that reads it, so a new exit of the bash is one
/// edit here rather than two. `what` is the verb, or `batch`, in the lines.
pub fn store_failure(
    what: &str,
    r: &Ran,
    bound: Duration,
    store: &str,
) -> Option<(i32, String, String)> {
    let secs = bound.as_secs();
    let named = |rc: i32, word: &str, line: String| Some((rc, word.to_string(), line));
    match r.status {
        // The program could not be run at all: the answer of the bash's own
        // `command -v` test, which is the one a poll whose store was never
        // installed can give.
        None => named(
            1,
            "timeout",
            format!("store: timeout: {what} did not answer in {secs} s"),
        ),
        Some(0) => None,
        Some(rc @ (1 | 64 | 78)) => {
            // The bash merges the error stream into the answer, so either of
            // them carries the store's own line.
            let text = if r.err.is_empty() { &r.out } else { &r.err };
            let first = text.lines().next().unwrap_or("").to_string();
            match word_of(&first) {
                // A refusal always names its word; the caller's branch hangs on
                // it, so one that does not is no answer to read.
                None if rc == 1 => named(
                    78,
                    "garbled",
                    format!("store: garbled: {what} refused without a word: {first}"),
                ),
                None => named(78, "garbled", format!("store: garbled: {what} exited {rc}")),
                Some(word) => {
                    let word = word.to_string();
                    named(rc, &word, first)
                }
            }
        }
        Some(126 | 127) => named(
            78,
            "absent",
            format!("store: absent: {store} could not be run"),
        ),
        Some(124 | 137) => named(
            1,
            "timeout",
            format!("store: timeout: {what} did not answer in {secs} s"),
        ),
        Some(rc) => named(78, "garbled", format!("store: garbled: {what} exited {rc}")),
    }
}

/// Whether the program of the store is an executable file, as lib/store.sh's
/// `-f` and `-x` tests read it: the answer of a poll whose store was never
/// installed is `absent`, and nothing else.
pub fn store_executable(store: &str) -> bool {
    std::fs::metadata(store).is_ok_and(|m| {
        use std::os::unix::fs::PermissionsExt;
        m.is_file() && m.permissions().mode() & 0o111 != 0
    })
}

/// The failure of a store call as lib/store.sh keeps it: {failed, word, rc, line}.
pub fn store_failed(rc: i32, word: &str, line: &str) -> Value {
    json!({"failed": true, "rc": rc, "word": word, "line": trim(line)})
}

/// The store's answer to one verb, or its failure as lib/store.sh keeps it:
/// {failed, word, rc, line}.
pub fn store(
    program: &str,
    args: &[&str],
    bound: Duration,
    grace: Duration,
    extra: &[(&str, &str)],
) -> Result<Value, Value> {
    let verb = args.first().copied().unwrap_or("");
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let r = run_full(program, &args, None, bound, grace, extra);
    // The exit ladder is the batch's and the bash's, in one place.
    if let Some((rc, word, line)) = store_failure(verb, &r, bound, program) {
        return Err(store_failed(rc, &word, &line));
    }
    let v: Value = serde_json::from_str(r.out.trim()).unwrap_or(Value::Null);
    // The bash reads the answer with stderr merged: anything there makes it no
    // answer.
    if r.err.is_empty()
        && !r.out.trim().contains('\n')
        && v.get("api") == Some(&json!(1))
        && v.get("ok") == Some(&json!(true))
        && v.get("verb").and_then(Value::as_str) == Some(verb)
    {
        Ok(v)
    } else {
        Err(store_failed(
            78,
            "garbled",
            &format!("store: garbled: {verb} answered 0 without its answer"),
        ))
    }
}

/// tehas_store_try: asked again, `wait` apart, only when the store answered 1
/// busy or timeout.
pub fn store_try(
    tries: u32,
    wait: Duration,
    mut call: impl FnMut() -> Result<Value, Value>,
) -> Result<Value, Value> {
    let mut i = 1;
    loop {
        let r = call();
        match &r {
            Err(f)
                if i < tries
                    && f.get("rc") == Some(&json!(1))
                    && matches!(
                        f.get("word").and_then(Value::as_str),
                        Some("busy" | "timeout")
                    ) =>
            {
                i += 1;
                std::thread::sleep(wait);
            }
            _ => return r,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(x: &[&str]) -> Vec<String> {
        x.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_failure_is_named_by_the_words_of_the_list_only() {
        assert_eq!(
            gh_why("HTTP 502: Bad Gateway for query with a sk-secret-looking body"),
            ": HTTP 502"
        );
        assert_eq!(
            gh_why("first line\nerror: API Secondary Rate Limit exceeded"),
            ": Secondary Rate Limit"
        );
        assert_eq!(
            gh_why("dial tcp: lookup api.github.com: no such host"),
            ": no such host"
        );
        assert_eq!(
            gh_why("read: connection reset by peer"),
            ": connection reset"
        );
        assert_eq!(gh_why("request timed out"), ": timed out");
        assert_eq!(gh_why("something an agent wrote"), "");
        assert_eq!(gh_why(""), "");
    }

    #[test]
    fn a_call_past_its_bound_is_cut_off() {
        let r = run("sleep", &a(&["5"]), Duration::from_millis(100));
        assert_eq!(r.status, None);
        assert!(run("true", &[], Duration::from_secs(5)).ok());
    }

    #[test]
    fn a_store_that_is_busy_is_asked_again_and_a_refusal_is_not() {
        let busy = || Err(json!({"failed": true, "rc": 1, "word": "busy"}));
        let mut n = 0;
        let r = store_try(3, Duration::ZERO, || {
            n += 1;
            busy()
        });
        assert!(r.is_err());
        assert_eq!(n, 3);
        let mut n = 0;
        let _ = store_try(3, Duration::ZERO, || {
            n += 1;
            Err(json!({"failed": true, "rc": 1, "word": "unapproved"}))
        });
        assert_eq!(n, 1);
    }
}

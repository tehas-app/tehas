//! The runner of gh that the forge's callers share: [`GhRunner`], the trait
//! a call to gh is made through, and [`Bounded`], the runner a host uses,
//! with the reading of a failed call's reason that the runner makes.
//!
//! The forge's terms and their GitHub side are the adapter's, `tehas-github`
//! (`console/github/`), behind the tickets port of `tehas-ports`. What stays
//! here is the runner, until its last caller outside the adapter moves
//! behind the ports and it goes into the adapter (T4.11).
//!
//! A failed call's reason is made from a fixed list of error words, so a
//! caller can log why it stopped without holding gh's whole answer, which
//! may name a host or carry a token. Nothing here panics.

/// gh, behind a trait of its own, so the adapter's rules are read without gh,
/// a token or a network: a test hands the adapter a stub that answers from
/// fixtures, and what it decides is the rules alone.
///
/// A runner runs one gh call and owns how: it holds the program, gives the
/// call its bound, and reads its standard output. `args` are the words after
/// the program, which the runner does not repeat, and the reason an adapter
/// holds is the runner's own for the failure, not what gh printed.
pub trait GhRunner {
    /// Run gh with these arguments: its standard output, or the reason it
    /// gave for failing.
    fn run(&mut self, args: &[&str]) -> Result<String, String>;

    /// Run gh with these arguments and `input` on its standard input, the way
    /// a write sends a body: a body may be as long as 1 MiB, past what one
    /// argument may hold (128 KiB), and an argument list is seen by every
    /// process on the host. The reason a call failed never holds the input.
    fn run_input(&mut self, args: &[&str], input: &str) -> Result<String, String> {
        let _ = (args, input);
        Err("this runner takes no input".to_string())
    }
}

/// Given to every gh call of tehas's own, by `Bounded` and by `call`'s
/// runners: the host's shim then sends a rate-limited write once, so a
/// call's bound is not spent in the shim's minute of waiting. An operator's
/// or a plugin's gh keeps the retry.
pub const GH_NO_RETRY: (&str, &str) = ("TEHAS_GH_RETRY", "0");

/// A program is gh by its file name: `gh` on the PATH, or a shim's path.
pub fn is_gh(program: &str) -> bool {
    std::path::Path::new(program).file_name() == Some("gh".as_ref())
}

/// The runner a host uses: gh of its own, in a bound of its own. One call per
/// [`GhRunner::run`], so nothing one call set is seen by the next. The call
/// has the shim's retry off ([`GH_NO_RETRY`]).
///
/// The child leads a process group and the group is signalled, as `timeout -k`
/// does: TERM first, KILL after the grace, so gh and what it started are
/// gone. A poll holds `state/dispatch.lock` for its whole read, so a gh that
/// never returns would hold the lock for as long as it lives, which is why the
/// bound belongs here and not in the caller's head. The dispatcher's
/// `read::run::run_input_env` is the same bound and the same signals; it is
/// `tehas-dispatch`'s, so the wait is held here where the forge is, and the two
/// go into one with the callers of #800.
pub struct Bounded {
    /// The program to run. A host runs its shim, `host/shim/gh`, which takes
    /// the token from `TEHAS_GH_TOKEN` when the caller holds one.
    pub program: String,
    /// How long one call may take before it is stopped.
    pub bound: std::time::Duration,
    /// The grace between TERM and KILL, as `timeout -k` gives.
    pub grace: std::time::Duration,
}

impl Bounded {
    /// The runner of one program under one bound, with the 5-second grace
    /// `timeout -k` uses by default. A poll gives a call of its own gh the
    /// same bound the dispatcher's reads use: thirty seconds.
    pub fn new(program: impl Into<String>, bound: std::time::Duration) -> Bounded {
        Bounded {
            program: program.into(),
            bound,
            grace: std::time::Duration::from_secs(5),
        }
    }
}

impl GhRunner for Bounded {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.call(args, None)
    }

    /// The input is written on a pipe of its own thread, closed once written,
    /// within the call's bound: a gh that stops reading is stopped with it.
    fn run_input(&mut self, args: &[&str], input: &str) -> Result<String, String> {
        self.call(args, Some(input))
    }
}

impl Bounded {
    fn call(&mut self, args: &[&str], input: Option<&str>) -> Result<String, String> {
        use std::io::{Read, Write};
        use std::os::unix::process::CommandExt;
        use std::process::{Command, Stdio};

        let mut child = Command::new(&self.program)
            .args(args)
            .env(GH_NO_RETRY.0, GH_NO_RETRY.1)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
            .map_err(|e| format!("{} could not be run: {e}", self.program))?;
        // Both pipes are drained on their own thread: a child that fills one
        // while the other is still being read would never get to its end.
        let out = child.stdout.take().expect("piped");
        // None when the stream could not be read to its end: an empty answer
        // is then no answer of gh's.
        let drain = |mut s: Box<dyn Read + Send>| {
            std::thread::spawn(move || {
                let mut text = Vec::new();
                s.read_to_end(&mut text).ok()?;
                Some(String::from_utf8_lossy(&text).into_owned())
            })
        };
        let reading = drain(Box::new(out));
        let reading_err = drain(Box::new(child.stderr.take().expect("piped")));
        // Dropped once written, so gh sees the end of its input.
        let writing = child.stdin.take().map(|mut pipe| {
            let text = input.unwrap_or_default().to_string();
            std::thread::spawn(move || {
                let _ = pipe.write_all(text.as_bytes());
            })
        });

        let code = wait(&mut child, self.bound, self.grace);
        if let Some(w) = writing {
            let _ = w.join();
        }
        let out = reading.join().ok().flatten();
        let err = reading_err.join().ok().flatten().unwrap_or_default();
        match code {
            Some(0) => out.ok_or_else(|| format!("{}'s answer could not be read", self.program)),
            // Outlived its bound: a call that says nothing of itself still
            // says what stopped it.
            None => Err(format!(
                "{} ran out of its {}s bound",
                self.program,
                self.bound.as_secs()
            )),
            Some(c) => Err(why(&err, Some(c))),
        }
    }
}

/// The child's exit status, the signal as 128 plus its number, and None when
/// it outlived its bound and was stopped. The child leads a group of its own,
/// so the signals go to it and to what it started, as `timeout -k` sends them.
fn wait(
    child: &mut std::process::Child,
    bound: std::time::Duration,
    grace: std::time::Duration,
) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    use std::time::{Duration, Instant};

    // Negative: kill signals the group whose id it is, the child and what it
    // started, not the child alone.
    let group = format!("-{}", child.id());
    let began = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(s)) => return s.code().or_else(|| s.signal().map(|g| 128 + g)),
            Ok(None) if began.elapsed() < bound => std::thread::sleep(Duration::from_millis(20)),
            _ => break,
        }
    }
    signal("-TERM", &group);
    let term = Instant::now();
    while term.elapsed() < grace {
        if let Ok(Some(_)) = child.try_wait() {
            // Stopped by TERM: timeout answers 124.
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    signal("-KILL", &group);
    let _ = child.kill();
    let _ = child.wait();
    // One that had to be killed: timeout -k answers 137.
    Some(137)
}

/// One signal to the group, in a call that can neither print nor block.
fn signal(which: &str, group: &str) {
    use std::process::Stdio;

    let _ = std::process::Command::new("kill")
        .args([which, "--", group])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Whether a failed removal answers that the label was not there, which is the
/// state wanted. The status is read whole out of the digits `find_http`
/// located, never as "404" anywhere in the text: a gateway error whose body
/// happens to carry those three digits is no removal. gh's own words
/// `Not Found` count only as the whole reason, never inside one that says
/// something else: a gh that could not start, or a proxy whose host was not
/// found, left the label where it was.
pub fn says_not_found(why: &str) -> bool {
    let w = why.to_ascii_lowercase();
    if w.strip_prefix(": ").unwrap_or(&w).trim() == "not found" {
        return true;
    }
    find_http(&w).is_some_and(|i| &w.as_bytes()[i + 5..i + 8] == b"404")
}

/// What a failed gh call's answer says of its error: ": " and the first of a
/// fixed list of words in what gh printed, else the exit status. The list is
/// the one the bash dispatcher's `gh_json` grepped and the dispatcher's
/// `gh_why` holds, so a caller logs the reason the poll logs. Only these words, or a
/// status: the rest of gh's answer is written by a host and may name one, or
/// carry a token.
pub fn why(err: &str, code: Option<i32>) -> String {
    // Longest first where two could match the same word, so a secondary limit
    // is not reported as a plain one where they start alike.
    const WORDS: [&str; 8] = [
        "secondary rate limit",
        "unexpected EOF",
        "connection reset",
        "connection refused",
        "rate limit",
        "timed out",
        "timeout",
        "no such host",
    ];
    for line in err.lines() {
        let low = line.to_ascii_lowercase();
        // The first line that says anything is the one quoted, and within it
        // the LEFTMOST word of the list, as grep -oE and the dispatcher's
        // regex answer: the first entry of the list would report "rate limit"
        // for a line whose only match is a secondary limit later on. Only
        // these words, so the rest of gh's answer - a host's address, a token
        // - stays out. The text is cut from `low`, the string that was
        // searched, so the byte offsets of `find` are its own.
        if let Some((i, w)) = WORDS
            .iter()
            .filter_map(|w| low.find(w).map(|i| (i, *w)))
            .min_by_key(|(i, w)| (*i, std::cmp::Reverse(w.len())))
        {
            return format!(": {}", &low[i..i + w.len()]);
        }
        // An HTTP status is gh's own word for the state and is kept whole.
        if let Some(i) = find_http(&low) {
            return format!(": HTTP {}", &low[i + 5..i + 8]);
        }
    }
    match code {
        Some(c) => format!(": gh exited {c}"),
        None => ": gh was stopped by a signal".to_string(),
    }
}

/// Where a lower-case line names an HTTP status: "http " and the digits after
/// it. The digits are checked so that a line that only holds the word is not
/// read as a status.
fn find_http(line: &str) -> Option<usize> {
    let b = line.as_bytes();
    let head = b"http ";
    let mut i = 0;
    while i + head.len() + 3 <= b.len() {
        if b[i..i + head.len()] == head[..] && b[i + 5..i + 8].iter().all(u8::is_ascii_digit) {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    /// The GitHub adapter (console/github, `ask_issue` in `ticket.rs`)
    /// reads this form: a question that fails with no reason of the list
    /// is settled by one more REST read. Change it there too.
    #[test]
    fn a_failure_with_no_reason_of_the_list_is_the_exit_status() {
        for err in [
            "",
            "GraphQL: Could not resolve to an Issue with the number of 9.",
        ] {
            assert_eq!(super::why(err, Some(1)), ": gh exited 1", "{err}");
        }
        assert_eq!(super::why("", Some(4)), ": gh exited 4");
    }

    #[test]
    fn a_gh_call_of_call_turns_the_shims_retry_off_and_another_program_is_left_alone() {
        use crate::call::{run, run_grace_env, GRACE};
        let d = crate::testdir::TestDir::new("gh-no-retry");
        let says = "#!/bin/sh\necho \"${TEHAS_GH_RETRY-unset}\"\n";
        let (gh, other) = (d.join("gh"), d.join("not-gh"));
        crate::testing::write_exe(&gh, says).unwrap();
        crate::testing::write_exe(&other, says).unwrap();
        let bound = std::time::Duration::from_secs(10);
        assert_eq!(run(gh.to_str().unwrap(), &[], bound).out, "0\n");
        assert_eq!(run(other.to_str().unwrap(), &[], bound).out, "unset\n");
        let kept = d.join("out");
        let f = std::fs::File::create(&kept).unwrap();
        let rc = run_grace_env(gh.to_str().unwrap(), &[], Some(f), bound, GRACE, &[]);
        assert_eq!(rc, Some(0));
        assert_eq!(std::fs::read_to_string(&kept).unwrap(), "0\n");
    }

    use super::{Bounded, GhRunner};
    use std::time::{Duration, Instant};

    /// A runner whose program writes each argument on a line of `args` and
    /// its standard input to `input`, then runs `then`.
    fn recording(d: &crate::testdir::TestDir, then: &str) -> Bounded {
        let program = d.join("runner");
        crate::testing::write_exe(
            &program,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" >'{a}'\ncat >'{i}'\n{then}\n",
                a = d.join("args").display(),
                i = d.join("input").display(),
            ),
        )
        .unwrap();
        Bounded::new(program.to_str().unwrap(), Duration::from_secs(10))
    }

    #[test]
    fn an_input_goes_on_standard_input_and_never_into_an_argument() {
        let d = crate::testdir::TestDir::new("forge-run-input");
        let body = format!("{}end", "body ".repeat(200 * 1024 / 5));
        let mut runner = recording(&d, "echo answered");
        let got = runner.run_input(&["api", "repos/x/issues", "--input", "-"], &body);
        assert_eq!(got, Ok("answered\n".to_string()));
        assert_eq!(std::fs::read_to_string(d.join("input")).unwrap(), body);
        let args = std::fs::read_to_string(d.join("args")).unwrap();
        assert_eq!(args, "api\nrepos/x/issues\n--input\n-\n");
    }

    #[test]
    fn a_failed_call_with_an_input_never_answers_the_input() {
        let d = crate::testdir::TestDir::new("forge-run-input-fails");
        let mut runner = recording(
            &d,
            &format!("cat '{}' >&2\nexit 1", d.join("input").display()),
        );
        let got = runner.run_input(&["api", "x"], "a private body\nof two lines\n");
        let why = got.unwrap_err();
        assert!(!why.contains("private"), "{why}");
        assert_eq!(why, ": gh exited 1");
    }

    #[test]
    fn a_gh_that_never_reads_its_input_is_stopped_within_its_bound() {
        let d = crate::testdir::TestDir::new("forge-run-input-unread");
        let program = d.join("runner");
        crate::testing::write_exe(&program, "#!/bin/sh\nsleep 30\n").unwrap();
        let mut runner = Bounded {
            program: program.to_str().unwrap().to_string(),
            bound: Duration::from_secs(1),
            grace: Duration::from_secs(1),
        };
        let began = Instant::now();
        let got = runner.run_input(&["api", "x"], &"x".repeat(1 << 20));
        assert!(
            began.elapsed() < Duration::from_secs(8),
            "{:?}",
            began.elapsed()
        );
        assert!(got.unwrap_err().contains("ran out of its 1s bound"));
    }

    #[test]
    fn what_a_gh_started_is_stopped_with_it_within_its_bound() {
        let d = crate::testdir::TestDir::new("forge-run-group");
        let program = d.join("runner");
        // A shim whose gh outlives it would hold the pipes open.
        crate::testing::write_exe(&program, "#!/bin/sh\nsleep 30 &\nwait\n").unwrap();
        let mut runner = Bounded {
            program: program.to_str().unwrap().to_string(),
            bound: Duration::from_secs(1),
            grace: Duration::from_secs(1),
        };
        let began = Instant::now();
        let got = runner.run(&["api", "x"]);
        assert!(
            began.elapsed() < Duration::from_secs(8),
            "{:?}",
            began.elapsed()
        );
        assert!(got.unwrap_err().contains("ran out of its 1s bound"));
    }

    #[test]
    fn a_runner_that_takes_no_input_says_so_and_runs_nothing() {
        struct Plain(u32);
        impl GhRunner for Plain {
            fn run(&mut self, _: &[&str]) -> Result<String, String> {
                self.0 += 1;
                Ok(String::new())
            }
        }
        let mut plain = Plain(0);
        assert_eq!(
            plain.run_input(&["api"], "body"),
            Err("this runner takes no input".to_string())
        );
        assert_eq!(plain.0, 0);
    }
}

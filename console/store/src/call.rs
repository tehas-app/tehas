//! One call of the store, as the program makes it: the verb parsed from its
//! arguments, `now` taken, the verb run and its line returned. The program's
//! `main` and the console both go through `call`, so a caller in the same
//! process gets the bytes the program would print.
//!
//! A build with debug assertions also has the tests' seam: the file that
//! `TEHAS_TEST_STORE_ANSWERS` names holds lines of `<args>|<rc>|<line>`, with
//! `|<times>` after them when only the first `<times>` calls are answered. A
//! call whose arguments are `<args>`, or begin with `<args>` and a space, gets
//! `<rc>` and `<line>` without running; a `batch` gets them when any call in it
//! matches. The newest line that matches is the one that answers, and the
//! calls it answered are counted in `<file>.n`. No file is no answers.
//! install.sh builds `--release`, which never reads the variable.

use crate::{batch, execute, now, parse, say, verbs, Verb, USAGE};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// The line of a verb, or its exit status and the line for standard error.
pub type Answer = Result<String, (u8, String)>;

/// The variable that names the tests' answers. Only `call` reads it, and only
/// in a build with debug assertions.
pub const ANSWERS: &str = "TEHAS_TEST_STORE_ANSWERS";

/// One call of the store in `home`. `input` is read by `batch` only, which
/// takes its calls from it.
pub fn call(home: &Path, args: &[&str], input: &mut dyn Read) -> Answer {
    #[cfg(debug_assertions)]
    let answers = std::env::var_os(ANSWERS).map(PathBuf::from);
    #[cfg(not(debug_assertions))]
    let answers: Option<PathBuf> = None;
    call_with(home, args, input, answers.as_deref())
}

/// `call` with the answers file given, so a test does not set it in the
/// environment every test of the process shares.
pub fn call_with(
    home: &Path,
    args: &[&str],
    input: &mut dyn Read,
    answers: Option<&Path>,
) -> Answer {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let store = |why: String| format!("store: {why}");
    let batch = args.first().is_some_and(|a| a == "batch");
    if !batch {
        if let Some(answer) = answers.and_then(|file| planted(file, &[args.join(" ")])) {
            return answer;
        }
    }
    if let Some(Err(why)) = verbs::parse(&args) {
        return Err((64, store(why)));
    }
    // A batch's home is judged before a byte of its request is read, so a call
    // with no usable home stops at 78 without holding the pipe open.
    if batch {
        if args[1..].iter().any(|a| a != "--json") {
            return Err((64, USAGE.to_string()));
        }
        let home = home_of(home)?;
        let now = now().map_err(|why| (78, store(why)))?;
        let text = batch::read_input(input).map_err(|why| (64, store(why)))?;
        if let Some(file) = answers {
            let calls: Vec<String> = serde_json::from_str::<Vec<Vec<String>>>(&text)
                .map(|calls| calls.iter().map(|c| c.join(" ")).collect())
                .unwrap_or_default();
            if let Some(answer) = planted(file, &calls) {
                return answer;
            }
        }
        let asked = batch::Request::of(&text).map_err(|why| (64, store(why)))?;
        return execute(home, &Verb::Batch(asked), now).map_err(|(rc, why)| (rc, store(why)));
    }
    let Some(verb) = parse(&args) else {
        return Err((64, USAGE.to_string()));
    };
    let home = home_of(home)?;
    let now = now().map_err(|why| (78, store(why)))?;
    execute(home, &verb, now).map_err(|(rc, why)| (rc, store(why)))
}

/// What `--say` prints for one call of `init`, `migrate` or `import`: the same
/// checks as `call`, then the verb's answer in the words a person reads.
pub fn said(home: &Path, args: &[String]) -> say::Said {
    let refused = |code: u8, line: String| say::Said {
        code,
        out: vec![],
        err: vec![line],
    };
    if let Some(Err(why)) = verbs::parse(args) {
        return refused(64, format!("store: {why}"));
    }
    let Some(verb) = parse(args) else {
        return refused(64, USAGE.to_string());
    };
    let home = match home_of(home) {
        Ok(h) => h,
        Err((code, line)) => return refused(code, line),
    };
    let now = match now() {
        Ok(t) => t,
        Err(why) => return refused(78, format!("store: {why}")),
    };
    say::say(home, &verb, execute(home, &verb, now), now)
}

/// The home of a call: the same rule for every verb, judged before anything
/// is read.
fn home_of(home: &Path) -> Result<&Path, (u8, String)> {
    if home.is_absolute() && home.is_dir() {
        Ok(home)
    } else {
        Err((
            78,
            "store: TEHAS_HOME is not set to a directory".to_string(),
        ))
    }
}

/// The planted answer for a call made of `calls` (one for a verb, each call of
/// a batch), when the newest line of `file` that matches one of them still has
/// times left. Each call it matched is counted in `<file>.n`.
fn planted(file: &Path, calls: &[String]) -> Option<Answer> {
    let text = fs::read_to_string(file).ok()?;
    let (rc, line, times) = text.lines().rev().find_map(|row| {
        let (args, rest) = row.split_once('|')?;
        let (rc, rest) = rest.split_once('|')?;
        let rc: u8 = rc.parse().ok()?;
        let (line, times) = match rest.rsplit_once('|') {
            Some((line, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => {
                (line, n.parse::<u64>().ok())
            }
            _ => (rest, None),
        };
        calls
            .iter()
            .any(|c| c == args || c.strip_prefix(args).is_some_and(|r| r.starts_with(' ')))
            .then(|| (rc, line.to_string(), times))
    })?;
    let count = counter(file);
    let n = fs::read_to_string(&count)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0)
        + 1;
    let _ = fs::write(&count, format!("{n}\n"));
    if times.is_some_and(|t| n > t) {
        return None;
    }
    Some(if rc == 0 { Ok(line) } else { Err((rc, line)) })
}

fn counter(file: &Path) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(".n");
    PathBuf::from(name)
}

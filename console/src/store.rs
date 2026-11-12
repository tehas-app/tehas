//! The state store, called in this process through the store crate's `call`,
//! and the two ways of running a command the console's other modules need.

use serde_json::{json, Value};
use std::fs;
use std::io::{self, Read};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread::sleep;
use std::time::Duration;

/// What a call that did not answer said: the status
/// (1 refused, busy or timeout; 64 usage; 78 unusable), the store's word or
/// ours (timeout, absent, garbled), and the first line, as `said` keeps it for
/// a log: no control bytes and at most 300 bytes.
#[derive(Debug)]
pub struct Refusal {
    pub rc: u8,
    pub word: String,
    pub line: String,
}

/// The two tests `tehas_store_ready` makes: the store an executable file of
/// this home, and its database a plain file and no link. One copy, so a caller
/// cannot forget one; a stat that errors is answered as a binary that is not
/// there, as the shell's `[ -f ]` and `[ -x ]` answered it.
pub fn ready(home: &Path) -> bool {
    let db = home.join("state").join("tehas.db");
    installed(home)
        && fs::symlink_metadata(&db).is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
}

/// Where install.sh puts the store program for the shell, the cycle and the
/// dispatcher. The console never runs it; it reads whether it is there.
pub fn program(home: &Path) -> PathBuf {
    home.join("libexec").join("store")
}

/// Whether the store program of this home is an executable file.
fn installed(home: &Path) -> bool {
    fs::metadata(program(home)).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// What a failed call sets, as `tehas_store_said` does: the line is for a log,
/// so every control character becomes `?` and it is cut at 300 bytes. Every
/// refusal of `call` goes through here, so a caller prints the line as it
/// stands.
fn refusal(rc: u8, word: &str, line: &str) -> Refusal {
    let cleaned: String = line
        .chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect();
    let mut cut = cleaned.len().min(300);
    while cut > 0 && !cleaned.is_char_boundary(cut) {
        cut -= 1;
    }
    Refusal {
        rc,
        word: word.to_string(),
        line: cleaned[..cut].to_string(),
    }
}

fn code(status: ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
}

/// Standard output and standard error as one text, and the exit status: 127
/// for a command that could not be started.
pub fn merged(cmd: &mut Command) -> (i32, String) {
    let Ok((mut reader, writer)) = io::pipe() else {
        return (127, String::new());
    };
    let Ok(other) = writer.try_clone() else {
        return (127, String::new());
    };
    cmd.stdout(writer).stderr(other);
    let child = cmd.spawn();
    // The command holds the writers; the read ends only once they are closed.
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    let Ok(mut child) = child else {
        return (127, String::new());
    };
    let mut said = Vec::new();
    let _ = reader.read_to_end(&mut said);
    let rc = child.wait().map(code).unwrap_or(127);
    (rc, String::from_utf8_lossy(&said).into_owned())
}

pub fn bounded(kill: &str, limit: &str, program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = Command::new("timeout");
    cmd.args(["-k", kill, limit]).arg(program);
    cmd
}

/// One call of the store: its answer, or what it said instead.
pub fn call(home: &Path, args: &[&str]) -> Result<Value, Refusal> {
    answer(home, args).map(|(v, _)| v)
}

/// The same call, as the line the store printed, keys in its order.
pub fn line(home: &Path, args: &[&str]) -> Result<String, Refusal> {
    answer(home, args).map(|(_, line)| line)
}

fn answer(home: &Path, args: &[&str]) -> Result<(Value, String), Refusal> {
    let verb = args[0];
    let no = |rc: u8, word: &str, line: String| Err(refusal(rc, word, &line));
    // A home without the program has no store this console may use either.
    if !installed(home) {
        return no(
            78,
            "absent",
            format!(
                "store: absent: {} could not be run",
                program(home).display()
            ),
        );
    }
    match run(home, args) {
        Ok(said) => {
            let v: Value = serde_json::from_str(&said).unwrap_or(Value::Null);
            if !said.contains('\n')
                && v["api"] == json!(1)
                && v["ok"] == json!(true)
                && v["verb"] == json!(verb)
            {
                Ok((v, said))
            } else {
                no(
                    78,
                    "garbled",
                    format!("store: garbled: {verb} answered 0 without its answer"),
                )
            }
        }
        Err((rc @ (1 | 64 | 78), said)) => {
            let first = said.split('\n').next().unwrap_or("");
            let word = first
                .strip_prefix("store: ")
                .map(|r| r.split([':', ' ']).next().unwrap_or("").to_string())
                .filter(|w| {
                    w.bytes().next().is_some_and(|c| c.is_ascii_lowercase())
                        && w.bytes()
                            .all(|c| c.is_ascii_lowercase() || c == b'_' || c == b'-')
                })
                .unwrap_or_default();
            if rc == 1 && word.is_empty() {
                return no(
                    78,
                    "garbled",
                    format!("store: garbled: {verb} refused without a word: {first}"),
                );
            }
            no(rc, &word, first.to_string())
        }
        Err((rc, _)) => no(78, "garbled", format!("store: garbled: {verb} exited {rc}")),
    }
}

/// The store's own `call`, in this process. A unit test's home may have
/// answers planted for it (`testing::plant`), which go to `call_with` rather
/// than through the environment the tests share.
fn run(home: &Path, args: &[&str]) -> ::store::Answer {
    #[cfg(test)]
    if let Some(file) = testing::noted(home, args) {
        return ::store::call_with(home, args, &mut io::empty(), Some(&file));
    }
    ::store::call(home, args, &mut io::empty())
}

/// `call` again while the word is busy or timeout, one second apart, at most
/// `tries` calls, as `tehas_store_try` does.
pub fn try_call(home: &Path, tries: u32, args: &[&str]) -> Result<Value, Refusal> {
    let mut made = 1;
    loop {
        match call(home, args) {
            Err(no)
                if no.rc == 1 && made < tries && matches!(no.word.as_str(), "busy" | "timeout") =>
            {
                made += 1;
                sleep(Duration::from_secs(1));
            }
            answer => return answer,
        }
    }
}

/// The unit tests' hook: answers planted for one home, and the calls made in
/// it. Keyed by home, so the tests of the harness, each with a home of its own,
/// run side by side.
#[cfg(test)]
pub mod testing {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, MutexGuard};

    #[derive(Default)]
    struct Home {
        answers: Option<PathBuf>,
        calls: Vec<String>,
    }

    static HOMES: Mutex<Option<HashMap<PathBuf, Home>>> = Mutex::new(None);

    fn homes() -> MutexGuard<'static, Option<HashMap<PathBuf, Home>>> {
        HOMES.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Calls in `home` are answered from `file`, a file of lines of
    /// `<args>|<rc>|<line>[|<times>]` (store/src/call.rs).
    pub fn plant(home: &Path, file: &Path) {
        homes()
            .get_or_insert_with(HashMap::new)
            .entry(home.to_path_buf())
            .or_default()
            .answers = Some(file.to_path_buf());
    }

    /// The calls made in `home` so far, each its arguments joined by spaces.
    pub fn calls(home: &Path) -> Vec<String> {
        homes()
            .as_ref()
            .and_then(|h| h.get(home))
            .map(|h| h.calls.clone())
            .unwrap_or_default()
    }

    /// Notes one call and gives the answers planted for its home.
    pub(super) fn noted(home: &Path, args: &[&str]) -> Option<PathBuf> {
        let mut homes = homes();
        let at = homes
            .get_or_insert_with(HashMap::new)
            .entry(home.to_path_buf())
            .or_default();
        at.calls.push(args.join(" "));
        at.answers.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tehas_core::testdir::TestDir;

    /// A home with the program where install.sh puts it and a store made by
    /// the store's own init, whose calls are answered from `answers`.
    fn home(name: &str, answers: &str) -> TestDir {
        let home = TestDir::new(&format!("store-{name}"));
        fs::create_dir_all(home.join("libexec")).unwrap();
        tehas_core::testing::write_exe(program(&home), "#!/bin/sh\nexit 1\n").unwrap();
        let made = ::store::call(
            &home,
            &[
                "init",
                "--version",
                "v0.0.0",
                "--commit",
                "0123456789abcdef0123456789abcdef01234567",
                "--instance",
                "instance",
                "--repo",
                "acme/widget",
            ],
            &mut io::empty(),
        );
        assert!(made.is_ok(), "{made:?}");
        let file = home.join("answers");
        fs::write(&file, answers).unwrap();
        testing::plant(&home, &file);
        home
    }

    const BUSY: &str = "check|1|store: busy: locked";

    #[test]
    fn a_busy_store_is_asked_again() {
        let home = home("again", &format!("{BUSY}|1\n"));
        assert!(try_call(&home, 3, &["check"]).is_ok());
        assert_eq!(testing::calls(&home), ["check", "check"]);
    }

    #[test]
    fn the_tries_are_a_ceiling_and_the_last_refusal_is_the_answer() {
        let home = home("ceiling", "check|1|store: busy: still\n");
        let no = try_call(&home, 3, &["check"]).unwrap_err();
        assert_eq!((no.rc, no.word.as_str()), (1, "busy"));
        assert_eq!(no.line, "store: busy: still");
        assert_eq!(testing::calls(&home).len(), 3);
    }

    #[test]
    fn another_refusal_is_not_asked_again() {
        let home = home("refused", "check|1|store: conflict #5: no\n");
        let no = try_call(&home, 3, &["check"]).unwrap_err();
        assert_eq!(no.word, "conflict");
        assert_eq!(testing::calls(&home).len(), 1);
    }

    #[test]
    fn a_call_answers_the_line_of_the_store() {
        let home = home("line", "");
        let line = line(&home, &["check"]).unwrap();
        assert!(
            line.starts_with("{\"api\":1,\"verb\":\"check\",\"ok\":true"),
            "{line}"
        );
    }

    #[test]
    fn a_home_without_the_program_is_absent() {
        let home = home("absent", "");
        fs::remove_file(program(&home)).unwrap();
        let no = call(&home, &["check"]).unwrap_err();
        assert_eq!((no.rc, no.word.as_str()), (78, "absent"));
        assert!(testing::calls(&home).is_empty());
    }

    #[test]
    fn a_refusal_without_a_word_or_an_answer_of_another_verb_is_garbled() {
        let home = home(
            "garbled",
            "check|1|no word here\nbackup|0|{\"api\":1,\"ok\":true,\"verb\":\"check\"}\n",
        );
        let no = call(&home, &["check"]).unwrap_err();
        assert_eq!((no.rc, no.word.as_str()), (78, "garbled"));
        let no = call(&home, &["backup"]).unwrap_err();
        assert_eq!((no.rc, no.word.as_str()), (78, "garbled"));
    }
}
